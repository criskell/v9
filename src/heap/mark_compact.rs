use crate::execution::isolate::Isolate;
use crate::heap::marking::MarkingBitmap;
use crate::heap::marking_worklist::MarkingWorklist;
use crate::objects::tagged::{Address, HeapObject, Tagged};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkCompactStats {
    pub marked_objects: usize,
    pub marked_bytes: usize,
    pub swept_objects: usize,
    pub swept_bytes: usize,
    pub moved_objects: usize,
    pub cleared_weak_references: usize,
    pub old_space_before: usize,
    pub old_space_after: usize,
}

impl Isolate {
    /// `MarkCompactCollector::CollectGarbage`
    pub fn collect_all_garbage(&mut self) -> MarkCompactStats {
        let mut stats = MarkCompactStats {
            old_space_before: self.heap.old_space().size(),
            ..Default::default()
        };

        let mut marking_bitmap = MarkingBitmap::new(
            self.heap.old_space().start(),
            (self.heap.old_space().limit() - self.heap.old_space().start()) as usize,
        );

        self.mark_live_objects(&mut marking_bitmap, &mut stats);
        self.clear_non_live_references(&marking_bitmap, &mut stats);
        self.evacuate(&marking_bitmap, &mut stats);

        stats.old_space_after = self.heap.old_space().size();
        self.heap.gc_count += 1;
        stats
    }

    /// `MarkCompactCollector::MarkLiveObjects`
    fn mark_live_objects(&mut self, bitmap: &mut MarkingBitmap, stats: &mut MarkCompactStats) {
        let mut worklist = MarkingWorklist::new();
        self.mark_roots(bitmap, &mut worklist);
        self.drain_marking_worklist(bitmap, &mut worklist, stats);
    }

    /// `MarkCompactCollector::MarkRoots`
    fn mark_roots(&mut self, bitmap: &mut MarkingBitmap, worklist: &mut MarkingWorklist) {
        let mut roots: Vec<Tagged> = Vec::new();
        self.iterate_roots(&mut |slot| roots.push(*slot));
        for value in roots {
            self.mark_object(bitmap, worklist, value);
        }
    }

    /// `MarkCompactCollector::MarkObject`
    fn mark_object(
        &mut self,
        bitmap: &mut MarkingBitmap,
        worklist: &mut MarkingWorklist,
        value: Tagged,
    ) {
        let Some(object) = value.to_strong_heap_object() else {
            return;
        };
        if !self.is_in_old_space(object.address()) {
            if self.is_outside_old_space(object.address()) {
                worklist.push(object);
            }
            return;
        }
        if bitmap.set(object.address()) {
            worklist.push(object);
        }
    }

    /// `MarkCompactCollector::DrainMarkingWorklist`
    fn drain_marking_worklist(
        &mut self,
        bitmap: &mut MarkingBitmap,
        worklist: &mut MarkingWorklist,
        stats: &mut MarkCompactStats,
    ) {
        let mut visited: Vec<Address> = Vec::new();
        while let Some(object) = worklist.pop() {
            if self.is_in_old_space(object.address()) {
                stats.marked_objects += 1;
                stats.marked_bytes += self.object_size(object) as usize;
            } else {
                if visited.contains(&object.address()) {
                    continue;
                }
                visited.push(object.address());
            }

            let map = self.heap.map_of(object).tagged();
            self.mark_object(bitmap, worklist, map);

            let mut slots: Vec<Address> = Vec::new();
            self.visit_object_slots(object, &mut |address| slots.push(address));
            for slot in slots {
                let value = self.heap.read_tagged(slot);
                if value.is_weak() {
                    continue;
                }
                self.mark_object(bitmap, worklist, value);
            }
        }
    }

    /// `MarkCompactCollector::ClearNonLiveReferences`
    fn clear_non_live_references(&mut self, bitmap: &MarkingBitmap, stats: &mut MarkCompactStats) {
        let cleared = Tagged::cleared();
        let mut weak_slots: Vec<Address> = Vec::new();
        self.for_each_live_object(bitmap, &mut |isolate, object| {
            let mut slots: Vec<Address> = Vec::new();
            isolate.visit_object_slots(object, &mut |address| slots.push(address));
            for slot in slots {
                if isolate.heap.read_tagged(slot).is_weak() {
                    weak_slots.push(slot);
                }
            }
        });

        for slot in weak_slots {
            let value = self.heap.read_tagged(slot);
            let Some(target) = value.to_heap_object() else {
                continue;
            };
            if self.is_marked_or_always_live(bitmap, target) {
                continue;
            }
            // The value of the weak reference is non-live.
            self.heap.write_tagged(slot, cleared);
            stats.cleared_weak_references += 1;
        }
    }

    /// `MarkingHelper::IsMarkedOrAlwaysLive`
    fn is_marked_or_always_live(&self, bitmap: &MarkingBitmap, object: HeapObject) -> bool {
        if !self.is_in_old_space(object.address()) {
            return true;
        }
        bitmap.get(object.address())
    }

    fn evacuate(&mut self, bitmap: &MarkingBitmap, stats: &mut MarkCompactStats) {
        let start = self.heap.old_space().start();
        let end = self.heap.old_space().top();

        let mut live: Vec<(Address, Address, usize)> = Vec::new();
        let mut target = start;
        let mut address = start;
        while address < end {
            let object = HeapObject::from_address(address);
            let size = self.object_size(object) as usize;
            if bitmap.get(address) {
                live.push((address, target, size));
                target += size as Address;
            } else {
                stats.swept_objects += 1;
                stats.swept_bytes += size;
            }
            address += size as Address;
        }

        let forwarding: std::collections::HashMap<Address, Address> = live
            .iter()
            .filter(|(from, to, _)| from != to)
            .map(|(from, to, _)| (*from, *to))
            .collect();

        self.update_pointers_after_evacuation(bitmap, &forwarding);

        for (from, to, size) in &live {
            if from != to {
                self.heap.copy_within(*from, *to, *size);
                stats.moved_objects += 1;
            }
        }

        self.heap.set_old_space_top(target);
    }

    /// `MarkCompactCollector::UpdatePointersAfterEvacuation`
    fn update_pointers_after_evacuation(
        &mut self,
        bitmap: &MarkingBitmap,
        forwarding: &std::collections::HashMap<Address, Address>,
    ) {
        let lookup = |address: Address| -> Option<Address> { forwarding.get(&address).copied() };

        // Roots.
        let mut root_values: Vec<Tagged> = Vec::new();
        self.iterate_roots(&mut |slot| root_values.push(*slot));
        for value in root_values.iter_mut() {
            *value = update_reference(*value, &lookup);
        }
        let mut index = 0;
        self.iterate_roots(&mut |slot| {
            *slot = root_values[index];
            index += 1;
        });

        let mut slots: Vec<Address> = Vec::new();
        self.for_each_live_object(bitmap, &mut |isolate, object| {
            isolate.visit_object_slots(object, &mut |address| slots.push(address));
        });
        self.for_each_object_outside_old_space(&mut |isolate, object| {
            isolate.visit_object_slots(object, &mut |address| slots.push(address));
        });
        for slot in slots {
            let value = self.heap.read_tagged(slot);
            let updated = update_reference(value, &lookup);
            if updated != value {
                self.heap.write_tagged(slot, updated);
            }
        }
    }

    fn is_in_old_space(&self, address: Address) -> bool {
        address >= self.heap.old_space().start() && address < self.heap.old_space().top()
    }

    fn is_outside_old_space(&self, address: Address) -> bool {
        !self.is_in_old_space(address)
    }

    /// `MarkCompactCollector::VisitLiveObjects`
    fn for_each_live_object(
        &mut self,
        bitmap: &MarkingBitmap,
        visit: &mut impl FnMut(&mut Isolate, HeapObject),
    ) {
        for address in bitmap.marked_addresses() {
            if address >= self.heap.old_space().top() {
                break;
            }
            visit(self, HeapObject::from_address(address));
        }
    }

    fn for_each_object_outside_old_space(
        &mut self,
        visit: &mut impl FnMut(&mut Isolate, HeapObject),
    ) {
        for (start, end) in [
            (
                self.heap.read_only_space().start(),
                self.heap.read_only_space().top(),
            ),
            (self.heap.new_space().start(), self.heap.new_space().top()),
        ] {
            let mut address = start;
            while address < end {
                let object = HeapObject::from_address(address);
                let size = self.object_size(object) as usize;
                visit(self, object);
                address += size as Address;
            }
        }
    }
}

/// `UpdateSlot`
#[inline]
fn update_reference(value: Tagged, lookup: &impl Fn(Address) -> Option<Address>) -> Tagged {
    if value.is_smi() || value.is_cleared() {
        return value;
    }
    let Some(object) = value.to_heap_object() else {
        return value;
    };
    match lookup(object.address()) {
        Some(to) => {
            if value.is_weak() {
                HeapObject::from_address(to).tagged_weak()
            } else {
                HeapObject::from_address(to).tagged()
            }
        }
        None => value,
    }
}

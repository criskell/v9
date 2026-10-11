//! Ported from `src/heap/scavenger.h` / `scavenger.cc`.
//!
//! The young generation collector: a Cheney-style copying collector over the
//! two semi-spaces, with V8's decomposition --
//! [`ScavengerCollector::CollectGarbage`](Isolate::collect_garbage) drives
//! the phases, `Scavenger::ScavengeObject` handles one slot,
//! `Scavenger::EvacuateObject` decides where a survivor goes, and
//! `Scavenger::ShouldBePromoted` is the age-mark test that decides it.
//!
//! ```text
//!   1. swap           to-space and from-space exchange roles
//!   2. scan roots     every root pointing into from-space is evacuated,
//!                     and the root is updated to the copy
//!   3. scan tenured   old and read-only space are swept for references
//!                     into from-space (vx has no remembered set yet)
//!   4. process        every object copied into to-space is itself scanned,
//!                     breadth-first, until the scan pointer catches the
//!                     allocation pointer -- Cheney's trick: to-space
//!                     doubles as the work queue
//!   5. reset          from-space is dropped whole, and the age mark is set
//!                     to where the survivors end
//! ```
//!
//! Evacuating an object leaves a forwarding address in its old map word, so a
//! second reference to the same object finds the copy instead of making
//! another ([`MapWord`]). An object that has already survived one scavenge --
//! it sits below the nursery's `age_mark` -- is promoted to old space rather
//! than copied again.
//!
//! Why this is safe here
//! ---------------------
//! A copying collector moves objects, so any address held outside the heap
//! while it runs is stale afterwards. `vx` never collects inside an
//! allocation; collection happens only when [`Isolate::collect_garbage`] is
//! called explicitly, at a point where the caller has nothing live but roots.
//! That is V8's safepoint discipline, just made the caller's decision instead
//! of the allocator's.
//!
//! Deviations, all tracked in `PORTING.md`:
//!
//! * **No remembered set.** V8's write barrier records old-to-young
//!   references so a scavenge touches only those. `vx` instead walks old and
//!   read-only space linearly, which is correct but O(old space) per
//!   collection.
//! * **Single threaded.** V8 scavenges in parallel, with per-worker
//!   `Scavenger` instances and a shared work list. `vx` has one.
//! * **Weak references are treated as strong.** V8's scavenger processes weak
//!   references in a separate phase and clears the ones whose target died.
//!   `vx` keeps them alive one cycle longer, preserving weakness in the
//!   updated slot. In practice nothing is affected yet: the only weak slots
//!   are transition targets, and maps live in read-only space.
//! * **No old-generation collection.** Old space grows and is never swept;
//!   that is mark-compact's job.

use crate::execution::isolate::Isolate;
use crate::heap::heap::AllocationType;
use crate::objects::map_word::MapWord;
use crate::objects::tagged::{Address, HeapObject, Tagged};

/// What one scavenge did, for `--trace-gc`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ScavengeStats {
    /// Bytes of live data copied within the nursery.
    pub bytes_evacuated: usize,
    /// Bytes of live data graduated into old space.
    pub bytes_promoted: usize,
    pub objects_evacuated: usize,
    pub objects_promoted: usize,
    /// New-space occupancy before and after.
    pub new_space_before: usize,
    pub new_space_after: usize,
}

impl ScavengeStats {
    pub fn bytes_reclaimed(&self) -> usize {
        self.new_space_before
            .saturating_sub(self.new_space_after + self.bytes_promoted)
    }
}

impl Isolate {
    /// `ScavengerCollector::CollectGarbage`.
    ///
    /// Only safe to call at a point where every live object is reachable from
    /// the root set -- the roots table, the string table and the handle
    /// scope. Anything held only in a Rust local across this call is stale
    /// afterwards.
    pub fn collect_garbage(&mut self) -> ScavengeStats {
        let mut stats = ScavengeStats {
            new_space_before: self.heap.new_space().size(),
            ..Default::default()
        };

        // 1. Swap. What was being allocated into becomes the space to
        //    evacuate; the empty one becomes the allocation target.
        self.heap.new_space_mut().swap_semi_spaces();
        self.heap.new_space_mut().to_space_mut().reset();

        // Promoted objects land above whatever old space already holds, so
        // that is where scanning them starts.
        self.promoted_scan = self.heap.old_space().top();

        // 2. Roots.
        self.iterate_and_scavenge_roots(&mut stats);

        // 3. Old and read-only space: no remembered set, so sweep them for
        //    references into the space being evacuated.
        self.scavenge_tenured_spaces(&mut stats);

        // 4. Drain the work list.
        self.process(&mut stats);

        // 5. Drop from-space whole, and record where the survivors end so the
        //    next scavenge can tell them from objects allocated after this
        //    one.
        self.heap.new_space_mut().from_space_mut().reset();
        let survivors_end = self.heap.new_space().top();
        self.heap.new_space_mut().set_age_mark(survivors_end);

        stats.new_space_after = self.heap.new_space().size();
        self.heap.gc_count += 1;
        self.heap.bytes_evacuated += stats.bytes_evacuated;
        self.heap.bytes_promoted += stats.bytes_promoted;
        stats
    }

    /// `ScavengerCollector::IterateRoots` plus `Scavenger::ScavengeObject` on
    /// each one.
    ///
    /// The root values are collected into a list first, because the visitor
    /// borrows the isolate and evacuating mutates it. V8 can scavenge in
    /// place because it is holding raw pointers; here the two-pass shape is
    /// what makes the borrow check work out, and it visits exactly the same
    /// slots in the same order.
    fn iterate_and_scavenge_roots(&mut self, stats: &mut ScavengeStats) {
        let mut root_values: Vec<Tagged> = Vec::new();
        self.iterate_roots(&mut |slot| root_values.push(*slot));
        for value in root_values.iter_mut() {
            *value = self.scavenge_object(*value, stats);
        }
        let mut index = 0;
        self.iterate_roots(&mut |slot| {
            *slot = root_values[index];
            index += 1;
        });
    }

    /// Stands in for V8's remembered set: walks old and read-only space
    /// object by object, updating any slot that points into from-space.
    fn scavenge_tenured_spaces(&mut self, stats: &mut ScavengeStats) {
        for range in [
            self.heap.old_space().start()..self.heap.old_space().top(),
            self.heap.read_only_space().start()..self.heap.read_only_space().top(),
        ] {
            let mut address = range.start;
            while address < range.end {
                let object = HeapObject::from_address(address);
                let size = self.object_size(object);
                self.visit_object(object, stats);
                address += object_pointer_align(size as u64);
            }
        }
    }

    /// `Scavenger::Process`: Cheney's scan loop.
    ///
    /// Objects copied into to-space are scanned in allocation order, and
    /// scanning one may copy more, which appends to the very region being
    /// scanned. Promotion appends to old space instead, so that has its own
    /// scan pointer and the two feed each other until both are exhausted.
    fn process(&mut self, stats: &mut ScavengeStats) {
        let mut scan = self.heap.new_space().start();
        loop {
            let top = self.heap.new_space().top();
            if scan >= top {
                let promoted_work = self.process_promoted(stats);
                if !promoted_work && scan >= self.heap.new_space().top() {
                    break;
                }
                continue;
            }
            let object = HeapObject::from_address(scan);
            let size = self.object_size(object);
            self.visit_object(object, stats);
            scan += object_pointer_align(size as u64);
        }
        debug_assert_eq!(scan, self.heap.new_space().top());
    }

    /// Scans the objects promoted into old space during this collection.
    /// Returns whether any work was done.
    fn process_promoted(&mut self, stats: &mut ScavengeStats) -> bool {
        let mut did_work = false;
        while self.promoted_scan < self.heap.old_space().top() {
            let object = HeapObject::from_address(self.promoted_scan);
            let size = self.object_size(object);
            self.visit_object(object, stats);
            self.promoted_scan += object_pointer_align(size as u64);
            did_work = true;
        }
        did_work
    }

    /// `ScavengeVisitor::VisitPointers`: update every reference inside
    /// `object`.
    fn visit_object(&mut self, object: HeapObject, stats: &mut ScavengeStats) {
        // The slots are collected first for the same borrow reason as the
        // roots: the visitor reads the isolate, evacuating writes it.
        let mut slots: Vec<Address> = Vec::new();
        self.visit_object_slots(object, &mut |address| slots.push(address));
        for slot in slots {
            let value = self.heap.read_tagged(slot);
            let updated = self.scavenge_object(value, stats);
            if updated != value {
                self.heap.write_tagged(slot, updated);
            }
        }
    }

    /// `Scavenger::ScavengeObject`: if `value` points into the space being
    /// evacuated, copy the target (or find the copy already made) and return
    /// a reference to it. Anything else is returned unchanged.
    fn scavenge_object(&mut self, value: Tagged, stats: &mut ScavengeStats) -> Tagged {
        let Some(object) = value.to_heap_object() else {
            // A Smi or a cleared weak reference: nothing to update.
            return value;
        };
        if !self.heap.in_from_space(object) {
            return value;
        }
        let copy = self.evacuate_object(object, stats);
        // Preserve weakness: a weak slot must stay weak after the update, or
        // the reference would start keeping its target alive.
        if value.is_weak_or_cleared() {
            copy.tagged_weak()
        } else {
            copy.tagged()
        }
    }

    /// `Scavenger::EvacuateObject`.
    ///
    /// Idempotent: the forwarding address left in the old map word means the
    /// second caller gets the first caller's copy rather than making another.
    fn evacuate_object(&mut self, object: HeapObject, stats: &mut ScavengeStats) -> HeapObject {
        let map_word = self.heap.map_word(object);
        if map_word.is_forwarding_address() {
            return map_word.to_forwarding_address();
        }

        let size = self.object_size(object) as usize;
        if self.should_be_promoted(object.address()) {
            return self.promote_object(object, size, stats);
        }
        match self.semi_space_copy_object(object, size, stats) {
            Some(target) => target,
            // The survivors do not fit in a semi-space. V8 handles this by
            // promoting the overflow rather than failing the collection.
            // Unreachable while both halves are the same size, since a copy
            // is the same size as its original.
            None => self.promote_object(object, size, stats),
        }
    }

    /// `Scavenger::ShouldBePromoted`: has this object already survived a
    /// collection?
    ///
    /// Survivors are copied to the bottom of to-space and fresh allocation
    /// continues above them, so the age mark recorded at the end of the last
    /// scavenge separates the two generations inside the nursery with a
    /// single address comparison.
    #[inline]
    fn should_be_promoted(&self, object_address: Address) -> bool {
        object_address < self.heap.new_space().age_mark()
    }

    /// `Scavenger::SemiSpaceCopyObject`: copy within the nursery.
    fn semi_space_copy_object(
        &mut self,
        object: HeapObject,
        size: usize,
        stats: &mut ScavengeStats,
    ) -> Option<HeapObject> {
        let target = self.heap.allocate_raw(size, AllocationType::Young)?;
        self.try_migrate_object(object, target, size);
        stats.bytes_evacuated += size;
        stats.objects_evacuated += 1;
        Some(target)
    }

    /// `Scavenger::PromoteObject`: graduate out of the nursery.
    fn promote_object(
        &mut self,
        object: HeapObject,
        size: usize,
        stats: &mut ScavengeStats,
    ) -> HeapObject {
        let target = self
            .heap
            .allocate_raw(size, AllocationType::Old)
            .expect("old space must accept a promotion");
        self.try_migrate_object(object, target, size);
        stats.bytes_promoted += size;
        stats.objects_promoted += 1;
        target
    }

    /// `Scavenger::TryMigrateObject`: copy the bytes and leave the trail.
    ///
    /// After this the original is a tombstone whose map word is a forwarding
    /// address, which is what makes a shared reference resolve to one copy
    /// and a cycle terminate.
    fn try_migrate_object(&mut self, object: HeapObject, target: HeapObject, size: usize) {
        self.heap
            .copy_within(object.address(), target.address(), size);
        self.heap
            .set_map_word(object, MapWord::from_forwarding_address(target));
    }
}

use crate::common::globals::object_pointer_align;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::{Heap, HeapConfig};
    use crate::objects::heap_number::number_value;
    use crate::objects::js_objects::JSObject;
    use crate::objects::smi::Smi;
    use crate::objects::tagged::Tagged;

    /// A nursery small enough that a handful of objects fills it.
    fn small_nursery_isolate() -> Isolate {
        Isolate::with_heap(Heap::with_config(HeapConfig {
            read_only_space_size: 256 * crate::common::globals::K_KB,
            semi_space_size: 8 * crate::common::globals::K_KB,
            initial_old_space_size: 256 * crate::common::globals::K_KB,
            max_old_space_size: 16 * crate::common::globals::K_MB,
        }))
    }

    /// Roots an object across a collection by parking it in the handle scope,
    /// and returns where it ended up.
    fn collect_keeping(isolate: &mut Isolate, value: Tagged) -> (Tagged, ScavengeStats) {
        isolate.handles.push(value);
        let stats = isolate.collect_garbage();
        let moved = *isolate.handles.last().unwrap();
        isolate.handles.pop();
        (moved, stats)
    }

    #[test]
    fn garbage_is_reclaimed() {
        let mut isolate = small_nursery_isolate();
        for i in 0..50 {
            // Nothing holds on to these.
            isolate.new_heap_number(i as f64);
        }
        let before = isolate.heap.new_space().size();
        assert!(before > 0);

        let stats = isolate.collect_garbage();
        assert_eq!(isolate.heap.new_space().size(), 0, "all of it was garbage");
        assert_eq!(stats.objects_evacuated, 0);
        assert_eq!(stats.new_space_before, before);
    }

    #[test]
    fn a_rooted_object_survives_and_its_contents_are_intact() {
        let mut isolate = small_nursery_isolate();
        let number = isolate.new_heap_number(1.5).tagged();
        // Surround it with garbage so it has to move.
        for i in 0..20 {
            isolate.new_heap_number(i as f64);
        }

        let (moved, stats) = collect_keeping(&mut isolate, number);
        assert_ne!(moved, number, "a copying collector moves survivors");
        assert_eq!(number_value(&isolate.heap, moved), Some(1.5));
        assert_eq!(stats.objects_evacuated, 1);
        assert!(isolate.heap.new_space().size() > 0);
    }

    #[test]
    fn an_unrooted_object_does_not_survive() {
        let mut isolate = small_nursery_isolate();
        isolate.new_heap_number(1.5);
        let stats = isolate.collect_garbage();
        assert_eq!(stats.objects_evacuated + stats.objects_promoted, 0);
    }

    #[test]
    fn the_whole_object_graph_is_traced() {
        let mut isolate = small_nursery_isolate();
        let object = isolate.new_js_object();
        let inner = isolate.new_js_object();
        let value = isolate.new_heap_number(2.5).tagged();
        isolate.set_property_by_str(inner, "deep", value);
        isolate.set_property_by_str(object, "inner", inner.tagged());

        for i in 0..20 {
            isolate.new_heap_number(i as f64);
        }

        let (moved, _) = collect_keeping(&mut isolate, object.tagged());
        let moved_object = JSObject::unchecked_cast(moved.to_strong_heap_object().unwrap());

        let inner_after = isolate.get_property_by_str(moved_object, "inner");
        let inner_object = JSObject::unchecked_cast(inner_after.to_strong_heap_object().unwrap());
        let deep = isolate.get_property_by_str(inner_object, "deep");
        assert_eq!(
            number_value(&isolate.heap, deep),
            Some(2.5),
            "a reference two levels down must be followed and updated"
        );
    }

    #[test]
    fn shared_references_are_not_duplicated() {
        let mut isolate = small_nursery_isolate();
        let shared = isolate.new_heap_number(3.5).tagged();
        let a = isolate.new_js_object();
        let b = isolate.new_js_object();
        isolate.set_property_by_str(a, "v", shared);
        isolate.set_property_by_str(b, "v", shared);

        let holder = isolate.new_fixed_array(2);
        holder.set(&mut isolate.heap, 0, a.tagged());
        holder.set(&mut isolate.heap, 1, b.tagged());

        let (moved, _) = collect_keeping(&mut isolate, holder.tagged());
        let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
            moved.to_strong_heap_object().unwrap(),
        );
        let a_after =
            JSObject::unchecked_cast(array.get(&isolate.heap, 0).to_strong_heap_object().unwrap());
        let b_after =
            JSObject::unchecked_cast(array.get(&isolate.heap, 1).to_strong_heap_object().unwrap());

        let from_a = isolate.get_property_by_str(a_after, "v");
        let from_b = isolate.get_property_by_str(b_after, "v");
        assert_eq!(
            from_a, from_b,
            "the forwarding address must make the second visit find the first copy"
        );
        assert_eq!(number_value(&isolate.heap, from_a), Some(3.5));
    }

    #[test]
    fn a_cycle_does_not_loop_forever() {
        let mut isolate = small_nursery_isolate();
        let a = isolate.new_js_object();
        let b = isolate.new_js_object();
        isolate.set_property_by_str(a, "peer", b.tagged());
        isolate.set_property_by_str(b, "peer", a.tagged());

        let (moved, _) = collect_keeping(&mut isolate, a.tagged());
        let a_after = JSObject::unchecked_cast(moved.to_strong_heap_object().unwrap());
        let b_after = JSObject::unchecked_cast(
            isolate
                .get_property_by_str(a_after, "peer")
                .to_strong_heap_object()
                .unwrap(),
        );
        let back = isolate.get_property_by_str(b_after, "peer");
        assert_eq!(
            back,
            a_after.tagged(),
            "the cycle must close back onto the copy"
        );
    }

    #[test]
    fn surviving_twice_promotes_an_object_out_of_the_nursery() {
        let mut isolate = small_nursery_isolate();
        let number = isolate.new_heap_number(4.5).tagged();

        // First collection: copied within the nursery, below the new age mark.
        let (after_first, first) = collect_keeping(&mut isolate, number);
        assert_eq!(first.objects_evacuated, 1);
        assert_eq!(first.objects_promoted, 0);
        assert!(isolate
            .heap
            .in_young_generation(after_first.to_strong_heap_object().unwrap()));

        // Second collection: it is below the age mark now, so it graduates.
        let (after_second, second) = collect_keeping(&mut isolate, after_first);
        assert_eq!(second.objects_promoted, 1, "it has survived twice");
        assert_eq!(second.objects_evacuated, 0);
        let promoted = after_second.to_strong_heap_object().unwrap();
        assert!(!isolate.heap.in_young_generation(promoted));
        assert_eq!(number_value(&isolate.heap, after_second), Some(4.5));

        // And it stops being copied from then on.
        let (after_third, third) = collect_keeping(&mut isolate, after_second);
        assert_eq!(after_third, after_second, "a tenured object stays put");
        assert_eq!(third.objects_promoted + third.objects_evacuated, 0);
    }

    #[test]
    fn an_old_object_referring_to_a_young_one_keeps_it_alive() {
        let mut isolate = small_nursery_isolate();
        // Promote a container.
        let container = isolate.new_fixed_array(1);
        let (container, _) = collect_keeping(&mut isolate, container.tagged());
        let (container, _) = collect_keeping(&mut isolate, container);
        let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
            container.to_strong_heap_object().unwrap(),
        );
        assert!(!isolate.heap.in_young_generation(array.object()));

        // Point it at a fresh young object, with no handle on that object.
        let young = isolate.new_heap_number(5.5).tagged();
        array.set(&mut isolate.heap, 0, young);

        let (container, stats) = collect_keeping(&mut isolate, container);
        assert!(
            stats.objects_evacuated + stats.objects_promoted >= 1,
            "the old-to-young reference must have been found"
        );
        let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
            container.to_strong_heap_object().unwrap(),
        );
        assert_eq!(
            number_value(&isolate.heap, array.get(&isolate.heap, 0)),
            Some(5.5),
            "and updated to the copy"
        );
    }

    #[test]
    fn the_root_table_and_string_table_survive_a_collection() {
        let mut isolate = small_nursery_isolate();
        let name = isolate.internalize_string("still here");
        isolate.collect_garbage();

        assert_eq!(isolate.internalize_string("still here"), name);
        assert_eq!(name.to_rust_string(&isolate.heap), "still here");
        // The roots still describe a working world.
        assert_eq!(
            isolate.heap.map(isolate.roots.meta_map().object()),
            isolate.roots.meta_map().object()
        );
        let object = isolate.new_js_object();
        assert!(isolate.set_property_by_str(object, "x", Smi::from_int(1)));
        assert_eq!(Smi::to_int(isolate.get_property_by_str(object, "x")), 1);
    }

    #[test]
    fn repeated_collections_keep_a_live_graph_correct() {
        let mut isolate = small_nursery_isolate();
        let root = isolate.new_fixed_array(8);
        isolate.handles.push(root.tagged());

        for round in 0..8 {
            // Fill the slots with fresh objects, drop the previous ones.
            let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
                isolate.handles[0].to_strong_heap_object().unwrap(),
            );
            for i in 0..8 {
                let value = isolate.new_heap_number((round * 8 + i) as f64).tagged();
                array.set(&mut isolate.heap, i, value);
            }
            // Plus a pile of garbage.
            for i in 0..30 {
                isolate.new_heap_number(i as f64);
            }
            isolate.collect_garbage();

            let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
                isolate.handles[0].to_strong_heap_object().unwrap(),
            );
            for i in 0..8 {
                assert_eq!(
                    number_value(&isolate.heap, array.get(&isolate.heap, i)),
                    Some((round * 8 + i) as f64),
                    "round {round}, slot {i}"
                );
            }
        }
        assert!(isolate.heap.gc_count >= 8);
    }

    #[test]
    fn survivors_always_fit_in_to_space() {
        // The invariant that makes a copying nursery work: to-space is the
        // same size as from-space, and a copy is the same size as its
        // original, so the survivors of a scavenge cannot overflow. The
        // promote-on-overflow branch in `evacuate` is therefore unreachable
        // today -- it exists for when to-space can be shrunk, which is what
        // V8 does when it resizes new space.
        let mut isolate = small_nursery_isolate();
        let capacity = isolate.heap.new_space().capacity();

        // Fill the nursery with nothing but live objects.
        let holder = isolate.new_fixed_array(64);
        isolate.handles.push(holder.tagged());
        let mut filled = 0;
        while isolate.heap.new_space().available() > 64 && filled < 64 {
            let value = isolate.new_heap_number(filled as f64).tagged();
            let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
                isolate.handles[0].to_strong_heap_object().unwrap(),
            );
            array.set(&mut isolate.heap, filled, value);
            filled += 1;
        }
        assert!(filled > 0);

        let stats = isolate.collect_garbage();
        assert_eq!(
            stats.objects_promoted, 0,
            "nothing had survived before, so nothing should graduate"
        );
        assert!(stats.objects_evacuated >= filled as usize);
        assert!(
            isolate.heap.new_space().size() <= capacity,
            "the survivors fit, by construction"
        );

        let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
            isolate.handles[0].to_strong_heap_object().unwrap(),
        );
        for i in 0..filled {
            assert_eq!(
                number_value(&isolate.heap, array.get(&isolate.heap, i)),
                Some(i as f64),
                "slot {i} lost across the scavenge"
            );
        }
    }

    #[test]
    fn hidden_classes_still_work_after_a_collection() {
        let mut isolate = small_nursery_isolate();
        let a = isolate.new_js_object();
        isolate.set_property_by_str(a, "x", Smi::from_int(1));
        isolate.set_property_by_str(a, "y", Smi::from_int(2));
        let map_before = a.map(&isolate.heap);

        let (moved, _) = collect_keeping(&mut isolate, a.tagged());
        let a_after = JSObject::unchecked_cast(moved.to_strong_heap_object().unwrap());
        assert_eq!(
            a_after.map(&isolate.heap),
            map_before,
            "maps are immovable, so the shape is unchanged"
        );

        // A new object still finds the same shape through the transitions.
        let b = isolate.new_js_object();
        isolate.set_property_by_str(b, "x", Smi::from_int(3));
        isolate.set_property_by_str(b, "y", Smi::from_int(4));
        assert_eq!(b.map(&isolate.heap), map_before);
        assert_eq!(Smi::to_int(isolate.get_property_by_str(a_after, "y")), 2);
    }
}

use crate::execution::isolate::Isolate;
use crate::heap::heap::AllocationType;
use crate::objects::map_word::MapWord;
use crate::objects::tagged::{Address, HeapObject, Tagged};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ScavengeStats {
    pub bytes_evacuated: usize,
    pub bytes_promoted: usize,
    pub objects_evacuated: usize,
    pub objects_promoted: usize,
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
    pub fn collect_garbage(&mut self) -> ScavengeStats {
        let mut stats = ScavengeStats {
            new_space_before: self.heap.new_space().size(),
            ..Default::default()
        };

        self.heap.new_space_mut().swap_semi_spaces();
        self.heap.new_space_mut().to_space_mut().reset();

        self.promoted_scan = self.heap.old_space().top();

        // 2. Roots.
        self.iterate_and_scavenge_roots(&mut stats);

        self.scavenge_tenured_spaces(&mut stats);

        self.process(&mut stats);

        self.heap.new_space_mut().from_space_mut().reset();
        let survivors_end = self.heap.new_space().top();
        self.heap.new_space_mut().set_age_mark(survivors_end);

        stats.new_space_after = self.heap.new_space().size();
        self.heap.gc_count += 1;
        self.heap.bytes_evacuated += stats.bytes_evacuated;
        self.heap.bytes_promoted += stats.bytes_promoted;
        stats
    }

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

    fn visit_object(&mut self, object: HeapObject, stats: &mut ScavengeStats) {
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

    fn scavenge_object(&mut self, value: Tagged, stats: &mut ScavengeStats) -> Tagged {
        let Some(object) = value.to_heap_object() else {
            return value;
        };
        if !self.heap.in_from_space(object) {
            return value;
        }
        let copy = self.evacuate_object(object, stats);
        if value.is_weak_or_cleared() {
            copy.tagged_weak()
        } else {
            copy.tagged()
        }
    }

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
            None => self.promote_object(object, size, stats),
        }
    }

    #[inline]
    fn should_be_promoted(&self, object_address: Address) -> bool {
        object_address < self.heap.new_space().age_mark()
    }

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

    fn small_nursery_isolate() -> Isolate {
        let mut isolate = Isolate::with_heap(Heap::with_config(HeapConfig {
            read_only_space_size: 256 * crate::common::globals::K_KB,
            semi_space_size: 8 * crate::common::globals::K_KB,
            initial_old_space_size: 256 * crate::common::globals::K_KB,
            max_old_space_size: 16 * crate::common::globals::K_MB,
        }));
        isolate.set_allocation_gc_interval(0);
        isolate
    }

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

        let (after_first, first) = collect_keeping(&mut isolate, number);
        assert_eq!(first.objects_evacuated, 1);
        assert_eq!(first.objects_promoted, 0);
        assert!(isolate
            .heap
            .in_young_generation(after_first.to_strong_heap_object().unwrap()));

        let (after_second, second) = collect_keeping(&mut isolate, after_first);
        assert_eq!(second.objects_promoted, 1, "it has survived twice");
        assert_eq!(second.objects_evacuated, 0);
        let promoted = after_second.to_strong_heap_object().unwrap();
        assert!(!isolate.heap.in_young_generation(promoted));
        assert_eq!(number_value(&isolate.heap, after_second), Some(4.5));

        let (after_third, third) = collect_keeping(&mut isolate, after_second);
        assert_eq!(after_third, after_second, "a tenured object stays put");
        assert_eq!(third.objects_promoted + third.objects_evacuated, 0);
    }

    #[test]
    fn an_old_object_referring_to_a_young_one_keeps_it_alive() {
        let mut isolate = small_nursery_isolate();
        let container = isolate.new_fixed_array(1);
        let (container, _) = collect_keeping(&mut isolate, container.tagged());
        let (container, _) = collect_keeping(&mut isolate, container);
        let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
            container.to_strong_heap_object().unwrap(),
        );
        assert!(!isolate.heap.in_young_generation(array.object()));

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
            let array = crate::objects::fixed_array::FixedArray::unchecked_cast(
                isolate.handles[0].to_strong_heap_object().unwrap(),
            );
            for i in 0..8 {
                let value = isolate.new_heap_number((round * 8 + i) as f64).tagged();
                array.set(&mut isolate.heap, i, value);
            }
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
        let mut isolate = small_nursery_isolate();
        let capacity = isolate.heap.new_space().capacity();

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

        let b = isolate.new_js_object();
        isolate.set_property_by_str(b, "x", Smi::from_int(3));
        isolate.set_property_by_str(b, "y", Smi::from_int(4));
        assert_eq!(b.map(&isolate.heap), map_before);
        assert_eq!(Smi::to_int(isolate.get_property_by_str(a_after, "y")), 2);
    }
}

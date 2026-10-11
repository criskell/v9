use crate::heap::heap::{AllocationType, Heap};
use crate::objects::fixed_array::WeakFixedArray;
use crate::objects::instance_type;
use crate::objects::map::Map;
use crate::objects::smi::Smi;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `TransitionArray`
    TransitionArray
);

/// `TransitionArray::kNotFound`
pub const K_NOT_FOUND: i32 = -1;

impl TransitionArray {
    pub const K_PROTOTYPE_TRANSITIONS_INDEX: i32 = 0;
    pub const K_SIDE_STEP_TRANSITIONS_INDEX: i32 = 1;
    pub const K_TRANSITION_LENGTH_INDEX: i32 = 2;
    /// `TransitionArray::kFirstIndex`
    pub const K_FIRST_INDEX: i32 = 3;

    pub const K_ENTRY_KEY_INDEX: i32 = 0;
    pub const K_ENTRY_TARGET_INDEX: i32 = 1;
    /// `TransitionArray::kTransitionSize`
    pub const K_TRANSITION_SIZE: i32 = 2;

    #[inline]
    pub const fn length_for(number_of_transitions: i32) -> i32 {
        Self::K_FIRST_INDEX + number_of_transitions * Self::K_TRANSITION_SIZE
    }

    #[inline]
    pub const fn size_for(number_of_transitions: i32) -> i32 {
        WeakFixedArray::size_for(Self::length_for(number_of_transitions))
    }

    #[inline]
    fn as_array(self) -> WeakFixedArray {
        WeakFixedArray::unchecked_cast(self.object())
    }

    #[inline]
    pub fn capacity(self, heap: &Heap) -> i32 {
        (self.as_array().length(heap) - Self::K_FIRST_INDEX) / Self::K_TRANSITION_SIZE
    }

    /// `TransitionArray::number_of_transitions`
    #[inline]
    pub fn number_of_transitions(self, heap: &Heap) -> i32 {
        Smi::to_int(self.as_array().get(heap, Self::K_TRANSITION_LENGTH_INDEX))
    }

    #[inline]
    pub fn set_number_of_transitions(self, heap: &mut Heap, number: i32) {
        self.as_array()
            .set(heap, Self::K_TRANSITION_LENGTH_INDEX, Smi::from_int(number));
    }

    #[inline]
    const fn key_index(transition_number: i32) -> i32 {
        Self::K_FIRST_INDEX + transition_number * Self::K_TRANSITION_SIZE + Self::K_ENTRY_KEY_INDEX
    }

    #[inline]
    const fn target_index(transition_number: i32) -> i32 {
        Self::K_FIRST_INDEX
            + transition_number * Self::K_TRANSITION_SIZE
            + Self::K_ENTRY_TARGET_INDEX
    }

    /// `TransitionArray::GetKey`
    #[inline]
    pub fn get_key(self, heap: &Heap, transition_number: i32) -> Tagged {
        debug_assert!(transition_number < self.number_of_transitions(heap));
        self.as_array()
            .get(heap, Self::key_index(transition_number))
    }

    #[inline]
    pub fn set_key(self, heap: &mut Heap, transition_number: i32, key: Tagged) {
        self.as_array()
            .set(heap, Self::key_index(transition_number), key);
    }

    #[inline]
    pub fn get_raw_target(self, heap: &Heap, transition_number: i32) -> Tagged {
        debug_assert!(transition_number < self.number_of_transitions(heap));
        self.as_array()
            .get(heap, Self::target_index(transition_number))
    }

    /// `TransitionArray::GetTargetIfExists`
    #[inline]
    pub fn get_target(self, heap: &Heap, transition_number: i32) -> Option<Map> {
        let raw = self.get_raw_target(heap, transition_number);
        Some(Map::unchecked_cast(raw.to_heap_object()?))
    }

    #[inline]
    pub fn set_raw_target(self, heap: &mut Heap, transition_number: i32, target: Tagged) {
        self.as_array()
            .set(heap, Self::target_index(transition_number), target);
    }

    #[inline]
    pub fn key_address(self, transition_number: i32) -> Address {
        self.as_array()
            .element_address(Self::key_index(transition_number))
    }

    #[inline]
    pub fn target_address(self, transition_number: i32) -> Address {
        self.as_array()
            .element_address(Self::target_index(transition_number))
    }

    pub fn search_name(self, heap: &Heap, name: Tagged) -> i32 {
        for i in 0..self.number_of_transitions(heap) {
            if self.get_key(heap, i) == name {
                return i;
            }
        }
        K_NOT_FOUND
    }
}

/// `TransitionsAccessor::Encoding`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Encoding {
    Uninitialized,
    WeakRef,
    /// A `TransitionArray`.
    FullTransitionArray,
}

pub struct TransitionsAccessor;

impl TransitionsAccessor {
    pub fn encoding(heap: &Heap, map: Map) -> Encoding {
        let raw = map.raw_transitions(heap);
        if raw.is_smi() {
            debug_assert_eq!(Smi::to_int(raw), 0);
            return Encoding::Uninitialized;
        }
        if raw.is_weak_or_cleared() {
            return Encoding::WeakRef;
        }
        let object = raw
            .to_strong_heap_object()
            .expect("transitions field holds a strong reference here");
        debug_assert_eq!(
            heap.instance_type(object),
            instance_type::TRANSITION_ARRAY_TYPE
        );
        Encoding::FullTransitionArray
    }

    /// `TransitionsAccessor::GetSimpleTransition`
    pub(crate) fn simple_transition(heap: &Heap, map: Map) -> Option<Map> {
        let raw = map.raw_transitions(heap);
        Some(Map::unchecked_cast(raw.to_heap_object()?))
    }

    fn simple_transition_key(heap: &Heap, target: Map) -> Tagged {
        target.last_added_key(heap)
    }

    pub fn search_transition(heap: &Heap, map: Map, name: Tagged) -> Option<Map> {
        match Self::encoding(heap, map) {
            Encoding::Uninitialized => None,
            Encoding::WeakRef => {
                let target = Self::simple_transition(heap, map)?;
                if Self::simple_transition_key(heap, target) == name {
                    Some(target)
                } else {
                    None
                }
            }
            Encoding::FullTransitionArray => {
                let array = Self::transition_array(heap, map);
                let index = array.search_name(heap, name);
                if index == K_NOT_FOUND {
                    None
                } else {
                    array.get_target(heap, index)
                }
            }
        }
    }

    fn transition_array(heap: &Heap, map: Map) -> TransitionArray {
        TransitionArray::unchecked_cast(
            map.raw_transitions(heap)
                .to_strong_heap_object()
                .expect("not a full transition array"),
        )
    }

    /// `TransitionsAccessor::NumberOfTransitions`
    pub fn number_of_transitions(heap: &Heap, map: Map) -> i32 {
        match Self::encoding(heap, map) {
            Encoding::Uninitialized => 0,
            Encoding::WeakRef => {
                if Self::simple_transition(heap, map).is_some() {
                    1
                } else {
                    0
                }
            }
            Encoding::FullTransitionArray => {
                Self::transition_array(heap, map).number_of_transitions(heap)
            }
        }
    }

    pub fn transitions(heap: &Heap, map: Map) -> Vec<(Tagged, Map)> {
        match Self::encoding(heap, map) {
            Encoding::Uninitialized => Vec::new(),
            Encoding::WeakRef => match Self::simple_transition(heap, map) {
                Some(target) => vec![(Self::simple_transition_key(heap, target), target)],
                None => Vec::new(),
            },
            Encoding::FullTransitionArray => {
                let array = Self::transition_array(heap, map);
                (0..array.number_of_transitions(heap))
                    .filter_map(|i| Some((array.get_key(heap, i), array.get_target(heap, i)?)))
                    .collect()
            }
        }
    }

    #[must_use]
    pub fn insert(
        heap: &mut Heap,
        map: Map,
        name: Tagged,
        target: Map,
        transition_array_map: Map,
    ) -> bool {
        debug_assert!(Self::search_transition(heap, map, name).is_none());

        match Self::encoding(heap, map) {
            Encoding::Uninitialized => {
                debug_assert_eq!(Self::simple_transition_key(heap, target), name);
                map.set_raw_transitions(heap, target.object().tagged_weak());
                true
            }
            Encoding::WeakRef => {
                let existing = Self::simple_transition(heap, map);
                let Some(array) = Self::allocate_transition_array(heap, 4, transition_array_map)
                else {
                    return false;
                };
                let mut count = 0;
                if let Some(existing) = existing {
                    let existing_key = Self::simple_transition_key(heap, existing);
                    array.set_key(heap, 0, existing_key);
                    array.set_raw_target(heap, 0, existing.object().tagged_weak());
                    count = 1;
                }
                array.set_key(heap, count, name);
                array.set_raw_target(heap, count, target.object().tagged_weak());
                array.set_number_of_transitions(heap, count + 1);
                map.set_raw_transitions(heap, array.tagged());
                true
            }
            Encoding::FullTransitionArray => {
                let array = Self::transition_array(heap, map);
                let number = array.number_of_transitions(heap);
                if number < array.capacity(heap) {
                    array.set_key(heap, number, name);
                    array.set_raw_target(heap, number, target.object().tagged_weak());
                    array.set_number_of_transitions(heap, number + 1);
                    return true;
                }
                let Some(grown) =
                    Self::allocate_transition_array(heap, (number + 1) * 2, transition_array_map)
                else {
                    return false;
                };
                for i in 0..number {
                    let key = array.get_key(heap, i);
                    let raw_target = array.get_raw_target(heap, i);
                    grown.set_key(heap, i, key);
                    grown.set_raw_target(heap, i, raw_target);
                }
                grown.set_key(heap, number, name);
                grown.set_raw_target(heap, number, target.object().tagged_weak());
                grown.set_number_of_transitions(heap, number + 1);
                map.set_raw_transitions(heap, grown.tagged());
                true
            }
        }
    }

    fn allocate_transition_array(
        heap: &mut Heap,
        capacity: i32,
        transition_array_map: Map,
    ) -> Option<TransitionArray> {
        let length = TransitionArray::length_for(capacity);
        let object = heap.allocate_raw(
            TransitionArray::size_for(capacity) as usize,
            AllocationType::Old,
        )?;
        heap.set_map(object, transition_array_map.object());
        let array = WeakFixedArray::unchecked_cast(object);
        array.set_length(heap, length);
        let transitions = TransitionArray::unchecked_cast(object);
        transitions.set_number_of_transitions(heap, 0);
        Some(transitions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::descriptor_array::DescriptorArray;
    use crate::objects::instance_type::{JS_OBJECT_TYPE, MAP_TYPE};
    use crate::objects::property_details::{default_data_field, Representation};

    struct World {
        heap: Heap,
        meta_map: Map,
        transition_array_map: Map,
        next_name: i32,
    }

    impl World {
        fn new() -> World {
            let mut heap = Heap::new();
            let object = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
            let meta_map = Map::unchecked_cast(object);
            heap.set_map(object, object);
            meta_map.set_instance_type(&mut heap, MAP_TYPE);
            meta_map.set_instance_size(&mut heap, Map::K_SIZE);
            let object = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
            let transition_array_map = Map::unchecked_cast(object);
            heap.set_map(object, meta_map.object());
            transition_array_map.set_instance_type(&mut heap, instance_type::TRANSITION_ARRAY_TYPE);
            transition_array_map.set_instance_size(&mut heap, 0);
            World {
                heap,
                meta_map,
                transition_array_map,
                next_name: 0,
            }
        }

        fn insert(&mut self, source: Map, name: Tagged, target: Map) -> bool {
            TransitionsAccessor::insert(
                &mut self.heap,
                source,
                name,
                target,
                self.transition_array_map,
            )
        }

        fn name(&mut self) -> Tagged {
            self.next_name += 1;
            let id = self.next_name;
            let object = self.heap.allocate_raw_or_fail(16, AllocationType::ReadOnly);
            self.heap.set_map(object, self.meta_map.object());
            self.heap
                .write_tagged(object.field_address(8), Smi::from_int(id));
            object.tagged()
        }

        fn descriptor_array(&mut self, capacity: i32) -> DescriptorArray {
            let object = self.heap.allocate_raw_or_fail(
                DescriptorArray::size_for(capacity) as usize,
                AllocationType::Old,
            );
            let array = DescriptorArray::unchecked_cast(object);
            array.set_number_of_all_descriptors(&mut self.heap, capacity);
            array.set_number_of_descriptors(&mut self.heap, 0);
            array
        }

        fn map_adding(&mut self, name: Tagged) -> Map {
            let descriptors = self.descriptor_array(1);
            descriptors.append(
                &mut self.heap,
                name,
                default_data_field(0, Representation::Tagged),
                Smi::zero(),
            );
            let object = self
                .heap
                .allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
            let map = Map::unchecked_cast(object);
            self.heap.set_map(object, self.meta_map.object());
            map.set_instance_type(&mut self.heap, JS_OBJECT_TYPE);
            map.set_instance_size(&mut self.heap, 32);
            map.set_instance_descriptors(&mut self.heap, descriptors.tagged());
            map.set_number_of_own_descriptors(&mut self.heap, 1);
            map
        }

        fn empty_map(&mut self) -> Map {
            let descriptors = self.descriptor_array(0);
            let object = self
                .heap
                .allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
            let map = Map::unchecked_cast(object);
            self.heap.set_map(object, self.meta_map.object());
            map.set_instance_type(&mut self.heap, JS_OBJECT_TYPE);
            map.set_instance_size(&mut self.heap, 32);
            map.set_instance_descriptors(&mut self.heap, descriptors.tagged());
            map.set_number_of_own_descriptors(&mut self.heap, 0);
            map
        }
    }

    #[test]
    fn layout_matches_v8_indices() {
        assert_eq!(TransitionArray::K_FIRST_INDEX, 3);
        assert_eq!(TransitionArray::K_TRANSITION_SIZE, 2);
        assert_eq!(TransitionArray::length_for(0), 3);
        assert_eq!(TransitionArray::length_for(1), 5);
        assert_eq!(TransitionArray::length_for(4), 11);
    }

    #[test]
    fn a_fresh_map_has_no_transitions() {
        let mut w = World::new();
        let map = w.empty_map();
        assert_eq!(
            TransitionsAccessor::encoding(&w.heap, map),
            Encoding::Uninitialized
        );
        assert_eq!(TransitionsAccessor::number_of_transitions(&w.heap, map), 0);
        let x = w.name();
        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, map, x),
            None
        );
    }

    #[test]
    fn the_first_transition_needs_no_array() {
        let mut w = World::new();
        let source = w.empty_map();
        let x = w.name();
        let target = w.map_adding(x);

        assert!(w.insert(source, x, target));

        assert_eq!(
            TransitionsAccessor::encoding(&w.heap, source),
            Encoding::WeakRef,
            "one transition should be a bare weak pointer"
        );
        assert_eq!(
            TransitionsAccessor::number_of_transitions(&w.heap, source),
            1
        );
        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, source, x),
            Some(target)
        );
        assert!(source.raw_transitions(&w.heap).is_weak());
    }

    #[test]
    fn a_second_transition_promotes_to_an_array_and_keeps_the_first() {
        let mut w = World::new();
        let source = w.empty_map();
        let x = w.name();
        let y = w.name();
        let to_x = w.map_adding(x);
        let to_y = w.map_adding(y);

        assert!(w.insert(source, x, to_x));
        assert!(w.insert(source, y, to_y));

        assert_eq!(
            TransitionsAccessor::encoding(&w.heap, source),
            Encoding::FullTransitionArray
        );
        assert_eq!(
            TransitionsAccessor::number_of_transitions(&w.heap, source),
            2
        );
        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, source, x),
            Some(to_x),
            "the original transition must survive promotion"
        );
        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, source, y),
            Some(to_y)
        );
    }

    #[test]
    fn searching_for_an_unrecorded_name_misses() {
        let mut w = World::new();
        let source = w.empty_map();
        let x = w.name();
        let to_x = w.map_adding(x);
        assert!(w.insert(source, x, to_x));

        let z = w.name();
        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, source, z),
            None
        );

        let y = w.name();
        let to_y = w.map_adding(y);
        assert!(w.insert(source, y, to_y));
        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, source, z),
            None
        );
    }

    #[test]
    fn the_array_grows_past_its_initial_capacity() {
        let mut w = World::new();
        let source = w.empty_map();
        let mut expected = Vec::new();
        for _ in 0..10 {
            let name = w.name();
            let target = w.map_adding(name);
            assert!(w.insert(source, name, target));
            expected.push((name, target));
        }
        assert_eq!(
            TransitionsAccessor::number_of_transitions(&w.heap, source),
            10
        );
        for (name, target) in expected {
            assert_eq!(
                TransitionsAccessor::search_transition(&w.heap, source, name),
                Some(target),
                "transition lost while growing the array"
            );
        }
    }

    #[test]
    fn transitions_are_weak_so_a_shape_graph_does_not_pin_its_maps() {
        let mut w = World::new();
        let source = w.empty_map();
        let x = w.name();
        let y = w.name();
        let to_x = w.map_adding(x);
        let to_y = w.map_adding(y);
        assert!(w.insert(source, x, to_x));
        assert!(w.insert(source, y, to_y));

        let array = TransitionsAccessor::transition_array(&w.heap, source);
        for i in 0..array.number_of_transitions(&w.heap) {
            assert!(
                array.get_raw_target(&w.heap, i).is_weak(),
                "target {i} must be a weak reference"
            );
            assert!(
                array.get_key(&w.heap, i).is_strong(),
                "the key must be strong -- it is a live property name"
            );
        }
    }

    #[test]
    fn a_cleared_target_reads_back_as_absent() {
        let mut w = World::new();
        let source = w.empty_map();
        let x = w.name();
        let y = w.name();
        let to_x = w.map_adding(x);
        let to_y = w.map_adding(y);
        assert!(w.insert(source, x, to_x));
        assert!(w.insert(source, y, to_y));

        let array = TransitionsAccessor::transition_array(&w.heap, source);
        let index = array.search_name(&w.heap, x);
        array.set_raw_target(&mut w.heap, index, Tagged::cleared());

        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, source, x),
            None,
            "a cleared transition must not be followed"
        );
        assert_eq!(
            TransitionsAccessor::search_transition(&w.heap, source, y),
            Some(to_y),
            "the other transitions are unaffected"
        );
        assert_eq!(TransitionsAccessor::transitions(&w.heap, source).len(), 1);
    }

    #[test]
    fn listing_transitions_reports_keys_and_targets() {
        let mut w = World::new();
        let source = w.empty_map();
        let x = w.name();
        let to_x = w.map_adding(x);

        assert!(w.insert(source, x, to_x));
        assert_eq!(
            TransitionsAccessor::transitions(&w.heap, source),
            vec![(x, to_x)]
        );

        let y = w.name();
        let to_y = w.map_adding(y);
        assert!(w.insert(source, y, to_y));
        let listed = TransitionsAccessor::transitions(&w.heap, source);
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&(x, to_x)));
        assert!(listed.contains(&(y, to_y)));
    }
}

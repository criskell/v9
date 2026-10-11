use crate::common::globals::{StoreOrigin, K_TAGGED_SIZE, K_TAGGED_SIZE_LOG2};
use crate::execution::isolate::Isolate;
use crate::handles::handles::*;
use crate::heap::factory::K_DESCRIPTOR_ARRAY_SLACK;
use crate::heap::heap::Heap;
use crate::objects::descriptor_array::{DescriptorArray, K_NOT_FOUND};
use crate::objects::elements_kind::{self, ElementsKind};
use crate::objects::instance_type::{self, InstanceType};
use crate::objects::property_details::{default_data_field, Representation};
use crate::objects::tagged::{Address, HeapObject, Tagged};

object_type!(
    /// `Map`
    Map
);

// ---------------------------------------------------------------------------
//   is_prototype_map                 bit  20
//   is_dictionary_map                bit  21
//   owns_descriptors                 bit  22
//   is_in_retained_map_list          bit  23
//   is_deprecated                    bit  24
//   is_unstable                      bit  25
//   is_migration_target              bit  26
//   is_extensible                    bit  27
//   may_have_interesting_properties  bit  28
// ---------------------------------------------------------------------------

const ENUM_LENGTH_SHIFT: u32 = 0;
const ENUM_LENGTH_BITS: u32 = 10;
const NUMBER_OF_OWN_DESCRIPTORS_SHIFT: u32 = 10;
const NUMBER_OF_OWN_DESCRIPTORS_BITS: u32 = 10;
const IS_PROTOTYPE_MAP_BIT: u32 = 20;
const IS_DICTIONARY_MAP_BIT: u32 = 21;
const OWNS_DESCRIPTORS_BIT: u32 = 22;
const IS_DEPRECATED_BIT: u32 = 24;
const IS_UNSTABLE_BIT: u32 = 25;
const IS_EXTENSIBLE_BIT: u32 = 27;

pub const K_INVALID_ENUM_CACHE_SENTINEL: u32 = (1 << ENUM_LENGTH_BITS) - 1;

const ELEMENTS_KIND_SHIFT: u8 = 2;
const ELEMENTS_KIND_BITS: u8 = 6;

const IS_CALLABLE_BIT: u8 = 0;
const IS_UNDETECTABLE_BIT: u8 = 1;
const IS_CONSTRUCTOR_BIT: u8 = 5;

#[inline]
const fn mask32(bits: u32) -> u32 {
    (1u32 << bits) - 1
}

const K_FAST_PROPERTIES_SOFT_LIMIT: i32 = 12;

#[inline]
const fn mask8(bits: u8) -> u8 {
    (1u8 << bits) - 1
}

impl Map {
    pub const K_INSTANCE_SIZE_IN_WORDS_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_INOBJECT_PROPERTIES_START_OFFSET: i32 = Self::K_INSTANCE_SIZE_IN_WORDS_OFFSET + 1;
    pub const K_USED_OR_UNUSED_INSTANCE_SIZE_IN_WORDS_OFFSET: i32 =
        Self::K_INOBJECT_PROPERTIES_START_OFFSET + 1;
    pub const K_VISITOR_ID_OFFSET: i32 = Self::K_USED_OR_UNUSED_INSTANCE_SIZE_IN_WORDS_OFFSET + 1;
    pub const K_INSTANCE_TYPE_OFFSET: i32 = Self::K_VISITOR_ID_OFFSET + 1;
    pub const K_BIT_FIELD_OFFSET: i32 = Self::K_INSTANCE_TYPE_OFFSET + 2;
    pub const K_BIT_FIELD2_OFFSET: i32 = Self::K_BIT_FIELD_OFFSET + 1;
    pub const K_BIT_FIELD3_OFFSET: i32 = Self::K_BIT_FIELD2_OFFSET + 1;
    const K_PADDING_OFFSET: i32 = Self::K_BIT_FIELD3_OFFSET + 4;
    pub const K_PROTOTYPE_OFFSET: i32 = Self::K_PADDING_OFFSET + 4;
    pub const K_CONSTRUCTOR_OR_BACK_POINTER_OFFSET: i32 = Self::K_PROTOTYPE_OFFSET + K_TAGGED_SIZE;
    pub const K_INSTANCE_DESCRIPTORS_OFFSET: i32 =
        Self::K_CONSTRUCTOR_OR_BACK_POINTER_OFFSET + K_TAGGED_SIZE;
    pub const K_TRANSITIONS_OR_PROTOTYPE_INFO_OFFSET: i32 =
        Self::K_INSTANCE_DESCRIPTORS_OFFSET + K_TAGGED_SIZE;
    /// `Map::kSize`
    pub const K_SIZE: i32 = Self::K_TRANSITIONS_OR_PROTOTYPE_INFO_OFFSET + K_TAGGED_SIZE;

    pub const K_INITIAL_INOBJECT_SLACK: i32 = 4;

    // -- instance size ------------------------------------------------------

    /// `Map::instance_size`
    #[inline]
    pub fn instance_size(self, heap: &Heap) -> i32 {
        self.instance_size_in_words(heap) * K_TAGGED_SIZE
    }

    #[inline]
    pub fn instance_size_in_words(self, heap: &Heap) -> i32 {
        heap.read_u8(self.field(Self::K_INSTANCE_SIZE_IN_WORDS_OFFSET)) as i32
    }

    #[inline]
    pub fn set_instance_size(self, heap: &mut Heap, size_in_bytes: i32) {
        debug_assert!(size_in_bytes % K_TAGGED_SIZE == 0);
        let words = size_in_bytes >> K_TAGGED_SIZE_LOG2;
        debug_assert!(
            words <= u8::MAX as i32,
            "instance size {size_in_bytes} too large"
        );
        heap.write_u8(
            self.field(Self::K_INSTANCE_SIZE_IN_WORDS_OFFSET),
            words as u8,
        );
    }

    #[inline]
    pub fn inobject_properties_start_in_words(self, heap: &Heap) -> i32 {
        heap.read_u8(self.field(Self::K_INOBJECT_PROPERTIES_START_OFFSET)) as i32
    }

    #[inline]
    pub fn set_inobject_properties_start_in_words(self, heap: &mut Heap, words: i32) {
        debug_assert!(words >= 0 && words <= u8::MAX as i32);
        heap.write_u8(
            self.field(Self::K_INOBJECT_PROPERTIES_START_OFFSET),
            words as u8,
        );
    }

    #[inline]
    pub fn inobject_properties(self, heap: &Heap) -> i32 {
        self.instance_size_in_words(heap) - self.inobject_properties_start_in_words(heap)
    }

    /// `Map::GetInObjectPropertyOffset`
    #[inline]
    pub fn inobject_property_offset(self, heap: &Heap, index: i32) -> i32 {
        (self.inobject_properties_start_in_words(heap) + index) * K_TAGGED_SIZE
    }

    // -- instance type ------------------------------------------------------

    /// `Map::instance_type`
    #[inline]
    pub fn instance_type(self, heap: &Heap) -> InstanceType {
        heap.read_u16(self.field(Self::K_INSTANCE_TYPE_OFFSET))
    }

    #[inline]
    pub fn set_instance_type(self, heap: &mut Heap, instance_type: InstanceType) {
        heap.write_u16(self.field(Self::K_INSTANCE_TYPE_OFFSET), instance_type);
    }

    // -- bit_field ----------------------------------------------------------

    #[inline]
    fn bit_field(self, heap: &Heap) -> u8 {
        heap.read_u8(self.field(Self::K_BIT_FIELD_OFFSET))
    }

    #[inline]
    fn set_bit_field_bit(self, heap: &mut Heap, bit: u8, value: bool) {
        let current = self.bit_field(heap);
        let updated = if value {
            current | (1 << bit)
        } else {
            current & !(1 << bit)
        };
        heap.write_u8(self.field(Self::K_BIT_FIELD_OFFSET), updated);
    }

    /// `Map::is_callable`
    #[inline]
    pub fn is_callable(self, heap: &Heap) -> bool {
        self.bit_field(heap) & (1 << IS_CALLABLE_BIT) != 0
    }

    #[inline]
    pub fn set_is_callable(self, heap: &mut Heap, value: bool) {
        self.set_bit_field_bit(heap, IS_CALLABLE_BIT, value);
    }

    #[inline]
    pub fn is_undetectable(self, heap: &Heap) -> bool {
        self.bit_field(heap) & (1 << IS_UNDETECTABLE_BIT) != 0
    }

    /// `Map::is_constructor`
    #[inline]
    pub fn is_constructor(self, heap: &Heap) -> bool {
        self.bit_field(heap) & (1 << IS_CONSTRUCTOR_BIT) != 0
    }

    #[inline]
    pub fn set_is_constructor(self, heap: &mut Heap, value: bool) {
        self.set_bit_field_bit(heap, IS_CONSTRUCTOR_BIT, value);
    }

    // -- bit_field2 ---------------------------------------------------------

    #[inline]
    pub fn bit_field2(self, heap: &Heap) -> u8 {
        heap.read_u8(self.field(Self::K_BIT_FIELD2_OFFSET))
    }

    #[inline]
    pub fn set_bit_field2(self, heap: &mut Heap, value: u8) {
        heap.write_u8(self.field(Self::K_BIT_FIELD2_OFFSET), value);
    }

    /// `Map::elements_kind`
    #[inline]
    pub fn elements_kind(self, heap: &Heap) -> ElementsKind {
        let bits = (self.bit_field2(heap) >> ELEMENTS_KIND_SHIFT) & mask8(ELEMENTS_KIND_BITS);
        ElementsKind::from_bits(bits)
    }

    /// `Map::set_elements_kind`
    #[inline]
    pub fn set_elements_kind(self, heap: &mut Heap, kind: ElementsKind) {
        let cleared = self.bit_field2(heap) & !(mask8(ELEMENTS_KIND_BITS) << ELEMENTS_KIND_SHIFT);
        self.set_bit_field2(heap, cleared | ((kind as u8) << ELEMENTS_KIND_SHIFT));
    }

    /// `Map::has_fast_elements`
    #[inline]
    pub fn has_fast_elements(self, heap: &Heap) -> bool {
        elements_kind::is_fast_elements_kind(self.elements_kind(heap))
    }

    /// `Map::has_fast_smi_elements`
    #[inline]
    pub fn has_fast_smi_elements(self, heap: &Heap) -> bool {
        elements_kind::is_smi_elements_kind(self.elements_kind(heap))
    }

    /// `Map::has_fast_object_elements`
    #[inline]
    pub fn has_fast_object_elements(self, heap: &Heap) -> bool {
        elements_kind::is_object_elements_kind(self.elements_kind(heap))
    }

    /// `Map::has_fast_double_elements`
    #[inline]
    pub fn has_fast_double_elements(self, heap: &Heap) -> bool {
        elements_kind::is_double_elements_kind(self.elements_kind(heap))
    }

    /// `Map::has_dictionary_elements`
    #[inline]
    pub fn has_dictionary_elements(self, heap: &Heap) -> bool {
        elements_kind::is_dictionary_elements_kind(self.elements_kind(heap))
    }

    // -- bit_field3 ---------------------------------------------------------

    #[inline]
    pub fn bit_field3(self, heap: &Heap) -> u32 {
        heap.read_u32(self.field(Self::K_BIT_FIELD3_OFFSET))
    }

    #[inline]
    pub fn set_bit_field3(self, heap: &mut Heap, value: u32) {
        heap.write_u32(self.field(Self::K_BIT_FIELD3_OFFSET), value);
    }

    #[inline]
    fn update_bit_field3_bit(self, heap: &mut Heap, bit: u32, value: bool) {
        let current = self.bit_field3(heap);
        let updated = if value {
            current | (1 << bit)
        } else {
            current & !(1 << bit)
        };
        self.set_bit_field3(heap, updated);
    }

    #[inline]
    pub fn number_of_own_descriptors(self, heap: &Heap) -> i32 {
        ((self.bit_field3(heap) >> NUMBER_OF_OWN_DESCRIPTORS_SHIFT)
            & mask32(NUMBER_OF_OWN_DESCRIPTORS_BITS)) as i32
    }

    #[inline]
    pub fn set_number_of_own_descriptors(self, heap: &mut Heap, number: i32) {
        debug_assert!(number >= 0 && (number as u32) <= mask32(NUMBER_OF_OWN_DESCRIPTORS_BITS));
        let cleared = self.bit_field3(heap)
            & !(mask32(NUMBER_OF_OWN_DESCRIPTORS_BITS) << NUMBER_OF_OWN_DESCRIPTORS_SHIFT);
        self.set_bit_field3(
            heap,
            cleared | ((number as u32) << NUMBER_OF_OWN_DESCRIPTORS_SHIFT),
        );
    }

    #[inline]
    pub fn enum_length(self, heap: &Heap) -> u32 {
        (self.bit_field3(heap) >> ENUM_LENGTH_SHIFT) & mask32(ENUM_LENGTH_BITS)
    }

    #[inline]
    pub fn set_enum_length(self, heap: &mut Heap, length: u32) {
        let cleared = self.bit_field3(heap) & !(mask32(ENUM_LENGTH_BITS) << ENUM_LENGTH_SHIFT);
        self.set_bit_field3(heap, cleared | (length << ENUM_LENGTH_SHIFT));
    }

    #[inline]
    pub fn is_dictionary_map(self, heap: &Heap) -> bool {
        self.bit_field3(heap) & (1 << IS_DICTIONARY_MAP_BIT) != 0
    }

    #[inline]
    pub fn set_is_dictionary_map(self, heap: &mut Heap, value: bool) {
        self.update_bit_field3_bit(heap, IS_DICTIONARY_MAP_BIT, value);
    }

    #[inline]
    pub fn owns_descriptors(self, heap: &Heap) -> bool {
        self.bit_field3(heap) & (1 << OWNS_DESCRIPTORS_BIT) != 0
    }

    #[inline]
    pub fn set_owns_descriptors(self, heap: &mut Heap, value: bool) {
        self.update_bit_field3_bit(heap, OWNS_DESCRIPTORS_BIT, value);
    }

    #[inline]
    pub fn is_extensible(self, heap: &Heap) -> bool {
        self.bit_field3(heap) & (1 << IS_EXTENSIBLE_BIT) != 0
    }

    #[inline]
    pub fn set_is_extensible(self, heap: &mut Heap, value: bool) {
        self.update_bit_field3_bit(heap, IS_EXTENSIBLE_BIT, value);
    }

    #[inline]
    pub fn is_prototype_map(self, heap: &Heap) -> bool {
        self.bit_field3(heap) & (1 << IS_PROTOTYPE_MAP_BIT) != 0
    }

    #[inline]
    pub fn set_is_prototype_map(self, heap: &mut Heap, value: bool) {
        self.update_bit_field3_bit(heap, IS_PROTOTYPE_MAP_BIT, value);
    }

    #[inline]
    pub fn is_deprecated(self, heap: &Heap) -> bool {
        self.bit_field3(heap) & (1 << IS_DEPRECATED_BIT) != 0
    }

    #[inline]
    pub fn set_is_deprecated(self, heap: &mut Heap, value: bool) {
        self.update_bit_field3_bit(heap, IS_DEPRECATED_BIT, value);
    }

    #[inline]
    pub fn is_unstable(self, heap: &Heap) -> bool {
        self.bit_field3(heap) & (1 << IS_UNSTABLE_BIT) != 0
    }

    #[inline]
    pub fn set_is_unstable(self, heap: &mut Heap, value: bool) {
        self.update_bit_field3_bit(heap, IS_UNSTABLE_BIT, value);
    }

    // -- tagged fields ------------------------------------------------------

    #[inline]
    pub fn prototype(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_PROTOTYPE_OFFSET))
    }

    #[inline]
    pub fn set_prototype(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_PROTOTYPE_OFFSET), value);
    }

    #[inline]
    pub fn prototype_address(self) -> Address {
        self.field(Self::K_PROTOTYPE_OFFSET)
    }

    #[inline]
    pub fn constructor_or_back_pointer(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_CONSTRUCTOR_OR_BACK_POINTER_OFFSET))
    }

    #[inline]
    pub fn set_constructor_or_back_pointer(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(
            self.field(Self::K_CONSTRUCTOR_OR_BACK_POINTER_OFFSET),
            value,
        );
    }

    #[inline]
    pub fn constructor_or_back_pointer_address(self) -> Address {
        self.field(Self::K_CONSTRUCTOR_OR_BACK_POINTER_OFFSET)
    }

    /// `Map::instance_descriptors`
    #[inline]
    pub fn instance_descriptors_raw(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_INSTANCE_DESCRIPTORS_OFFSET))
    }

    #[inline]
    pub fn instance_descriptors(self, heap: &Heap) -> DescriptorArray {
        DescriptorArray::unchecked_cast(
            self.instance_descriptors_raw(heap)
                .to_strong_heap_object()
                .expect("a Map always has a descriptor array"),
        )
    }

    #[inline]
    pub fn set_instance_descriptors(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_INSTANCE_DESCRIPTORS_OFFSET), value);
    }

    #[inline]
    pub fn instance_descriptors_address(self) -> Address {
        self.field(Self::K_INSTANCE_DESCRIPTORS_OFFSET)
    }

    /// `Map::raw_transitions`
    #[inline]
    pub fn raw_transitions(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_TRANSITIONS_OR_PROTOTYPE_INFO_OFFSET))
    }

    #[inline]
    pub fn set_raw_transitions(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(
            self.field(Self::K_TRANSITIONS_OR_PROTOTYPE_INFO_OFFSET),
            value,
        );
    }

    #[inline]
    pub fn raw_transitions_address(self) -> Address {
        self.field(Self::K_TRANSITIONS_OR_PROTOTYPE_INFO_OFFSET)
    }

    pub fn back_pointer(self, heap: &Heap) -> Option<Map> {
        let value = self.constructor_or_back_pointer(heap);
        let object = value.to_strong_heap_object()?;
        let map_of = heap.map(object);
        if instance_type::is_map(Map::unchecked_cast(map_of).instance_type(heap)) {
            Some(Map::unchecked_cast(object))
        } else {
            None
        }
    }

    pub fn last_added(self, heap: &Heap) -> i32 {
        let number = self.number_of_own_descriptors(heap);
        debug_assert!(number > 0, "a root map added nothing");
        number - 1
    }

    pub fn last_added_key(self, heap: &Heap) -> Tagged {
        self.instance_descriptors(heap)
            .get_key(heap, self.last_added(heap))
    }

    pub fn find_descriptor(self, heap: &Heap, name: Tagged) -> i32 {
        self.instance_descriptors(heap).search_within(
            heap,
            name,
            self.number_of_own_descriptors(heap),
        )
    }

    pub fn find_property_details(
        self,
        heap: &Heap,
        name: Tagged,
    ) -> Option<crate::objects::property_details::PropertyDetails> {
        let index = self.find_descriptor(heap, name);
        if index == crate::objects::descriptor_array::K_NOT_FOUND {
            None
        } else {
            Some(self.instance_descriptors(heap).get_details(heap, index))
        }
    }

    pub fn own_property_names(self, heap: &Heap) -> Vec<Tagged> {
        let descriptors = self.instance_descriptors(heap);
        (0..self.number_of_own_descriptors(heap))
            .map(|i| descriptors.get_key(heap, i))
            .collect()
    }

    pub fn is_js_receiver_map(self, heap: &Heap) -> bool {
        instance_type::is_js_receiver(self.instance_type(heap))
    }
}

impl Heap {
    #[inline]
    pub fn instance_type(&self, object: HeapObject) -> InstanceType {
        Map::unchecked_cast(self.map(object)).instance_type(self)
    }

    #[inline]
    pub fn map_of(&self, object: HeapObject) -> Map {
        Map::unchecked_cast(self.map(object))
    }

    #[inline]
    pub fn instance_type_of(&self, value: Tagged) -> Option<InstanceType> {
        Some(self.instance_type(value.to_strong_heap_object()?))
    }
}

impl Isolate {
    pub fn transition_to_data_property(
        &mut self,
        map: Map,
        name: Tagged,
        representation: Representation,
    ) -> Map {
        debug_assert!(
            instance_type::is_name(self.heap.instance_type_of(name).unwrap()),
            "a property key must be a Name"
        );
        debug_assert_eq!(
            map.find_descriptor(&self.heap, name),
            K_NOT_FOUND,
            "the property is already on this map"
        );

        use crate::objects::transitions::TransitionsAccessor;
        if let Some(target) = TransitionsAccessor::search_transition(&self.heap, map, name) {
            return target;
        }

        let new_map = self.copy_add_descriptor(map, name, representation);

        let transition_array_map = self.roots.transition_array_map();
        let _recorded =
            TransitionsAccessor::insert(&mut self.heap, map, name, new_map, transition_array_map);
        new_map
    }

    /// `Map::TooManyFastProperties`
    pub(crate) fn too_many_fast_properties(&self, map: Map, store_origin: StoreOrigin) -> bool {
        if store_origin != StoreOrigin::MaybeKeyed {
            return false;
        }
        if map.is_prototype_map(&self.heap) {
            return false;
        }
        let limit = K_FAST_PROPERTIES_SOFT_LIMIT.max(map.inobject_properties(&self.heap));
        let external =
            map.number_of_own_descriptors(&self.heap) - map.inobject_properties(&self.heap);
        external > limit
    }

    /// `Map::CopyNormalized`
    pub(crate) fn copy_normalized(&mut self, map: Map) -> Map {
        let result = self.copy_map_shell(map);
        result.set_is_dictionary_map(&mut self.heap, true);
        let kind = map.elements_kind(&self.heap);
        result.set_elements_kind(&mut self.heap, kind);
        result
    }

    /// `Map::Normalize`
    pub(crate) fn normalize_map(&mut self, fast_map: Map) -> Map {
        debug_assert!(!fast_map.is_dictionary_map(&self.heap));
        self.copy_normalized(fast_map)
    }

    pub(crate) fn copy_add_descriptor(
        &mut self,
        map: Map,
        name: Tagged,
        representation: Representation,
    ) -> Map {
        let number_of_own = map.number_of_own_descriptors(&self.heap);
        let field_index = number_of_own as u32;
        let details = default_data_field(field_index, representation);
        let hole = self.the_hole_value();

        let descriptors = map.instance_descriptors(&self.heap);
        let can_share = map.owns_descriptors(&self.heap)
            && descriptors.number_of_descriptors(&self.heap) == number_of_own
            && descriptors.number_of_slack_descriptors(&self.heap) > 0;

        let new_descriptors = if can_share {
            descriptors.append(&mut self.heap, name, details, hole);
            map.set_owns_descriptors(&mut self.heap, false);
            descriptors
        } else {
            let capacity = number_of_own + 1 + K_DESCRIPTOR_ARRAY_SLACK;
            let scope = self.handle_scope();
            let old_handle = self.create_object_handle(descriptors.object());
            let name_handle = self.create_handle(name);
            let hole_handle = self.create_handle(hole);
            let fresh = self.new_descriptor_array(capacity);
            let descriptors = DescriptorArray::unchecked_cast(old_handle.object(self));
            for i in 0..number_of_own {
                let key = descriptors.get_key(&self.heap, i);
                let entry_details = descriptors.get_details(&self.heap, i);
                let value = descriptors.get_value(&self.heap, i);
                fresh.append(&mut self.heap, key, entry_details, value);
            }
            let name = name_handle.get(self);
            let hole = hole_handle.get(self);
            fresh.append(&mut self.heap, name, details, hole);
            self.close_handle_scope(scope);
            fresh
        };

        let new_map = self.copy_map_shell(map);
        new_map.set_instance_descriptors(&mut self.heap, new_descriptors.tagged());
        new_map.set_number_of_own_descriptors(&mut self.heap, number_of_own + 1);
        new_map.set_owns_descriptors(&mut self.heap, true);
        new_map.set_constructor_or_back_pointer(&mut self.heap, map.tagged());
        new_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::AllocationType;
    use crate::objects::instance_type::{JS_OBJECT_TYPE, MAP_TYPE};

    fn raw_map(heap: &mut Heap, instance_type: InstanceType, instance_size: i32) -> Map {
        let object = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        let map = Map::unchecked_cast(object);
        map.set_instance_type(heap, instance_type);
        map.set_instance_size(heap, instance_size);
        map
    }

    #[test]
    fn layout_is_the_documented_56_bytes() {
        assert_eq!(Map::K_INSTANCE_SIZE_IN_WORDS_OFFSET, 8);
        assert_eq!(Map::K_INSTANCE_TYPE_OFFSET, 12);
        assert_eq!(Map::K_BIT_FIELD_OFFSET, 14);
        assert_eq!(Map::K_BIT_FIELD3_OFFSET, 16);
        assert_eq!(Map::K_PROTOTYPE_OFFSET, 24);
        assert_eq!(Map::K_CONSTRUCTOR_OR_BACK_POINTER_OFFSET, 32);
        assert_eq!(Map::K_INSTANCE_DESCRIPTORS_OFFSET, 40);
        assert_eq!(Map::K_TRANSITIONS_OR_PROTOTYPE_INFO_OFFSET, 48);
        assert_eq!(Map::K_SIZE, 56);
        for offset in [
            Map::K_PROTOTYPE_OFFSET,
            Map::K_CONSTRUCTOR_OR_BACK_POINTER_OFFSET,
            Map::K_INSTANCE_DESCRIPTORS_OFFSET,
            Map::K_TRANSITIONS_OR_PROTOTYPE_INFO_OFFSET,
        ] {
            assert_eq!(offset % K_TAGGED_SIZE, 0, "offset {offset} is misaligned");
        }
    }

    #[test]
    fn instance_size_is_stored_in_words() {
        let mut heap = Heap::new();
        let map = raw_map(&mut heap, JS_OBJECT_TYPE, 80);
        assert_eq!(map.instance_size(&heap), 80);
        assert_eq!(map.instance_size_in_words(&heap), 10);
    }

    #[test]
    fn inline_property_slots_are_what_is_left_after_the_header() {
        let mut heap = Heap::new();
        let map = raw_map(&mut heap, JS_OBJECT_TYPE, (3 + 4) * K_TAGGED_SIZE);
        map.set_inobject_properties_start_in_words(&mut heap, 3);

        assert_eq!(map.inobject_properties(&heap), 4);
        assert_eq!(map.inobject_property_offset(&heap, 0), 24);
        assert_eq!(map.inobject_property_offset(&heap, 1), 32);
        assert_eq!(map.inobject_property_offset(&heap, 3), 48);
    }

    #[test]
    fn bit_field3_subfields_are_independent() {
        let mut heap = Heap::new();
        let map = raw_map(&mut heap, JS_OBJECT_TYPE, 24);

        map.set_number_of_own_descriptors(&mut heap, 7);
        map.set_enum_length(&mut heap, 3);
        map.set_is_extensible(&mut heap, true);
        map.set_owns_descriptors(&mut heap, true);

        assert_eq!(map.number_of_own_descriptors(&heap), 7);
        assert_eq!(map.enum_length(&heap), 3);
        assert!(map.is_extensible(&heap));
        assert!(map.owns_descriptors(&heap));
        assert!(!map.is_dictionary_map(&heap));
        assert!(!map.is_deprecated(&heap));
        assert!(!map.is_prototype_map(&heap));

        map.set_is_deprecated(&mut heap, true);
        assert!(map.is_deprecated(&heap));
        assert_eq!(map.number_of_own_descriptors(&heap), 7);
        assert_eq!(map.enum_length(&heap), 3);
        assert!(map.is_extensible(&heap));

        map.set_is_extensible(&mut heap, false);
        assert!(!map.is_extensible(&heap));
        assert!(map.is_deprecated(&heap));
        assert_eq!(map.number_of_own_descriptors(&heap), 7);
    }

    #[test]
    fn descriptor_count_uses_its_full_ten_bits() {
        let mut heap = Heap::new();
        let map = raw_map(&mut heap, JS_OBJECT_TYPE, 24);
        let max = (1 << 10) - 1;
        map.set_number_of_own_descriptors(&mut heap, max);
        assert_eq!(map.number_of_own_descriptors(&heap), max);
        assert_eq!(map.enum_length(&heap), 0, "must not bleed into enum_length");
    }

    #[test]
    fn bit_field_flags_do_not_collide() {
        let mut heap = Heap::new();
        let map = raw_map(&mut heap, JS_OBJECT_TYPE, 24);
        assert!(!map.is_callable(&heap));
        assert!(!map.is_constructor(&heap));

        map.set_is_callable(&mut heap, true);
        assert!(map.is_callable(&heap));
        assert!(!map.is_constructor(&heap));
        assert!(!map.is_undetectable(&heap));

        map.set_is_constructor(&mut heap, true);
        assert!(
            map.is_callable(&heap),
            "setting constructor must not clear callable"
        );
        assert!(map.is_constructor(&heap));

        map.set_is_callable(&mut heap, false);
        assert!(!map.is_callable(&heap));
        assert!(map.is_constructor(&heap));
    }

    #[test]
    fn a_maps_own_map_is_the_meta_map_and_instance_type_reads_through_it() {
        let mut heap = Heap::new();
        let meta_map = raw_map(&mut heap, MAP_TYPE, Map::K_SIZE);
        heap.set_map(meta_map.object(), meta_map.object());

        let object_map = raw_map(&mut heap, JS_OBJECT_TYPE, 32);
        heap.set_map(object_map.object(), meta_map.object());

        assert_eq!(heap.map(meta_map.object()), meta_map.object());
        assert_eq!(heap.instance_type(meta_map.object()), MAP_TYPE);
        assert_eq!(heap.instance_type(object_map.object()), MAP_TYPE);
        assert_eq!(object_map.instance_type(&heap), JS_OBJECT_TYPE);
        assert!(instance_type::is_map(
            heap.instance_type(object_map.object())
        ));
    }

    #[test]
    fn back_pointer_distinguishes_a_map_from_a_constructor() {
        let mut heap = Heap::new();
        let meta_map = raw_map(&mut heap, MAP_TYPE, Map::K_SIZE);
        heap.set_map(meta_map.object(), meta_map.object());

        let root = raw_map(&mut heap, JS_OBJECT_TYPE, 32);
        heap.set_map(root.object(), meta_map.object());
        let child = raw_map(&mut heap, JS_OBJECT_TYPE, 32);
        heap.set_map(child.object(), meta_map.object());

        let not_a_map = heap.allocate_raw_or_fail(32, AllocationType::Old);
        heap.set_map(not_a_map, root.object());
        root.set_constructor_or_back_pointer(&mut heap, not_a_map.tagged());
        assert_eq!(root.back_pointer(&heap), None);

        child.set_constructor_or_back_pointer(&mut heap, root.tagged());
        assert_eq!(child.back_pointer(&heap), Some(root));
    }

    #[test]
    fn maps_live_in_read_only_space_so_they_never_move() {
        let mut heap = Heap::new();
        let map = raw_map(&mut heap, JS_OBJECT_TYPE, 32);
        assert!(
            heap.in_read_only_space(map.object()),
            "a moving map would invalidate every inline cache"
        );
        assert!(!heap.in_young_generation(map.object()));
    }
}

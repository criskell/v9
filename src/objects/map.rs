//! Ported from `src/objects/map.h`, `map.tq`, `map-inl.h` and the transition
//! logic in `map.cc`.
//!
//! The hidden class. A `Map` describes the *shape* of every object that
//! points at it: how big the object is, which properties it has and at which
//! slot, what its prototype is. Objects with the same history share a map,
//! which is what makes a property load a map check plus a fixed-offset read
//! instead of a hash lookup.
//!
//! Layout (after the map word that every heap object starts with):
//!
//! ```text
//!   0  map word (every Map's own map is the meta map)
//!   8  instance_size_in_words                        u8
//!   9  inobject_properties_start_or_ctor_fn_index    u8
//!  10  used_or_unused_instance_size_in_words         u8
//!  11  visitor_id                                    u8
//!  12  instance_type                                 u16
//!  14  bit_field                                     u8
//!  15  bit_field2                                    u8
//!  16  bit_field3                                    u32
//!  20  (padding)                                     u32
//!  24  prototype                                     tagged
//!  32  constructor_or_back_pointer                   tagged
//!  40  instance_descriptors                          tagged
//!  48  transitions_or_prototype_info                 tagged
//!  56  == Map::kSize
//! ```
//!
//! Deviation: V8 also carries `dependent_code` and `prototype_validity_cell`.
//! Both exist to invalidate optimized code when a shape assumption breaks,
//! and `vx` has no optimizing tier to invalidate yet. They are omitted rather
//! than stubbed, so that adding a JIT has to confront them honestly.

use crate::common::globals::{K_TAGGED_SIZE, K_TAGGED_SIZE_LOG2};
use crate::execution::isolate::Isolate;
use crate::heap::factory::K_DESCRIPTOR_ARRAY_SLACK;
use crate::heap::heap::Heap;
use crate::objects::descriptor_array::{DescriptorArray, K_NOT_FOUND};
use crate::objects::instance_type::{self, InstanceType};
use crate::objects::property_details::{default_data_field, Representation};
use crate::objects::tagged::{Address, HeapObject, Tagged};

object_type!(
    /// `Map`
    Map
);

// ---------------------------------------------------------------------------
// bit_field3, from `MapBits3` in map.tq:
//
//   enum_length                      bits  0..10
//   number_of_own_descriptors        bits 10..20
//   is_prototype_map                 bit  20
//   is_dictionary_map                bit  21
//   owns_descriptors                 bit  22
//   is_in_retained_map_list          bit  23
//   is_deprecated                    bit  24
//   is_unstable                      bit  25
//   is_migration_target              bit  26
//   is_extensible                    bit  27
//   may_have_interesting_properties  bit  28
//   construction_counter             bits 29..32
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

/// `kNoEnumLengthSentinel`-ish: V8 uses `kInvalidEnumCacheSentinel`.
pub const K_INVALID_ENUM_CACHE_SENTINEL: u32 = (1 << ENUM_LENGTH_BITS) - 1;

// bit_field (MapBits1)
const IS_CALLABLE_BIT: u8 = 0;
const IS_UNDETECTABLE_BIT: u8 = 1;
const IS_CONSTRUCTOR_BIT: u8 = 5;

#[inline]
const fn mask32(bits: u32) -> u32 {
    (1u32 << bits) - 1
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

    /// `Map::kSlackTrackingSlack` in spirit: how many spare inline property
    /// slots a fresh object literal map gets, so that the first few
    /// `o.x = ...` assignments do not need a property backing store.
    /// V8 derives this from slack tracking on the constructor; `vx` uses a
    /// fixed number (V8's own default for object literals).
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

    /// `Map::GetInObjectPropertiesStartInWords`: where the inline property
    /// slots begin, in words from the start of the object.
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

    /// `Map::GetInObjectProperties`: how many properties fit inside the
    /// object itself. Anything beyond this spills into the `PropertyArray`.
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

    /// `Map::is_undetectable` -- `document.all` and nothing else.
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

    /// `Map::NumberOfOwnDescriptors`.
    ///
    /// This is the count *this* map can see, which may be fewer than the
    /// descriptor array holds: a map shares its array with its transition
    /// targets and reads only the prefix it owns. That prefix is the whole
    /// trick behind sharing one array across a transition chain.
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

    /// `Map::is_dictionary_map`: the object gave up on hidden classes and
    /// went to a hash table. `vx` never sets this yet -- see `PORTING.md`.
    #[inline]
    pub fn is_dictionary_map(self, heap: &Heap) -> bool {
        self.bit_field3(heap) & (1 << IS_DICTIONARY_MAP_BIT) != 0
    }

    #[inline]
    pub fn set_is_dictionary_map(self, heap: &mut Heap, value: bool) {
        self.update_bit_field3_bit(heap, IS_DICTIONARY_MAP_BIT, value);
    }

    /// `Map::owns_descriptors`: whether this map may append to its descriptor
    /// array in place. Only the map at the tip of a transition chain owns the
    /// array; the others share it read-only.
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

    /// `Map::is_deprecated`: a newer map generalised one of this map's
    /// fields, so objects still using it should migrate.
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

    /// `Map::constructor_or_back_pointer`.
    ///
    /// One field, two meanings, as in V8: on a map reached by a transition it
    /// is the *back pointer* to the map transitioned from, which is how the
    /// engine walks a shape's history backwards. On a root map it is the
    /// constructor.
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

    // -- derived queries ----------------------------------------------------

    /// `Map::GetBackPointer`: the map this one was reached from, or `None` for
    /// a root map.
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

    /// `Map::LastAdded`: the index of the descriptor this map added relative
    /// to its back pointer -- i.e. the property whose addition created it.
    pub fn last_added(self, heap: &Heap) -> i32 {
        let number = self.number_of_own_descriptors(heap);
        debug_assert!(number > 0, "a root map added nothing");
        number - 1
    }

    /// The name of the property this map added, which is also the key of the
    /// transition that leads to it. V8 relies on this to store a single
    /// transition without allocating a `TransitionArray`:
    /// `TransitionsAccessor::GetSimpleTransitionKey` reads the key back out
    /// of the target map instead of storing it.
    pub fn last_added_key(self, heap: &Heap) -> Tagged {
        self.instance_descriptors(heap)
            .get_key(heap, self.last_added(heap))
    }

    /// `Map::FindDescriptor`: the index of `name` among the descriptors this
    /// map owns, or `DescriptorArray::kNotFound`.
    pub fn find_descriptor(self, heap: &Heap, name: Tagged) -> i32 {
        self.instance_descriptors(heap).search_within(
            heap,
            name,
            self.number_of_own_descriptors(heap),
        )
    }

    /// The details of the property `name`, if this map has it.
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

    /// Every property this map owns, in insertion order. For diagnostics and
    /// for `for...in`'s eventual enumeration order.
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
    /// `HeapObject::map` followed by `Map::instance_type` -- the question
    /// nearly every dispatch asks, so it gets to be one call.
    #[inline]
    pub fn instance_type(&self, object: HeapObject) -> InstanceType {
        Map::unchecked_cast(self.map(object)).instance_type(self)
    }

    /// The map of `object`, as a [`Map`].
    #[inline]
    pub fn map_of(&self, object: HeapObject) -> Map {
        Map::unchecked_cast(self.map(object))
    }

    /// The instance type of a tagged value, or `None` if it is a Smi.
    #[inline]
    pub fn instance_type_of(&self, value: Tagged) -> Option<InstanceType> {
        Some(self.instance_type(value.to_strong_heap_object()?))
    }
}

// ---------------------------------------------------------------------------
// The transition half of `src/objects/map.cc`.
//
// Adding a property does not mutate a shape, it *moves the object to a
// different shape*:
//
//   {}            o.x = 1          o.y = 2
//   Map0  --x-->  Map1    --y-->   Map2
//   0 props       1 prop           2 props
//
// The arrows are transitions, cached on the source map. The second object to
// take the same path finds `Map1` already there and reuses it, so every
// `{x, y}` in a program shares one `Map2` -- and therefore one field layout,
// which is what makes `o.x` compile to "check the map, load slot 0".
//
// These hang off `Isolate` rather than `Map` because they allocate, and
// allocation needs the heap, the roots and the factory at once. V8 passes the
// isolate into the equivalent static methods for the same reason.
// ---------------------------------------------------------------------------

impl Isolate {
    /// `Map::TransitionToDataProperty`: the map you get by adding `name` to
    /// an object whose map is `map`.
    ///
    /// Reuses an existing transition when the same shape has been built
    /// before, which is the common case and the whole reason transitions are
    /// recorded.
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

        // Record the edge so the next object down this path reuses the map.
        // If the array cannot be allocated the transition is simply not
        // cached: the cost is a duplicate map, never a wrong answer.
        let transition_array_map = self.roots.transition_array_map();
        let _recorded =
            TransitionsAccessor::insert(&mut self.heap, map, name, new_map, transition_array_map);
        new_map
    }

    /// `Map::CopyAddDescriptor`, including the `Map::ShareDescriptor` fast
    /// path.
    ///
    /// The sharing is what keeps a long shape chain cheap. If `map` owns its
    /// descriptor array and sits at its tip with slack left over, the new
    /// descriptor is appended *in place* and the child map shares the same
    /// array, reading one entry further into it. Ownership moves to the
    /// child, because the child is now the one allowed to append.
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
            // Copy into a fresh array with slack for the next few properties.
            let capacity = number_of_own + 1 + K_DESCRIPTOR_ARRAY_SLACK;
            let fresh = self.new_descriptor_array(capacity);
            for i in 0..number_of_own {
                let key = descriptors.get_key(&self.heap, i);
                let entry_details = descriptors.get_details(&self.heap, i);
                let value = descriptors.get_value(&self.heap, i);
                fresh.append(&mut self.heap, key, entry_details, value);
            }
            fresh.append(&mut self.heap, name, details, hole);
            fresh
        };

        let new_map = self.copy_map_shell(map);
        new_map.set_instance_descriptors(&mut self.heap, new_descriptors.tagged());
        new_map.set_number_of_own_descriptors(&mut self.heap, number_of_own + 1);
        new_map.set_owns_descriptors(&mut self.heap, true);
        // The back pointer is how the engine walks a shape's history, and the
        // field it shares with the constructor.
        new_map.set_constructor_or_back_pointer(&mut self.heap, map.tagged());
        new_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::AllocationType;
    use crate::objects::instance_type::{JS_OBJECT_TYPE, MAP_TYPE};

    /// Builds a Map by hand. `Factory` is what does this for real; these
    /// tests are about the layout and the bit fields.
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
        // Every tagged field must be word aligned or the heap will reject it.
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
        // A JSObject header is 3 words (map, properties, elements); give it
        // 4 inline property slots on top of that.
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

        // Flipping one flag must not disturb the counts.
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

        // The meta map describes itself -- the root of the whole type system.
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

        // A root map holds its constructor there; not a map, so no back pointer.
        let not_a_map = heap.allocate_raw_or_fail(32, AllocationType::Old);
        heap.set_map(not_a_map, root.object());
        root.set_constructor_or_back_pointer(&mut heap, not_a_map.tagged());
        assert_eq!(root.back_pointer(&heap), None);

        // A transitioned map holds the map it came from.
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

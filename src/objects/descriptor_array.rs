//! Ported from `src/objects/descriptor-array.h` and `descriptor-array.tq`.
//!
//! A map's property table. Each entry is three tagged slots -- the property's
//! name, its [`PropertyDetails`] as a Smi, and its value (used only for
//! constants; a normal data property's value lives in the object):
//!
//! ```text
//!   [ map | all_descs:u16 | n_descs:u16 | flags:u32 | enum_cache
//!     | key0 | details0 | value0 | key1 | details1 | value1 | ... ]
//! ```
//!
//! `number_of_all_descriptors` is the capacity and `number_of_descriptors`
//! the occupancy: the difference is slack, which is what lets a chain of maps
//! append descriptors while sharing one array. That sharing is the reason
//! `o.x = 1; o.y = 2` on a thousand objects costs one descriptor array rather
//! than a thousand.

use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::property_details::PropertyDetails;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `DescriptorArray`
    DescriptorArray
);

/// `DescriptorArray::kNotFound`
pub const K_NOT_FOUND: i32 = -1;

impl DescriptorArray {
    pub const K_NUMBER_OF_ALL_DESCRIPTORS_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_NUMBER_OF_DESCRIPTORS_OFFSET: i32 = Self::K_NUMBER_OF_ALL_DESCRIPTORS_OFFSET + 2;
    pub const K_FLAGS_OFFSET: i32 = Self::K_NUMBER_OF_DESCRIPTORS_OFFSET + 2;
    pub const K_ENUM_CACHE_OFFSET: i32 = Self::K_FLAGS_OFFSET + 4;
    /// `DescriptorArray::kHeaderSize`
    pub const K_HEADER_SIZE: i32 = Self::K_ENUM_CACHE_OFFSET + K_TAGGED_SIZE;

    /// Slots per entry: key, details, value.
    pub const K_ENTRY_SIZE: i32 = 3;
    pub const K_ENTRY_KEY_INDEX: i32 = 0;
    pub const K_ENTRY_DETAILS_INDEX: i32 = 1;
    pub const K_ENTRY_VALUE_INDEX: i32 = 2;

    /// `DescriptorArray::SizeFor`
    #[inline]
    pub const fn size_for(number_of_all_descriptors: i32) -> i32 {
        Self::K_HEADER_SIZE + number_of_all_descriptors * Self::K_ENTRY_SIZE * K_TAGGED_SIZE
    }

    #[inline]
    pub const fn offset_of_entry_slot(descriptor: i32, slot: i32) -> i32 {
        Self::K_HEADER_SIZE + (descriptor * Self::K_ENTRY_SIZE + slot) * K_TAGGED_SIZE
    }

    /// Capacity. `const` in V8: an array is never grown in place, only
    /// replaced by a bigger copy.
    #[inline]
    pub fn number_of_all_descriptors(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_NUMBER_OF_ALL_DESCRIPTORS_OFFSET)) as i32
    }

    #[inline]
    pub fn set_number_of_all_descriptors(self, heap: &mut Heap, value: i32) {
        heap.write_u16(
            self.field(Self::K_NUMBER_OF_ALL_DESCRIPTORS_OFFSET),
            value as u16,
        );
    }

    /// Occupancy: how many entries are actually in use.
    #[inline]
    pub fn number_of_descriptors(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_NUMBER_OF_DESCRIPTORS_OFFSET)) as i32
    }

    #[inline]
    pub fn set_number_of_descriptors(self, heap: &mut Heap, value: i32) {
        debug_assert!(value <= self.number_of_all_descriptors(heap));
        heap.write_u16(
            self.field(Self::K_NUMBER_OF_DESCRIPTORS_OFFSET),
            value as u16,
        );
    }

    /// `DescriptorArray::number_of_slack_descriptors`
    #[inline]
    pub fn number_of_slack_descriptors(self, heap: &Heap) -> i32 {
        self.number_of_all_descriptors(heap) - self.number_of_descriptors(heap)
    }

    #[inline]
    pub fn enum_cache(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_ENUM_CACHE_OFFSET))
    }

    #[inline]
    pub fn set_enum_cache(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_ENUM_CACHE_OFFSET), value);
    }

    #[inline]
    pub fn key_address(self, descriptor: i32) -> Address {
        self.field(Self::offset_of_entry_slot(
            descriptor,
            Self::K_ENTRY_KEY_INDEX,
        ))
    }

    #[inline]
    pub fn value_address(self, descriptor: i32) -> Address {
        self.field(Self::offset_of_entry_slot(
            descriptor,
            Self::K_ENTRY_VALUE_INDEX,
        ))
    }

    /// `DescriptorArray::GetKey`
    #[inline]
    pub fn get_key(self, heap: &Heap, descriptor: i32) -> Tagged {
        debug_assert!(descriptor >= 0 && descriptor < self.number_of_descriptors(heap));
        heap.read_tagged(self.key_address(descriptor))
    }

    /// `DescriptorArray::GetDetails`
    #[inline]
    pub fn get_details(self, heap: &Heap, descriptor: i32) -> PropertyDetails {
        debug_assert!(descriptor >= 0 && descriptor < self.number_of_descriptors(heap));
        PropertyDetails::from_smi(heap.read_tagged(self.field(Self::offset_of_entry_slot(
            descriptor,
            Self::K_ENTRY_DETAILS_INDEX,
        ))))
    }

    /// `DescriptorArray::GetStrongValue`
    #[inline]
    pub fn get_value(self, heap: &Heap, descriptor: i32) -> Tagged {
        debug_assert!(descriptor >= 0 && descriptor < self.number_of_descriptors(heap));
        heap.read_tagged(self.value_address(descriptor))
    }

    pub fn set_details(self, heap: &mut Heap, descriptor: i32, details: PropertyDetails) {
        heap.write_tagged(
            self.field(Self::offset_of_entry_slot(
                descriptor,
                Self::K_ENTRY_DETAILS_INDEX,
            )),
            details.as_smi(),
        );
    }

    /// `DescriptorArray::Set`: writes an entry at a known index. The index
    /// must already be within the occupancy -- [`DescriptorArray::append`] is
    /// what extends it.
    pub fn set(
        self,
        heap: &mut Heap,
        descriptor: i32,
        key: Tagged,
        details: PropertyDetails,
        value: Tagged,
    ) {
        debug_assert!(descriptor >= 0 && descriptor < self.number_of_all_descriptors(heap));
        heap.write_tagged(self.key_address(descriptor), key);
        heap.write_tagged(
            self.field(Self::offset_of_entry_slot(
                descriptor,
                Self::K_ENTRY_DETAILS_INDEX,
            )),
            details.as_smi(),
        );
        heap.write_tagged(self.value_address(descriptor), value);
    }

    /// `DescriptorArray::Append`: claims the next slack slot.
    ///
    /// Returns the index of the new descriptor. Panics if there is no slack,
    /// because growing means allocating a replacement array and that is the
    /// caller's decision to make (and the caller's roots to worry about).
    pub fn append(
        self,
        heap: &mut Heap,
        key: Tagged,
        details: PropertyDetails,
        value: Tagged,
    ) -> i32 {
        let index = self.number_of_descriptors(heap);
        assert!(
            index < self.number_of_all_descriptors(heap),
            "no slack left in the descriptor array; allocate a larger one"
        );
        self.set(heap, index, key, details, value);
        self.set_number_of_descriptors(heap, index + 1);
        index
    }

    /// `DescriptorArray::Search`, linear form.
    ///
    /// V8 binary-searches by the name's hash once an array is large and keeps
    /// the entries sorted to make that possible. `vx` searches linearly and
    /// leaves the entries in insertion order. That is O(n) per miss, but a
    /// *miss* is what it costs: a hit on a hot property never reaches here,
    /// because the inline cache answers from the cached field index. Sorting
    /// plus binary search is tracked in `PORTING.md`.
    ///
    /// Keys are compared by address, which is sound because property names
    /// are internalized -- one string object per distinct name.
    pub fn search(self, heap: &Heap, name: Tagged) -> i32 {
        self.search_within(heap, name, self.number_of_descriptors(heap))
    }

    /// `DescriptorArray::Search(name, map)`: searches only the first `limit`
    /// entries.
    ///
    /// The bound is not an optimisation, it is a correctness requirement. A
    /// map shares its descriptor array with the maps it transitions to, and
    /// sees only the prefix it owns: a map with two properties must not find
    /// the third descriptor that a sibling transition appended.
    pub fn search_within(self, heap: &Heap, name: Tagged, limit: i32) -> i32 {
        debug_assert!(limit <= self.number_of_descriptors(heap));
        for i in 0..limit {
            if self.get_key(heap, i) == name {
                return i;
            }
        }
        K_NOT_FOUND
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::AllocationType;
    use crate::objects::property_details::{default_data_field, Representation};
    use crate::objects::smi::Smi;

    fn raw_descriptor_array(heap: &mut Heap, capacity: i32) -> DescriptorArray {
        let object = heap.allocate_raw_or_fail(
            DescriptorArray::size_for(capacity) as usize,
            AllocationType::Old,
        );
        let array = DescriptorArray::unchecked_cast(object);
        array.set_number_of_all_descriptors(heap, capacity);
        array.set_number_of_descriptors(heap, 0);
        array
    }

    /// Stands in for an internalized name; the search only compares addresses.
    fn fake_name(heap: &mut Heap, id: i32) -> Tagged {
        let object = heap.allocate_raw_or_fail(16, AllocationType::ReadOnly);
        heap.write_tagged(object.field_address(8), Smi::from_int(id));
        object.tagged()
    }

    #[test]
    fn layout_matches_three_slots_per_entry() {
        assert_eq!(DescriptorArray::K_HEADER_SIZE, 24);
        assert_eq!(DescriptorArray::size_for(0), 24);
        assert_eq!(DescriptorArray::size_for(1), 48);
        assert_eq!(DescriptorArray::size_for(2), 72);
        assert_eq!(DescriptorArray::offset_of_entry_slot(0, 0), 24);
        assert_eq!(DescriptorArray::offset_of_entry_slot(1, 0), 48);
        assert_eq!(DescriptorArray::offset_of_entry_slot(1, 2), 64);
    }

    #[test]
    fn appending_claims_slack_and_grows_occupancy() {
        let mut heap = Heap::new();
        let array = raw_descriptor_array(&mut heap, 4);
        assert_eq!(array.number_of_descriptors(&heap), 0);
        assert_eq!(array.number_of_slack_descriptors(&heap), 4);

        let x = fake_name(&mut heap, 1);
        let index = array.append(
            &mut heap,
            x,
            default_data_field(0, Representation::Smi),
            Smi::zero(),
        );
        assert_eq!(index, 0);
        assert_eq!(array.number_of_descriptors(&heap), 1);
        assert_eq!(array.number_of_slack_descriptors(&heap), 3);
        assert_eq!(array.get_key(&heap, 0), x);
        assert_eq!(array.get_details(&heap, 0).field_index(), 0);
    }

    #[test]
    fn entries_are_independent() {
        let mut heap = Heap::new();
        let array = raw_descriptor_array(&mut heap, 3);
        let names: Vec<Tagged> = (0..3).map(|i| fake_name(&mut heap, i)).collect();
        for (i, name) in names.iter().enumerate() {
            array.append(
                &mut heap,
                *name,
                default_data_field(i as u32, Representation::Tagged),
                Smi::from_int(100 + i as i32),
            );
        }
        for (i, name) in names.iter().enumerate() {
            assert_eq!(array.get_key(&heap, i as i32), *name);
            assert_eq!(array.get_details(&heap, i as i32).field_index(), i as u32);
            assert_eq!(
                Smi::to_int(array.get_value(&heap, i as i32)),
                100 + i as i32
            );
        }
    }

    #[test]
    fn search_finds_by_identity_and_reports_misses() {
        let mut heap = Heap::new();
        let array = raw_descriptor_array(&mut heap, 4);
        let x = fake_name(&mut heap, 1);
        let y = fake_name(&mut heap, 2);
        let absent = fake_name(&mut heap, 3);

        array.append(
            &mut heap,
            x,
            default_data_field(0, Representation::Smi),
            Smi::zero(),
        );
        array.append(
            &mut heap,
            y,
            default_data_field(1, Representation::Smi),
            Smi::zero(),
        );

        assert_eq!(array.search(&heap, x), 0);
        assert_eq!(array.search(&heap, y), 1);
        assert_eq!(array.search(&heap, absent), K_NOT_FOUND);
    }

    #[test]
    fn search_does_not_see_slack_slots() {
        let mut heap = Heap::new();
        let array = raw_descriptor_array(&mut heap, 4);
        let x = fake_name(&mut heap, 1);
        // Write into a slack slot directly, without bumping the occupancy.
        array.set(
            &mut heap,
            2,
            x,
            default_data_field(2, Representation::Smi),
            Smi::zero(),
        );
        assert_eq!(
            array.search(&heap, x),
            K_NOT_FOUND,
            "slack must be invisible until it is appended"
        );
    }

    #[test]
    fn details_can_be_updated_in_place_for_representation_widening() {
        let mut heap = Heap::new();
        let array = raw_descriptor_array(&mut heap, 2);
        let x = fake_name(&mut heap, 1);
        array.append(
            &mut heap,
            x,
            default_data_field(0, Representation::Smi),
            Smi::zero(),
        );
        assert_eq!(
            array.get_details(&heap, 0).representation(),
            Representation::Smi
        );

        let widened = array
            .get_details(&heap, 0)
            .with_representation(Representation::Tagged);
        array.set_details(&mut heap, 0, widened);
        assert_eq!(
            array.get_details(&heap, 0).representation(),
            Representation::Tagged
        );
        assert_eq!(array.get_details(&heap, 0).field_index(), 0);
        assert_eq!(
            array.get_key(&heap, 0),
            x,
            "widening must not touch the key"
        );
    }

    #[test]
    #[should_panic(expected = "no slack left")]
    fn appending_past_capacity_is_refused() {
        let mut heap = Heap::new();
        let array = raw_descriptor_array(&mut heap, 1);
        let x = fake_name(&mut heap, 1);
        let y = fake_name(&mut heap, 2);
        array.append(
            &mut heap,
            x,
            default_data_field(0, Representation::Smi),
            Smi::zero(),
        );
        array.append(
            &mut heap,
            y,
            default_data_field(1, Representation::Smi),
            Smi::zero(),
        );
    }
}

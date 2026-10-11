//! Ported from `src/objects/fixed-array.h` and `fixed-array.tq`.
//!
//! The engine's workhorse array: a length followed by that many tagged
//! slots. `PropertyArray` has the same shape and is the out-of-object
//! property backing store -- where a property goes once an object has run
//! out of inline slots.
//!
//! ```text
//!   FixedArray      [ map | length (Smi) | e0 | e1 | ... ]
//!   PropertyArray   [ map | length_and_hash (Smi) | p0 | p1 | ... ]
//!   WeakFixedArray  [ map | length (Smi) | w0 | w1 | ... ]   (weak slots)
//! ```

use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::smi::Smi;
use crate::objects::tagged::{Address, HeapObject, Tagged};

object_type!(
    /// `FixedArray`
    FixedArray
);

impl FixedArray {
    pub const K_LENGTH_OFFSET: i32 = K_TAGGED_SIZE;
    /// `FixedArray::kHeaderSize`
    pub const K_HEADER_SIZE: i32 = Self::K_LENGTH_OFFSET + K_TAGGED_SIZE;

    /// `FixedArray::SizeFor`
    #[inline]
    pub const fn size_for(length: i32) -> i32 {
        Self::K_HEADER_SIZE + length * K_TAGGED_SIZE
    }

    /// `FixedArray::OffsetOfElementAt`
    #[inline]
    pub const fn offset_of_element_at(index: i32) -> i32 {
        Self::K_HEADER_SIZE + index * K_TAGGED_SIZE
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        Smi::to_int(heap.read_tagged(self.field(Self::K_LENGTH_OFFSET)))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        heap.write_tagged(self.field(Self::K_LENGTH_OFFSET), Smi::from_int(length));
    }

    /// The address of element `index`, for the GC to visit.
    #[inline]
    pub fn element_address(self, index: i32) -> Address {
        self.field(Self::offset_of_element_at(index))
    }

    /// `FixedArray::get`
    #[inline]
    pub fn get(self, heap: &Heap, index: i32) -> Tagged {
        debug_assert!(
            index >= 0 && index < self.length(heap),
            "FixedArray index {index} out of range 0..{}",
            self.length(heap)
        );
        heap.read_tagged(self.element_address(index))
    }

    /// `FixedArray::set`
    #[inline]
    pub fn set(self, heap: &mut Heap, index: i32, value: Tagged) {
        debug_assert!(
            index >= 0 && index < self.length(heap),
            "FixedArray index {index} out of range 0..{}",
            self.length(heap)
        );
        heap.write_tagged(self.element_address(index), value);
    }

    /// Iterates the elements. Returns an owned `Vec` rather than a borrowing
    /// iterator on purpose: anything that walks an array usually wants to
    /// allocate while doing so, and holding a borrow of the heap across an
    /// allocation is precisely what this port is trying to make impossible.
    pub fn to_vec(self, heap: &Heap) -> Vec<Tagged> {
        (0..self.length(heap)).map(|i| self.get(heap, i)).collect()
    }
}

object_type!(
    /// `PropertyArray`: the out-of-object property backing store.
    ///
    /// V8 packs a hash into the length field (`length_and_hash`); `vx` stores
    /// only the length, since nothing here needs the receiver's identity hash
    /// yet. The accessors are named after the V8 field so the resemblance is
    /// not accidental.
    PropertyArray
);

impl PropertyArray {
    pub const K_LENGTH_AND_HASH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_LENGTH_AND_HASH_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub const fn size_for(length: i32) -> i32 {
        Self::K_HEADER_SIZE + length * K_TAGGED_SIZE
    }

    #[inline]
    pub const fn offset_of_element_at(index: i32) -> i32 {
        Self::K_HEADER_SIZE + index * K_TAGGED_SIZE
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        Smi::to_int(heap.read_tagged(self.field(Self::K_LENGTH_AND_HASH_OFFSET)))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        heap.write_tagged(
            self.field(Self::K_LENGTH_AND_HASH_OFFSET),
            Smi::from_int(length),
        );
    }

    #[inline]
    pub fn element_address(self, index: i32) -> Address {
        self.field(Self::offset_of_element_at(index))
    }

    #[inline]
    pub fn get(self, heap: &Heap, index: i32) -> Tagged {
        debug_assert!(index >= 0 && index < self.length(heap));
        heap.read_tagged(self.element_address(index))
    }

    #[inline]
    pub fn set(self, heap: &mut Heap, index: i32, value: Tagged) {
        debug_assert!(index >= 0 && index < self.length(heap));
        heap.write_tagged(self.element_address(index), value);
    }

    #[inline]
    pub fn as_fixed_array_like(self) -> HeapObject {
        self.object()
    }
}

object_type!(
    /// `WeakFixedArray`: slots hold weak references, so the GC may clear them.
    /// `TransitionArray` is built on this -- a map transition must not keep
    /// its target map alive on its own.
    WeakFixedArray
);

impl WeakFixedArray {
    pub const K_LENGTH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_LENGTH_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub const fn size_for(length: i32) -> i32 {
        Self::K_HEADER_SIZE + length * K_TAGGED_SIZE
    }

    #[inline]
    pub const fn offset_of_element_at(index: i32) -> i32 {
        Self::K_HEADER_SIZE + index * K_TAGGED_SIZE
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        Smi::to_int(heap.read_tagged(self.field(Self::K_LENGTH_OFFSET)))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        heap.write_tagged(self.field(Self::K_LENGTH_OFFSET), Smi::from_int(length));
    }

    #[inline]
    pub fn element_address(self, index: i32) -> Address {
        self.field(Self::offset_of_element_at(index))
    }

    #[inline]
    pub fn get(self, heap: &Heap, index: i32) -> Tagged {
        debug_assert!(index >= 0 && index < self.length(heap));
        heap.read_tagged(self.element_address(index))
    }

    #[inline]
    pub fn set(self, heap: &mut Heap, index: i32, value: Tagged) {
        debug_assert!(index >= 0 && index < self.length(heap));
        heap.write_tagged(self.element_address(index), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::AllocationType;

    /// Allocates a bare FixedArray-shaped object. Real allocation goes
    /// through `Factory`; these tests only need the layout.
    fn raw_fixed_array(heap: &mut Heap, length: i32) -> FixedArray {
        let object =
            heap.allocate_raw_or_fail(FixedArray::size_for(length) as usize, AllocationType::Old);
        let array = FixedArray::unchecked_cast(object);
        array.set_length(heap, length);
        array
    }

    #[test]
    fn size_accounts_for_the_header_and_every_element() {
        assert_eq!(FixedArray::K_HEADER_SIZE, 16);
        assert_eq!(FixedArray::size_for(0), 16);
        assert_eq!(FixedArray::size_for(1), 24);
        assert_eq!(FixedArray::size_for(4), 48);
        assert_eq!(FixedArray::offset_of_element_at(0), 16);
        assert_eq!(FixedArray::offset_of_element_at(3), 40);
    }

    #[test]
    fn elements_round_trip() {
        let mut heap = Heap::new();
        let array = raw_fixed_array(&mut heap, 4);
        assert_eq!(array.length(&heap), 4);
        for i in 0..4 {
            array.set(&mut heap, i, Smi::from_int(i * 10));
        }
        for i in 0..4 {
            assert_eq!(Smi::to_int(array.get(&heap, i)), i * 10);
        }
        assert_eq!(
            array
                .to_vec(&heap)
                .iter()
                .map(|t| Smi::to_int(*t))
                .collect::<Vec<_>>(),
            vec![0, 10, 20, 30]
        );
    }

    #[test]
    fn a_fresh_array_is_full_of_smi_zeroes() {
        let mut heap = Heap::new();
        let array = raw_fixed_array(&mut heap, 3);
        for i in 0..3 {
            assert!(array.get(&heap, i).is_smi(), "slot {i} must be GC-safe");
        }
    }

    #[test]
    fn two_arrays_do_not_share_storage() {
        let mut heap = Heap::new();
        let a = raw_fixed_array(&mut heap, 2);
        let b = raw_fixed_array(&mut heap, 2);
        a.set(&mut heap, 0, Smi::from_int(1));
        a.set(&mut heap, 1, Smi::from_int(2));
        b.set(&mut heap, 0, Smi::from_int(3));
        b.set(&mut heap, 1, Smi::from_int(4));
        assert_eq!(Smi::to_int(a.get(&heap, 0)), 1);
        assert_eq!(Smi::to_int(a.get(&heap, 1)), 2);
        assert_eq!(Smi::to_int(b.get(&heap, 0)), 3);
        assert_eq!(b.length(&heap), 2);
    }

    #[test]
    #[should_panic]
    fn indexing_past_the_end_is_caught_in_debug_builds() {
        let mut heap = Heap::new();
        let array = raw_fixed_array(&mut heap, 2);
        array.get(&heap, 2);
    }

    #[test]
    fn property_array_has_the_same_shape() {
        let mut heap = Heap::new();
        let object =
            heap.allocate_raw_or_fail(PropertyArray::size_for(3) as usize, AllocationType::Old);
        let array = PropertyArray::unchecked_cast(object);
        array.set_length(&mut heap, 3);
        assert_eq!(array.length(&heap), 3);
        array.set(&mut heap, 2, Smi::from_int(99));
        assert_eq!(Smi::to_int(array.get(&heap, 2)), 99);
        assert_eq!(PropertyArray::size_for(3), FixedArray::size_for(3));
    }

    #[test]
    fn weak_fixed_array_slots_hold_weak_references() {
        let mut heap = Heap::new();
        let object =
            heap.allocate_raw_or_fail(WeakFixedArray::size_for(2) as usize, AllocationType::Old);
        let array = WeakFixedArray::unchecked_cast(object);
        array.set_length(&mut heap, 2);

        let target = heap.allocate_raw_or_fail(8, AllocationType::Old);
        array.set(&mut heap, 0, target.tagged_weak());
        let slot = array.get(&heap, 0);
        assert!(slot.is_weak());
        assert_eq!(slot.to_heap_object(), Some(target));

        array.set(&mut heap, 1, Tagged::cleared());
        assert!(array.get(&heap, 1).is_cleared());
    }
}

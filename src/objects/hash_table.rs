use crate::heap::heap::Heap;
use crate::objects::fixed_array::FixedArray;
use crate::objects::smi::Smi;
use crate::objects::tagged::{Address, Tagged};

pub const K_NOT_FOUND: i32 = -1;

object_type!(
    HashTableBase
);

impl HashTableBase {
    pub const K_NUMBER_OF_ELEMENTS_INDEX: i32 = 0;
    pub const K_NUMBER_OF_DELETED_ELEMENTS_INDEX: i32 = 1;
    pub const K_CAPACITY_INDEX: i32 = 2;
    pub const K_PREFIX_START_INDEX: i32 = 3;

    // Minimum capacity for newly created hash tables.
    pub const K_MIN_CAPACITY: u32 = 4;

    #[inline]
    pub fn as_fixed_array(self) -> FixedArray {
        FixedArray::unchecked_cast(self.object())
    }

    #[inline]
    pub fn number_of_elements(self, heap: &Heap) -> i32 {
        Smi::to_int(
            self.as_fixed_array()
                .get(heap, Self::K_NUMBER_OF_ELEMENTS_INDEX),
        )
    }

    #[inline]
    pub fn set_number_of_elements(self, heap: &mut Heap, count: i32) {
        self.as_fixed_array()
            .set(heap, Self::K_NUMBER_OF_ELEMENTS_INDEX, Smi::from_int(count));
    }

    #[inline]
    pub fn number_of_deleted_elements(self, heap: &Heap) -> i32 {
        Smi::to_int(
            self.as_fixed_array()
                .get(heap, Self::K_NUMBER_OF_DELETED_ELEMENTS_INDEX),
        )
    }

    #[inline]
    pub fn set_number_of_deleted_elements(self, heap: &mut Heap, count: i32) {
        self.as_fixed_array().set(
            heap,
            Self::K_NUMBER_OF_DELETED_ELEMENTS_INDEX,
            Smi::from_int(count),
        );
    }

    #[inline]
    pub fn capacity(self, heap: &Heap) -> u32 {
        Smi::to_int(self.as_fixed_array().get(heap, Self::K_CAPACITY_INDEX)) as u32
    }

    #[inline]
    pub fn set_capacity(self, heap: &mut Heap, capacity: u32) {
        self.as_fixed_array()
            .set(heap, Self::K_CAPACITY_INDEX, Smi::from_int(capacity as i32));
    }

    #[inline]
    pub fn element_added(self, heap: &mut Heap) {
        let count = self.number_of_elements(heap);
        self.set_number_of_elements(heap, count + 1);
    }

    #[inline]
    pub fn element_removed(self, heap: &mut Heap) {
        let count = self.number_of_elements(heap);
        self.set_number_of_elements(heap, count - 1);
        let deleted = self.number_of_deleted_elements(heap);
        self.set_number_of_deleted_elements(heap, deleted + 1);
    }

    // Computes the required capacity for a table holding the given
    // number of elements.
    #[inline]
    pub fn compute_capacity(at_least_space_for: u32) -> u32 {
        // Add 50% slack to make slot collisions sufficiently unlikely.
        let raw_cap = at_least_space_for + (at_least_space_for >> 1);
        let capacity = raw_cap.next_power_of_two();
        capacity.max(Self::K_MIN_CAPACITY)
    }

    #[inline]
    pub fn next_probe(last: u32, number: u32, size: u32) -> u32 {
        (last + number) & (size - 1)
    }

    #[inline]
    pub fn first_probe(hash: u32, size: u32) -> u32 {
        hash & (size - 1)
    }
}

#[derive(Clone, Copy)]
pub struct HashTableShape {
    pub prefix_size: i32,
    pub entry_size: i32,
}

impl HashTableShape {
    #[inline]
    pub const fn elements_start_index(&self) -> i32 {
        HashTableBase::K_PREFIX_START_INDEX + self.prefix_size
    }

    // Returns the index for an entry (of the key)
    #[inline]
    pub const fn entry_to_index(&self, entry: i32) -> i32 {
        (entry * self.entry_size) + self.elements_start_index()
    }

    // Returns the entry for an index (of the key)
    #[inline]
    pub const fn index_to_entry(&self, index: i32) -> i32 {
        (index - self.elements_start_index()) / self.entry_size
    }

    #[inline]
    pub const fn size_for(&self, capacity: u32) -> i32 {
        self.elements_start_index() + capacity as i32 * self.entry_size
    }
}

impl HashTableBase {
    // Returns the key at entry.
    #[inline]
    pub fn key_at(self, heap: &Heap, shape: HashTableShape, entry: i32) -> Tagged {
        self.as_fixed_array().get(heap, shape.entry_to_index(entry))
    }

    #[inline]
    pub fn set_key_at(self, heap: &mut Heap, shape: HashTableShape, entry: i32, key: Tagged) {
        self.as_fixed_array()
            .set(heap, shape.entry_to_index(entry), key);
    }

    #[inline]
    pub fn entry_address(self, shape: HashTableShape, entry: i32, offset: i32) -> Address {
        self.as_fixed_array()
            .element_address(shape.entry_to_index(entry) + offset)
    }

    // Returns whether k is a real key. The hole and undefined are not allowed
    // as keys and can be used to indicate missing or deleted elements.
    #[inline]
    pub fn is_key(heap: &Heap, k: Tagged) -> bool {
        !heap.is_undefined(k) && !heap.is_the_hole(k)
    }

    pub fn has_sufficient_capacity_to_add(self, heap: &Heap, number_of_additional: i32) -> bool {
        HashTableBase::has_sufficient_capacity_to_add_for(
            self.capacity(heap) as i32,
            self.number_of_elements(heap),
            self.number_of_deleted_elements(heap),
            number_of_additional,
        )
    }

    pub fn has_sufficient_capacity_to_add_for(
        capacity: i32,
        number_of_elements: i32,
        number_of_deleted_elements: i32,
        number_of_additional_elements: i32,
    ) -> bool {
        let nof = number_of_elements + number_of_additional_elements;
        // Return true if:
        //   50% is still free after adding number_of_additional_elements elements and
        //   at most 50% of the free elements are deleted elements.
        if (nof < capacity) && (number_of_deleted_elements <= (capacity - nof) / 2) {
            let needed_free = nof / 2;
            if nof + needed_free <= capacity {
                return true;
            }
        }
        false
    }
}

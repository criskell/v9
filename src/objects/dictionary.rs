use crate::execution::isolate::Isolate;
use crate::heap::heap::Heap;
use crate::objects::fixed_array::FixedArray;
use crate::objects::hash_table::{HashTableBase, HashTableShape, K_NOT_FOUND};
use crate::objects::property_details::PropertyDetails;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

object_type!(
    NumberDictionary
);

pub const NUMBER_DICTIONARY_SHAPE: HashTableShape = HashTableShape {
    prefix_size: 1,
    entry_size: 3,
};

impl NumberDictionary {
    pub const K_ENTRY_KEY_INDEX: i32 = 0;
    pub const K_ENTRY_VALUE_INDEX: i32 = 1;
    pub const K_ENTRY_DETAILS_INDEX: i32 = 2;

    pub const K_MAX_NUMBER_KEY_INDEX: i32 = HashTableBase::K_PREFIX_START_INDEX;

    pub const K_REQUIRES_SLOW_ELEMENTS_MASK: i32 = 1;
    pub const K_REQUIRES_SLOW_ELEMENTS_TAG_SIZE: i32 = 1;
    pub const K_REQUIRES_SLOW_ELEMENTS_LIMIT: u32 = (1 << 29) - 1;

    // JSObjects prefer dictionary elements if the dictionary saves this much
    // memory compared to a fast elements backing store.
    pub const K_PREFER_FAST_ELEMENTS_SIZE_FACTOR: u32 = 3;

    #[inline]
    pub fn as_hash_table(self) -> HashTableBase {
        HashTableBase::unchecked_cast(self.object())
    }

    #[inline]
    pub fn as_fixed_array(self) -> FixedArray {
        FixedArray::unchecked_cast(self.object())
    }

    #[inline]
    pub fn capacity(self, heap: &Heap) -> u32 {
        self.as_hash_table().capacity(heap)
    }

    #[inline]
    pub fn number_of_elements(self, heap: &Heap) -> i32 {
        self.as_hash_table().number_of_elements(heap)
    }

    #[inline]
    pub fn hash(key: u32) -> u32 {
        crate::utils::utils::compute_seeded_hash(key, 0)
    }

    #[inline]
    pub fn is_match(heap: &Heap, key: u32, other: Tagged) -> bool {
        match Smi::try_to_int(other) {
            Some(value) => value >= 0 && value as u32 == key,
            None => match crate::objects::heap_number::number_value(heap, other) {
                Some(value) => value == key as f64,
                None => false,
            },
        }
    }

    #[inline]
    pub fn key_at(self, heap: &Heap, entry: i32) -> Tagged {
        self.as_hash_table()
            .key_at(heap, NUMBER_DICTIONARY_SHAPE, entry)
    }

    #[inline]
    pub fn value_at(self, heap: &Heap, entry: i32) -> Tagged {
        self.as_fixed_array().get(
            heap,
            NUMBER_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_VALUE_INDEX,
        )
    }

    #[inline]
    pub fn value_at_put(self, heap: &mut Heap, entry: i32, value: Tagged) {
        self.as_fixed_array().set(
            heap,
            NUMBER_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_VALUE_INDEX,
            value,
        );
    }

    #[inline]
    pub fn details_at(self, heap: &Heap, entry: i32) -> PropertyDetails {
        let raw = self.as_fixed_array().get(
            heap,
            NUMBER_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_DETAILS_INDEX,
        );
        PropertyDetails::from_raw(Smi::to_int(raw) as u32)
    }

    #[inline]
    pub fn details_at_put(self, heap: &mut Heap, entry: i32, details: PropertyDetails) {
        self.as_fixed_array().set(
            heap,
            NUMBER_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_DETAILS_INDEX,
            Smi::from_int(details.raw() as i32),
        );
    }

    #[inline]
    pub fn requires_slow_elements(self, heap: &Heap) -> bool {
        let max_index_object = self
            .as_fixed_array()
            .get(heap, Self::K_MAX_NUMBER_KEY_INDEX);
        match Smi::try_to_int(max_index_object) {
            Some(value) => (value & Self::K_REQUIRES_SLOW_ELEMENTS_MASK) != 0,
            None => false,
        }
    }

    #[inline]
    pub fn set_requires_slow_elements(self, heap: &mut Heap) {
        self.as_fixed_array().set(
            heap,
            Self::K_MAX_NUMBER_KEY_INDEX,
            Smi::from_int(Self::K_REQUIRES_SLOW_ELEMENTS_MASK),
        );
    }

    // Get the value of the max number key that has been added to this
    // dictionary. max_number_key can only be called if requires_slow_elements
    // returns false.
    #[inline]
    pub fn max_number_key(self, heap: &Heap) -> u32 {
        debug_assert!(!self.requires_slow_elements(heap));
        let max_index_object = self
            .as_fixed_array()
            .get(heap, Self::K_MAX_NUMBER_KEY_INDEX);
        match Smi::try_to_int(max_index_object) {
            Some(value) => (value as u32) >> Self::K_REQUIRES_SLOW_ELEMENTS_TAG_SIZE,
            None => 0,
        }
    }

    pub fn update_max_number_key(self, heap: &mut Heap, key: u32) {
        // If the dictionary requires slow elements an element has already
        // been added at a high index.
        if self.requires_slow_elements(heap) {
            return;
        }
        if key > Self::K_REQUIRES_SLOW_ELEMENTS_LIMIT {
            self.set_requires_slow_elements(heap);
            return;
        }
        // Update max key value.
        if key > self.max_number_key(heap) {
            self.as_fixed_array().set(
                heap,
                Self::K_MAX_NUMBER_KEY_INDEX,
                Smi::from_int((key << Self::K_REQUIRES_SLOW_ELEMENTS_TAG_SIZE) as i32),
            );
        }
    }

    // Find entry for key otherwise return kNotFound.
    pub fn find_entry(self, heap: &Heap, key: u32) -> i32 {
        let capacity = self.capacity(heap);
        if capacity == 0 {
            return K_NOT_FOUND;
        }
        let hash = Self::hash(key);
        let mut count = 1;
        let mut entry = HashTableBase::first_probe(hash, capacity);
        loop {
            let element = self.key_at(heap, entry as i32);
            if heap.is_undefined(element) {
                return K_NOT_FOUND;
            }
            if !heap.is_the_hole(element) && Self::is_match(heap, key, element) {
                return entry as i32;
            }
            entry = HashTableBase::next_probe(entry, count, capacity);
            count += 1;
        }
    }

    pub fn clear_entry(self, heap: &mut Heap, entry: i32) {
        let the_hole = heap.the_hole_value;
        self.as_hash_table()
            .set_key_at(heap, NUMBER_DICTIONARY_SHAPE, entry, the_hole);
        self.value_at_put(heap, entry, the_hole);
        self.details_at_put(heap, entry, PropertyDetails::from_raw(0));
    }

    pub fn find_insertion_entry(self, heap: &Heap, hash: u32) -> i32 {
        let capacity = self.capacity(heap);
        let mut count = 1;
        let mut entry = HashTableBase::first_probe(hash, capacity);
        loop {
            let element = self.key_at(heap, entry as i32);
            if !HashTableBase::is_key(heap, element) {
                return entry as i32;
            }
            entry = HashTableBase::next_probe(entry, count, capacity);
            count += 1;
        }
    }
}

impl Isolate {
    pub fn new_number_dictionary(&mut self, at_least_space_for: u32) -> NumberDictionary {
        let capacity = HashTableBase::compute_capacity(at_least_space_for);
        let length = NUMBER_DICTIONARY_SHAPE.size_for(capacity);
        let map = self
            .roots
            .number_dictionary_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            FixedArray::size_for(length) as usize,
            crate::heap::heap::AllocationType::Young,
        );
        self.heap.set_map(object, map);
        let array = FixedArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        let undefined = self.undefined_value();
        for index in 0..length {
            array.set(&mut self.heap, index, undefined);
        }
        let dictionary = NumberDictionary::unchecked_cast(object);
        dictionary
            .as_hash_table()
            .set_number_of_elements(&mut self.heap, 0);
        dictionary
            .as_hash_table()
            .set_number_of_deleted_elements(&mut self.heap, 0);
        dictionary
            .as_hash_table()
            .set_capacity(&mut self.heap, capacity);
        dictionary.as_fixed_array().set(
            &mut self.heap,
            NumberDictionary::K_MAX_NUMBER_KEY_INDEX,
            Smi::from_int(0),
        );
        dictionary
    }

    pub fn dictionary_add(
        &mut self,
        dictionary: NumberDictionary,
        key: u32,
        value: Tagged,
        details: PropertyDetails,
    ) -> NumberDictionary {
        debug_assert_eq!(dictionary.find_entry(&self.heap, key), K_NOT_FOUND);

        // Check whether the dictionary should be extended.
        let scope = self.handle_scope();
        let value_handle = self.create_handle(value);
        let dictionary = self.dictionary_ensure_capacity(dictionary, 1);
        let dictionary_handle = self.create_object_handle(dictionary.object());

        let hash = NumberDictionary::hash(key);
        let entry = dictionary.find_insertion_entry(&self.heap, hash);
        let key_object = self.new_number(key as f64);
        let dictionary = NumberDictionary::unchecked_cast(dictionary_handle.object(self));
        let value = value_handle.get(self);
        dictionary.as_hash_table().set_key_at(
            &mut self.heap,
            NUMBER_DICTIONARY_SHAPE,
            entry,
            key_object,
        );
        dictionary.value_at_put(&mut self.heap, entry, value);
        dictionary.details_at_put(&mut self.heap, entry, details);
        dictionary.as_hash_table().element_added(&mut self.heap);
        dictionary.update_max_number_key(&mut self.heap, key);
        self.close_handle_scope(scope);
        dictionary
    }

    pub fn dictionary_set(
        &mut self,
        dictionary: NumberDictionary,
        key: u32,
        value: Tagged,
        details: PropertyDetails,
    ) -> NumberDictionary {
        let entry = dictionary.find_entry(&self.heap, key);
        if entry == K_NOT_FOUND {
            return self.dictionary_add(dictionary, key, value, details);
        }
        dictionary.value_at_put(&mut self.heap, entry, value);
        dictionary.details_at_put(&mut self.heap, entry, details);
        dictionary
    }

    fn dictionary_ensure_capacity(
        &mut self,
        dictionary: NumberDictionary,
        number_of_additional: u32,
    ) -> NumberDictionary {
        if dictionary
            .as_hash_table()
            .has_sufficient_capacity_to_add(&self.heap, number_of_additional as i32)
        {
            return dictionary;
        }
        let at_least_space_for =
            dictionary.number_of_elements(&self.heap) as u32 + number_of_additional;
        self.dictionary_rehash(dictionary, at_least_space_for)
    }

    fn dictionary_rehash(
        &mut self,
        dictionary: NumberDictionary,
        at_least_space_for: u32,
    ) -> NumberDictionary {
        let old_capacity = dictionary.capacity(&self.heap);
        self.handles.push(dictionary.tagged());
        let fresh = self.new_number_dictionary(at_least_space_for);
        let dictionary = NumberDictionary::unchecked_cast(
            self.handles.pop().unwrap().to_strong_heap_object().unwrap(),
        );

        for entry in 0..old_capacity as i32 {
            let key = dictionary.key_at(&self.heap, entry);
            if !HashTableBase::is_key(&self.heap, key) {
                continue;
            }
            let Some(index) = self.to_array_index(key) else {
                continue;
            };
            let value = dictionary.value_at(&self.heap, entry);
            let details = dictionary.details_at(&self.heap, entry);
            let hash = NumberDictionary::hash(index);
            let target = fresh.find_insertion_entry(&self.heap, hash);
            fresh
                .as_hash_table()
                .set_key_at(&mut self.heap, NUMBER_DICTIONARY_SHAPE, target, key);
            fresh.value_at_put(&mut self.heap, target, value);
            fresh.details_at_put(&mut self.heap, target, details);
            fresh.as_hash_table().element_added(&mut self.heap);
            fresh.update_max_number_key(&mut self.heap, index);
        }
        fresh
    }
}

object_type!(
    NameDictionary
);

pub const NAME_DICTIONARY_SHAPE: HashTableShape = HashTableShape {
    prefix_size: 3,
    entry_size: 3,
};

impl NameDictionary {
    pub const K_ENTRY_KEY_INDEX: i32 = 0;
    pub const K_ENTRY_VALUE_INDEX: i32 = 1;
    pub const K_ENTRY_DETAILS_INDEX: i32 = 2;

    pub const K_NEXT_ENUMERATION_INDEX_INDEX: i32 = HashTableBase::K_PREFIX_START_INDEX;
    pub const K_OBJECT_HASH_INDEX: i32 = Self::K_NEXT_ENUMERATION_INDEX_INDEX + 1;
    pub const K_FLAGS_INDEX: i32 = Self::K_OBJECT_HASH_INDEX + 1;

    pub const K_INITIAL_CAPACITY: u32 = 2;

    #[inline]
    pub fn as_hash_table(self) -> HashTableBase {
        HashTableBase::unchecked_cast(self.object())
    }

    #[inline]
    pub fn as_fixed_array(self) -> FixedArray {
        FixedArray::unchecked_cast(self.object())
    }

    #[inline]
    pub fn capacity(self, heap: &Heap) -> u32 {
        self.as_hash_table().capacity(heap)
    }

    #[inline]
    pub fn number_of_elements(self, heap: &Heap) -> i32 {
        self.as_hash_table().number_of_elements(heap)
    }

    #[inline]
    pub fn next_enumeration_index(self, heap: &Heap) -> i32 {
        Smi::to_int(
            self.as_fixed_array()
                .get(heap, Self::K_NEXT_ENUMERATION_INDEX_INDEX),
        )
    }

    #[inline]
    pub fn set_next_enumeration_index(self, heap: &mut Heap, index: i32) {
        self.as_fixed_array().set(
            heap,
            Self::K_NEXT_ENUMERATION_INDEX_INDEX,
            Smi::from_int(index),
        );
    }

    #[inline]
    pub fn hash(heap: &Heap, key: Tagged) -> u32 {
        crate::objects::name::Name::unchecked_cast(key.to_strong_heap_object().expect("a Name key"))
            .raw_hash_field_hash(heap)
    }

    #[inline]
    pub fn is_match(key: Tagged, other: Tagged) -> bool {
        key == other
    }

    #[inline]
    pub fn key_at(self, heap: &Heap, entry: i32) -> Tagged {
        self.as_hash_table()
            .key_at(heap, NAME_DICTIONARY_SHAPE, entry)
    }

    #[inline]
    pub fn value_at(self, heap: &Heap, entry: i32) -> Tagged {
        self.as_fixed_array().get(
            heap,
            NAME_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_VALUE_INDEX,
        )
    }

    #[inline]
    pub fn value_address(self, entry: i32) -> crate::objects::tagged::Address {
        self.as_fixed_array().element_address(
            NAME_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_VALUE_INDEX,
        )
    }

    #[inline]
    pub fn value_at_put(self, heap: &mut Heap, entry: i32, value: Tagged) {
        self.as_fixed_array().set(
            heap,
            NAME_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_VALUE_INDEX,
            value,
        );
    }

    #[inline]
    pub fn details_at(self, heap: &Heap, entry: i32) -> PropertyDetails {
        let raw = self.as_fixed_array().get(
            heap,
            NAME_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_DETAILS_INDEX,
        );
        PropertyDetails::from_raw(Smi::to_int(raw) as u32)
    }

    #[inline]
    pub fn details_at_put(self, heap: &mut Heap, entry: i32, details: PropertyDetails) {
        self.as_fixed_array().set(
            heap,
            NAME_DICTIONARY_SHAPE.entry_to_index(entry) + Self::K_ENTRY_DETAILS_INDEX,
            Smi::from_int(details.raw() as i32),
        );
    }

    pub fn clear_entry(self, heap: &mut Heap, entry: i32) {
        let the_hole = heap.the_hole_value;
        self.as_hash_table()
            .set_key_at(heap, NAME_DICTIONARY_SHAPE, entry, the_hole);
        self.value_at_put(heap, entry, the_hole);
        self.details_at_put(heap, entry, PropertyDetails::from_raw(0));
    }

    pub fn find_entry(self, heap: &Heap, key: Tagged) -> i32 {
        let capacity = self.capacity(heap);
        if capacity == 0 {
            return K_NOT_FOUND;
        }
        let hash = Self::hash(heap, key);
        let mut count = 1;
        let mut entry = HashTableBase::first_probe(hash, capacity);
        loop {
            let element = self.key_at(heap, entry as i32);
            if heap.is_undefined(element) {
                return K_NOT_FOUND;
            }
            if Self::is_match(key, element) {
                return entry as i32;
            }
            entry = HashTableBase::next_probe(entry, count, capacity);
            count += 1;
        }
    }

    pub fn find_insertion_entry(self, heap: &Heap, hash: u32) -> i32 {
        let capacity = self.capacity(heap);
        let mut count = 1;
        let mut entry = HashTableBase::first_probe(hash, capacity);
        loop {
            let element = self.key_at(heap, entry as i32);
            if !HashTableBase::is_key(heap, element) {
                return entry as i32;
            }
            entry = HashTableBase::next_probe(entry, count, capacity);
            count += 1;
        }
    }

    pub fn iterate_entries(self, heap: &Heap) -> Vec<i32> {
        (0..self.capacity(heap) as i32)
            .filter(|entry| HashTableBase::is_key(heap, self.key_at(heap, *entry)))
            .collect()
    }
}

impl Isolate {
    pub fn new_name_dictionary(&mut self, at_least_space_for: u32) -> NameDictionary {
        let capacity = HashTableBase::compute_capacity(at_least_space_for);
        let length = NAME_DICTIONARY_SHAPE.size_for(capacity);
        let map = self
            .roots
            .name_dictionary_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            FixedArray::size_for(length) as usize,
            crate::heap::heap::AllocationType::Young,
        );
        self.heap.set_map(object, map);
        let array = FixedArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        let undefined = self.undefined_value();
        for index in 0..length {
            array.set(&mut self.heap, index, undefined);
        }
        let dictionary = NameDictionary::unchecked_cast(object);
        let table = dictionary.as_hash_table();
        table.set_number_of_elements(&mut self.heap, 0);
        table.set_number_of_deleted_elements(&mut self.heap, 0);
        table.set_capacity(&mut self.heap, capacity);
        dictionary.set_next_enumeration_index(
            &mut self.heap,
            crate::objects::property_details::K_FIRST_INDEX,
        );
        dictionary.as_fixed_array().set(
            &mut self.heap,
            NameDictionary::K_OBJECT_HASH_INDEX,
            Smi::from_int(0),
        );
        dictionary.as_fixed_array().set(
            &mut self.heap,
            NameDictionary::K_FLAGS_INDEX,
            Smi::from_int(0),
        );
        dictionary
    }

    pub fn name_dictionary_add(
        &mut self,
        dictionary: NameDictionary,
        key: Tagged,
        value: Tagged,
        details: PropertyDetails,
    ) -> NameDictionary {
        debug_assert_eq!(dictionary.find_entry(&self.heap, key), K_NOT_FOUND);

        let scope = self.handle_scope();
        let key_handle = self.create_handle(key);
        let value_handle = self.create_handle(value);
        let dictionary = self.name_dictionary_ensure_capacity(dictionary, 1);
        let key = key_handle.get(self);
        let value = value_handle.get(self);
        self.close_handle_scope(scope);

        let index = dictionary.next_enumeration_index(&self.heap);
        let details = details.set_dictionary_index(index as u32);
        dictionary.set_next_enumeration_index(&mut self.heap, index + 1);

        let hash = NameDictionary::hash(&self.heap, key);
        let entry = dictionary.find_insertion_entry(&self.heap, hash);
        dictionary
            .as_hash_table()
            .set_key_at(&mut self.heap, NAME_DICTIONARY_SHAPE, entry, key);
        dictionary.value_at_put(&mut self.heap, entry, value);
        dictionary.details_at_put(&mut self.heap, entry, details);
        dictionary.as_hash_table().element_added(&mut self.heap);
        dictionary
    }

    pub fn name_dictionary_set(
        &mut self,
        dictionary: NameDictionary,
        key: Tagged,
        value: Tagged,
        details: PropertyDetails,
    ) -> NameDictionary {
        let entry = dictionary.find_entry(&self.heap, key);
        if entry == K_NOT_FOUND {
            return self.name_dictionary_add(dictionary, key, value, details);
        }
        dictionary.value_at_put(&mut self.heap, entry, value);
        dictionary
    }

    pub fn name_dictionary_delete(
        &mut self,
        dictionary: NameDictionary,
        entry: i32,
    ) -> NameDictionary {
        dictionary.clear_entry(&mut self.heap, entry);
        dictionary.as_hash_table().element_removed(&mut self.heap);
        dictionary
    }

    fn name_dictionary_ensure_capacity(
        &mut self,
        dictionary: NameDictionary,
        number_of_additional: i32,
    ) -> NameDictionary {
        if dictionary
            .as_hash_table()
            .has_sufficient_capacity_to_add(&self.heap, number_of_additional)
        {
            return dictionary;
        }
        let at_least_space_for = dictionary.number_of_elements(&self.heap) + number_of_additional;
        self.name_dictionary_rehash(dictionary, at_least_space_for as u32)
    }

    fn name_dictionary_rehash(
        &mut self,
        dictionary: NameDictionary,
        at_least_space_for: u32,
    ) -> NameDictionary {
        let old_capacity = dictionary.capacity(&self.heap);
        self.handles.push(dictionary.tagged());
        let fresh = self.new_name_dictionary(at_least_space_for);
        let dictionary = NameDictionary::unchecked_cast(
            self.handles.pop().unwrap().to_strong_heap_object().unwrap(),
        );

        for entry in 0..old_capacity as i32 {
            let key = dictionary.key_at(&self.heap, entry);
            if !HashTableBase::is_key(&self.heap, key) {
                continue;
            }
            let value = dictionary.value_at(&self.heap, entry);
            let details = dictionary.details_at(&self.heap, entry);
            let hash = NameDictionary::hash(&self.heap, key);
            let target = fresh.find_insertion_entry(&self.heap, hash);
            fresh
                .as_hash_table()
                .set_key_at(&mut self.heap, NAME_DICTIONARY_SHAPE, target, key);
            fresh.value_at_put(&mut self.heap, target, value);
            fresh.details_at_put(&mut self.heap, target, details);
            fresh.as_hash_table().element_added(&mut self.heap);
        }
        let index = dictionary.next_enumeration_index(&self.heap);
        fresh.set_next_enumeration_index(&mut self.heap, index);
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::property_details::{default_data_field, Representation};

    fn details() -> PropertyDetails {
        default_data_field(0, Representation::Tagged)
    }

    #[test]
    fn a_key_can_be_found_after_it_is_added() {
        let mut isolate = Isolate::new();
        let mut dictionary = isolate.new_number_dictionary(4);
        for key in [0u32, 1, 7, 1000, 1_000_000] {
            dictionary =
                isolate.dictionary_add(dictionary, key, Smi::from_int(key as i32 * 2), details());
        }
        for key in [0u32, 1, 7, 1000, 1_000_000] {
            let entry = dictionary.find_entry(&isolate.heap, key);
            assert_ne!(entry, K_NOT_FOUND, "key {key} should be present");
            assert_eq!(
                Smi::to_int(dictionary.value_at(&isolate.heap, entry)),
                key as i32 * 2
            );
        }
        assert_eq!(dictionary.find_entry(&isolate.heap, 5), K_NOT_FOUND);
        assert_eq!(dictionary.number_of_elements(&isolate.heap), 5);
    }

    #[test]
    fn the_table_grows_and_keeps_every_key() {
        let mut isolate = Isolate::new();
        let mut dictionary = isolate.new_number_dictionary(4);
        let initial_capacity = dictionary.capacity(&isolate.heap);
        for key in 0..200u32 {
            dictionary =
                isolate.dictionary_add(dictionary, key * 7, Smi::from_int(key as i32), details());
        }
        assert!(dictionary.capacity(&isolate.heap) > initial_capacity);
        assert_eq!(dictionary.number_of_elements(&isolate.heap), 200);
        for key in 0..200u32 {
            let entry = dictionary.find_entry(&isolate.heap, key * 7);
            assert_ne!(entry, K_NOT_FOUND, "key {} lost in a rehash", key * 7);
            assert_eq!(
                Smi::to_int(dictionary.value_at(&isolate.heap, entry)),
                key as i32
            );
        }
    }

    #[test]
    fn setting_an_existing_key_replaces_it_rather_than_growing() {
        let mut isolate = Isolate::new();
        let mut dictionary = isolate.new_number_dictionary(4);
        dictionary = isolate.dictionary_add(dictionary, 3, Smi::from_int(1), details());
        dictionary = isolate.dictionary_set(dictionary, 3, Smi::from_int(2), details());
        assert_eq!(dictionary.number_of_elements(&isolate.heap), 1);
        let entry = dictionary.find_entry(&isolate.heap, 3);
        assert_eq!(Smi::to_int(dictionary.value_at(&isolate.heap, entry)), 2);
    }

    #[test]
    fn a_very_high_index_forces_slow_elements_forever() {
        let mut isolate = Isolate::new();
        let mut dictionary = isolate.new_number_dictionary(4);
        dictionary = isolate.dictionary_add(dictionary, 5, Smi::from_int(1), details());
        assert!(!dictionary.requires_slow_elements(&isolate.heap));
        assert_eq!(dictionary.max_number_key(&isolate.heap), 5);

        dictionary = isolate.dictionary_add(
            dictionary,
            NumberDictionary::K_REQUIRES_SLOW_ELEMENTS_LIMIT + 1,
            Smi::from_int(2),
            details(),
        );
        assert!(dictionary.requires_slow_elements(&isolate.heap));

        dictionary = isolate.dictionary_add(dictionary, 6, Smi::from_int(3), details());
        assert!(
            dictionary.requires_slow_elements(&isolate.heap),
            "a later low index must not undo it"
        );
    }
}

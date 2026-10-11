use std::collections::HashMap;

use crate::execution::isolate::Isolate;
use crate::heap::heap::AllocationType;
use crate::objects::name::K_EMPTY_HASH_FIELD;
use crate::objects::string::String as VxString;
use crate::objects::tagged::Tagged;

/// `StringTable`
#[derive(Default)]
pub struct StringTable {
    entries: HashMap<std::string::String, Tagged>,
}

impl StringTable {
    pub fn new() -> StringTable {
        StringTable::default()
    }

    /// `StringTable::NumberOfElements`
    pub fn number_of_elements(&self) -> usize {
        self.entries.len()
    }

    pub fn contains(&self, s: &str) -> bool {
        self.entries.contains_key(s)
    }

    pub fn get(&self, s: &str) -> Option<Tagged> {
        self.entries.get(s).copied()
    }

    pub fn insert(&mut self, s: &str, string: Tagged) {
        self.entries.insert(s.to_owned(), string);
    }

    pub fn iterate(&mut self, visit: &mut impl FnMut(&mut Tagged)) {
        for value in self.entries.values_mut() {
            visit(value);
        }
    }
}

impl Isolate {
    pub fn internalize_string(&mut self, s: &str) -> VxString {
        if let Some(existing) = self.string_table.get(s) {
            return VxString::unchecked_cast(existing.to_strong_heap_object().unwrap());
        }
        let one_byte = VxString::is_one_byte_content(s);
        let map = if one_byte {
            self.roots.internalized_one_byte_string_map
        } else {
            self.roots.internalized_two_byte_string_map
        };
        let string = self.allocate_string_raw(s, map, one_byte, AllocationType::ReadOnly);
        string.as_name().hash(&mut self.heap);
        self.string_table.insert(s, string.tagged());
        string
    }

    pub fn is_internalized(&self, s: &str) -> bool {
        self.string_table.contains(s)
    }

    /// `StringTable::NumberOfElements`
    pub fn string_table_size(&self) -> usize {
        self.string_table.number_of_elements()
    }

    pub fn iterate_string_table(&mut self, visit: &mut impl FnMut(&mut Tagged)) {
        self.string_table.iterate(visit);
    }

    pub(crate) fn allocate_string_raw(
        &mut self,
        s: &str,
        map: Tagged,
        one_byte: bool,
        allocation: AllocationType,
    ) -> VxString {
        let length = VxString::utf16_length(s);
        let size = if one_byte {
            VxString::size_for_one_byte(length)
        } else {
            VxString::size_for_two_byte(length)
        };
        let object = self.allocate_raw_with_retry_or_fail(size as usize, allocation);
        self.heap
            .set_map(object, map.to_strong_heap_object().unwrap());
        let string = VxString::unchecked_cast(object);
        string.set_length(&mut self.heap, length);
        string
            .as_name()
            .set_raw_hash_field(&mut self.heap, K_EMPTY_HASH_FIELD);
        string.write_chars(&mut self.heap, s);
        string
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::instance_type;

    #[test]
    fn internalizing_the_same_text_twice_yields_the_same_object() {
        let mut isolate = Isolate::new();
        let a = isolate.internalize_string("length");
        let b = isolate.internalize_string("length");
        let c = isolate.internalize_string("size");

        assert_eq!(
            a, b,
            "that identity is what makes name lookup a pointer compare"
        );
        assert_ne!(a, c);
        assert_eq!(a.to_rust_string(&isolate.heap), "length");
        assert!(a.is_internalized(&isolate.heap));
    }

    #[test]
    fn internalized_strings_pick_their_representation() {
        let mut isolate = Isolate::new();
        let latin1 = isolate.internalize_string("olá");
        let wide = isolate.internalize_string("☃");
        assert!(latin1.is_one_byte(&isolate.heap));
        assert!(!wide.is_one_byte(&isolate.heap));
        assert_eq!(latin1.to_rust_string(&isolate.heap), "olá");
        assert_eq!(wide.to_rust_string(&isolate.heap), "☃");
        assert!(instance_type::is_internalized_string(
            isolate.heap.instance_type(wide.object())
        ));
    }

    #[test]
    fn the_empty_string_is_a_root_and_is_internalized() {
        let mut isolate = Isolate::new();
        let empty = isolate.internalize_string("");
        assert_eq!(empty.tagged(), isolate.roots.empty_string);
        assert_eq!(empty.length(&isolate.heap), 0);
    }
}

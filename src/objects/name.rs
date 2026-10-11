use crate::heap::heap::Heap;
use crate::objects::string::String as VxString;
use crate::objects::tagged::HeapObject;

object_type!(Name);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HashFieldType {
    Hash = 0b10,
    IntegerIndex = 0b00,
    ForwardingIndex = 0b01,
    Empty = 0b11,
}

const HASH_FIELD_TYPE_BITS: u32 = 2;
const HASH_FIELD_TYPE_MASK: u32 = (1 << HASH_FIELD_TYPE_BITS) - 1;
const HASH_SHIFT: u32 = HASH_FIELD_TYPE_BITS;

pub const K_HASH_NOT_COMPUTED_MASK: u32 = 1;

pub const K_EMPTY_HASH_FIELD: u32 = HashFieldType::Empty as u32;

pub const K_MAX_HASH: u32 = u32::MAX >> HASH_SHIFT;

impl Name {
    pub const K_RAW_HASH_FIELD_OFFSET: i32 = 8;

    #[inline]
    pub fn raw_hash_field(self, heap: &Heap) -> u32 {
        heap.read_u32(self.field(Self::K_RAW_HASH_FIELD_OFFSET))
    }

    #[inline]
    pub fn set_raw_hash_field(self, heap: &mut Heap, value: u32) {
        heap.write_u32(self.field(Self::K_RAW_HASH_FIELD_OFFSET), value);
    }

    #[inline]
    pub fn is_hash_field_computed(raw_hash_field: u32) -> bool {
        raw_hash_field & K_HASH_NOT_COMPUTED_MASK == 0
    }

    #[inline]
    pub fn hash_field_type(raw_hash_field: u32) -> HashFieldType {
        match raw_hash_field & HASH_FIELD_TYPE_MASK {
            0b10 => HashFieldType::Hash,
            0b00 => HashFieldType::IntegerIndex,
            0b01 => HashFieldType::ForwardingIndex,
            _ => HashFieldType::Empty,
        }
    }

    pub fn hash(self, heap: &mut Heap) -> u32 {
        let raw = self.raw_hash_field(heap);
        if Name::is_hash_field_computed(raw) {
            return raw >> HASH_SHIFT;
        }
        let string = VxString::unchecked_cast(self.object());
        let hash = crate::strings::string_hasher::StringHasher::hash_sequential_string(
            &string.to_rust_string(heap),
        );
        self.set_raw_hash_field(heap, (hash << HASH_SHIFT) | (HashFieldType::Hash as u32));
        hash
    }

    #[inline]
    pub fn raw_hash_field_hash(self, heap: &Heap) -> u32 {
        let raw = self.raw_hash_field(heap);
        debug_assert!(Name::is_hash_field_computed(raw));
        raw >> HASH_SHIFT
    }

    #[inline]
    pub fn from_heap_object(object: HeapObject) -> Name {
        Name::unchecked_cast(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::AllocationType;
    use crate::objects::instance_type::{self, SEQ_ONE_BYTE_STRING_TYPE};
    use crate::objects::map::Map;
    use crate::objects::string::String as VxString;

    struct World {
        heap: Heap,
        one_byte_map: Map,
    }

    impl World {
        fn new() -> World {
            let mut heap = Heap::new();
            let meta = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
            heap.set_map(meta, meta);
            let meta_map = Map::unchecked_cast(meta);
            meta_map.set_instance_type(&mut heap, instance_type::MAP_TYPE);
            meta_map.set_instance_size(&mut heap, Map::K_SIZE);

            let object = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
            heap.set_map(object, meta);
            let one_byte_map = Map::unchecked_cast(object);
            one_byte_map.set_instance_type(&mut heap, SEQ_ONE_BYTE_STRING_TYPE);
            one_byte_map.set_instance_size(&mut heap, 0);

            World { heap, one_byte_map }
        }

        fn new_string(&mut self, s: &str, map: Map) -> VxString {
            let length = VxString::utf16_length(s);
            let size = VxString::size_for_one_byte(length);
            let object = self
                .heap
                .allocate_raw_or_fail(size as usize, AllocationType::Young);
            self.heap.set_map(object, map.object());
            let string = VxString::unchecked_cast(object);
            string.set_length(&mut self.heap, length);
            string
                .as_name()
                .set_raw_hash_field(&mut self.heap, K_EMPTY_HASH_FIELD);
            string.write_chars(&mut self.heap, s);
            string
        }
    }

    #[test]
    fn the_hash_is_computed_once_and_cached() {
        let mut w = World::new();
        let map = w.one_byte_map;
        let s = w.new_string("length", map);
        let name = s.as_name();

        let raw = name.raw_hash_field(&w.heap);
        assert_eq!(raw, K_EMPTY_HASH_FIELD);
        assert!(!Name::is_hash_field_computed(raw));
        assert_eq!(Name::hash_field_type(raw), HashFieldType::Empty);

        let hash = name.hash(&mut w.heap);
        assert_ne!(hash, 0, "zero is reserved for 'no hash'");
        assert!(Name::is_hash_field_computed(name.raw_hash_field(&w.heap)));
        assert_eq!(
            Name::hash_field_type(name.raw_hash_field(&w.heap)),
            HashFieldType::Hash
        );
        assert_eq!(name.hash(&mut w.heap), hash, "second read must be cached");
    }

    #[test]
    fn the_hash_field_distinguishes_its_four_states() {
        assert_eq!(
            Name::hash_field_type(K_EMPTY_HASH_FIELD),
            HashFieldType::Empty
        );
        assert_eq!(Name::hash_field_type(0b10), HashFieldType::Hash);
        assert_eq!(Name::hash_field_type(0b00), HashFieldType::IntegerIndex);
        assert_eq!(Name::hash_field_type(0b01), HashFieldType::ForwardingIndex);

        assert!(Name::is_hash_field_computed(0b10));
        assert!(!Name::is_hash_field_computed(K_EMPTY_HASH_FIELD));
        assert!(!Name::is_hash_field_computed(0b01));
    }

    #[test]
    fn the_hash_fits_below_the_type_bits() {
        assert_eq!(K_MAX_HASH, u32::MAX >> 2);
        let hash = crate::strings::string_hasher::StringHasher::hash_sequential_string("length");
        assert!(hash <= K_MAX_HASH);
        let field = (hash << HASH_SHIFT) | (HashFieldType::Hash as u32);
        assert_eq!(Name::hash_field_type(field), HashFieldType::Hash);
        assert_eq!(field >> HASH_SHIFT, hash);
    }
}

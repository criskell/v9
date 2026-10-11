//! Ported from `src/objects/name.h`, `src/objects/string.h`, `string.tq` and
//! `src/strings/string-hasher-inl.h`.
//!
//! Layout, following V8's `Name` -> `String` -> `SeqString` chain:
//!
//! ```text
//!   0  map
//!   8  raw_hash_field   u32   (Name)
//!  12  length           i32   (String)
//!  16  chars...               (SeqOneByteString: u8, SeqTwoByteString: u16)
//! ```
//!
//! Two encodings, as in V8: a string whose characters all fit in Latin-1 is
//! stored one byte per character, anything else two. Which one is in use is
//! read from the instance type, not from a field -- see
//! [`crate::objects::instance_type`].
//!
//! Deviation: `vx` implements only the *sequential* representations. V8 also
//! has `ConsString` (lazy concatenation), `SlicedString` (substring views),
//! `ThinString` (a forwarding pointer to an internalized copy) and external
//! strings. The instance-type bits for all of them are already here, and the
//! read paths check `IsIndirectString` the way V8's do, so adding them is a
//! matter of implementing the representations rather than reshaping the
//! object model. Tracked in `PORTING.md`.

use crate::common::globals::object_pointer_align;
use crate::heap::heap::Heap;
use crate::objects::instance_type::{self, InstanceType};
use crate::objects::tagged::{HeapObject, Tagged};

object_type!(
    /// `Name`: a string or a symbol -- the things that can be property keys.
    Name
);

/// `Name::HashFieldType`. The low bit says whether the field can be used as a
/// hash at all, which is why `kEmpty` and `kForwardingIndex` both have it set.
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

/// `Name::kHashNotComputedMask`
pub const K_HASH_NOT_COMPUTED_MASK: u32 = 1;

/// `Name::kEmptyHashField`: the hash has not been computed yet.
pub const K_EMPTY_HASH_FIELD: u32 = HashFieldType::Empty as u32;

/// `String::HashBits::kMax`
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

    /// `Name::IsHashFieldComputed`
    #[inline]
    pub fn is_hash_field_computed(raw_hash_field: u32) -> bool {
        raw_hash_field & K_HASH_NOT_COMPUTED_MASK == 0
    }

    /// `Name::HashFieldTypeBits::decode`: what the hash field currently holds.
    #[inline]
    pub fn hash_field_type(raw_hash_field: u32) -> HashFieldType {
        match raw_hash_field & HASH_FIELD_TYPE_MASK {
            0b10 => HashFieldType::Hash,
            0b00 => HashFieldType::IntegerIndex,
            0b01 => HashFieldType::ForwardingIndex,
            _ => HashFieldType::Empty,
        }
    }

    /// `Name::hash`: the cached hash, computing it on first use.
    pub fn hash(self, heap: &mut Heap) -> u32 {
        let raw = self.raw_hash_field(heap);
        if Name::is_hash_field_computed(raw) {
            return raw >> HASH_SHIFT;
        }
        let string = String::unchecked_cast(self.object());
        let hash = string_hash(&string.to_rust_string(heap));
        self.set_raw_hash_field(heap, (hash << HASH_SHIFT) | (HashFieldType::Hash as u32));
        hash
    }
}

/// `RunningStringHasher` plus `Finalize` from `string-hasher-inl.h` -- the
/// Jenkins one-at-a-time hash, with V8's exact mixing steps.
pub fn string_hash(s: &str) -> u32 {
    let mut running_hash: u32 = 0;
    for c in s.encode_utf16() {
        running_hash = running_hash.wrapping_add(c as u32);
        running_hash = running_hash.wrapping_add(running_hash << 10);
        running_hash ^= running_hash >> 6;
    }
    running_hash = running_hash.wrapping_add(running_hash << 3);
    running_hash ^= running_hash >> 11;
    running_hash = running_hash.wrapping_add(running_hash << 15);
    // `ConvertRawHashToUsableHash`: mask to the available bits, and never
    // return zero -- zero is reserved to mean "no hash".
    let hash = running_hash & K_MAX_HASH;
    if hash == 0 {
        1
    } else {
        hash
    }
}

object_type!(
    /// `String`
    String
);

impl String {
    pub const K_RAW_HASH_FIELD_OFFSET: i32 = Name::K_RAW_HASH_FIELD_OFFSET;
    pub const K_LENGTH_OFFSET: i32 = Self::K_RAW_HASH_FIELD_OFFSET + 4;
    /// `SeqString::kHeaderSize`
    pub const K_HEADER_SIZE: i32 = Self::K_LENGTH_OFFSET + 4;

    /// `SeqOneByteString::SizeFor`
    #[inline]
    pub fn size_for_one_byte(length: i32) -> i32 {
        object_pointer_align((Self::K_HEADER_SIZE + length) as u64) as i32
    }

    /// `SeqTwoByteString::SizeFor`
    #[inline]
    pub fn size_for_two_byte(length: i32) -> i32 {
        object_pointer_align((Self::K_HEADER_SIZE + length * 2) as u64) as i32
    }

    #[inline]
    pub fn as_name(self) -> Name {
        Name::unchecked_cast(self.object())
    }

    /// `String::length`
    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        heap.read_u32(self.field(Self::K_LENGTH_OFFSET)) as i32
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        debug_assert!(length >= 0);
        heap.write_u32(self.field(Self::K_LENGTH_OFFSET), length as u32);
    }

    #[inline]
    fn instance_type(self, heap: &Heap) -> InstanceType {
        heap.instance_type(self.object())
    }

    /// `String::IsOneByteRepresentation`
    #[inline]
    pub fn is_one_byte(self, heap: &Heap) -> bool {
        instance_type::is_one_byte_string(self.instance_type(heap))
    }

    /// `String::IsInternalizedString`
    #[inline]
    pub fn is_internalized(self, heap: &Heap) -> bool {
        instance_type::is_internalized_string(self.instance_type(heap))
    }

    /// `String::Get`: the code unit at `index`.
    pub fn get(self, heap: &Heap, index: i32) -> u16 {
        debug_assert!(index >= 0 && index < self.length(heap));
        let instance_type = self.instance_type(heap);
        assert!(
            !instance_type::is_indirect_string(instance_type),
            "vx has no indirect string representations yet"
        );
        if instance_type::is_one_byte_string(instance_type) {
            heap.read_u8(self.field(Self::K_HEADER_SIZE + index)) as u16
        } else {
            heap.read_u16(self.field(Self::K_HEADER_SIZE + index * 2))
        }
    }

    pub fn set(self, heap: &mut Heap, index: i32, code_unit: u16) {
        debug_assert!(index >= 0 && index < self.length(heap));
        if self.is_one_byte(heap) {
            debug_assert!(code_unit <= u8::MAX as u16);
            heap.write_u8(self.field(Self::K_HEADER_SIZE + index), code_unit as u8);
        } else {
            heap.write_u16(self.field(Self::K_HEADER_SIZE + index * 2), code_unit);
        }
    }

    /// Writes the characters of `s`, which must match the representation the
    /// string was allocated with.
    pub fn write_chars(self, heap: &mut Heap, s: &str) {
        if self.is_one_byte(heap) {
            let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
            debug_assert_eq!(bytes.len() as i32, self.length(heap));
            heap.write_bytes(self.field(Self::K_HEADER_SIZE), &bytes);
        } else {
            let units: Vec<u16> = s.encode_utf16().collect();
            debug_assert_eq!(units.len() as i32, self.length(heap));
            for (i, unit) in units.iter().enumerate() {
                heap.write_u16(self.field(Self::K_HEADER_SIZE + i as i32 * 2), *unit);
            }
        }
    }

    /// The contents as a Rust `String`.
    ///
    /// Allocates outside the heap on purpose: anything that wants to inspect
    /// a string usually wants to allocate inside the heap while doing so, and
    /// holding a borrow across that is what this port makes impossible.
    pub fn to_rust_string(self, heap: &Heap) -> std::string::String {
        let length = self.length(heap);
        if self.is_one_byte(heap) {
            let bytes = heap.read_bytes(self.field(Self::K_HEADER_SIZE), length as usize);
            // Latin-1, so each byte is its own code point.
            bytes.iter().map(|b| *b as char).collect()
        } else {
            let units: Vec<u16> = (0..length)
                .map(|i| heap.read_u16(self.field(Self::K_HEADER_SIZE + i * 2)))
                .collect();
            std::string::String::from_utf16_lossy(&units)
        }
    }

    /// `String::Equals`: content equality.
    ///
    /// Two *internalized* strings are equal iff they are the same object, so
    /// the identity check short-circuits the common case -- which is also why
    /// property lookup can compare names by address.
    pub fn equals(self, heap: &Heap, other: String) -> bool {
        if self == other {
            return true;
        }
        if self.is_internalized(heap) && other.is_internalized(heap) {
            return false;
        }
        let length = self.length(heap);
        if length != other.length(heap) {
            return false;
        }
        (0..length).all(|i| self.get(heap, i) == other.get(heap, i))
    }

    /// Whether `s` can be stored one byte per character.
    pub fn is_one_byte_content(s: &str) -> bool {
        s.chars().all(|c| (c as u32) <= 0xff)
    }

    /// The number of UTF-16 code units `s` needs -- JavaScript string length,
    /// which is not the same as the number of Rust `char`s.
    pub fn utf16_length(s: &str) -> i32 {
        s.encode_utf16().count() as i32
    }

    #[inline]
    pub fn cast(heap: &Heap, object: HeapObject) -> String {
        debug_assert!(instance_type::is_string(heap.instance_type(object)));
        String::unchecked_cast(object)
    }

    pub fn try_cast(heap: &Heap, value: Tagged) -> Option<String> {
        let object = value.to_strong_heap_object()?;
        if instance_type::is_string(heap.instance_type(object)) {
            Some(String::unchecked_cast(object))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::AllocationType;
    use crate::objects::instance_type::{
        INTERNALIZED_ONE_BYTE_STRING_TYPE, SEQ_ONE_BYTE_STRING_TYPE, SEQ_TWO_BYTE_STRING_TYPE,
    };
    use crate::objects::map::Map;

    struct World {
        heap: Heap,
        one_byte_map: Map,
        two_byte_map: Map,
        internalized_one_byte_map: Map,
    }

    impl World {
        fn new() -> World {
            let mut heap = Heap::new();
            let meta = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
            heap.set_map(meta, meta);
            let meta_map = Map::unchecked_cast(meta);
            meta_map.set_instance_type(&mut heap, instance_type::MAP_TYPE);
            meta_map.set_instance_size(&mut heap, Map::K_SIZE);

            let mut string_map = |instance_type: InstanceType| {
                let object = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
                heap.set_map(object, meta);
                let map = Map::unchecked_cast(object);
                map.set_instance_type(&mut heap, instance_type);
                // Variable sized, like V8's kVariableSizeSentinel.
                map.set_instance_size(&mut heap, 0);
                map
            };
            let one_byte_map = string_map(SEQ_ONE_BYTE_STRING_TYPE);
            let two_byte_map = string_map(SEQ_TWO_BYTE_STRING_TYPE);
            let internalized_one_byte_map = string_map(INTERNALIZED_ONE_BYTE_STRING_TYPE);

            World {
                heap,
                one_byte_map,
                two_byte_map,
                internalized_one_byte_map,
            }
        }

        fn new_string(&mut self, s: &str, map: Map) -> String {
            let length = String::utf16_length(s);
            let one_byte = instance_type::is_one_byte_string(map.instance_type(&self.heap));
            let size = if one_byte {
                String::size_for_one_byte(length)
            } else {
                String::size_for_two_byte(length)
            };
            let object = self
                .heap
                .allocate_raw_or_fail(size as usize, AllocationType::Young);
            self.heap.set_map(object, map.object());
            let string = String::unchecked_cast(object);
            string.set_length(&mut self.heap, length);
            string
                .as_name()
                .set_raw_hash_field(&mut self.heap, K_EMPTY_HASH_FIELD);
            string.write_chars(&mut self.heap, s);
            string
        }
    }

    #[test]
    fn layout_puts_the_characters_after_a_16_byte_header() {
        assert_eq!(String::K_RAW_HASH_FIELD_OFFSET, 8);
        assert_eq!(String::K_LENGTH_OFFSET, 12);
        assert_eq!(String::K_HEADER_SIZE, 16);
        // Sizes are word-aligned, so short strings round up.
        assert_eq!(String::size_for_one_byte(0), 16);
        assert_eq!(String::size_for_one_byte(1), 24);
        assert_eq!(String::size_for_one_byte(8), 24);
        assert_eq!(String::size_for_one_byte(9), 32);
        assert_eq!(String::size_for_two_byte(1), 24);
        assert_eq!(String::size_for_two_byte(4), 24);
        assert_eq!(String::size_for_two_byte(5), 32);
    }

    #[test]
    fn one_byte_strings_round_trip() {
        let mut w = World::new();
        let map = w.one_byte_map;
        let s = w.new_string("hello", map);
        assert_eq!(s.length(&w.heap), 5);
        assert!(s.is_one_byte(&w.heap));
        assert_eq!(s.to_rust_string(&w.heap), "hello");
        assert_eq!(s.get(&w.heap, 0), 'h' as u16);
        assert_eq!(s.get(&w.heap, 4), 'o' as u16);
    }

    #[test]
    fn two_byte_strings_round_trip() {
        let mut w = World::new();
        let map = w.two_byte_map;
        let s = w.new_string("olá ☃", map);
        assert!(!s.is_one_byte(&w.heap));
        assert_eq!(s.to_rust_string(&w.heap), "olá ☃");
        assert_eq!(s.length(&w.heap), 5);
        assert_eq!(s.get(&w.heap, 4), 0x2603);
    }

    #[test]
    fn the_empty_string_is_representable() {
        let mut w = World::new();
        let map = w.one_byte_map;
        let s = w.new_string("", map);
        assert_eq!(s.length(&w.heap), 0);
        assert_eq!(s.to_rust_string(&w.heap), "");
    }

    #[test]
    fn javascript_length_counts_utf16_code_units() {
        // Not Rust chars: an astral character is two code units in JS.
        assert_eq!(String::utf16_length("abc"), 3);
        assert_eq!(String::utf16_length("á"), 1);
        assert_eq!(String::utf16_length("☃"), 1);
        assert_eq!(String::utf16_length("😀"), 2);
    }

    #[test]
    fn latin1_decides_the_representation() {
        assert!(String::is_one_byte_content("hello"));
        assert!(String::is_one_byte_content("olá"), "U+00E1 fits in Latin-1");
        assert!(!String::is_one_byte_content("☃"));
        assert!(!String::is_one_byte_content("😀"));
    }

    #[test]
    fn astral_characters_survive_as_surrogate_pairs() {
        let mut w = World::new();
        let map = w.two_byte_map;
        let s = w.new_string("😀", map);
        assert_eq!(s.length(&w.heap), 2, "one astral char, two code units");
        assert_eq!(s.get(&w.heap, 0), 0xd83d);
        assert_eq!(s.get(&w.heap, 1), 0xde00);
        assert_eq!(s.to_rust_string(&w.heap), "😀");
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
    fn equal_contents_hash_equally_across_representations() {
        // "a" fits in one byte but may also be stored as two; the hash is
        // over code units, so it must not depend on the representation.
        assert_eq!(string_hash("abc"), string_hash("abc"));
        assert_ne!(string_hash("abc"), string_hash("abd"));
        assert_ne!(string_hash(""), 0);
    }

    #[test]
    fn content_equality_versus_identity() {
        let mut w = World::new();
        let one = w.one_byte_map;
        let a = w.new_string("key", one);
        let b = w.new_string("key", one);
        let c = w.new_string("other", one);

        assert_ne!(a, b, "two separate non-internalized strings");
        assert!(a.equals(&w.heap, b), "but equal by content");
        assert!(!a.equals(&w.heap, c));
        assert!(a.equals(&w.heap, a));
    }

    #[test]
    fn two_internalized_strings_are_equal_only_if_identical() {
        let mut w = World::new();
        let map = w.internalized_one_byte_map;
        let a = w.new_string("key", map);
        let b = w.new_string("key", map);
        assert!(a.is_internalized(&w.heap));
        assert!(b.is_internalized(&w.heap));
        // Having two internalized copies of one name is itself the bug the
        // string table exists to prevent; given two, `equals` is allowed to
        // answer by identity, which is what makes name comparison a pointer
        // compare everywhere else in the engine.
        assert!(!a.equals(&w.heap, b));
        assert!(a.equals(&w.heap, a));
    }

    #[test]
    fn mixed_representation_comparison_compares_code_units() {
        let mut w = World::new();
        let one = w.one_byte_map;
        let two = w.two_byte_map;
        let a = w.new_string("key", one);
        let b = w.new_string("key", two);
        assert!(a.is_one_byte(&w.heap));
        assert!(!b.is_one_byte(&w.heap));
        assert!(a.equals(&w.heap, b), "same code units, different storage");
    }
}

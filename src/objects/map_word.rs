//! Ported from `src/objects/map-word.h` and `src/objects/map-word-inl.h`.
//!
//! Every heap object's first word is normally its map pointer. During a
//! scavenge, the first word of an object that has already been evacuated
//! instead holds the forwarding address of the copy. `MapWord` is the
//! abstraction over "whatever is in the first word right now".
//!
//! The two states are told apart by the tag, exactly as in V8: a map pointer
//! is a strong reference (low bits `01`), while a forwarding address is the
//! raw, 8-byte-aligned address of the copy (low bits `00`, `kForwardingTag`).

use crate::common::globals::*;
use crate::objects::tagged::{HeapObject, Tagged};

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct MapWord(u64);

impl MapWord {
    /// `MapWord::FromMap`
    #[inline]
    pub const fn from_map(map: HeapObject) -> MapWord {
        MapWord(map.tagged().ptr())
    }

    /// `MapWord::ToMap`
    #[inline]
    pub const fn to_map(self) -> HeapObject {
        debug_assert!(!self.is_forwarding_address());
        HeapObject::from_address(self.0 & !K_HEAP_OBJECT_TAG_MASK)
    }

    /// `MapWord::FromForwardingAddress`
    ///
    /// V8 writes `object.ptr() - kHeapObjectTag`, i.e. the untagged address.
    #[inline]
    pub const fn from_forwarding_address(object: HeapObject) -> MapWord {
        MapWord(object.address() | K_FORWARDING_TAG)
    }

    /// `MapWord::IsForwardingAddress`
    ///
    /// Only meaningful during a scavenge, when every map word in the space
    /// being evacuated is either a map pointer or a forwarding address.
    #[inline]
    pub const fn is_forwarding_address(self) -> bool {
        self.0 & K_FORWARDING_TAG_MASK == K_FORWARDING_TAG
    }

    /// `MapWord::ToForwardingAddress`
    #[inline]
    pub const fn to_forwarding_address(self) -> HeapObject {
        debug_assert!(self.is_forwarding_address());
        HeapObject::from_address(self.0 & !K_FORWARDING_TAG_MASK)
    }

    /// The raw word, for storing into or loading out of the heap.
    #[inline]
    pub const fn ptr(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn from_raw(value: u64) -> MapWord {
        MapWord(value)
    }

    #[inline]
    pub const fn as_tagged(self) -> Tagged {
        Tagged(self.0)
    }
}

impl std::fmt::Debug for MapWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_forwarding_address() {
            write!(f, "Forwarded(0x{:x})", self.0 & !K_FORWARDING_TAG_MASK)
        } else {
            write!(f, "Map(0x{:x})", self.0 & !K_HEAP_OBJECT_TAG_MASK)
        }
    }
}

/// Alignment is what makes the two states distinguishable, so assert it.
const _: () = assert!(K_OBJECT_ALIGNMENT > K_FORWARDING_TAG_MASK);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::objects::tagged::Address;

    const SOME_MAP: Address = 0x4_0100;
    const SOME_COPY: Address = 0x8_0200;

    #[test]
    fn a_map_word_holding_a_map_is_not_forwarded() {
        let map = HeapObject::from_address(SOME_MAP);
        let w = MapWord::from_map(map);
        assert!(!w.is_forwarding_address());
        assert_eq!(w.to_map(), map);
    }

    #[test]
    fn a_forwarded_map_word_yields_the_copy() {
        let copy = HeapObject::from_address(SOME_COPY);
        let w = MapWord::from_forwarding_address(copy);
        assert!(w.is_forwarding_address());
        assert_eq!(w.to_forwarding_address(), copy);
    }

    #[test]
    fn the_two_states_never_alias() {
        // Same address in both roles must produce different words, otherwise
        // the scavenger could not tell a live object from an evacuated one.
        let o = HeapObject::from_address(SOME_MAP);
        let as_map = MapWord::from_map(o);
        let as_fwd = MapWord::from_forwarding_address(o);
        assert_ne!(as_map.ptr(), as_fwd.ptr());
        assert!(!as_map.is_forwarding_address());
        assert!(as_fwd.is_forwarding_address());
    }

    #[test]
    fn a_map_word_survives_a_trip_through_the_heap_as_a_raw_word() {
        let map = HeapObject::from_address(SOME_MAP);
        let raw = MapWord::from_map(map).ptr();
        assert_eq!(MapWord::from_raw(raw).to_map(), map);
    }
}

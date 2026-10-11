//! Ported from `src/objects/oddball.h` and `oddball.tq`.
//!
//! `undefined`, `null`, `true`, `false` and the internal hole are heap
//! objects in V8, not immediate values. Each one carries the results of
//! converting it, so `ToNumber(undefined)` and `typeof null` are field reads
//! rather than branches:
//!
//! ```text
//!   0  map
//!   8  to_number_raw   float64
//!  16  to_string       String
//!  24  to_number       Number
//!  32  type_of         String
//!  40  kind            Smi
//!  48  == Oddball::kSize
//! ```
//!
//! They live in read-only space, so a comparison against `undefined` is an
//! address comparison that can never be invalidated by a collection.

use crate::heap::heap::Heap;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

object_type!(
    /// `Oddball`
    Oddball
);

/// `Oddball::kKind` values, from `src/objects/oddball.h`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OddballKind {
    /// `Oddball::kFalse`
    False = 0,
    /// `Oddball::kTrue`
    True = 1,
    /// `Oddball::kNull`
    Null = 3,
    /// `Oddball::kUndefined`
    Undefined = 4,
    /// `Oddball::kTheHole` -- never visible to JavaScript. Marks an
    /// uninitialized `let` binding and an absent array element.
    TheHole = 5,
    /// `Oddball::kUninitialized` -- an inline cache slot nothing has reached
    /// yet.
    Uninitialized = 6,
}

impl OddballKind {
    pub fn from_smi(value: Tagged) -> Option<OddballKind> {
        match Smi::to_int(value) {
            0 => Some(OddballKind::False),
            1 => Some(OddballKind::True),
            3 => Some(OddballKind::Null),
            4 => Some(OddballKind::Undefined),
            5 => Some(OddballKind::TheHole),
            6 => Some(OddballKind::Uninitialized),
            _ => None,
        }
    }
}

impl Oddball {
    pub const K_TO_NUMBER_RAW_OFFSET: i32 = 8;
    pub const K_TO_STRING_OFFSET: i32 = Self::K_TO_NUMBER_RAW_OFFSET + 8;
    pub const K_TO_NUMBER_OFFSET: i32 = Self::K_TO_STRING_OFFSET + 8;
    pub const K_TYPE_OF_OFFSET: i32 = Self::K_TO_NUMBER_OFFSET + 8;
    pub const K_KIND_OFFSET: i32 = Self::K_TYPE_OF_OFFSET + 8;
    /// `Oddball::kSize`
    pub const K_SIZE: i32 = Self::K_KIND_OFFSET + 8;

    #[inline]
    pub fn to_number_raw(self, heap: &Heap) -> f64 {
        heap.read_f64(self.field(Self::K_TO_NUMBER_RAW_OFFSET))
    }

    #[inline]
    pub fn set_to_number_raw(self, heap: &mut Heap, value: f64) {
        heap.write_f64(self.field(Self::K_TO_NUMBER_RAW_OFFSET), value);
    }

    #[inline]
    pub fn to_string(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_TO_STRING_OFFSET))
    }

    #[inline]
    pub fn set_to_string(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_TO_STRING_OFFSET), value);
    }

    #[inline]
    pub fn to_number(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_TO_NUMBER_OFFSET))
    }

    #[inline]
    pub fn set_to_number(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_TO_NUMBER_OFFSET), value);
    }

    #[inline]
    pub fn type_of(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_TYPE_OF_OFFSET))
    }

    #[inline]
    pub fn set_type_of(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_TYPE_OF_OFFSET), value);
    }

    #[inline]
    pub fn kind(self, heap: &Heap) -> Option<OddballKind> {
        OddballKind::from_smi(heap.read_tagged(self.field(Self::K_KIND_OFFSET)))
    }

    #[inline]
    pub fn set_kind(self, heap: &mut Heap, kind: OddballKind) {
        heap.write_tagged(self.field(Self::K_KIND_OFFSET), Smi::from_int(kind as i32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        assert_eq!(Oddball::K_SIZE, 48);
        assert_eq!(Oddball::K_KIND_OFFSET, 40);
    }

    #[test]
    fn kinds_round_trip_through_a_smi() {
        for kind in [
            OddballKind::False,
            OddballKind::True,
            OddballKind::Null,
            OddballKind::Undefined,
            OddballKind::TheHole,
            OddballKind::Uninitialized,
        ] {
            assert_eq!(
                OddballKind::from_smi(Smi::from_int(kind as i32)),
                Some(kind)
            );
        }
        assert_eq!(OddballKind::from_smi(Smi::from_int(99)), None);
    }

    #[test]
    fn kinds_match_v8s_numbering() {
        // false/true are 0/1 so that a boolean's kind doubles as its value.
        assert_eq!(OddballKind::False as i32, 0);
        assert_eq!(OddballKind::True as i32, 1);
        assert_eq!(OddballKind::Null as i32, 3);
        assert_eq!(OddballKind::Undefined as i32, 4);
    }
}

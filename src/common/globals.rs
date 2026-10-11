//! Ported from `include/v8-internal.h` and `src/common/globals.h`.
//!
//! `vx` targets one single V8 configuration, the 64-bit build **without**
//! pointer compression (`V8_COMPRESS_POINTERS` off, `V8_EXTERNAL_CODE_SPACE`
//! off, sandbox off). That is a real, supported V8 configuration and it keeps
//! a tagged word the same width as a machine word, so there is no compression
//! cage to model.
//!
//! Everything derived from that choice lives here, so there is exactly one
//! place to look when the numbers have to be checked against V8.

pub const K_INT32_SIZE: i32 = 4;
pub const K_INT64_SIZE: i32 = 8;
pub const K_DOUBLE_SIZE: i32 = 8;
pub const K_SYSTEM_POINTER_SIZE: i32 = 8;
pub const K_SYSTEM_POINTER_SIZE_LOG2: i32 = 3;

/// `kTaggedSize` == `kSystemPointerSize` because pointer compression is off.
pub const K_TAGGED_SIZE: i32 = K_SYSTEM_POINTER_SIZE;
pub const K_TAGGED_SIZE_LOG2: i32 = K_SYSTEM_POINTER_SIZE_LOG2;

// ---------------------------------------------------------------------------
// Tagging. From `include/v8-internal.h`:
//
//   const int kHeapObjectTag = 1;
//   const int kWeakHeapObjectTag = 3;
//   const int kHeapObjectTagSize = 2;
//   const int kSmiTag = 0;
//   const int kSmiTagSize = 1;
//   const int kForwardingTag = 0;
//   const int kForwardingTagSize = 2;
//
// So the low two bits of a tagged word mean:
//
//   00  a Smi, or (in a map slot, during a scavenge) a forwarding address
//   01  a strong reference to a heap object
//   10  a Smi
//   11  a weak reference to a heap object
//
// Note that `kSmiTagSize` is 1 while `kHeapObjectTagSize` is 2: a Smi is
// anything with the low bit clear, which is why `00` is shared between Smis
// and forwarding addresses. A forwarding address is only ever read out of an
// object's map slot during a scavenge, and a map slot never holds a Smi, so
// the two uses cannot collide.
// ---------------------------------------------------------------------------

pub const K_HEAP_OBJECT_TAG: u64 = 1;
pub const K_WEAK_HEAP_OBJECT_TAG: u64 = 3;
pub const K_HEAP_OBJECT_TAG_SIZE: u32 = 2;
pub const K_HEAP_OBJECT_TAG_MASK: u64 = (1 << K_HEAP_OBJECT_TAG_SIZE) - 1;
pub const K_HEAP_OBJECT_REFERENCE_TAG_MASK: u64 = 1 << (K_HEAP_OBJECT_TAG_SIZE - 1);

pub const K_SMI_TAG: u64 = 0;
pub const K_SMI_TAG_SIZE: u32 = 1;
pub const K_SMI_TAG_MASK: u64 = (1 << K_SMI_TAG_SIZE) - 1;

pub const K_FORWARDING_TAG: u64 = 0;
pub const K_FORWARDING_TAG_SIZE: u32 = 2;
pub const K_FORWARDING_TAG_MASK: u64 = (1 << K_FORWARDING_TAG_SIZE) - 1;

/// `SmiTagging<8>` from `include/v8-internal.h`: on 64-bit without pointer
/// compression a Smi carries a full `int32`, parked in the upper half of the
/// word, with 31 bits of zero padding below it.
///
/// ```text
///   [ 32 bit signed int ][ 31 bits zero padding ][ 0 ]
/// ```
pub const K_SMI_VALUE_SIZE: u32 = 32;
pub const K_SMI_SHIFT_SIZE: u32 = 31;
/// `kSmiTagSize + kSmiShiftSize` -- how far a Smi payload is shifted up.
pub const K_SMI_SHIFT_BITS: u32 = K_SMI_TAG_SIZE + K_SMI_SHIFT_SIZE;

pub const K_SMI_MIN_VALUE: i64 = i32::MIN as i64;
pub const K_SMI_MAX_VALUE: i64 = i32::MAX as i64;

/// From `src/common/globals.h`. A cleared weak reference is the weak tag with
/// an otherwise empty payload.
pub const K_CLEARED_WEAK_HEAP_OBJECT_LOWER32: u32 = 3;

// ---------------------------------------------------------------------------
// Object alignment.
// ---------------------------------------------------------------------------

pub const K_OBJECT_ALIGNMENT_BITS: u32 = K_TAGGED_SIZE_LOG2 as u32;
pub const K_OBJECT_ALIGNMENT: u64 = 1 << K_OBJECT_ALIGNMENT_BITS;
pub const K_OBJECT_ALIGNMENT_MASK: u64 = K_OBJECT_ALIGNMENT - 1;

/// `OBJECT_POINTER_ALIGN` from `src/common/globals.h`.
#[inline]
pub const fn object_pointer_align(value: u64) -> u64 {
    (value + K_OBJECT_ALIGNMENT_MASK) & !K_OBJECT_ALIGNMENT_MASK
}

#[inline]
pub const fn is_aligned(value: u64, alignment: u64) -> bool {
    value & (alignment - 1) == 0
}

/// `InlineCacheState` from `src/common/globals.h`.
///
/// The lattice an inline cache climbs. It only ever moves one way -- a site
/// that has gone megamorphic never goes back -- which is what makes the
/// machine terminate instead of thrashing.
///
/// ```text
///   UNINITIALIZED   never executed
///   MONOMORPHIC     one receiver shape seen; one map check, then a fixed slot
///   POLYMORPHIC     a few shapes seen; a short linear scan of map checks
///   MEGAMORPHIC     too many shapes; give up and do the full lookup
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum InlineCacheState {
    /// No feedback will be collected.
    NoFeedback,
    /// Has never been executed.
    Uninitialized,
    /// Has been executed and only one receiver type has been seen.
    Monomorphic,
    /// Multiple receiver types have been seen.
    Polymorphic,
    /// Many receiver types have been seen.
    Megamorphic,
    /// A generic handler is installed and no extra feedback is recorded.
    Generic,
}

/// `DEFAULT_MAX_POLYMORPHIC_MAP_COUNT` from `src/flags/flag-definitions.h`:
/// how many shapes a site tracks before giving up.
///
/// Four. Small on purpose -- past a handful, the linear scan of map checks
/// costs more than the lookup it is avoiding.
pub const K_MAX_POLYMORPHIC_MAP_COUNT: usize = 4;

pub const K_KB: usize = 1024;
pub const K_MB: usize = K_KB * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_do_not_overlap() {
        // A strong reference is distinguishable from a weak one, and both are
        // distinguishable from a Smi.
        assert_eq!(K_HEAP_OBJECT_TAG & K_SMI_TAG_MASK, 1);
        assert_eq!(K_WEAK_HEAP_OBJECT_TAG & K_SMI_TAG_MASK, 1);
        assert_eq!(
            K_HEAP_OBJECT_TAG & K_HEAP_OBJECT_TAG_MASK,
            K_HEAP_OBJECT_TAG
        );
        assert_eq!(
            K_WEAK_HEAP_OBJECT_TAG & K_HEAP_OBJECT_TAG_MASK,
            K_WEAK_HEAP_OBJECT_TAG
        );
        // ...and a forwarding address looks like neither.
        assert_eq!(K_FORWARDING_TAG & K_HEAP_OBJECT_TAG_MASK, 0);
    }

    #[test]
    fn smi_shift_matches_v8() {
        // static_assert((kSmiValueSize + kSmiShiftSize + kSmiTagSize) == 64)
        assert_eq!(K_SMI_VALUE_SIZE + K_SMI_SHIFT_SIZE + K_SMI_TAG_SIZE, 64);
        assert_eq!(K_SMI_SHIFT_BITS, 32);
    }

    #[test]
    fn alignment() {
        assert_eq!(K_OBJECT_ALIGNMENT, 8);
        assert_eq!(object_pointer_align(0), 0);
        assert_eq!(object_pointer_align(1), 8);
        assert_eq!(object_pointer_align(8), 8);
        assert_eq!(object_pointer_align(9), 16);
        assert!(is_aligned(16, K_OBJECT_ALIGNMENT));
        assert!(!is_aligned(12, K_OBJECT_ALIGNMENT));
    }
}

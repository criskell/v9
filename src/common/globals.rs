pub const K_INT32_SIZE: i32 = 4;
pub const K_INT64_SIZE: i32 = 8;
pub const K_DOUBLE_SIZE: i32 = 8;

pub const K_HOLE_NAN_UPPER32: u32 = 0xFFF7FFFF;
pub const K_HOLE_NAN_LOWER32: u32 = 0xFFF7FFFF;
pub const K_HOLE_NAN_INT64: u64 = ((K_HOLE_NAN_UPPER32 as u64) << 32) | K_HOLE_NAN_LOWER32 as u64;
pub const K_SYSTEM_POINTER_SIZE: i32 = 8;
pub const K_SYSTEM_POINTER_SIZE_LOG2: i32 = 3;

pub const K_TAGGED_SIZE: i32 = K_SYSTEM_POINTER_SIZE;
pub const K_TAGGED_SIZE_LOG2: i32 = K_SYSTEM_POINTER_SIZE_LOG2;

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

pub const K_SMI_VALUE_SIZE: u32 = 32;
// Mask applied to an integer hash so the result fits in a non-negative Smi
// payload (safe to pass to Smi::FromInt). One bit narrower than the Smi value
// field so the sign bit stays clear.
pub const K_SMI_HASH_MASK: u32 = ((1u64 << (K_SMI_VALUE_SIZE - 1)) - 1) as u32;
pub const K_SMI_SHIFT_SIZE: u32 = 31;
pub const K_SMI_SHIFT_BITS: u32 = K_SMI_TAG_SIZE + K_SMI_SHIFT_SIZE;

pub const K_SMI_MIN_VALUE: i64 = i32::MIN as i64;
pub const K_SMI_MAX_VALUE: i64 = i32::MAX as i64;

pub const K_CLEARED_WEAK_HEAP_OBJECT_LOWER32: u32 = 3;


pub const K_OBJECT_ALIGNMENT_BITS: u32 = K_TAGGED_SIZE_LOG2 as u32;
pub const K_OBJECT_ALIGNMENT: u64 = 1 << K_OBJECT_ALIGNMENT_BITS;
pub const K_OBJECT_ALIGNMENT_MASK: u64 = K_OBJECT_ALIGNMENT - 1;

#[inline]
pub const fn object_pointer_align(value: u64) -> u64 {
    (value + K_OBJECT_ALIGNMENT_MASK) & !K_OBJECT_ALIGNMENT_MASK
}

#[inline]
pub const fn is_aligned(value: u64, alignment: u64) -> bool {
    value & (alignment - 1) == 0
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StoreOrigin {
    MaybeKeyed,
    Named,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VariableLocation {
    Unallocated,
    Parameter,
    Local,
    Context,
}

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
    Generic,
}

pub const K_MAX_POLYMORPHIC_MAP_COUNT: usize = 4;

pub const K_KB: usize = 1024;
pub const K_MB: usize = K_KB * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_do_not_overlap() {
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
        assert_eq!(K_FORWARDING_TAG & K_HEAP_OBJECT_TAG_MASK, 0);
    }

    #[test]
    fn smi_shift_matches_v8() {
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

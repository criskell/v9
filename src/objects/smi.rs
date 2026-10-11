use crate::common::globals::*;
use crate::objects::tagged::Tagged;

// Smi represents integer Numbers that can be stored in 31 bits.
// Smis are immediate which means they are NOT allocated in the heap.
// The ptr_ value has the following format: [31 bit signed int] 0
// For long smis it has the following format:
//     [32 bit signed int] [31 bits zero padding] 0
// Smi stands for small integer.
pub struct Smi;

impl Smi {
    pub const MIN_VALUE: i32 = K_SMI_MIN_VALUE as i32;
    pub const MAX_VALUE: i32 = K_SMI_MAX_VALUE as i32;

    #[inline]
    pub const fn zero() -> Tagged {
        Smi::from_int(0)
    }

    // Convert a value to a Smi object.
    #[inline]
    pub const fn from_int(value: i32) -> Tagged {
        Tagged(((value as i64) << K_SMI_SHIFT_BITS) as u64 | K_SMI_TAG)
    }

    #[inline]
    pub const fn try_from_i64(value: i64) -> Option<Tagged> {
        if Smi::is_valid(value) {
            Some(Smi::from_int(value as i32))
        } else {
            None
        }
    }

    // Returns whether value can be represented in a Smi.
    #[inline]
    pub const fn is_valid(value: i64) -> bool {
        value >= K_SMI_MIN_VALUE && value <= K_SMI_MAX_VALUE
    }

    // Convert a Smi object to an int.
    #[inline]
    pub const fn to_int(object: Tagged) -> i32 {
        debug_assert!(object.is_smi());
        (object.ptr() as i64 >> K_SMI_SHIFT_BITS) as i32
    }

    #[inline]
    pub const fn try_to_int(object: Tagged) -> Option<i32> {
        if object.is_smi() {
            Some((object.ptr() as i64 >> K_SMI_SHIFT_BITS) as i32)
        } else {
            None
        }
    }

    // Returns true if value can be converted to a SMI, and returns the
    // resulting integer value of the SMI in |smi_int_value|.
    #[inline]
    pub fn try_from_f64(value: f64) -> Option<Tagged> {
        if value == 0.0 && value.is_sign_negative() {
            return None;
        }
        if value.fract() != 0.0 {
            return None;
        }
        if !(value >= K_SMI_MIN_VALUE as f64 && value <= K_SMI_MAX_VALUE as f64) {
            return None;
        }
        Some(Smi::from_int(value as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_over_the_whole_range() {
        for v in [0i32, 1, -1, 42, -42, 1 << 30, i32::MAX, i32::MIN] {
            let s = Smi::from_int(v);
            assert!(s.is_smi(), "{v} should encode as a Smi");
            assert_eq!(Smi::to_int(s), v, "roundtrip failed for {v}");
        }
    }

    #[test]
    fn payload_sits_in_the_upper_half() {
        assert_eq!(Smi::from_int(1).ptr(), 1u64 << 32);
        assert_eq!(Smi::from_int(0).ptr(), 0);
        assert_eq!(Smi::from_int(-1).ptr(), u64::MAX << 32);
    }

    #[test]
    fn range_check() {
        assert!(Smi::is_valid(i32::MAX as i64));
        assert!(Smi::is_valid(i32::MIN as i64));
        assert!(!Smi::is_valid(i32::MAX as i64 + 1));
        assert!(!Smi::is_valid(i32::MIN as i64 - 1));
        assert!(Smi::try_from_i64(i32::MAX as i64 + 1).is_none());
    }

    #[test]
    fn doubles_that_need_boxing() {
        assert_eq!(Smi::try_from_f64(3.0).map(Smi::to_int), Some(3));
        assert!(
            Smi::try_from_f64(-0.0).is_none(),
            "-0 must stay a HeapNumber"
        );
        assert!(Smi::try_from_f64(1.5).is_none());
        assert!(Smi::try_from_f64(3e10).is_none());
        assert!(Smi::try_from_f64(f64::NAN).is_none());
        assert!(Smi::try_from_f64(f64::INFINITY).is_none());
    }
}

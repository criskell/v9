#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::{
    kDoubleExponentBits, kDoubleMantissaBits, kFloatExponentBits, kFloatMantissaBits,
};

#[inline]
pub fn count_leading_zeros(value: u64, width: i32) -> i32 {
    debug_assert!(crate::base::bits::is_power_of_two(width as u32) && (width <= 64));
    if value == 0 {
        return width;
    }
    (value << (64 - width)).leading_zeros() as i32
}

#[inline]
pub fn count_trailing_zeros(value: u64, width: i32) -> i32 {
    debug_assert!(crate::base::bits::is_power_of_two(width as u32) && (width <= 64));
    if value == 0 {
        return width;
    }
    ((value << (64 - width)) >> (64 - width)).trailing_zeros() as i32
}

#[inline]
pub fn largest_power_of_2_divisor(value: u64) -> u64 {
    value & ((!value).wrapping_add(1))
}

pub fn count_leading_sign_bits(value: i64, width: i32) -> i32 {
    debug_assert!(crate::base::bits::is_power_of_two(width as u32) && (width <= 64));
    if value >= 0 {
        count_leading_zeros(value as u64, width) - 1
    } else {
        count_leading_zeros(!value as u64, width) - 1
    }
}

pub fn count_set_bits(value: u64, width: i32) -> i32 {
    debug_assert!(crate::base::bits::is_power_of_two(width as u32) && (width <= 64));
    ((value << (64 - width)) >> (64 - width)).count_ones() as i32
}

pub fn float_sign(val: f32) -> u32 {
    (val.to_bits() >> 31) & 1
}

pub fn float_exp(val: f32) -> u32 {
    (val.to_bits() >> 23) & 0xff
}

pub fn float_mantissa(val: f32) -> u32 {
    val.to_bits() & 0x7fffff
}

pub fn double_sign(val: f64) -> u32 {
    ((val.to_bits() >> 63) & 1) as u32
}

pub fn double_exp(val: f64) -> u32 {
    ((val.to_bits() >> 52) & 0x7ff) as u32
}

pub fn double_mantissa(val: f64) -> u64 {
    val.to_bits() & 0xfffffffffffff
}

pub fn float_pack(sign: u32, exp: u32, mantissa: u32) -> f32 {
    let bits = sign << kFloatExponentBits | exp;
    f32::from_bits((bits << kFloatMantissaBits) | mantissa)
}

pub fn double_pack(sign: u64, exp: u64, mantissa: u64) -> f64 {
    let bits = sign << kDoubleExponentBits | exp;
    f64::from_bits((bits << kDoubleMantissaBits) | mantissa)
}

pub fn lowest_set_bit_position(value: u64) -> u32 {
    debug_assert_ne!(value, 0);
    value.trailing_zeros() + 1
}

pub fn highest_set_bit_position(value: u64) -> u32 {
    debug_assert_ne!(value, 0);
    63 - count_leading_zeros(value, 64) as u32
}

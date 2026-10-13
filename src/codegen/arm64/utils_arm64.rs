#![allow(non_upper_case_globals)]

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

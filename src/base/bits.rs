#[inline]
pub fn is_power_of_two(value: u32) -> bool {
    value != 0 && (value & (value - 1)) == 0
}

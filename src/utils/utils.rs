use crate::common::globals::K_SMI_HASH_MASK;

#[inline]
pub fn smi_hash64(key: u64) -> u32 {
    (crate::base::hashing::hash64(key) as u32) & K_SMI_HASH_MASK
}

#[inline]
pub fn compute_seeded_hash(key: u32, seed: u64) -> u32 {
    smi_hash64(key as u64 ^ seed)
}

#[inline]
pub fn compute_address_hash(address: u64) -> u32 {
    (crate::base::hashing::hash64(address & 0xFFFFFFFF) as u32) & K_SMI_HASH_MASK
}

#[inline]
pub fn is_intn(x: i64, n: u32) -> bool {
    debug_assert!((0 < n) && (n < 64));
    let limit = 1i64 << (n - 1);
    (-limit <= x) && (x < limit)
}

#[inline]
pub fn is_uintn(x: i64, n: u32) -> bool {
    debug_assert!((0 < n) && (n < 64));
    (x >> n) == 0
}

#[inline]
pub fn truncate_to_intn(x: i64, n: u32) -> i64 {
    debug_assert!((0 < n) && (n < 64));
    x & ((1i64 << n) - 1)
}

#[inline]
pub fn is_int9(x: i64) -> bool {
    is_intn(x, 9)
}

#[inline]
pub fn is_int14(x: i64) -> bool {
    is_intn(x, 14)
}

#[inline]
pub fn is_int19(x: i64) -> bool {
    is_intn(x, 19)
}

#[inline]
pub fn is_int26(x: i64) -> bool {
    is_intn(x, 26)
}

#[inline]
pub fn is_uint2(x: i64) -> bool {
    is_uintn(x, 2)
}

#[inline]
pub fn is_uint5(x: i64) -> bool {
    is_uintn(x, 5)
}

#[inline]
pub fn is_uint6(x: i64) -> bool {
    is_uintn(x, 6)
}

#[inline]
pub fn is_uint7(x: i64) -> bool {
    is_uintn(x, 7)
}

#[inline]
pub fn is_uint12(x: i64) -> bool {
    is_uintn(x, 12)
}

#[inline]
pub fn is_uint16(x: i64) -> bool {
    is_uintn(x, 16)
}

#[inline]
pub fn is_uint32(x: i64) -> bool {
    is_uintn(x, 32)
}

#[inline]
pub fn checked_truncate_to_int9(x: i64) -> i64 {
    assert!(is_int9(x));
    truncate_to_intn(x, 9)
}

#[inline]
pub fn checked_truncate_to_int14(x: i64) -> i64 {
    assert!(is_int14(x));
    truncate_to_intn(x, 14)
}

#[inline]
pub fn checked_truncate_to_int19(x: i64) -> i64 {
    assert!(is_int19(x));
    truncate_to_intn(x, 19)
}

#[inline]
pub fn checked_truncate_to_int26(x: i64) -> i64 {
    assert!(is_int26(x));
    truncate_to_intn(x, 26)
}

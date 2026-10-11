use crate::common::globals::K_SMI_HASH_MASK;

/// `SmiHash64`
#[inline]
pub fn smi_hash64(key: u64) -> u32 {
    (crate::base::hashing::hash64(key) as u32) & K_SMI_HASH_MASK
}

/// `ComputeSeededHash`
#[inline]
pub fn compute_seeded_hash(key: u32, seed: u64) -> u32 {
    smi_hash64(key as u64 ^ seed)
}

/// `ComputeAddressHash`
#[inline]
pub fn compute_address_hash(address: u64) -> u32 {
    (crate::base::hashing::hash64(address & 0xFFFFFFFF) as u32) & K_SMI_HASH_MASK
}

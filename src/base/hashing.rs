pub const K_RAPIDHASH_SECRET1: u64 = 0x2d358dccaa6c78a5;
pub const K_RAPIDHASH_SECRET2: u64 = 0x8bb84b93962eacc9;

/// `rapid_mul128`
#[inline]
fn rapid_mul128(a: u64, b: u64) -> (u64, u64) {
    let product = (a as u128) * (b as u128);
    ((product >> 64) as u64, product as u64)
}

/// `rapid_mix`
#[inline]
pub fn rapid_mix(a: u64, b: u64) -> u64 {
    let (a, b) = rapid_mul128(a, b);
    a ^ b
}

/// `base::hash64`
// Canonical integer hash. Uses the rapidhash "mum" mixer
// ((key ^ s1) * (key ^ s2) -> high64 ^ low64) on 64-bit targets.
#[inline]
pub fn hash64(key: u64) -> u64 {
    rapid_mix(key ^ K_RAPIDHASH_SECRET1, key ^ K_RAPIDHASH_SECRET2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mixer_spreads_adjacent_keys() {
        let hashes: Vec<u64> = (0..8).map(hash64).collect();
        for window in hashes.windows(2) {
            let difference = window[0] ^ window[1];
            assert!(
                difference.count_ones() > 16,
                "{:x} and {:x} differ in only {} bits",
                window[0],
                window[1],
                difference.count_ones()
            );
        }
    }

    #[test]
    fn distinct_keys_fill_the_buckets_of_a_small_table() {
        let mut buckets = [0usize; 16];
        for key in 0..64u64 {
            buckets[(hash64(key) & 15) as usize] += 1;
        }
        assert!(
            buckets.iter().all(|count| *count > 0),
            "every bucket should be used: {buckets:?}"
        );
    }
}

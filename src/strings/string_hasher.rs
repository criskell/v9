#[derive(Default)]
pub struct RunningStringHasher {
    running_hash: u32,
}

impl RunningStringHasher {
    pub fn new() -> RunningStringHasher {
        RunningStringHasher::default()
    }

    #[inline]
    pub fn add_character(&mut self, c: u16) {
        self.running_hash = self.running_hash.wrapping_add(c as u32);
        self.running_hash = self.running_hash.wrapping_add(self.running_hash << 10);
        self.running_hash ^= self.running_hash >> 6;
    }

    pub fn finalize(mut self) -> u32 {
        self.running_hash = self.running_hash.wrapping_add(self.running_hash << 3);
        self.running_hash ^= self.running_hash >> 11;
        self.running_hash = self.running_hash.wrapping_add(self.running_hash << 15);
        convert_raw_hash_to_usable_hash(self.running_hash)
    }
}

#[inline]
fn convert_raw_hash_to_usable_hash(raw: u32) -> u32 {
    let hash = raw & crate::objects::name::K_MAX_HASH;
    if hash == 0 {
        1
    } else {
        hash
    }
}

pub struct StringHasher;

impl StringHasher {
    pub fn hash_sequential_string(s: &str) -> u32 {
        let mut hasher = RunningStringHasher::new();
        for c in s.encode_utf16() {
            hasher.add_character(c);
        }
        hasher.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_contents_hash_equally() {
        assert_eq!(
            StringHasher::hash_sequential_string("abc"),
            StringHasher::hash_sequential_string("abc")
        );
        assert_ne!(
            StringHasher::hash_sequential_string("abc"),
            StringHasher::hash_sequential_string("abd")
        );
    }

    #[test]
    fn the_hash_is_never_zero() {
        for s in ["", "a", "abc", "length", "\u{0}"] {
            assert_ne!(StringHasher::hash_sequential_string(s), 0, "{s:?}");
        }
    }

    #[test]
    fn the_hash_fits_the_field() {
        for s in ["", "a", "a longer string to mix the bits thoroughly"] {
            assert!(
                StringHasher::hash_sequential_string(s) <= crate::objects::name::K_MAX_HASH,
                "{s:?}"
            );
        }
    }

    #[test]
    fn hashing_is_over_code_units_not_bytes() {
        let hasher = |s: &str| {
            let mut h = RunningStringHasher::new();
            for c in s.encode_utf16() {
                h.add_character(c);
            }
            h.finalize()
        };
        assert_eq!(
            hasher("ol\u{e1}"),
            StringHasher::hash_sequential_string("ol\u{e1}")
        );
        assert_ne!(
            StringHasher::hash_sequential_string("\u{2603}"),
            StringHasher::hash_sequential_string("\u{2604}")
        );
    }
}

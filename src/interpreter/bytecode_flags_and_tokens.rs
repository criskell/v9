/// `TryFinallyContinuationToken`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TryFinallyContinuationToken {
    // Fixed value tokens for paths we know we need.
    // Fallthrough is set to -1 to make it the fallthrough case of the jump
    // table, where the remaining cases start at 0.
    Fallthrough = -1,
    Rethrow = 0,
}

impl TryFinallyContinuationToken {
    pub const K_RETURN_TOKEN: i32 = 1;

    #[inline]
    pub fn value(self) -> i32 {
        self as i32
    }
}

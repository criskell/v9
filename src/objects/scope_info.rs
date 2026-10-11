//! Ported from `src/objects/scope-info.h`, in miniature.
//!
//! Describes a [`Context`](crate::objects::contexts::Context)'s slots: how
//! many there are, and which name each one holds.
//!
//! ```text
//!   0  map
//!   8  context_local_count  i32
//!  12  (padding)            u32
//!  16  names[count]         tagged
//! ```
//!
//! Nothing on the hot path reads this -- a context slot is reached by index,
//! not by name, which is the point of scope analysis. It exists because the
//! context has to say what it is: V8 needs it for the debugger, for `eval`,
//! for stack traces and for serialising a closure into a snapshot.
//!
//! Deviation, tracked in `PORTING.md`: V8's `ScopeInfo` is a large packed
//! object carrying the scope type, the language mode, the function kind, the
//! parameter count, flags for every special binding, and a positions table.
//! `vx` keeps the names and the count.

use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `ScopeInfo`
    ScopeInfo
);

impl ScopeInfo {
    pub const K_CONTEXT_LOCAL_COUNT_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_CONTEXT_LOCAL_COUNT_OFFSET + 8;

    pub fn size_for(context_local_count: i32) -> i32 {
        Self::K_HEADER_SIZE + context_local_count * K_TAGGED_SIZE
    }

    /// `ScopeInfo::ContextLocalCount`
    #[inline]
    pub fn context_local_count(self, heap: &Heap) -> i32 {
        heap.read_u32(self.field(Self::K_CONTEXT_LOCAL_COUNT_OFFSET)) as i32
    }

    #[inline]
    pub fn set_context_local_count(self, heap: &mut Heap, value: i32) {
        heap.write_u32(self.field(Self::K_CONTEXT_LOCAL_COUNT_OFFSET), value as u32);
    }

    #[inline]
    pub fn name_address(self, index: i32) -> Address {
        self.field(Self::K_HEADER_SIZE + index * K_TAGGED_SIZE)
    }

    /// `ScopeInfo::ContextLocalName`
    #[inline]
    pub fn context_local_name(self, heap: &Heap, index: i32) -> Tagged {
        debug_assert!(index >= 0 && index < self.context_local_count(heap));
        heap.read_tagged(self.name_address(index))
    }

    #[inline]
    pub fn set_context_local_name(self, heap: &mut Heap, index: i32, value: Tagged) {
        heap.write_tagged(self.name_address(index), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        assert_eq!(ScopeInfo::K_CONTEXT_LOCAL_COUNT_OFFSET, 8);
        assert_eq!(ScopeInfo::K_HEADER_SIZE, 16);
        assert_eq!(ScopeInfo::size_for(0), 16);
        assert_eq!(ScopeInfo::size_for(2), 32);
    }
}

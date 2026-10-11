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

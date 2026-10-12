use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::bytecode_array::BytecodeArray;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    SharedFunctionInfo
);

const IS_NATIVE_BIT: u32 = 5;

impl SharedFunctionInfo {
    pub const K_FUNCTION_DATA_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_NAME_OFFSET: i32 = Self::K_FUNCTION_DATA_OFFSET + K_TAGGED_SIZE;
    pub const K_FEEDBACK_METADATA_OFFSET: i32 = Self::K_NAME_OFFSET + K_TAGGED_SIZE;
    pub const K_LENGTH_OFFSET: i32 = Self::K_FEEDBACK_METADATA_OFFSET + K_TAGGED_SIZE;
    pub const K_FORMAL_PARAMETER_COUNT_OFFSET: i32 = Self::K_LENGTH_OFFSET + 2;
    pub const K_FLAGS_OFFSET: i32 = Self::K_FORMAL_PARAMETER_COUNT_OFFSET + 2;
    pub const K_SIZE: i32 = Self::K_FLAGS_OFFSET + 4;

    #[inline]
    pub fn function_data(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_FUNCTION_DATA_OFFSET))
    }

    #[inline]
    pub fn set_function_data(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_FUNCTION_DATA_OFFSET), value);
    }

    #[inline]
    pub fn function_data_address(self) -> Address {
        self.field(Self::K_FUNCTION_DATA_OFFSET)
    }

    #[inline]
    pub fn bytecode_array(self, heap: &Heap) -> BytecodeArray {
        BytecodeArray::unchecked_cast(
            self.function_data(heap)
                .to_strong_heap_object()
                .expect("this function has no bytecode"),
        )
    }

    #[inline]
    pub fn has_bytecode_array(self, heap: &Heap) -> bool {
        !self.is_native(heap) && self.function_data(heap).is_strong()
    }

    #[inline]
    pub fn name(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_NAME_OFFSET))
    }

    #[inline]
    pub fn set_name(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_NAME_OFFSET), value);
    }

    #[inline]
    pub fn name_address(self) -> Address {
        self.field(Self::K_NAME_OFFSET)
    }

    #[inline]
    pub fn feedback_metadata(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_FEEDBACK_METADATA_OFFSET))
    }

    #[inline]
    pub fn set_feedback_metadata(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_FEEDBACK_METADATA_OFFSET), value);
    }

    #[inline]
    pub fn feedback_metadata_address(self) -> Address {
        self.field(Self::K_FEEDBACK_METADATA_OFFSET)
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_LENGTH_OFFSET)) as i32
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, value: i32) {
        heap.write_u16(self.field(Self::K_LENGTH_OFFSET), value as u16);
    }

    #[inline]
    pub fn formal_parameter_count(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_FORMAL_PARAMETER_COUNT_OFFSET)) as i32
    }

    #[inline]
    pub fn set_formal_parameter_count(self, heap: &mut Heap, value: i32) {
        heap.write_u16(
            self.field(Self::K_FORMAL_PARAMETER_COUNT_OFFSET),
            value as u16,
        );
    }

    #[inline]
    pub fn flags(self, heap: &Heap) -> u32 {
        heap.read_u32(self.field(Self::K_FLAGS_OFFSET))
    }

    #[inline]
    pub fn set_flags(self, heap: &mut Heap, value: u32) {
        heap.write_u32(self.field(Self::K_FLAGS_OFFSET), value);
    }

    #[inline]
    pub fn is_native(self, heap: &Heap) -> bool {
        self.flags(heap) & (1 << IS_NATIVE_BIT) != 0
    }

    pub fn set_is_native(self, heap: &mut Heap, value: bool) {
        let flags = self.flags(heap);
        let updated = if value {
            flags | (1 << IS_NATIVE_BIT)
        } else {
            flags & !(1 << IS_NATIVE_BIT)
        };
        self.set_flags(heap, updated);
    }

    pub fn builtin_id(self, heap: &Heap) -> i32 {
        debug_assert!(self.is_native(heap));
        crate::objects::smi::Smi::to_int(self.function_data(heap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        assert_eq!(SharedFunctionInfo::K_FUNCTION_DATA_OFFSET, 8);
        assert_eq!(SharedFunctionInfo::K_NAME_OFFSET, 16);
        assert_eq!(SharedFunctionInfo::K_FEEDBACK_METADATA_OFFSET, 24);
        assert_eq!(SharedFunctionInfo::K_LENGTH_OFFSET, 32);
        assert_eq!(SharedFunctionInfo::K_FORMAL_PARAMETER_COUNT_OFFSET, 34);
        assert_eq!(SharedFunctionInfo::K_FLAGS_OFFSET, 36);
        assert_eq!(SharedFunctionInfo::K_SIZE, 40);
    }
}

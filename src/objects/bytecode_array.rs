use crate::common::globals::{object_pointer_align, K_TAGGED_SIZE};
use crate::heap::heap::Heap;
use crate::objects::fixed_array::FixedArray;
use crate::objects::smi::Smi;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    BytecodeArray
);

impl BytecodeArray {
    pub const K_LENGTH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_HANDLER_TABLE_OFFSET: i32 = Self::K_LENGTH_OFFSET + K_TAGGED_SIZE;
    pub const K_SOURCE_POSITION_TABLE_OFFSET: i32 = Self::K_HANDLER_TABLE_OFFSET + K_TAGGED_SIZE;
    pub const K_CONSTANT_POOL_OFFSET: i32 = Self::K_SOURCE_POSITION_TABLE_OFFSET + K_TAGGED_SIZE;
    pub const K_FRAME_SIZE_OFFSET: i32 = Self::K_CONSTANT_POOL_OFFSET + K_TAGGED_SIZE;
    pub const K_PARAMETER_COUNT_OFFSET: i32 = Self::K_FRAME_SIZE_OFFSET + 4;
    pub const K_MAX_ARGUMENTS_OFFSET: i32 = Self::K_PARAMETER_COUNT_OFFSET + 2;
    pub const K_HEADER_SIZE: i32 = Self::K_MAX_ARGUMENTS_OFFSET + 2;

    #[inline]
    pub fn size_for(length: i32) -> i32 {
        object_pointer_align((Self::K_HEADER_SIZE + length) as u64) as i32
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        Smi::to_int(heap.read_tagged(self.field(Self::K_LENGTH_OFFSET)))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        heap.write_tagged(self.field(Self::K_LENGTH_OFFSET), Smi::from_int(length));
    }

    #[inline]
    pub fn handler_table(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_HANDLER_TABLE_OFFSET))
    }

    #[inline]
    pub fn set_handler_table(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_HANDLER_TABLE_OFFSET), value);
    }

    #[inline]
    pub fn handler_table_address(self) -> crate::objects::tagged::Address {
        self.field(Self::K_HANDLER_TABLE_OFFSET)
    }

    pub fn source_position_table(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_SOURCE_POSITION_TABLE_OFFSET))
    }

    #[inline]
    pub fn set_source_position_table(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_SOURCE_POSITION_TABLE_OFFSET), value);
    }

    #[inline]
    pub fn source_position_table_address(self) -> Address {
        self.field(Self::K_SOURCE_POSITION_TABLE_OFFSET)
    }

    #[inline]
    pub fn constant_pool(self, heap: &Heap) -> FixedArray {
        FixedArray::unchecked_cast(
            heap.read_tagged(self.field(Self::K_CONSTANT_POOL_OFFSET))
                .to_strong_heap_object()
                .expect("a BytecodeArray always has a constant pool"),
        )
    }

    #[inline]
    pub fn set_constant_pool(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_CONSTANT_POOL_OFFSET), value);
    }

    #[inline]
    pub fn constant_pool_address(self) -> Address {
        self.field(Self::K_CONSTANT_POOL_OFFSET)
    }

    #[inline]
    pub fn frame_size(self, heap: &Heap) -> i32 {
        heap.read_u32(self.field(Self::K_FRAME_SIZE_OFFSET)) as i32
    }

    #[inline]
    pub fn set_frame_size(self, heap: &mut Heap, value: i32) {
        heap.write_u32(self.field(Self::K_FRAME_SIZE_OFFSET), value as u32);
    }

    #[inline]
    pub fn register_count(self, heap: &Heap) -> i32 {
        self.frame_size(heap) / K_TAGGED_SIZE
    }

    pub fn set_register_count(self, heap: &mut Heap, register_count: i32) {
        self.set_frame_size(heap, register_count * K_TAGGED_SIZE);
    }

    #[inline]
    pub fn parameter_count(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_PARAMETER_COUNT_OFFSET)) as i32
    }

    #[inline]
    pub fn set_parameter_count(self, heap: &mut Heap, value: i32) {
        heap.write_u16(self.field(Self::K_PARAMETER_COUNT_OFFSET), value as u16);
    }

    #[inline]
    pub fn parameter_count_without_receiver(self, heap: &Heap) -> i32 {
        self.parameter_count(heap) - 1
    }

    #[inline]
    pub fn max_arguments(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_MAX_ARGUMENTS_OFFSET)) as i32
    }

    #[inline]
    pub fn set_max_arguments(self, heap: &mut Heap, value: i32) {
        heap.write_u16(self.field(Self::K_MAX_ARGUMENTS_OFFSET), value as u16);
    }

    #[inline]
    pub fn first_bytecode_address(self) -> Address {
        self.field(Self::K_HEADER_SIZE)
    }

    #[inline]
    pub fn get(self, heap: &Heap, offset: i32) -> u8 {
        debug_assert!(offset >= 0 && offset < self.length(heap));
        heap.read_u8(self.field(Self::K_HEADER_SIZE + offset))
    }

    pub fn set(self, heap: &mut Heap, offset: i32, value: u8) {
        debug_assert!(offset >= 0 && offset < self.length(heap));
        heap.write_u8(self.field(Self::K_HEADER_SIZE + offset), value);
    }

    pub fn bytes(self, heap: &Heap) -> &[u8] {
        heap.read_bytes(self.first_bytecode_address(), self.length(heap) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_puts_the_instructions_after_a_48_byte_header() {
        assert_eq!(BytecodeArray::K_LENGTH_OFFSET, 8);
        assert_eq!(BytecodeArray::K_HANDLER_TABLE_OFFSET, 16);
        assert_eq!(BytecodeArray::K_SOURCE_POSITION_TABLE_OFFSET, 24);
        assert_eq!(BytecodeArray::K_CONSTANT_POOL_OFFSET, 32);
        assert_eq!(BytecodeArray::K_FRAME_SIZE_OFFSET, 40);
        assert_eq!(BytecodeArray::K_PARAMETER_COUNT_OFFSET, 44);
        assert_eq!(BytecodeArray::K_MAX_ARGUMENTS_OFFSET, 46);
        assert_eq!(BytecodeArray::K_HEADER_SIZE, 48);
        assert_eq!(BytecodeArray::size_for(0), 48);
        assert_eq!(BytecodeArray::size_for(1), 56);
        assert_eq!(BytecodeArray::size_for(8), 56);
        assert_eq!(BytecodeArray::size_for(9), 64);
    }

    #[test]
    fn the_register_count_is_derived_from_the_frame_size() {
        let mut heap = Heap::new();
        let object = heap.allocate_raw_or_fail(
            BytecodeArray::size_for(0) as usize,
            crate::heap::heap::AllocationType::Old,
        );
        let array = BytecodeArray::unchecked_cast(object);
        array.set_register_count(&mut heap, 5);
        assert_eq!(array.frame_size(&heap), 40);
        assert_eq!(array.register_count(&heap), 5);
    }
}

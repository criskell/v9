//! Ported from `src/objects/bytecode-array.h` and `bytecode-array.tq`.
//!
//! A function's compiled bytecode, as a heap object: the instruction bytes
//! inline, plus the constant pool they index into and the frame size they
//! need.
//!
//! ```text
//!   0  map
//!   8  length                 Smi   -- bytes of instructions
//!  16  source_position_table  tagged
//!  24  constant_pool          tagged (FixedArray)
//!  32  frame_size             i32   -- in bytes
//!  36  parameter_count        u16   -- including the receiver
//!  38  max_arguments          u16
//!  40  bytes[length]
//! ```
//!
//! The constant pool has to be a heap object and not a side table, because
//! its entries *are* heap values -- the strings, the boxed numbers, the
//! nested function infos. Keeping it here is what makes them reachable from
//! the function and therefore traceable by the collector.
//!
//! Deviations, tracked in `PORTING.md`: no handler table (no `try`/`catch`),
//! no `incoming_new_target_or_generator_register` (no generators), and no
//! sandbox wrapper. V8 also puts this in trusted space so that corrupting it
//! cannot forge bytecode; `vx` has one heap.

use crate::common::globals::{object_pointer_align, K_TAGGED_SIZE};
use crate::heap::heap::Heap;
use crate::objects::fixed_array::FixedArray;
use crate::objects::smi::Smi;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `BytecodeArray`
    BytecodeArray
);

impl BytecodeArray {
    pub const K_LENGTH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_SOURCE_POSITION_TABLE_OFFSET: i32 = Self::K_LENGTH_OFFSET + K_TAGGED_SIZE;
    pub const K_CONSTANT_POOL_OFFSET: i32 = Self::K_SOURCE_POSITION_TABLE_OFFSET + K_TAGGED_SIZE;
    pub const K_FRAME_SIZE_OFFSET: i32 = Self::K_CONSTANT_POOL_OFFSET + K_TAGGED_SIZE;
    pub const K_PARAMETER_COUNT_OFFSET: i32 = Self::K_FRAME_SIZE_OFFSET + 4;
    pub const K_MAX_ARGUMENTS_OFFSET: i32 = Self::K_PARAMETER_COUNT_OFFSET + 2;
    /// `BytecodeArray::kHeaderSize`
    pub const K_HEADER_SIZE: i32 = Self::K_MAX_ARGUMENTS_OFFSET + 2;

    /// `BytecodeArray::SizeFor`
    #[inline]
    pub fn size_for(length: i32) -> i32 {
        object_pointer_align((Self::K_HEADER_SIZE + length) as u64) as i32
    }

    /// `BytecodeArray::length`
    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        Smi::to_int(heap.read_tagged(self.field(Self::K_LENGTH_OFFSET)))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        heap.write_tagged(self.field(Self::K_LENGTH_OFFSET), Smi::from_int(length));
    }

    #[inline]
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

    /// `BytecodeArray::constant_pool`
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

    /// `BytecodeArray::frame_size`, in bytes. The register count is derived
    /// from it, as in V8 -- the frame size is what the interpreter actually
    /// needs to reserve.
    #[inline]
    pub fn frame_size(self, heap: &Heap) -> i32 {
        heap.read_u32(self.field(Self::K_FRAME_SIZE_OFFSET)) as i32
    }

    #[inline]
    pub fn set_frame_size(self, heap: &mut Heap, value: i32) {
        heap.write_u32(self.field(Self::K_FRAME_SIZE_OFFSET), value as u32);
    }

    /// `BytecodeArray::register_count`
    #[inline]
    pub fn register_count(self, heap: &Heap) -> i32 {
        self.frame_size(heap) / K_TAGGED_SIZE
    }

    pub fn set_register_count(self, heap: &mut Heap, register_count: i32) {
        self.set_frame_size(heap, register_count * K_TAGGED_SIZE);
    }

    /// `BytecodeArray::parameter_count`, including the receiver.
    #[inline]
    pub fn parameter_count(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_PARAMETER_COUNT_OFFSET)) as i32
    }

    #[inline]
    pub fn set_parameter_count(self, heap: &mut Heap, value: i32) {
        heap.write_u16(self.field(Self::K_PARAMETER_COUNT_OFFSET), value as u16);
    }

    /// `BytecodeArray::parameter_count_without_receiver`
    #[inline]
    pub fn parameter_count_without_receiver(self, heap: &Heap) -> i32 {
        self.parameter_count(heap) - 1
    }

    /// `BytecodeArray::max_arguments`: the largest argument count any call in
    /// this function passes, which the frame has to have room for.
    #[inline]
    pub fn max_arguments(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_MAX_ARGUMENTS_OFFSET)) as i32
    }

    #[inline]
    pub fn set_max_arguments(self, heap: &mut Heap, value: i32) {
        heap.write_u16(self.field(Self::K_MAX_ARGUMENTS_OFFSET), value as u16);
    }

    /// `BytecodeArray::GetFirstBytecodeAddress`
    #[inline]
    pub fn first_bytecode_address(self) -> Address {
        self.field(Self::K_HEADER_SIZE)
    }

    /// `BytecodeArray::get`
    #[inline]
    pub fn get(self, heap: &Heap, offset: i32) -> u8 {
        debug_assert!(offset >= 0 && offset < self.length(heap));
        heap.read_u8(self.field(Self::K_HEADER_SIZE + offset))
    }

    pub fn set(self, heap: &mut Heap, offset: i32, value: u8) {
        debug_assert!(offset >= 0 && offset < self.length(heap));
        heap.write_u8(self.field(Self::K_HEADER_SIZE + offset), value);
    }

    /// The instruction bytes, for the iterator and the disassembler.
    pub fn bytes(self, heap: &Heap) -> &[u8] {
        heap.read_bytes(self.first_bytecode_address(), self.length(heap) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_puts_the_instructions_after_a_40_byte_header() {
        assert_eq!(BytecodeArray::K_LENGTH_OFFSET, 8);
        assert_eq!(BytecodeArray::K_SOURCE_POSITION_TABLE_OFFSET, 16);
        assert_eq!(BytecodeArray::K_CONSTANT_POOL_OFFSET, 24);
        assert_eq!(BytecodeArray::K_FRAME_SIZE_OFFSET, 32);
        assert_eq!(BytecodeArray::K_PARAMETER_COUNT_OFFSET, 36);
        assert_eq!(BytecodeArray::K_MAX_ARGUMENTS_OFFSET, 38);
        assert_eq!(BytecodeArray::K_HEADER_SIZE, 40);
        // Sizes are word aligned, so the instructions round up.
        assert_eq!(BytecodeArray::size_for(0), 40);
        assert_eq!(BytecodeArray::size_for(1), 48);
        assert_eq!(BytecodeArray::size_for(8), 48);
        assert_eq!(BytecodeArray::size_for(9), 56);
    }

    #[test]
    fn the_register_count_is_derived_from_the_frame_size() {
        // Which is why V8 stores only the frame size: the two cannot
        // disagree.
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

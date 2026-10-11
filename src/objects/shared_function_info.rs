//! Ported from `src/objects/shared-function-info.h` and
//! `shared-function-info.tq`.
//!
//! Everything about a function *literal* that every closure over it shares:
//! its name, its compiled bytecode, how many parameters it declares. A
//! `function` written once inside a loop produces many [`JSFunction`]s and
//! exactly one `SharedFunctionInfo`, which is the point -- the bytecode is
//! compiled once.
//!
//! ```text
//!   0  map
//!   8  function_data           tagged (BytecodeArray)
//!  16  name                    tagged (String)
//!  24  length                  u16   -- declared parameters, no receiver
//!  26  formal_parameter_count  u16   -- including the receiver
//!  28  flags                   u32
//!  32  == SharedFunctionInfo::kSize
//! ```
//!
//! Deviations, tracked in `PORTING.md`: V8 splits `function_data` into a
//! trusted and an untrusted half for the sandbox, and overloads it to hold a
//! `Code` object, a lazy-compilation stub or a builtin id depending on the
//! function's compilation state. `vx` compiles eagerly and has one tier, so
//! it holds a `BytecodeArray` or `undefined` for a native. There is also no
//! `ScopeInfo` (no closures yet) and no `Script`.
//!
//! [`JSFunction`]: crate::objects::js_function::JSFunction

use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::bytecode_array::BytecodeArray;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `SharedFunctionInfo`
    SharedFunctionInfo
);

/// `SharedFunctionInfo::flags`, reduced to what `vx` tracks.
const IS_NATIVE_BIT: u32 = 0;

impl SharedFunctionInfo {
    pub const K_FUNCTION_DATA_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_NAME_OFFSET: i32 = Self::K_FUNCTION_DATA_OFFSET + K_TAGGED_SIZE;
    pub const K_LENGTH_OFFSET: i32 = Self::K_NAME_OFFSET + K_TAGGED_SIZE;
    pub const K_FORMAL_PARAMETER_COUNT_OFFSET: i32 = Self::K_LENGTH_OFFSET + 2;
    pub const K_FLAGS_OFFSET: i32 = Self::K_FORMAL_PARAMETER_COUNT_OFFSET + 2;
    /// `SharedFunctionInfo::kSize`
    pub const K_SIZE: i32 = Self::K_FLAGS_OFFSET + 4;

    /// `SharedFunctionInfo::function_data`
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

    /// `SharedFunctionInfo::GetBytecodeArray`
    #[inline]
    pub fn bytecode_array(self, heap: &Heap) -> BytecodeArray {
        BytecodeArray::unchecked_cast(
            self.function_data(heap)
                .to_strong_heap_object()
                .expect("this function has no bytecode"),
        )
    }

    /// `SharedFunctionInfo::HasBytecodeArray`
    #[inline]
    pub fn has_bytecode_array(self, heap: &Heap) -> bool {
        !self.is_native(heap) && self.function_data(heap).is_strong()
    }

    /// `SharedFunctionInfo::Name`
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

    /// `SharedFunctionInfo::length`: declared parameters, receiver excluded.
    /// This is what `Function.prototype.length` reports.
    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        heap.read_u16(self.field(Self::K_LENGTH_OFFSET)) as i32
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, value: i32) {
        heap.write_u16(self.field(Self::K_LENGTH_OFFSET), value as u16);
    }

    /// `SharedFunctionInfo::internal_formal_parameter_count_with_receiver`
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

    /// `SharedFunctionInfo::native`: implemented in Rust rather than in
    /// JavaScript. `function_data` then holds the builtin's id as a Smi.
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

    /// The builtin id of a native function, from `function_data`.
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
        assert_eq!(SharedFunctionInfo::K_LENGTH_OFFSET, 24);
        assert_eq!(SharedFunctionInfo::K_FORMAL_PARAMETER_COUNT_OFFSET, 26);
        assert_eq!(SharedFunctionInfo::K_FLAGS_OFFSET, 28);
        assert_eq!(SharedFunctionInfo::K_SIZE, 32);
    }
}

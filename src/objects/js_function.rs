//! Ported from `src/objects/js-function.h` and `js-function.tq`.
//!
//! A callable object. `JSFunction` extends `JSObject`, so it carries the same
//! properties/elements header and then its own fields, which is why a
//! function can have properties like any other object.

use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::js_objects::JSObject;
use crate::objects::tagged::Tagged;

object_type!(
    /// `JSFunction`.
    ///
    /// Deviation: V8's `JSFunction` points at a `SharedFunctionInfo` (the
    /// per-function-literal data) and a `Context` (the captured scope). `vx`
    /// has neither yet -- the bytecode pipeline is where they arrive -- so
    /// this carries the shared-info *index* as a Smi into a side table and a
    /// feedback vector. See `PORTING.md`.
    JSFunction
);

impl JSFunction {
    pub const K_PROPERTIES_OR_HASH_OFFSET: i32 = JSObject::K_PROPERTIES_OR_HASH_OFFSET;
    pub const K_ELEMENTS_OFFSET: i32 = JSObject::K_ELEMENTS_OFFSET;
    /// Index into the isolate's code store, or a builtin id for natives.
    pub const K_SHARED_FUNCTION_INFO_OFFSET: i32 = JSObject::K_HEADER_SIZE;
    pub const K_FEEDBACK_VECTOR_OFFSET: i32 = Self::K_SHARED_FUNCTION_INFO_OFFSET + K_TAGGED_SIZE;
    pub const K_SIZE: i32 = Self::K_FEEDBACK_VECTOR_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub fn as_js_object(self) -> JSObject {
        JSObject::unchecked_cast(self.object())
    }

    #[inline]
    pub fn shared_function_info(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET))
    }

    #[inline]
    pub fn set_shared_function_info(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET), value);
    }

    #[inline]
    pub fn feedback_vector(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_FEEDBACK_VECTOR_OFFSET))
    }

    #[inline]
    pub fn set_feedback_vector(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_FEEDBACK_VECTOR_OFFSET), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_own_fields_come_after_the_js_object_header() {
        assert_eq!(
            JSFunction::K_SHARED_FUNCTION_INFO_OFFSET,
            JSObject::K_HEADER_SIZE
        );
        assert_eq!(JSFunction::K_SHARED_FUNCTION_INFO_OFFSET, 24);
        assert_eq!(JSFunction::K_FEEDBACK_VECTOR_OFFSET, 32);
        assert_eq!(JSFunction::K_SIZE, 40);
        // It shares the JSObject header, so a function has properties and
        // elements like any other object.
        assert_eq!(
            JSFunction::K_PROPERTIES_OR_HASH_OFFSET,
            JSObject::K_PROPERTIES_OR_HASH_OFFSET
        );
        assert_eq!(JSFunction::K_ELEMENTS_OFFSET, JSObject::K_ELEMENTS_OFFSET);
    }
}

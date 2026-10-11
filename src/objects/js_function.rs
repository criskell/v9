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
    /// `JSFunction`: a closure.
    ///
    /// Points at the [`SharedFunctionInfo`] holding the code, which is what
    /// every closure over the same function literal shares.
    ///
    /// Deviation: V8 also stores the captured `Context` here. `vx` has no
    /// contexts yet, so a function can only reach its own locals and the
    /// globals. See `PORTING.md`.
    ///
    /// [`SharedFunctionInfo`]: crate::objects::shared_function_info::SharedFunctionInfo
    JSFunction
);

impl JSFunction {
    pub const K_PROPERTIES_OR_HASH_OFFSET: i32 = JSObject::K_PROPERTIES_OR_HASH_OFFSET;
    pub const K_ELEMENTS_OFFSET: i32 = JSObject::K_ELEMENTS_OFFSET;
    pub const K_SHARED_FUNCTION_INFO_OFFSET: i32 = JSObject::K_HEADER_SIZE;
    /// `JSFunction::context`: the context this closure was created in.
    ///
    /// This single field is what makes a closure a closure. Two closures
    /// made in the same call hold the same context and therefore share the
    /// captured variable; two made in different calls hold different ones.
    pub const K_CONTEXT_OFFSET: i32 = Self::K_SHARED_FUNCTION_INFO_OFFSET + K_TAGGED_SIZE;
    pub const K_FEEDBACK_VECTOR_OFFSET: i32 = Self::K_CONTEXT_OFFSET + K_TAGGED_SIZE;
    pub const K_SIZE: i32 = Self::K_FEEDBACK_VECTOR_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub fn as_js_object(self) -> JSObject {
        JSObject::unchecked_cast(self.object())
    }

    #[inline]
    pub fn raw_shared_function_info(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET))
    }

    /// `JSFunction::shared`
    #[inline]
    pub fn shared(self, heap: &Heap) -> crate::objects::shared_function_info::SharedFunctionInfo {
        crate::objects::shared_function_info::SharedFunctionInfo::unchecked_cast(
            self.raw_shared_function_info(heap)
                .to_strong_heap_object()
                .expect("a JSFunction always has a SharedFunctionInfo"),
        )
    }

    #[inline]
    pub fn set_shared_function_info(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET), value);
    }

    /// `JSFunction::context`
    #[inline]
    pub fn context(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_CONTEXT_OFFSET))
    }

    #[inline]
    pub fn set_context(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_CONTEXT_OFFSET), value);
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
        assert_eq!(JSFunction::K_CONTEXT_OFFSET, 32);
        assert_eq!(JSFunction::K_FEEDBACK_VECTOR_OFFSET, 40);
        assert_eq!(JSFunction::K_SIZE, 48);
        // It shares the JSObject header, so a function has properties and
        // elements like any other object.
        assert_eq!(
            JSFunction::K_PROPERTIES_OR_HASH_OFFSET,
            JSObject::K_PROPERTIES_OR_HASH_OFFSET
        );
        assert_eq!(JSFunction::K_ELEMENTS_OFFSET, JSObject::K_ELEMENTS_OFFSET);
    }
}

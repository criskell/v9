use crate::heap::heap::Heap;
use crate::objects::js_objects::JSObject;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

object_type!(
    // The JSArray describes JavaScript Arrays
    //  Such an array can be in one of two modes:
    //    - fast, backing storage is a FixedArray and length <=
    //       elements.length();
    //       Please note: push and pop can be used to grow and shrink the
    //       array.
    //    - slow, backing storage is a HashTable with numbers as keys.
    JSArray
);

impl JSArray {
    // [length]: The length property.
    pub const K_LENGTH_OFFSET: i32 = JSObject::K_HEADER_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_LENGTH_OFFSET + crate::common::globals::K_TAGGED_SIZE;
    pub const K_SIZE: i32 = Self::K_HEADER_SIZE;

    // Valid array indices range from +0 <= i < 2^32 - 1 (kMaxUInt32).
    pub const K_MAX_ARRAY_LENGTH: u32 = u32::MAX;
    pub const K_MAX_ARRAY_INDEX: u32 = u32::MAX - 1;

    // This constant is somewhat arbitrary. Any large enough value would work.
    pub const K_MAX_FAST_ARRAY_LENGTH: u32 = 32 * 1024 * 1024;

    #[inline]
    pub fn as_js_object(self) -> JSObject {
        JSObject::unchecked_cast(self.object())
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_LENGTH_OFFSET))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_LENGTH_OFFSET), value);
    }

    #[inline]
    pub fn length_value(self, heap: &Heap) -> u32 {
        Smi::to_int(self.length(heap)) as u32
    }

    #[inline]
    pub fn set_length_value(self, heap: &mut Heap, length: u32) {
        self.set_length(heap, Smi::from_int(length as i32));
    }

    #[inline]
    pub fn elements(self, heap: &Heap) -> Tagged {
        self.as_js_object().elements(heap)
    }

    #[inline]
    pub fn set_elements(self, heap: &mut Heap, value: Tagged) {
        self.as_js_object().set_elements(heap, value);
    }
}

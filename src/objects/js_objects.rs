//! Ported from `src/objects/js-objects.h` and `js-objects.tq`.
//!
//! ```text
//!   0  map                   (HeapObject)
//!   8  properties_or_hash    (JSReceiver)  -- the out-of-object store
//!  16  elements              (JSObject)    -- indexed properties
//!  24  inline property slots ...           -- as many as the map says
//! ```
//!
//! A property lives in one of two places, and which one is decided when the
//! property is added, never later: the first few go in the inline slots
//! inside the object, and the rest in the `PropertyArray` hanging off
//! `properties_or_hash`. The map records the split, so a load is still a
//! fixed offset either way.
//!
//! V8 encodes an out-of-object field index as a negative number in the
//! property details; `vx` keeps the same convention via
//! [`JSObject::property_address`].

use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::fixed_array::PropertyArray;
use crate::objects::map::Map;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `JSObject`
    JSObject
);

impl JSObject {
    /// `JSReceiver::kPropertiesOrHashOffset`
    pub const K_PROPERTIES_OR_HASH_OFFSET: i32 = K_TAGGED_SIZE;
    /// `JSObject::kElementsOffset`
    pub const K_ELEMENTS_OFFSET: i32 = Self::K_PROPERTIES_OR_HASH_OFFSET + K_TAGGED_SIZE;
    /// `JSObject::kHeaderSize`: where the inline property slots begin.
    pub const K_HEADER_SIZE: i32 = Self::K_ELEMENTS_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub fn raw_properties_or_hash(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_PROPERTIES_OR_HASH_OFFSET))
    }

    #[inline]
    pub fn set_raw_properties_or_hash(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_PROPERTIES_OR_HASH_OFFSET), value);
    }

    #[inline]
    pub fn properties_or_hash_address(self) -> Address {
        self.field(Self::K_PROPERTIES_OR_HASH_OFFSET)
    }

    /// `JSObject::property_array`
    #[inline]
    pub fn property_array(self, heap: &Heap) -> PropertyArray {
        PropertyArray::unchecked_cast(
            self.raw_properties_or_hash(heap)
                .to_strong_heap_object()
                .expect("a JSObject always has a property backing store"),
        )
    }

    #[inline]
    pub fn elements(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_ELEMENTS_OFFSET))
    }

    #[inline]
    pub fn set_elements(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_ELEMENTS_OFFSET), value);
    }

    #[inline]
    pub fn elements_address(self) -> Address {
        self.field(Self::K_ELEMENTS_OFFSET)
    }

    #[inline]
    pub fn map(self, heap: &Heap) -> Map {
        heap.map_of(self.object())
    }

    /// `JSObject::RawFastPropertyAt` for an inline slot.
    #[inline]
    pub fn in_object_property_at(self, heap: &Heap, map: Map, index: i32) -> Tagged {
        debug_assert!(index < map.inobject_properties(heap));
        heap.read_tagged(self.field(map.inobject_property_offset(heap, index)))
    }

    #[inline]
    pub fn set_in_object_property_at(self, heap: &mut Heap, map: Map, index: i32, value: Tagged) {
        debug_assert!(index < map.inobject_properties(heap));
        let offset = map.inobject_property_offset(heap, index);
        heap.write_tagged(self.field(offset), value);
    }

    /// Reads the property at a map-assigned `field_index`, picking the inline
    /// slot or the backing store as the map dictates. This -- a map check
    /// followed by one indexed read -- is the whole point of hidden classes,
    /// and it is exactly what an inline cache memoizes.
    pub fn fast_property_at(self, heap: &Heap, field_index: i32) -> Tagged {
        let map = self.map(heap);
        let inobject = map.inobject_properties(heap);
        if field_index < inobject {
            self.in_object_property_at(heap, map, field_index)
        } else {
            self.property_array(heap).get(heap, field_index - inobject)
        }
    }

    pub fn set_fast_property_at(self, heap: &mut Heap, field_index: i32, value: Tagged) {
        let map = self.map(heap);
        let inobject = map.inobject_properties(heap);
        if field_index < inobject {
            self.set_in_object_property_at(heap, map, field_index, value);
        } else {
            self.property_array(heap)
                .set(heap, field_index - inobject, value);
        }
    }

    /// The address of a field, for the GC to visit and for inline caches to
    /// remember.
    pub fn property_address(self, heap: &Heap, field_index: i32) -> Address {
        let map = self.map(heap);
        let inobject = map.inobject_properties(heap);
        if field_index < inobject {
            self.field(map.inobject_property_offset(heap, field_index))
        } else {
            self.property_array(heap)
                .element_address(field_index - inobject)
        }
    }

    /// How many out-of-object slots a map with `field_count` fields needs.
    pub fn out_of_object_property_count(heap: &Heap, map: Map, field_count: i32) -> i32 {
        (field_count - map.inobject_properties(heap)).max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_leaves_the_header_at_24_bytes() {
        assert_eq!(JSObject::K_PROPERTIES_OR_HASH_OFFSET, 8);
        assert_eq!(JSObject::K_ELEMENTS_OFFSET, 16);
        assert_eq!(JSObject::K_HEADER_SIZE, 24);
    }
}

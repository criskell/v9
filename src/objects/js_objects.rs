use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::elements_kind::ElementsKind;
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
    pub const K_HEADER_SIZE: i32 = Self::K_ELEMENTS_OFFSET + K_TAGGED_SIZE;

    // Maximal gap that can be introduced by adding an element beyond
    // the current elements length.
    pub const K_MAX_GAP: u32 = 1024;

    // Maximal length of fast elements array that won't be checked for
    // being dense enough on expansion.
    pub const K_MAX_UNCHECKED_FAST_ELEMENTS_LENGTH: u32 = 5000;

    pub const K_MIN_ADDED_ELEMENTS_CAPACITY: u32 = 16;

    /// `JSObject::NewElementsCapacity`
    // Computes the new capacity when expanding the elements of a JSObject.
    #[inline]
    pub const fn new_elements_capacity(old_capacity: u32) -> u32 {
        // (old_capacity + 50%) + kMinAddedElementsCapacity
        old_capacity + (old_capacity >> 1) + Self::K_MIN_ADDED_ELEMENTS_CAPACITY
    }

    /// `JSObject::GetElementsKind`
    #[inline]
    pub fn get_elements_kind(self, heap: &Heap) -> ElementsKind {
        heap.map_of(self.object()).elements_kind(heap)
    }

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

    pub fn out_of_object_property_count(heap: &Heap, map: Map, field_count: i32) -> i32 {
        (field_count - map.inobject_properties(heap)).max(0)
    }
}

impl crate::execution::isolate::Isolate {
    /// `ShouldConvertToSlowElements`
    pub(crate) fn should_convert_to_slow_elements(&self, capacity: u32, index: u32) -> (bool, u32) {
        if index < capacity {
            return (false, capacity);
        }
        if index - capacity >= JSObject::K_MAX_GAP {
            return (true, capacity);
        }
        let new_capacity = JSObject::new_elements_capacity(index + 1);
        (false, new_capacity)
    }

    /// `ShouldConvertToFastElements`
    pub(crate) fn should_convert_to_fast_elements(
        &self,
        object: JSObject,
        dictionary: crate::objects::dictionary::NumberDictionary,
        index: u32,
    ) -> (bool, u32) {
        use crate::objects::instance_type::JS_ARRAY_TYPE;
        use crate::objects::js_array::JSArray;

        // If properties with non-standard attributes or accessors were added,
        // we cannot go back to fast elements.
        if dictionary.requires_slow_elements(&self.heap) {
            return (false, 0);
        }

        // Adding a property with this index will require slow elements.
        if index >= crate::objects::smi::Smi::MAX_VALUE as u32 {
            return (false, 0);
        }

        let mut new_capacity = if self.heap.instance_type(object.object()) == JS_ARRAY_TYPE {
            JSArray::unchecked_cast(object.object()).length_value(&self.heap)
        } else {
            dictionary.max_number_key(&self.heap) + 1
        };
        new_capacity = new_capacity.max(index + 1);

        let dictionary_size = dictionary.capacity(&self.heap)
            * crate::objects::dictionary::NUMBER_DICTIONARY_SHAPE.entry_size as u32;

        // Turn fast if the dictionary only saves 50% space.
        (2 * dictionary_size >= new_capacity, new_capacity)
    }

    /// `JSObject::BestFittingFastElementsKind`
    pub(crate) fn best_fitting_fast_elements_kind(&self, object: JSObject) -> ElementsKind {
        use crate::objects::elements_kind::ElementsKind::*;
        let _ = object;
        HoleyElements
    }

    /// `JSObject::GetElementsTransitionMap`
    pub(crate) fn get_elements_transition_map(
        &mut self,
        object: JSObject,
        to_kind: ElementsKind,
    ) -> Tagged {
        use crate::objects::instance_type::JS_ARRAY_TYPE;
        let map = self.heap.map_of(object.object());
        if map.instance_type(&self.heap) == JS_ARRAY_TYPE {
            if to_kind == ElementsKind::DictionaryElements {
                return self.roots.js_array_dictionary_elements_map;
            }
            return self.roots.get_initial_js_array_map(to_kind);
        }
        map.set_elements_kind(&mut self.heap, to_kind);
        map.tagged()
    }

    /// `JSObject::SetMapAndElements`
    pub(crate) fn set_map_and_elements(
        &mut self,
        object: JSObject,
        new_map: Tagged,
        elements: Tagged,
    ) {
        let map = new_map.to_strong_heap_object().expect("a map");
        self.heap.set_map(object.object(), map);
        object.set_elements(&mut self.heap, elements);
    }

    /// `JSObject::TransitionElementsKind`
    pub(crate) fn transition_elements_kind(&mut self, object: JSObject, to_kind: ElementsKind) {
        use crate::objects::elements_kind as ek;
        let from_kind = object.get_elements_kind(&self.heap);
        let mut to_kind = to_kind;

        if ek::is_holey_elements_kind(from_kind) {
            to_kind = ek::get_holey_elements_kind(to_kind);
        }

        if from_kind == to_kind {
            return;
        }

        let elements = object.elements(&self.heap);
        if elements == self.roots.empty_fixed_array
            || ek::is_double_elements_kind(from_kind) == ek::is_double_elements_kind(to_kind)
        {
            // No change is needed to the elements() buffer, the transition
            // only requires a map change.
            let new_map = self.get_elements_transition_map(object, to_kind);
            let map = new_map.to_strong_heap_object().expect("a map");
            self.heap.set_map(object.object(), map);
        } else {
            let capacity = crate::objects::elements::ElementsAccessor::for_kind(from_kind)
                .get_capacity(&self.heap, elements);
            self.basic_grow_capacity_and_convert(object, from_kind, to_kind, capacity);
        }
    }

    /// `JSObject::AddDataElement`
    pub(crate) fn add_data_element(&mut self, object: JSObject, index: u32, value: Tagged) {
        use crate::objects::elements_kind as ek;
        use crate::objects::instance_type::JS_ARRAY_TYPE;
        use crate::objects::js_array::JSArray;

        let is_js_array =
            self.heap.map_of(object.object()).instance_type(&self.heap) == JS_ARRAY_TYPE;
        let array = JSArray::unchecked_cast(object.object());
        let old_length = if is_js_array {
            array.length_value(&self.heap)
        } else {
            0
        };

        let mut kind = object.get_elements_kind(&self.heap);
        let elements = object.elements(&self.heap);
        let capacity = crate::objects::elements::ElementsAccessor::for_kind(kind)
            .get_capacity(&self.heap, elements);
        let dictionary_kind = ek::ElementsKind::DictionaryElements;
        let mut new_capacity = capacity;
        if ek::is_dictionary_elements_kind(kind) {
            let dictionary = crate::objects::dictionary::NumberDictionary::unchecked_cast(
                elements.to_strong_heap_object().expect("a dictionary"),
            );
            let (to_fast, fast_capacity) =
                self.should_convert_to_fast_elements(object, dictionary, index);
            if to_fast {
                new_capacity = fast_capacity;
                kind = self.best_fitting_fast_elements_kind(object);
            } else {
                kind = dictionary_kind;
            }
        } else {
            let (to_slow, grown) = self.should_convert_to_slow_elements(capacity, index);
            new_capacity = grown;
            if to_slow {
                kind = dictionary_kind;
            }
        }

        if kind == dictionary_kind {
            self.normalize_elements(object);
            let accessor = crate::objects::elements::ElementsAccessor::for_kind(dictionary_kind);
            accessor.set(self, object, index, value);
            if is_js_array && index >= old_length {
                array.set_length_value(&mut self.heap, index + 1);
            }
            return;
        }

        let mut to = ek::optimal_elements_kind(&self.heap, value);
        if ek::is_holey_elements_kind(kind) || !is_js_array || index > old_length {
            to = ek::get_holey_elements_kind(to);
            kind = ek::get_holey_elements_kind(kind);
        }
        let to = ek::get_more_general_elements_kind(kind, to);
        self.add_element(object, index, value, to, new_capacity);

        if is_js_array && index >= old_length {
            array.set_length_value(&mut self.heap, index + 1);
        }
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

use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::elements_kind::ElementsKind;
use crate::objects::fixed_array::PropertyArray;
use crate::objects::map::Map;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    JSObject
);

impl JSObject {
    pub const K_PROPERTIES_OR_HASH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_ELEMENTS_OFFSET: i32 = Self::K_PROPERTIES_OR_HASH_OFFSET + K_TAGGED_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_ELEMENTS_OFFSET + K_TAGGED_SIZE;

    // Maximal gap that can be introduced by adding an element beyond
    // the current elements length.
    pub const K_MAX_GAP: u32 = 1024;

    // Maximal length of fast elements array that won't be checked for
    // being dense enough on expansion.
    pub const K_MAX_UNCHECKED_FAST_ELEMENTS_LENGTH: u32 = 5000;

    pub const K_MIN_ADDED_ELEMENTS_CAPACITY: u32 = 16;

    // Computes the new capacity when expanding the elements of a JSObject.
    #[inline]
    pub const fn new_elements_capacity(old_capacity: u32) -> u32 {
        // (old_capacity + 50%) + kMinAddedElementsCapacity
        old_capacity + (old_capacity >> 1) + Self::K_MIN_ADDED_ELEMENTS_CAPACITY
    }

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

    #[inline]
    pub fn property_dictionary(self, heap: &Heap) -> crate::objects::dictionary::NameDictionary {
        crate::objects::dictionary::NameDictionary::unchecked_cast(
            self.raw_properties_or_hash(heap)
                .to_strong_heap_object()
                .expect("a dictionary-mode JSObject has a NameDictionary"),
        )
    }

    #[inline]
    pub fn has_fast_properties(self, heap: &Heap) -> bool {
        !heap.map_of(self.object()).is_dictionary_map(heap)
    }

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

    pub(crate) fn best_fitting_fast_elements_kind(&self, object: JSObject) -> ElementsKind {
        use crate::objects::elements_kind::ElementsKind::*;
        let _ = object;
        HoleyElements
    }

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
            let scope = self.handle_scope();
            let object_handle = self.create_object_handle(object.object());
            let value_handle = self.create_handle(value);
            self.normalize_elements(object);
            let accessor = crate::objects::elements::ElementsAccessor::for_kind(dictionary_kind);
            let object = JSObject::unchecked_cast(object_handle.object(self));
            let value = value_handle.get(self);
            accessor.set(self, object, index, value);
            if is_js_array && index >= old_length {
                let array = JSArray::unchecked_cast(object_handle.object(self));
                array.set_length_value(&mut self.heap, index + 1);
            }
            self.close_handle_scope(scope);
            return;
        }

        let mut to = ek::optimal_elements_kind(&self.heap, value);
        if ek::is_holey_elements_kind(kind) || !is_js_array || index > old_length {
            to = ek::get_holey_elements_kind(to);
            kind = ek::get_holey_elements_kind(kind);
        }
        let to = ek::get_more_general_elements_kind(kind, to);
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        self.add_element(object, index, value, to, new_capacity);

        if is_js_array && index >= old_length {
            let array = JSArray::unchecked_cast(object_handle.object(self));
            array.set_length_value(&mut self.heap, index + 1);
        }
        self.close_handle_scope(scope);
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OrdinaryToPrimitiveHint {
    Number,
    String,
}

impl crate::execution::isolate::Isolate {
    pub(crate) fn js_receiver_to_primitive(
        &mut self,
        receiver: JSObject,
        hint: crate::objects::objects::ToPrimitiveHint,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        use crate::objects::objects::ToPrimitiveHint;

        self.ordinary_to_primitive(
            receiver,
            if hint == ToPrimitiveHint::String {
                OrdinaryToPrimitiveHint::String
            } else {
                OrdinaryToPrimitiveHint::Number
            },
        )
    }

    pub(crate) fn ordinary_to_primitive(
        &mut self,
        receiver: JSObject,
        hint: OrdinaryToPrimitiveHint,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        use crate::interpreter::interpreter::Thrown;

        let method_names = match hint {
            OrdinaryToPrimitiveHint::Number => ["valueOf", "toString"],
            OrdinaryToPrimitiveHint::String => ["toString", "valueOf"],
        };

        let scope = self.handle_scope();
        let receiver_handle = self.create_object_handle(receiver.object());
        for name in method_names {
            let name = self.internalize_string(name).tagged();
            let name = self.create_handle(name);
            let holder = JSObject::unchecked_cast(receiver_handle.object(self));
            let name = name.get(self);
            let method = self.get_property_with_accessors(holder, name)?;
            if self.is_callable_value(method) {
                let method = self.create_handle(method);
                let receiver = receiver_handle.get(self);
                let function = crate::objects::js_function::JSFunction::unchecked_cast(
                    method.object(self),
                );
                let result = self.execute(function, receiver, &[])?;
                if self.is_primitive(result) {
                    self.close_handle_scope(scope);
                    return Ok(result);
                }
            }
        }
        self.close_handle_scope(scope);
        let message = self
            .new_string("Cannot convert object to primitive value")
            .tagged();
        Err(Thrown(message))
    }

    pub(crate) fn delete_normalized_property(&mut self, object: JSObject, entry: i32) {
        debug_assert!(!object.has_fast_properties(&self.heap));
        let dictionary = object.property_dictionary(&self.heap);
        let updated = self.name_dictionary_delete(dictionary, entry).tagged();
        object.set_raw_properties_or_hash(&mut self.heap, updated);
    }

    pub(crate) fn migrate_to_map(&mut self, object: JSObject, new_map: Map) {
        let old_map = object.map(&self.heap);
        if old_map == new_map {
            return;
        }
        if old_map.is_dictionary_map(&self.heap) {
            debug_assert!(new_map.is_dictionary_map(&self.heap));
            self.heap.set_map(object.object(), new_map.object());
        } else if !new_map.is_dictionary_map(&self.heap) {
            self.migrate_fast_to_fast(object, new_map);
        } else {
            self.migrate_fast_to_slow(object, new_map);
        }
    }

    pub(crate) fn migrate_fast_to_fast(&mut self, object: JSObject, new_map: Map) {
        let inobject = new_map.inobject_properties(&self.heap);
        let number_of_fields = new_map.number_of_own_descriptors(&self.heap);
        if number_of_fields > inobject {
            let scope = self.handle_scope();
            let object_handle = self.create_object_handle(object.object());
            let new_map_handle = self.create_object_handle(new_map.object());
            self.grow_property_array(object, number_of_fields - inobject);
            let object = JSObject::unchecked_cast(object_handle.object(self));
            let new_map = new_map_handle.object(self);
            self.heap.set_map(object.object(), new_map);
            self.close_handle_scope(scope);
            return;
        }
        self.heap.set_map(object.object(), new_map.object());
    }

    pub(crate) fn set_normalized_property(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        details: crate::objects::property_details::PropertyDetails,
    ) {
        use crate::objects::hash_table::K_NOT_FOUND;

        debug_assert!(!object.has_fast_properties(&self.heap));
        let dictionary = object.property_dictionary(&self.heap);
        let entry = dictionary.find_entry(&self.heap, name);

        if entry == K_NOT_FOUND {
            let scope = self.handle_scope();
            let object_handle = self.create_object_handle(object.object());
            let dictionary = self
                .name_dictionary_add(dictionary, name, value, details)
                .tagged();
            let object = JSObject::unchecked_cast(object_handle.object(self));
            object.set_raw_properties_or_hash(&mut self.heap, dictionary);
            self.close_handle_scope(scope);
        } else {
            let original_details = dictionary.details_at(&self.heap, entry);
            let enumeration_index = original_details.dictionary_index();
            let details = details.set_dictionary_index(enumeration_index);
            dictionary.set_entry(&mut self.heap, entry, name, value, details);
        }
    }

    pub(crate) fn migrate_slow_to_fast(&mut self, object: JSObject, unused_property_fields: i32) {
        use crate::objects::descriptor_array::DescriptorArray;
        use crate::objects::dictionary::NameDictionary;
        use crate::objects::elements_kind::is_transitionable_fast_elements_kind;
        use crate::objects::property_details::{
            accessor_constant, data_field, PropertyConstness, PropertyKind,
            PropertyLocation, Representation, K_MAX_NUMBER_OF_DESCRIPTORS,
        };

        if object.has_fast_properties(&self.heap) {
            return;
        }
        let dictionary = object.property_dictionary(&self.heap);
        let number_of_elements = dictionary.number_of_elements(&self.heap);

        if number_of_elements > K_MAX_NUMBER_OF_DESCRIPTORS as i32 {
            return;
        }

        let iteration_order = dictionary.iteration_indices(&self.heap);

        let mut number_of_fields = 0;

        for entry in &iteration_order {
            let kind = dictionary.details_at(&self.heap, *entry).kind();
            if kind == PropertyKind::Data {
                number_of_fields += 1;
            }
        }

        let old_map = object.map(&self.heap);

        let inobject_props = old_map.inobject_properties(&self.heap);

        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let dictionary_handle = self.create_object_handle(dictionary.object());
        let old_map_handle = self.create_object_handle(old_map.object());

        let old_map = Map::unchecked_cast(old_map_handle.object(self));
        let new_map = self.copy_drop_descriptors(old_map);
        new_map.set_is_dictionary_map(&mut self.heap, false);
        let new_map_handle = self.create_object_handle(new_map.object());

        if number_of_elements == 0 {
            let new_map = new_map_handle.object(self);
            let object = JSObject::unchecked_cast(object_handle.object(self));
            self.heap.set_map(object.object(), new_map);
            let empty = self.roots.empty_property_array;
            object.set_raw_properties_or_hash(&mut self.heap, empty);
            debug_assert!(object.has_fast_properties(&self.heap));
            self.close_handle_scope(scope);
            return;
        }

        let descriptors = self.new_descriptor_array(number_of_elements);
        let descriptors_handle = self.create_object_handle(descriptors.object());

        let mut number_of_allocated_fields =
            number_of_fields + unused_property_fields - inobject_props;
        if number_of_allocated_fields < 0 {
            number_of_allocated_fields = 0;
        }

        let fields = self.new_property_array(number_of_allocated_fields);
        let fields_handle = self.create_object_handle(fields.object());

        let old_map = Map::unchecked_cast(old_map_handle.object(self));
        let is_transitionable_elements_kind =
            is_transitionable_fast_elements_kind(old_map.elements_kind(&self.heap));

        let mut field_index = 0;
        for entry in &iteration_order {
            let dictionary = NameDictionary::unchecked_cast(dictionary_handle.object(self));
            let key = dictionary.key_at(&self.heap, *entry);
            let value = dictionary.value_at(&self.heap, *entry);
            let details = dictionary.details_at(&self.heap, *entry);

            let (details, descriptor_value) = if details.kind() == PropertyKind::Data {
                let constness = if is_transitionable_elements_kind {
                    PropertyConstness::Mutable
                } else {
                    PropertyConstness::Const
                };
                let details = data_field(
                    field_index as u32,
                    Representation::Tagged,
                    details.attributes(),
                )
                .with_constness(constness);
                (details, self.the_hole_value())
            } else {
                debug_assert_eq!(details.kind(), PropertyKind::Accessor);
                (accessor_constant(details.attributes()), value)
            };

            if details.location() == PropertyLocation::Field {
                if field_index < inobject_props {
                    let new_map = Map::unchecked_cast(new_map_handle.object(self));
                    let object = JSObject::unchecked_cast(object_handle.object(self));
                    object.set_in_object_property_at(&mut self.heap, new_map, field_index, value);
                } else {
                    let fields = PropertyArray::unchecked_cast(fields_handle.object(self));
                    fields.set(&mut self.heap, field_index - inobject_props, value);
                }
                field_index += 1;
            }
            let descriptors = DescriptorArray::unchecked_cast(descriptors_handle.object(self));
            descriptors.append(&mut self.heap, key, details, descriptor_value);
        }

        let new_map = Map::unchecked_cast(new_map_handle.object(self));
        let descriptors = DescriptorArray::unchecked_cast(descriptors_handle.object(self));
        new_map.initialize_descriptors(&mut self.heap, descriptors);
        new_map.set_owns_descriptors(&mut self.heap, true);

        let new_map = new_map_handle.object(self);
        let object = JSObject::unchecked_cast(object_handle.object(self));
        self.heap.set_map(object.object(), new_map);

        let fields = fields_handle.get(self);
        let object = JSObject::unchecked_cast(object_handle.object(self));
        object.set_raw_properties_or_hash(&mut self.heap, fields);
        self.close_handle_scope(scope);
    }
}

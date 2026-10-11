use crate::execution::isolate::Isolate;
use crate::heap::heap::Heap;
use crate::objects::dictionary::NumberDictionary;
use crate::objects::elements_kind::{self, ElementsKind};
use crate::objects::fixed_array::FixedArray;
use crate::objects::fixed_primitive_array::FixedDoubleArray;
use crate::objects::hash_table::{HashTableBase, K_NOT_FOUND};
use crate::objects::heap_number;
use crate::objects::js_array::JSArray;
use crate::objects::js_objects::JSObject;
use crate::objects::property_details::{default_data_field, Representation};
use crate::objects::tagged::Tagged;

// Abstract base class for handles that can operate on objects with differing
// ElementsKinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ElementsAccessor {
    kind: ElementsKind,
}

impl ElementsAccessor {
    // Returns a shared ElementsAccessor for the specified ElementsKind.
    #[inline]
    pub fn for_kind(elements_kind: ElementsKind) -> ElementsAccessor {
        ElementsAccessor {
            kind: elements_kind,
        }
    }

    #[inline]
    pub fn kind(self) -> ElementsKind {
        self.kind
    }

    /// `ElementsAccessorBase::GetCapacityImpl`
    pub fn get_capacity(self, heap: &Heap, backing_store: Tagged) -> u32 {
        let Some(store) = backing_store.to_strong_heap_object() else {
            return 0;
        };
        if elements_kind::is_dictionary_elements_kind(self.kind) {
            NumberDictionary::unchecked_cast(store).capacity(heap)
        } else if elements_kind::is_double_elements_kind(self.kind) {
            FixedDoubleArray::unchecked_cast(store).length(heap) as u32
        } else {
            FixedArray::unchecked_cast(store).length(heap) as u32
        }
    }

    /// `ElementsAccessorBase::HasEntryImpl`
    pub fn has_entry(self, heap: &Heap, backing_store: Tagged, index: u32) -> bool {
        if elements_kind::is_dictionary_elements_kind(self.kind) {
            let Some(store) = backing_store.to_strong_heap_object() else {
                return false;
            };
            return NumberDictionary::unchecked_cast(store).find_entry(heap, index) != K_NOT_FOUND;
        }
        if index >= self.get_capacity(heap, backing_store) {
            return false;
        }
        let store = backing_store.to_strong_heap_object().unwrap();
        if elements_kind::is_double_elements_kind(self.kind) {
            !FixedDoubleArray::unchecked_cast(store).is_the_hole(heap, index as i32)
        } else {
            let value = FixedArray::unchecked_cast(store).get(heap, index as i32);
            !heap.is_the_hole(value)
        }
    }

    /// `ElementsAccessorBase::HasElementImpl`
    pub fn has_element(self, heap: &Heap, holder: JSObject, index: u32) -> bool {
        self.has_entry(heap, holder.elements(heap), index)
    }

    /// `ElementsAccessorBase::GetImpl`
    pub fn get(self, isolate: &mut Isolate, holder: JSObject, index: u32) -> Option<Tagged> {
        let backing_store = holder.elements(&isolate.heap);
        if !self.has_entry(&isolate.heap, backing_store, index) {
            return None;
        }
        let store = backing_store.to_strong_heap_object().unwrap();
        if elements_kind::is_dictionary_elements_kind(self.kind) {
            let dictionary = NumberDictionary::unchecked_cast(store);
            let entry = dictionary.find_entry(&isolate.heap, index);
            return Some(dictionary.value_at(&isolate.heap, entry));
        }
        if elements_kind::is_double_elements_kind(self.kind) {
            let value =
                FixedDoubleArray::unchecked_cast(store).get_scalar(&isolate.heap, index as i32);
            Some(isolate.new_number(value))
        } else {
            Some(FixedArray::unchecked_cast(store).get(&isolate.heap, index as i32))
        }
    }

    /// `ElementsAccessorBase::SetImpl`
    pub fn set(self, isolate: &mut Isolate, holder: JSObject, index: u32, value: Tagged) {
        let store = holder
            .elements(&isolate.heap)
            .to_strong_heap_object()
            .expect("a backing store");
        if elements_kind::is_dictionary_elements_kind(self.kind) {
            let dictionary = NumberDictionary::unchecked_cast(store);
            let details = default_data_field(0, Representation::Tagged);
            let updated = isolate.dictionary_set(dictionary, index, value, details);
            holder.set_elements(&mut isolate.heap, updated.tagged());
            return;
        }
        if elements_kind::is_double_elements_kind(self.kind) {
            let number = heap_number::number_value(&isolate.heap, value)
                .expect("a double store takes only numbers");
            FixedDoubleArray::unchecked_cast(store).set(&mut isolate.heap, index as i32, number);
        } else {
            FixedArray::unchecked_cast(store).set(&mut isolate.heap, index as i32, value);
        }
    }

    /// `ElementsAccessorBase::NumberOfElementsImpl`
    pub fn number_of_elements(self, heap: &Heap, receiver: JSObject) -> u32 {
        let backing_store = receiver.elements(heap);
        if elements_kind::is_dictionary_elements_kind(self.kind) {
            let Some(store) = backing_store.to_strong_heap_object() else {
                return 0;
            };
            return NumberDictionary::unchecked_cast(store).number_of_elements(heap) as u32;
        }
        let capacity = self.get_capacity(heap, backing_store);
        (0..capacity)
            .filter(|index| self.has_entry(heap, backing_store, *index))
            .count() as u32
    }
}

impl Isolate {
    /// `ElementsAccessor::ConvertElementsWithCapacity`
    pub(crate) fn convert_elements_with_capacity(
        &mut self,
        object: JSObject,
        from_kind: ElementsKind,
        to_kind: ElementsKind,
        capacity: u32,
    ) -> Tagged {
        let old_elements = object.elements(&self.heap);
        let from = ElementsAccessor::for_kind(from_kind);
        let old_capacity = from.get_capacity(&self.heap, old_elements);
        let copy = old_capacity.min(capacity);

        if elements_kind::is_double_elements_kind(to_kind) {
            let elements = self.new_fixed_double_array(capacity as i32);
            for index in 0..copy {
                if !from.has_entry(&self.heap, old_elements, index) {
                    continue;
                }
                let value = self
                    .element_as_f64(old_elements, from_kind, index)
                    .expect("a number");
                elements.set(&mut self.heap, index as i32, value);
            }
            elements.tagged()
        } else {
            let elements = self.new_fixed_array_with_holes(capacity as i32);
            for index in 0..copy {
                if !from.has_entry(&self.heap, old_elements, index) {
                    continue;
                }
                let value = self.element_as_tagged(old_elements, from_kind, index);
                elements.set(&mut self.heap, index as i32, value);
            }
            elements.tagged()
        }
    }

    fn element_as_f64(&self, backing_store: Tagged, kind: ElementsKind, index: u32) -> Option<f64> {
        let store = backing_store.to_strong_heap_object()?;
        if elements_kind::is_double_elements_kind(kind) {
            Some(FixedDoubleArray::unchecked_cast(store).get_scalar(&self.heap, index as i32))
        } else {
            let value = FixedArray::unchecked_cast(store).get(&self.heap, index as i32);
            heap_number::number_value(&self.heap, value)
        }
    }

    fn element_as_tagged(
        &mut self,
        backing_store: Tagged,
        kind: ElementsKind,
        index: u32,
    ) -> Tagged {
        let store = backing_store.to_strong_heap_object().expect("a store");
        if elements_kind::is_double_elements_kind(kind) {
            let value =
                FixedDoubleArray::unchecked_cast(store).get_scalar(&self.heap, index as i32);
            self.new_number(value)
        } else {
            FixedArray::unchecked_cast(store).get(&self.heap, index as i32)
        }
    }

    /// `ElementsAccessorBase::BasicGrowCapacityAndConvertImpl`
    pub(crate) fn basic_grow_capacity_and_convert(
        &mut self,
        object: JSObject,
        from_kind: ElementsKind,
        to_kind: ElementsKind,
        capacity: u32,
    ) {
        let elements = self.convert_elements_with_capacity(object, from_kind, to_kind, capacity);
        let mut to_kind = to_kind;
        if elements_kind::is_holey_elements_kind(from_kind) {
            to_kind = elements_kind::get_holey_elements_kind(to_kind);
        }
        let new_map = self.get_elements_transition_map(object, to_kind);
        self.set_map_and_elements(object, new_map, elements);
    }

    /// `ElementsAccessorBase::GrowCapacityAndConvertImpl`
    pub(crate) fn grow_capacity_and_convert(
        &mut self,
        object: JSObject,
        to_kind: ElementsKind,
        capacity: u32,
    ) {
        let from_kind = object.get_elements_kind(&self.heap);
        self.basic_grow_capacity_and_convert(object, from_kind, to_kind, capacity);
    }

    /// `FastElementsAccessor::AddImpl`
    pub(crate) fn add_element(
        &mut self,
        object: JSObject,
        index: u32,
        value: Tagged,
        to_kind: ElementsKind,
        new_capacity: u32,
    ) {
        let from_kind = object.get_elements_kind(&self.heap);
        let accessor = ElementsAccessor::for_kind(to_kind);
        let elements = object.elements(&self.heap);
        if elements_kind::is_dictionary_elements_kind(from_kind)
            || elements_kind::is_double_elements_kind(from_kind)
                != elements_kind::is_double_elements_kind(to_kind)
            || ElementsAccessor::for_kind(from_kind).get_capacity(&self.heap, elements)
                != new_capacity
        {
            self.grow_capacity_and_convert(object, to_kind, new_capacity);
        } else if elements_kind::is_fast_elements_kind(from_kind) && from_kind != to_kind {
            self.transition_elements_kind(object, to_kind);
        }
        accessor.set(self, object, index, value);
    }

    /// `FastElementsAccessor::NormalizeImpl`
    pub(crate) fn normalize_elements_impl(&mut self, object: JSObject) -> NumberDictionary {
        let kind = object.get_elements_kind(&self.heap);
        let accessor = ElementsAccessor::for_kind(kind);
        let elements = object.elements(&self.heap);
        let capacity = accessor.get_capacity(&self.heap, elements);
        let used = accessor.number_of_elements(&self.heap, object);

        let mut dictionary = self.new_number_dictionary(used.max(1));
        let details = default_data_field(0, Representation::Tagged);
        for index in 0..capacity {
            if !accessor.has_entry(&self.heap, elements, index) {
                continue;
            }
            let value = self.element_as_tagged(elements, kind, index);
            dictionary = self.dictionary_add(dictionary, index, value, details);
        }
        dictionary
    }

    /// `JSObject::NormalizeElements`
    pub(crate) fn normalize_elements(&mut self, object: JSObject) -> NumberDictionary {
        use crate::objects::elements_kind::ElementsKind::DictionaryElements;
        let elements = object.elements(&self.heap);
        if elements_kind::is_dictionary_elements_kind(object.get_elements_kind(&self.heap)) {
            return NumberDictionary::unchecked_cast(elements.to_strong_heap_object().unwrap());
        }
        let dictionary = self.normalize_elements_impl(object);
        let new_map = self.get_elements_transition_map(object, DictionaryElements);
        self.set_map_and_elements(object, new_map, dictionary.tagged());
        dictionary
    }

    /// `DictionaryElementsAccessor::SetLengthImpl`
    fn set_dictionary_length(&mut self, array: JSArray, length: u32) {
        let object = array.as_js_object();
        let dictionary = NumberDictionary::unchecked_cast(
            object
                .elements(&self.heap)
                .to_strong_heap_object()
                .expect("a dictionary"),
        );
        let old_length = array.length_value(&self.heap);
        if length < old_length {
            if length == 0 {
                // Flush the backing store.
                let empty = self.roots.empty_fixed_array;
                let new_map = self.get_elements_transition_map(
                    object,
                    elements_kind::get_initial_fast_elements_kind(),
                );
                self.set_map_and_elements(object, new_map, empty);
            } else {
                // Remove elements that should be deleted.
                let mut removed_entries = 0;
                for entry in 0..dictionary.capacity(&self.heap) as i32 {
                    let key = dictionary.key_at(&self.heap, entry);
                    if !HashTableBase::is_key(&self.heap, key) {
                        continue;
                    }
                    let Some(number) = self.to_array_length(key) else {
                        continue;
                    };
                    if length <= number && number < old_length {
                        dictionary.clear_entry(&mut self.heap, entry);
                        removed_entries += 1;
                    }
                }
                for _ in 0..removed_entries {
                    dictionary.as_hash_table().element_removed(&mut self.heap);
                }
            }
        }
        array.set_length_value(&mut self.heap, length);
    }

    /// `ElementsAccessorBase::SetLengthImpl`
    pub(crate) fn set_elements_length(&mut self, array: JSArray, length: u32) {
        let object = array.as_js_object();
        let mut kind = object.get_elements_kind(&self.heap);
        if elements_kind::is_dictionary_elements_kind(kind) {
            return self.set_dictionary_length(array, length);
        }
        let old_length = array.length_value(&self.heap);

        if old_length < length && !elements_kind::is_holey_elements_kind(kind) {
            kind = elements_kind::get_holey_elements_kind(kind);
            self.transition_elements_kind(object, kind);
        }

        // Check whether the backing store should be shrunk or grown.
        let accessor = ElementsAccessor::for_kind(kind);
        let capacity = accessor.get_capacity(&self.heap, object.elements(&self.heap));
        let old_length = old_length.min(capacity);
        if length == 0 {
            let empty = self.roots.empty_fixed_array;
            object.set_elements(&mut self.heap, empty);
        } else if length <= capacity {
            // Fill the non-trimmed elements with holes.
            self.fill_elements_with_holes(object, kind, length, old_length.min(capacity));
        } else {
            // Calculate a new capacity for the array.
            let new_capacity = if capacity == 0 {
                // If the existing capacity is zero, assume we are setting the
                // length to presize to the exact size we want.
                length
            } else {
                // Otherwise, assume we want exponential growing semantics, and
                // grow as if we were pushing. We might not grow enough for the
                // length, so take the max of hte two values.
                length.max(JSObject::new_elements_capacity(capacity))
            };
            self.grow_capacity_and_convert(object, kind, new_capacity);
        }

        array.set_length_value(&mut self.heap, length);
    }

    fn fill_elements_with_holes(
        &mut self,
        object: JSObject,
        kind: ElementsKind,
        from: u32,
        to: u32,
    ) {
        let Some(store) = object.elements(&self.heap).to_strong_heap_object() else {
            return;
        };
        if elements_kind::is_double_elements_kind(kind) {
            FixedDoubleArray::unchecked_cast(store).fill_with_holes(
                &mut self.heap,
                from as i32,
                to as i32,
            );
        } else {
            let the_hole = self.the_hole_value();
            let array = FixedArray::unchecked_cast(store);
            for index in from..to {
                array.set(&mut self.heap, index as i32, the_hole);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::elements_kind::ElementsKind::*;
    use crate::objects::smi::Smi;

    #[test]
    fn a_hole_is_not_an_element_but_undefined_is() {
        let mut isolate = Isolate::new();
        let array = isolate.new_js_array(4);
        let object = array.as_js_object();
        let accessor = ElementsAccessor::for_kind(HoleyElements);
        let elements = object.elements(&isolate.heap);

        assert_eq!(accessor.get_capacity(&isolate.heap, elements), 4);
        assert!(!accessor.has_entry(&isolate.heap, elements, 0));

        let undefined = isolate.undefined_value();
        FixedArray::unchecked_cast(elements.to_strong_heap_object().unwrap()).set(
            &mut isolate.heap,
            0,
            undefined,
        );
        assert!(
            accessor.has_entry(&isolate.heap, elements, 0),
            "a stored undefined is an element; only the hole is absent"
        );
    }

    #[test]
    fn converting_to_double_elements_unboxes_what_was_there() {
        let mut isolate = Isolate::new();
        let array = isolate.new_js_array(2);
        let object = array.as_js_object();
        ElementsAccessor::for_kind(PackedSmiElements).set(
            &mut isolate,
            object,
            0,
            Smi::from_int(7),
        );

        let elements = isolate.convert_elements_with_capacity(
            object,
            PackedSmiElements,
            PackedDoubleElements,
            4,
        );
        let doubles = FixedDoubleArray::unchecked_cast(elements.to_strong_heap_object().unwrap());
        assert_eq!(doubles.length(&isolate.heap), 4);
        assert_eq!(doubles.get_scalar(&isolate.heap, 0), 7.0);
        assert!(
            doubles.is_the_hole(&isolate.heap, 1),
            "capacity beyond what was copied is holes, not zeroes"
        );
    }
}

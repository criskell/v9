//! Ported from `src/heap/heap-visitor.h`, `heap-visitor.cc` and
//! `heap-visitor-inl.h`.
//!
//! Given an object, enumerate the slots in it that hold tagged values. This
//! is the one piece of knowledge the garbage collector cannot derive: a word
//! of heap memory is a reference or a float or a packed bitfield depending
//! entirely on what kind of object it is in.
//!
//! V8 dispatches on the map's `visitor_id`, a precomputed byte, so the hot
//! loop never switches on the instance type. `vx` switches on the instance
//! type directly; the `visitor_id` field exists in the `Map` layout for when
//! that becomes worth doing.
//!
//! Getting this wrong in either direction is fatal, which is why it lives in
//! one place: miss a slot and the collector drops a live object, visit a
//! non-slot and it will try to follow a float as a pointer.

use crate::execution::isolate::Isolate;
use crate::objects::bytecode_array::BytecodeArray;
use crate::objects::descriptor_array::DescriptorArray;
use crate::objects::fixed_array::{FixedArray, PropertyArray, WeakFixedArray};
use crate::objects::instance_type as it;
use crate::objects::js_function::JSFunction;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::oddball::Oddball;
use crate::objects::shared_function_info::SharedFunctionInfo;
use crate::objects::tagged::{Address, HeapObject};

impl Isolate {
    /// Calls `visit` with the address of every tagged slot inside `object`.
    ///
    /// The map word itself is *not* included: it is handled separately,
    /// because during a scavenge it may hold a forwarding address rather than
    /// a map, and because maps live in read-only space and never move.
    pub fn visit_object_slots(&self, object: HeapObject, visit: &mut impl FnMut(Address)) {
        let map = self.heap.map_of(object);
        let instance_type = map.instance_type(&self.heap);

        // Strings carry character data, not references. Visiting their bodies
        // would mean following text as pointers.
        if it::is_string(instance_type) {
            return;
        }

        match instance_type {
            it::MAP_TYPE => {
                let map = Map::unchecked_cast(object);
                visit(map.prototype_address());
                visit(map.constructor_or_back_pointer_address());
                visit(map.instance_descriptors_address());
                visit(map.raw_transitions_address());
            }

            it::HEAP_NUMBER_TYPE => {
                // A boxed double has no references.
            }

            it::ODDBALL_TYPE => {
                let oddball = Oddball::unchecked_cast(object);
                visit(oddball.field(Oddball::K_TO_STRING_OFFSET));
                visit(oddball.field(Oddball::K_TO_NUMBER_OFFSET));
                visit(oddball.field(Oddball::K_TYPE_OF_OFFSET));
            }

            it::FIXED_ARRAY_TYPE => {
                let array = FixedArray::unchecked_cast(object);
                for i in 0..array.length(&self.heap) {
                    visit(array.element_address(i));
                }
            }

            it::PROPERTY_ARRAY_TYPE => {
                let array = PropertyArray::unchecked_cast(object);
                for i in 0..array.length(&self.heap) {
                    visit(array.element_address(i));
                }
            }

            it::TRANSITION_ARRAY_TYPE => {
                let array = WeakFixedArray::unchecked_cast(object);
                for i in 0..array.length(&self.heap) {
                    visit(array.element_address(i));
                }
            }

            it::DESCRIPTOR_ARRAY_TYPE => {
                let array = DescriptorArray::unchecked_cast(object);
                visit(array.field(DescriptorArray::K_ENUM_CACHE_OFFSET));
                // The full capacity, not just the occupied prefix: the slack
                // entries are zeroed, so visiting them is harmless, and this
                // way a concurrent append cannot leave a slot unvisited.
                for i in 0..array.number_of_all_descriptors(&self.heap) {
                    visit(array.key_address(i));
                    visit(array.value_address(i));
                    // The details slot is a Smi and needs no visiting.
                }
            }

            it::JS_FUNCTION_TYPE | it::JS_NATIVE_FUNCTION_TYPE => {
                let function = JSFunction::unchecked_cast(object);
                visit(function.field(JSFunction::K_PROPERTIES_OR_HASH_OFFSET));
                visit(function.field(JSFunction::K_ELEMENTS_OFFSET));
                visit(function.field(JSFunction::K_SHARED_FUNCTION_INFO_OFFSET));
                visit(function.field(JSFunction::K_FEEDBACK_VECTOR_OFFSET));
            }

            _ if it::is_js_object(instance_type) => {
                let js_object = JSObject::unchecked_cast(object);
                visit(js_object.properties_or_hash_address());
                visit(js_object.elements_address());
                for i in 0..map.inobject_properties(&self.heap) {
                    visit(js_object.field(map.inobject_property_offset(&self.heap, i)));
                }
            }

            it::BYTECODE_ARRAY_TYPE => {
                let array = BytecodeArray::unchecked_cast(object);
                visit(array.source_position_table_address());
                visit(array.constant_pool_address());
                // The instruction bytes are not slots. Tracing them would
                // mean following opcodes as pointers.
            }

            it::SHARED_FUNCTION_INFO_TYPE => {
                let info = SharedFunctionInfo::unchecked_cast(object);
                visit(info.function_data_address());
                visit(info.name_address());
                visit(info.feedback_metadata_address());
            }

            it::FEEDBACK_VECTOR_TYPE => {
                use crate::objects::feedback_vector::FeedbackVector;
                let vector = FeedbackVector::unchecked_cast(object);
                visit(vector.shared_function_info_address());
                visit(vector.metadata_address());
                // The slots hold maps weakly and handlers as Smis, so they
                // are visited like any other tagged slot -- the weakness is
                // in the reference, not in how it is traced.
                for slot in 0..vector.length(&self.heap) {
                    visit(vector.slot_address(slot));
                }
            }

            it::FEEDBACK_METADATA_TYPE => {
                // Just a run of bytes.
            }

            it::PROPERTY_CELL_TYPE => {
                use crate::objects::property_cell::PropertyCell;
                let cell = PropertyCell::unchecked_cast(object);
                visit(cell.name_address());
                visit(cell.value_address());
                visit(cell.dependent_code_address());
                // The details slot is a Smi.
            }

            it::FILLER_TYPE => {
                // Free space: nothing to trace.
            }

            other => panic!(
                "no visitor for instance type {} ({}) at 0x{:x}",
                other,
                it::instance_type_name(other),
                object.address()
            ),
        }
    }

    /// Hands every root slot to `visit`: the root table, the string table and
    /// the handle scope. `Isolate::Iterate` in V8.
    ///
    /// This is the full set of places a live object can be reachable from
    /// without being reachable *through* another heap object. Once the
    /// interpreter exists, its register frames join the list.
    pub fn iterate_roots(&mut self, visit: &mut impl FnMut(&mut crate::objects::tagged::Tagged)) {
        self.roots.iterate(visit);
        self.iterate_string_table(visit);
        for handle in self.handles.iter_mut() {
            visit(handle);
        }
        // The interpreter's register file, which is why an object held only
        // in a register is not collected.
        for slot in self.stack.iter_mut() {
            visit(slot);
        }
        for info in self.shared_function_infos.iter_mut() {
            visit(info);
        }
        visit(&mut self.script_function);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::smi::Smi;
    use crate::objects::tagged::Tagged;

    fn slots_of(isolate: &Isolate, object: HeapObject) -> Vec<Address> {
        let mut slots = Vec::new();
        isolate.visit_object_slots(object, &mut |address| slots.push(address));
        slots
    }

    #[test]
    fn a_plain_object_exposes_its_header_and_inline_slots() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let slots = slots_of(&isolate, object.object());
        // properties, elements, and the four inline property slots.
        assert_eq!(slots.len(), 2 + Map::K_INITIAL_INOBJECT_SLACK as usize);
        assert_eq!(slots[0], object.properties_or_hash_address());
        assert_eq!(slots[1], object.elements_address());
        // Never the map word.
        assert!(!slots.contains(&object.address()));
    }

    #[test]
    fn every_reference_an_object_holds_is_reachable_through_a_visited_slot() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let value = isolate.new_heap_number(1.5).tagged();
        isolate.set_property_by_str(object, "x", value);

        let slots = slots_of(&isolate, object.object());
        let found = slots
            .iter()
            .any(|address| isolate.heap.read_tagged(*address) == value);
        assert!(found, "the property's value must be traceable");
    }

    #[test]
    fn strings_are_never_traced_as_references() {
        let mut isolate = Isolate::new();
        // Characters that would look like tagged pointers if misread.
        let string = isolate.new_string("\u{1}\u{1}\u{1}\u{1}\u{1}\u{1}\u{1}\u{1}");
        assert!(
            slots_of(&isolate, string.object()).is_empty(),
            "character data is not a slot"
        );
    }

    #[test]
    fn heap_numbers_are_never_traced() {
        let mut isolate = Isolate::new();
        // A double whose bit pattern is a plausible tagged pointer.
        let number = isolate.new_heap_number(f64::from_bits(0x1_0001));
        assert!(slots_of(&isolate, number.object()).is_empty());
    }

    #[test]
    fn a_map_exposes_exactly_its_four_reference_fields() {
        let isolate = Isolate::new();
        let map = isolate.roots.js_object_map();
        let slots = slots_of(&isolate, map.object());
        assert_eq!(slots.len(), 4);
        assert!(slots.contains(&map.prototype_address()));
        assert!(slots.contains(&map.constructor_or_back_pointer_address()));
        assert!(slots.contains(&map.instance_descriptors_address()));
        assert!(slots.contains(&map.raw_transitions_address()));
    }

    #[test]
    fn arrays_expose_every_element() {
        let mut isolate = Isolate::new();
        let array = isolate.new_fixed_array(5);
        let slots = slots_of(&isolate, array.object());
        assert_eq!(slots.len(), 5);
        for i in 0..5 {
            assert!(slots.contains(&array.element_address(i)));
        }
    }

    #[test]
    fn a_descriptor_array_exposes_keys_and_values_but_not_details() {
        let mut isolate = Isolate::new();
        let array = isolate.new_descriptor_array(3);
        let slots = slots_of(&isolate, array.object());
        // enum cache + (key, value) per entry.
        assert_eq!(slots.len(), 1 + 3 * 2);
        for i in 0..3 {
            assert!(slots.contains(&array.key_address(i)));
            assert!(slots.contains(&array.value_address(i)));
        }
    }

    #[test]
    fn the_root_set_covers_the_roots_the_string_table_and_the_handles() {
        let mut isolate = Isolate::new();
        let name = isolate.internalize_string("pinned").tagged();
        isolate.handles.push(Smi::from_int(99));

        let mut seen: Vec<Tagged> = Vec::new();
        isolate.iterate_roots(&mut |slot| seen.push(*slot));

        assert!(seen.contains(&name), "the string table is a root set");
        assert!(seen.contains(&Smi::from_int(99)), "handles are roots");
        assert!(seen.len() > 20, "the root table itself should be in there");

        // And the interpreter's registers.
        isolate.stack.push(Smi::from_int(7));
        let mut seen: Vec<Tagged> = Vec::new();
        isolate.iterate_roots(&mut |slot| seen.push(*slot));
        assert!(
            seen.contains(&Smi::from_int(7)),
            "a value live only in a register must be a root"
        );
    }
}

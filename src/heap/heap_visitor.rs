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
    pub fn visit_object_slots(&self, object: HeapObject, visit: &mut impl FnMut(Address)) {
        let map = self.heap.map_of(object);
        let instance_type = map.instance_type(&self.heap);

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

            it::HEAP_NUMBER_TYPE => {}

            it::ODDBALL_TYPE => {
                let oddball = Oddball::unchecked_cast(object);
                visit(oddball.field(Oddball::K_TO_STRING_OFFSET));
                visit(oddball.field(Oddball::K_TO_NUMBER_OFFSET));
                visit(oddball.field(Oddball::K_TYPE_OF_OFFSET));
            }

            it::FIXED_ARRAY_TYPE | it::NUMBER_DICTIONARY_TYPE | it::NAME_DICTIONARY_TYPE => {
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

            it::ACCESSOR_PAIR_TYPE => {
                use crate::objects::r#struct::AccessorPair;
                let pair = AccessorPair::unchecked_cast(object);
                visit(pair.getter_address());
                visit(pair.setter_address());
            }
            it::SYMBOL_TYPE => {
                use crate::objects::name::Symbol;
                visit(Symbol::unchecked_cast(object).description_address());
            }
            it::TUPLE2_TYPE => {
                use crate::objects::r#struct::Tuple2;
                let tuple = Tuple2::unchecked_cast(object);
                visit(tuple.value1_address());
                visit(tuple.value2_address());
            }

            it::ENUM_CACHE_TYPE => {
                use crate::objects::descriptor_array::EnumCache;
                let cache = EnumCache::unchecked_cast(object);
                visit(cache.keys_address());
                visit(cache.indices_address());
            }

            it::FIXED_DOUBLE_ARRAY_TYPE => {}

            it::TRANSITION_ARRAY_TYPE => {
                let array = WeakFixedArray::unchecked_cast(object);
                for i in 0..array.length(&self.heap) {
                    visit(array.element_address(i));
                }
            }

            it::DESCRIPTOR_ARRAY_TYPE => {
                let array = DescriptorArray::unchecked_cast(object);
                visit(array.field(DescriptorArray::K_ENUM_CACHE_OFFSET));
                for i in 0..array.number_of_all_descriptors(&self.heap) {
                    visit(array.key_address(i));
                    visit(array.value_address(i));
                }
            }

            it::JS_FUNCTION_TYPE | it::JS_NATIVE_FUNCTION_TYPE => {
                let function = JSFunction::unchecked_cast(object);
                visit(function.field(JSFunction::K_PROPERTIES_OR_HASH_OFFSET));
                visit(function.field(JSFunction::K_ELEMENTS_OFFSET));
                visit(function.field(JSFunction::K_SHARED_FUNCTION_INFO_OFFSET));
                visit(function.field(JSFunction::K_CONTEXT_OFFSET));
                visit(function.field(JSFunction::K_FEEDBACK_VECTOR_OFFSET));
                visit(function.prototype_or_initial_map_address());
                let js_object = JSObject::unchecked_cast(object);
                for i in 0..map.inobject_properties(&self.heap) {
                    visit(js_object.field(map.inobject_property_offset(&self.heap, i)));
                }
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
                visit(array.handler_table_address());
                visit(array.source_position_table_address());
                visit(array.constant_pool_address());
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
                for slot in 0..vector.length(&self.heap) {
                    visit(vector.slot_address(slot));
                }
            }

            it::FEEDBACK_METADATA_TYPE => {}

            it::CONTEXT_TYPE => {
                use crate::objects::contexts::Context;
                let context = Context::unchecked_cast(object);
                for index in 0..context.length(&self.heap) {
                    visit(context.slot_address(index));
                }
            }

            it::SCOPE_INFO_TYPE => {
                use crate::objects::scope_info::ScopeInfo;
                let info = ScopeInfo::unchecked_cast(object);
                for index in 0..info.context_local_count(&self.heap) {
                    visit(info.name_address(index));
                }
            }

            it::PROPERTY_CELL_TYPE => {
                use crate::objects::property_cell::PropertyCell;
                let cell = PropertyCell::unchecked_cast(object);
                visit(cell.name_address());
                visit(cell.value_address());
                visit(cell.dependent_code_address());
            }

            it::FILLER_TYPE => {}

            other => panic!(
                "no visitor for instance type {} ({}) at 0x{:x}",
                other,
                it::instance_type_name(other),
                object.address()
            ),
        }
    }

    pub fn iterate_roots(&mut self, visit: &mut impl FnMut(&mut crate::objects::tagged::Tagged)) {
        self.roots.iterate(visit);
        self.iterate_string_table(visit);
        for handle in self.handles.iter_mut() {
            visit(handle);
        }
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
    use crate::objects::js_objects::JSObject;
    #[derive(Copy, Clone)]
    struct Rooted(usize);

    impl Rooted {
        fn new(isolate: &mut Isolate, object: JSObject) -> Rooted {
            isolate.handles.push(object.tagged());
            Rooted(isolate.handles.len() - 1)
        }

        fn get(self, isolate: &Isolate) -> JSObject {
            JSObject::unchecked_cast(isolate.handles[self.0].to_strong_heap_object().unwrap())
        }
    }

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
        let object = Rooted::new(&mut isolate, object);
        let slots = slots_of(&isolate, object.get(&isolate).object());
        assert_eq!(slots.len(), 2 + Map::K_INITIAL_INOBJECT_SLACK as usize);
        assert_eq!(slots[0], object.get(&isolate).properties_or_hash_address());
        assert_eq!(slots[1], object.get(&isolate).elements_address());
        assert!(!slots.contains(&object.get(&isolate).address()));
    }

    #[test]
    fn every_reference_an_object_holds_is_reachable_through_a_visited_slot() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        let value = isolate.new_heap_number(1.5).tagged();
        isolate.handles.push(value);
        let value_slot = isolate.handles.len() - 1;
        let target = object.get(&isolate);
        isolate.set_property_by_str(target, "x", value);

        let value = isolate.handles[value_slot];
        let slots = slots_of(&isolate, object.get(&isolate).object());
        let found = slots
            .iter()
            .any(|address| isolate.heap.read_tagged(*address) == value);
        assert!(found, "the property's value must be traceable");
    }

    #[test]
    fn strings_are_never_traced_as_references() {
        let mut isolate = Isolate::new();
        let string = isolate.new_string("\u{1}\u{1}\u{1}\u{1}\u{1}\u{1}\u{1}\u{1}");
        assert!(
            slots_of(&isolate, string.object()).is_empty(),
            "character data is not a slot"
        );
    }

    #[test]
    fn heap_numbers_are_never_traced() {
        let mut isolate = Isolate::new();
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

        isolate.stack.push(Smi::from_int(7));
        let mut seen: Vec<Tagged> = Vec::new();
        isolate.iterate_roots(&mut |slot| seen.push(*slot));
        assert!(
            seen.contains(&Smi::from_int(7)),
            "a value live only in a register must be a root"
        );
    }
}

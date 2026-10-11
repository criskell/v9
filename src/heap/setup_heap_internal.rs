use crate::execution::isolate::Isolate;
use crate::heap::heap::AllocationType;
use crate::objects::descriptor_array::DescriptorArray;
use crate::objects::elements_kind::{self, ElementsKind};
use crate::objects::fixed_array::{FixedArray, PropertyArray};
use crate::objects::instance_type::*;
use crate::objects::js_array::JSArray;
use crate::objects::map::Map;
use crate::objects::oddball::{Oddball, OddballKind};
use crate::objects::tagged::Tagged;

impl Isolate {
    /// `Heap::CreateReadOnlyHeapObjects`
    pub(crate) fn setup_read_only_roots(&mut self) {
        let meta = self
            .heap
            .allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        self.heap.set_map(meta, meta);
        let meta_map = Map::unchecked_cast(meta);
        meta_map.set_instance_type(&mut self.heap, MAP_TYPE);
        meta_map.set_instance_size(&mut self.heap, Map::K_SIZE);
        meta_map.set_inobject_properties_start_in_words(
            &mut self.heap,
            Map::K_SIZE / crate::common::globals::K_TAGGED_SIZE,
        );
        self.roots.meta_map = meta.tagged();

        self.roots.descriptor_array_map = self.new_bare_map(DESCRIPTOR_ARRAY_TYPE, 0).tagged();
        self.roots.fixed_array_map = self.new_bare_map(FIXED_ARRAY_TYPE, 0).tagged();
        self.roots.fixed_double_array_map = self.new_bare_map(FIXED_DOUBLE_ARRAY_TYPE, 0).tagged();
        self.roots.number_dictionary_map = self.new_bare_map(NUMBER_DICTIONARY_TYPE, 0).tagged();
        self.roots.name_dictionary_map = self.new_bare_map(NAME_DICTIONARY_TYPE, 0).tagged();
        self.roots.accessor_pair_map = self
            .new_bare_map(
                ACCESSOR_PAIR_TYPE,
                crate::objects::r#struct::AccessorPair::K_SIZE,
            )
            .tagged();
        self.roots.enum_cache_map = self
            .new_bare_map(
                ENUM_CACHE_TYPE,
                crate::objects::descriptor_array::EnumCache::K_SIZE,
            )
            .tagged();
        self.roots.property_array_map = self.new_bare_map(PROPERTY_ARRAY_TYPE, 0).tagged();
        self.roots.transition_array_map = self.new_bare_map(TRANSITION_ARRAY_TYPE, 0).tagged();
        self.roots.feedback_vector_map = self.new_bare_map(FEEDBACK_VECTOR_TYPE, 0).tagged();
        self.roots.bytecode_array_map = self.new_bare_map(BYTECODE_ARRAY_TYPE, 0).tagged();
        self.roots.feedback_metadata_map = self.new_bare_map(FEEDBACK_METADATA_TYPE, 0).tagged();
        self.roots.property_cell_map = self
            .new_bare_map(
                PROPERTY_CELL_TYPE,
                crate::objects::property_cell::PropertyCell::K_SIZE,
            )
            .tagged();
        self.roots.scope_info_map = self.new_bare_map(SCOPE_INFO_TYPE, 0).tagged();
        self.roots.function_context_map = self.new_bare_map(CONTEXT_TYPE, 0).tagged();
        self.roots.shared_function_info_map = self
            .new_bare_map(
                SHARED_FUNCTION_INFO_TYPE,
                crate::objects::shared_function_info::SharedFunctionInfo::K_SIZE,
            )
            .tagged();
        self.roots.filler_map = self.new_bare_map(FILLER_TYPE, 0).tagged();

        let empty_descriptors = self.allocate_descriptor_array_raw(0);
        self.roots.empty_descriptor_array = empty_descriptors.tagged();
        let empty_fixed_array = self.allocate_fixed_array_raw(0);
        self.roots.empty_fixed_array = empty_fixed_array.tagged();
        let empty_property_array = self.allocate_property_array_raw(0);
        self.roots.empty_property_array = empty_property_array.tagged();

        let built_so_far = [
            self.roots.meta_map,
            self.roots.descriptor_array_map,
            self.roots.fixed_array_map,
            self.roots.property_array_map,
            self.roots.transition_array_map,
            self.roots.feedback_vector_map,
            self.roots.bytecode_array_map,
            self.roots.feedback_metadata_map,
            self.roots.property_cell_map,
            self.roots.scope_info_map,
            self.roots.function_context_map,
            self.roots.shared_function_info_map,
            self.roots.filler_map,
        ];
        for map in built_so_far {
            let map = Map::unchecked_cast(map.to_strong_heap_object().unwrap());
            map.set_instance_descriptors(&mut self.heap, empty_descriptors.tagged());
        }

        self.roots.seq_one_byte_string_map =
            self.new_bare_map(SEQ_ONE_BYTE_STRING_TYPE, 0).tagged();
        self.roots.seq_two_byte_string_map =
            self.new_bare_map(SEQ_TWO_BYTE_STRING_TYPE, 0).tagged();
        self.roots.internalized_one_byte_string_map = self
            .new_bare_map(INTERNALIZED_ONE_BYTE_STRING_TYPE, 0)
            .tagged();
        self.roots.internalized_two_byte_string_map = self
            .new_bare_map(INTERNALIZED_TWO_BYTE_STRING_TYPE, 0)
            .tagged();
        self.roots.empty_string = self.internalize_string("").tagged();

        self.roots.heap_number_map = self
            .new_bare_map(
                HEAP_NUMBER_TYPE,
                crate::objects::heap_number::HeapNumber::K_SIZE,
            )
            .tagged();
        self.roots.oddball_map = self.new_bare_map(ODDBALL_TYPE, Oddball::K_SIZE).tagged();

        self.roots.undefined_value = self
            .new_oddball(OddballKind::Undefined, "undefined", f64::NAN, "undefined")
            .tagged();
        self.roots.null_value = self
            .new_oddball(OddballKind::Null, "null", 0.0, "object")
            .tagged();
        self.roots.true_value = self
            .new_oddball(OddballKind::True, "true", 1.0, "boolean")
            .tagged();
        self.roots.false_value = self
            .new_oddball(OddballKind::False, "false", 0.0, "boolean")
            .tagged();
        self.roots.the_hole_value = self
            .new_oddball(OddballKind::TheHole, "hole", f64::NAN, "undefined")
            .tagged();
        self.heap.the_hole_value = self.roots.the_hole_value;
        self.heap.undefined_value = self.roots.undefined_value;
        self.heap.null_value = self.roots.null_value;
        self.roots.uninitialized_value = self
            .new_oddball(
                OddballKind::Uninitialized,
                "uninitialized",
                f64::NAN,
                "undefined",
            )
            .tagged();
        self.roots.megamorphic_symbol = self
            .new_oddball(
                OddballKind::Uninitialized,
                "megamorphic",
                f64::NAN,
                "undefined",
            )
            .tagged();

        self.roots.js_object_map = self
            .new_js_object_map(JS_OBJECT_TYPE, Map::K_INITIAL_INOBJECT_SLACK)
            .tagged();
        for kind in elements_kind::FAST_ELEMENTS_KIND_SEQUENCE {
            let map = self.new_js_array_map(kind);
            self.roots.set_initial_js_array_map(kind, map.tagged());
        }
        self.roots.js_array_dictionary_elements_map = self
            .new_js_array_map(ElementsKind::DictionaryElements)
            .tagged();
        self.roots.js_global_object_map = self
            .new_js_object_map(JS_GLOBAL_OBJECT_TYPE, Map::K_INITIAL_INOBJECT_SLACK)
            .tagged();
        self.roots.js_function_map = self.new_js_function_map(JS_FUNCTION_TYPE).tagged();
        self.roots.js_native_function_map =
            self.new_js_function_map(JS_NATIVE_FUNCTION_TYPE).tagged();

        for map in [
            self.roots.js_function_map,
            self.roots.js_native_function_map,
        ] {
            let map = Map::unchecked_cast(map.to_strong_heap_object().unwrap());
            map.set_is_callable(&mut self.heap, true);
            map.set_is_constructor(&mut self.heap, true);
        }
    }

    fn new_bare_map(&mut self, instance_type: InstanceType, instance_size: i32) -> Map {
        let meta = self.roots.meta_map;
        let object = self
            .heap
            .allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        self.heap
            .set_map(object, meta.to_strong_heap_object().unwrap());
        let map = Map::unchecked_cast(object);
        map.set_instance_type(&mut self.heap, instance_type);
        map.set_instance_size(&mut self.heap, instance_size);
        map.set_instance_descriptors(&mut self.heap, self.roots.empty_descriptor_array);
        map.set_number_of_own_descriptors(&mut self.heap, 0);
        map.set_owns_descriptors(&mut self.heap, true);
        map.set_is_extensible(&mut self.heap, true);
        map.set_prototype(&mut self.heap, self.roots.null_value);
        map.set_raw_transitions(&mut self.heap, Tagged::ZERO);
        map
    }

    fn new_oddball(
        &mut self,
        kind: OddballKind,
        to_string: &str,
        to_number_raw: f64,
        type_of: &str,
    ) -> Oddball {
        let oddball_map = self.roots.oddball_map.to_strong_heap_object().unwrap();
        let object = self
            .heap
            .allocate_raw_or_fail(Oddball::K_SIZE as usize, AllocationType::ReadOnly);
        self.heap.set_map(object, oddball_map);
        let oddball = Oddball::unchecked_cast(object);
        oddball.set_kind(&mut self.heap, kind);
        oddball.set_to_number_raw(&mut self.heap, to_number_raw);
        let to_string = self.internalize_string(to_string).tagged();
        oddball.set_to_string(&mut self.heap, to_string);
        let type_of = self.internalize_string(type_of).tagged();
        oddball.set_type_of(&mut self.heap, type_of);
        oddball.set_to_number(&mut self.heap, Tagged::ZERO);
        oddball
    }

    #[inline]

    fn allocate_fixed_array_raw(&mut self, length: i32) -> FixedArray {
        let object = self.heap.allocate_raw_or_fail(
            FixedArray::size_for(length) as usize,
            AllocationType::ReadOnly,
        );
        self.heap.set_map(
            object,
            self.roots.fixed_array_map.to_strong_heap_object().unwrap(),
        );
        let array = FixedArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        array
    }

    fn allocate_property_array_raw(&mut self, length: i32) -> PropertyArray {
        let object = self.heap.allocate_raw_or_fail(
            PropertyArray::size_for(length) as usize,
            AllocationType::ReadOnly,
        );
        self.heap.set_map(
            object,
            self.roots
                .property_array_map
                .to_strong_heap_object()
                .unwrap(),
        );
        let array = PropertyArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        array
    }

    fn allocate_descriptor_array_raw(&mut self, capacity: i32) -> DescriptorArray {
        let object = self.heap.allocate_raw_or_fail(
            DescriptorArray::size_for(capacity) as usize,
            AllocationType::ReadOnly,
        );
        self.heap.set_map(
            object,
            self.roots
                .descriptor_array_map
                .to_strong_heap_object()
                .unwrap(),
        );
        let array = DescriptorArray::unchecked_cast(object);
        array.set_number_of_all_descriptors(&mut self.heap, capacity);
        array.set_number_of_descriptors(&mut self.heap, 0);
        array.set_enum_cache(&mut self.heap, Tagged::ZERO);
        array
    }

    fn new_js_function_map(&mut self, instance_type: InstanceType) -> Map {
        use crate::objects::js_function::JSFunction;
        let header_words = JSFunction::K_SIZE / crate::common::globals::K_TAGGED_SIZE;
        let slack = Map::K_INITIAL_INOBJECT_SLACK;
        let instance_size = (header_words + slack) * crate::common::globals::K_TAGGED_SIZE;
        let map = self.new_bare_map(instance_type, instance_size);
        map.set_inobject_properties_start_in_words(&mut self.heap, header_words);
        map.set_prototype(&mut self.heap, self.roots.null_value);
        map
    }

    fn new_js_array_map(&mut self, kind: ElementsKind) -> Map {
        let map = self.new_bare_map(JS_ARRAY_TYPE, JSArray::K_SIZE);
        map.set_inobject_properties_start_in_words(
            &mut self.heap,
            JSArray::K_SIZE / crate::common::globals::K_TAGGED_SIZE,
        );
        map.set_prototype(&mut self.heap, self.roots.null_value);
        map.set_elements_kind(&mut self.heap, kind);
        map
    }

    fn new_js_object_map(&mut self, instance_type: InstanceType, inobject_properties: i32) -> Map {
        use crate::objects::js_objects::JSObject;
        let header_words = JSObject::K_HEADER_SIZE / crate::common::globals::K_TAGGED_SIZE;
        let instance_size =
            (header_words + inobject_properties) * crate::common::globals::K_TAGGED_SIZE;
        let map = self.new_bare_map(instance_type, instance_size);
        map.set_inobject_properties_start_in_words(&mut self.heap, header_words);
        map.set_prototype(&mut self.heap, self.roots.null_value);
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::instance_type;
    use crate::objects::string::String as VxString;

    #[test]
    fn the_meta_map_is_its_own_map() {
        let isolate = Isolate::new();
        let meta = isolate.roots.meta_map();
        assert_eq!(
            isolate.heap.map(meta.object()),
            meta.object(),
            "the type system has to bottom out somewhere"
        );
        assert_eq!(meta.instance_type(&isolate.heap), MAP_TYPE);
    }

    #[test]
    fn every_root_is_populated_after_bootstrap() {
        let isolate = Isolate::new();
        for (name, value) in isolate.roots.named() {
            if matches!(
                name,
                "global_object"
                    | "native_context"
                    | "object_prototype"
                    | "function_prototype"
                    | "array_prototype"
            ) {
                continue;
            }
            assert!(
                value.is_strong(),
                "root `{name}` was left unset by the bootstrap"
            );
        }
    }

    #[test]
    fn every_root_map_is_a_map() {
        let isolate = Isolate::new();
        for (name, value) in isolate.roots.named() {
            if !name.ends_with("_map") {
                continue;
            }
            let object = value.to_strong_heap_object().expect(name);
            assert_eq!(
                isolate.heap.instance_type(object),
                MAP_TYPE,
                "root `{name}` should be a Map"
            );
        }
    }

    #[test]
    fn everything_immortal_lives_in_read_only_space() {
        let isolate = Isolate::new();
        for (name, value) in isolate.roots.named() {
            let Some(object) = value.to_strong_heap_object() else {
                continue;
            };
            assert!(
                isolate.heap.in_read_only_space(object),
                "root `{name}` must be immovable"
            );
        }
    }

    #[test]
    fn the_oddballs_know_how_to_convert_themselves() {
        let isolate = Isolate::new();
        let heap = &isolate.heap;

        let undefined =
            Oddball::unchecked_cast(isolate.undefined_value().to_strong_heap_object().unwrap());
        assert_eq!(undefined.kind(heap), Some(OddballKind::Undefined));
        assert!(
            undefined.to_number_raw(heap).is_nan(),
            "Number(undefined) is NaN"
        );
        assert_eq!(
            VxString::unchecked_cast(undefined.type_of(heap).to_strong_heap_object().unwrap())
                .to_rust_string(heap),
            "undefined"
        );

        let null = Oddball::unchecked_cast(isolate.null_value().to_strong_heap_object().unwrap());
        assert_eq!(null.kind(heap), Some(OddballKind::Null));
        assert_eq!(null.to_number_raw(heap), 0.0, "Number(null) is 0");
        assert_eq!(
            VxString::unchecked_cast(null.type_of(heap).to_strong_heap_object().unwrap())
                .to_rust_string(heap),
            "object",
            "the famous typeof null"
        );

        let t = Oddball::unchecked_cast(isolate.true_value().to_strong_heap_object().unwrap());
        assert_eq!(t.to_number_raw(heap), 1.0);
        assert_eq!(
            VxString::unchecked_cast(t.type_of(heap).to_strong_heap_object().unwrap())
                .to_rust_string(heap),
            "boolean"
        );
    }

    #[test]
    fn the_oddballs_are_distinct_objects() {
        let isolate = Isolate::new();
        let values = [
            isolate.undefined_value(),
            isolate.null_value(),
            isolate.true_value(),
            isolate.false_value(),
            isolate.the_hole_value(),
        ];
        for (i, a) in values.iter().enumerate() {
            for b in values.iter().skip(i + 1) {
                assert_ne!(a, b, "oddballs must be distinguishable by identity");
            }
        }
        assert_eq!(isolate.boolean_value(true), isolate.true_value());
        assert_eq!(isolate.boolean_value(false), isolate.false_value());
    }

    #[test]
    fn function_maps_are_callable_and_plain_object_maps_are_not() {
        let isolate = Isolate::new();
        let heap = &isolate.heap;
        let function_map = Map::unchecked_cast(
            isolate
                .roots
                .js_function_map
                .to_strong_heap_object()
                .unwrap(),
        );
        let object_map = isolate.roots.js_object_map();

        assert!(function_map.is_callable(heap));
        assert!(function_map.is_constructor(heap));
        assert!(!object_map.is_callable(heap));
        assert!(instance_type::is_callable(function_map.instance_type(heap)));
        assert!(!instance_type::is_callable(object_map.instance_type(heap)));
    }

    #[test]
    fn the_initial_object_map_has_room_for_a_few_properties() {
        let isolate = Isolate::new();
        let map = isolate.roots.js_object_map();
        assert_eq!(
            map.inobject_properties(&isolate.heap),
            Map::K_INITIAL_INOBJECT_SLACK
        );
        assert_eq!(map.number_of_own_descriptors(&isolate.heap), 0);
        assert!(map.owns_descriptors(&isolate.heap));
        assert!(map.is_extensible(&isolate.heap));
        assert_eq!(
            map.instance_descriptors(&isolate.heap),
            isolate.empty_descriptor_array(),
            "a fresh map shares the empty descriptor array"
        );
    }
}

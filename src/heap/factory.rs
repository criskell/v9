use crate::common::globals::K_TAGGED_SIZE;
use crate::execution::isolate::Isolate;
use crate::heap::heap::AllocationType;
use crate::objects::descriptor_array::DescriptorArray;
use crate::objects::elements_kind::{self, ElementsKind};
use crate::objects::fixed_array::{FixedArray, PropertyArray};
use crate::objects::fixed_primitive_array::FixedDoubleArray;
use crate::objects::heap_number::HeapNumber;
use crate::objects::instance_type::InstanceType;
use crate::objects::js_array::JSArray;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::smi::Smi;
use crate::objects::string::String as VxString;
use crate::objects::tagged::Tagged;

pub const K_DESCRIPTOR_ARRAY_SLACK: i32 = 4;

impl Isolate {
    // -- numbers ------------------------------------------------------------

    /// `Factory::NewHeapNumber`
    pub fn new_heap_number(&mut self, value: f64) -> HeapNumber {
        let map = self.roots.heap_number_map.to_strong_heap_object().unwrap();
        let object = self
            .allocate_raw_with_retry_or_fail(HeapNumber::K_SIZE as usize, AllocationType::Young);
        self.heap.set_map(object, map);
        let number = HeapNumber::unchecked_cast(object);
        number.set_value(&mut self.heap, value);
        number
    }

    pub fn new_number(&mut self, value: f64) -> Tagged {
        match Smi::try_from_f64(value) {
            Some(smi) => smi,
            None => self.new_heap_number(value).tagged(),
        }
    }

    pub fn new_number_from_i64(&mut self, value: i64) -> Tagged {
        match Smi::try_from_i64(value) {
            Some(smi) => smi,
            None => self.new_heap_number(value as f64).tagged(),
        }
    }

    // -- strings ------------------------------------------------------------

    pub fn new_string(&mut self, s: &str) -> VxString {
        let one_byte = VxString::is_one_byte_content(s);
        let map = if one_byte {
            self.roots.seq_one_byte_string_map
        } else {
            self.roots.seq_two_byte_string_map
        };
        self.allocate_string_raw(s, map, one_byte, AllocationType::Young)
    }

    // -- arrays -------------------------------------------------------------

    pub fn new_fixed_array(&mut self, length: i32) -> FixedArray {
        if length == 0 {
            return self.empty_fixed_array();
        }
        let map = self.roots.fixed_array_map.to_strong_heap_object().unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            FixedArray::size_for(length) as usize,
            AllocationType::Young,
        );
        self.heap.set_map(object, map);
        let array = FixedArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        let undefined = self.undefined_value();
        for i in 0..length {
            array.set(&mut self.heap, i, undefined);
        }
        array
    }

    /// `Factory::NewFixedDoubleArray`
    pub fn new_fixed_double_array(&mut self, length: i32) -> FixedDoubleArray {
        let map = self
            .roots
            .fixed_double_array_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            FixedDoubleArray::size_for(length) as usize,
            AllocationType::Young,
        );
        self.heap.set_map(object, map);
        let array = FixedDoubleArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        array.fill_with_holes(&mut self.heap, 0, length);
        array
    }

    /// `Factory::NewJSArrayWithElements`
    pub fn new_js_array_with_elements(
        &mut self,
        elements: Tagged,
        kind: ElementsKind,
        length: u32,
    ) -> JSArray {
        let scope = self.handle_scope();
        let elements_handle = self.create_handle(elements);
        let map = self
            .roots
            .get_initial_js_array_map(kind)
            .to_strong_heap_object()
            .unwrap();
        let object =
            self.allocate_raw_with_retry_or_fail(JSArray::K_SIZE as usize, AllocationType::Young);
        self.heap.set_map(object, map);
        let array = JSArray::unchecked_cast(object);
        let empty = self.roots.empty_fixed_array;
        array
            .as_js_object()
            .set_raw_properties_or_hash(&mut self.heap, empty);
        let elements = elements_handle.get(self);
        self.close_handle_scope(scope);
        array.set_elements(&mut self.heap, elements);
        array.set_length_value(&mut self.heap, length);
        array
    }

    /// `Factory::NewJSArray`
    pub fn new_js_array(&mut self, capacity: i32) -> JSArray {
        let kind = elements_kind::get_initial_fast_elements_kind();
        let elements = if capacity == 0 {
            self.roots.empty_fixed_array
        } else {
            self.new_fixed_array_with_holes(capacity).tagged()
        };
        self.new_js_array_with_elements(elements, kind, 0)
    }

    /// `Factory::NewFixedArrayWithHoles`
    pub fn new_fixed_array_with_holes(&mut self, length: i32) -> FixedArray {
        let array = self.new_fixed_array(length);
        let the_hole = self.the_hole_value();
        for index in 0..length {
            array.set(&mut self.heap, index, the_hole);
        }
        array
    }

    /// `Factory::NewPropertyArray`
    pub fn new_property_array(&mut self, length: i32) -> PropertyArray {
        if length == 0 {
            return self.empty_property_array();
        }
        let map = self
            .roots
            .property_array_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            PropertyArray::size_for(length) as usize,
            AllocationType::Young,
        );
        self.heap.set_map(object, map);
        let array = PropertyArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        let undefined = self.undefined_value();
        for i in 0..length {
            array.set(&mut self.heap, i, undefined);
        }
        array
    }

    pub fn new_descriptor_array(&mut self, capacity: i32) -> DescriptorArray {
        if capacity == 0 {
            return self.empty_descriptor_array();
        }
        let map = self
            .roots
            .descriptor_array_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            DescriptorArray::size_for(capacity) as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let array = DescriptorArray::unchecked_cast(object);
        array.set_number_of_all_descriptors(&mut self.heap, capacity);
        array.set_number_of_descriptors(&mut self.heap, 0);
        array.set_enum_cache(&mut self.heap, Tagged::ZERO);
        array
    }

    // -- maps ---------------------------------------------------------------

    pub fn new_map(
        &mut self,
        instance_type: InstanceType,
        instance_size: i32,
        inobject_properties_start_in_words: i32,
    ) -> Map {
        let meta = self.roots.meta_map.to_strong_heap_object().unwrap();
        let object =
            self.allocate_raw_with_retry_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        self.heap.set_map(object, meta);
        let map = Map::unchecked_cast(object);
        map.set_instance_type(&mut self.heap, instance_type);
        map.set_instance_size(&mut self.heap, instance_size);
        map.set_inobject_properties_start_in_words(
            &mut self.heap,
            inobject_properties_start_in_words,
        );
        map.set_instance_descriptors(&mut self.heap, self.roots.empty_descriptor_array);
        map.set_number_of_own_descriptors(&mut self.heap, 0);
        map.set_owns_descriptors(&mut self.heap, true);
        map.set_is_extensible(&mut self.heap, true);
        map.set_prototype(&mut self.heap, self.roots.null_value);
        map.set_constructor_or_back_pointer(&mut self.heap, self.roots.undefined_value);
        map.set_raw_transitions(&mut self.heap, Tagged::ZERO);
        map
    }

    pub fn copy_map_shell(&mut self, source: Map) -> Map {
        let instance_type = source.instance_type(&self.heap);
        let instance_size = source.instance_size(&self.heap);
        let start = source.inobject_properties_start_in_words(&self.heap);
        let map = self.new_map(instance_type, instance_size, start);

        let prototype = source.prototype(&self.heap);
        map.set_prototype(&mut self.heap, prototype);
        let bit_field = self.heap.read_u8(source.field(Map::K_BIT_FIELD_OFFSET));
        self.heap
            .write_u8(map.field(Map::K_BIT_FIELD_OFFSET), bit_field);
        let is_extensible = source.is_extensible(&self.heap);
        map.set_is_extensible(&mut self.heap, is_extensible);
        map
    }

    // -- objects ------------------------------------------------------------

    pub fn new_js_object_from_map(&mut self, map: Map) -> JSObject {
        let instance_size = map.instance_size(&self.heap);
        let object =
            self.allocate_raw_with_retry_or_fail(instance_size as usize, AllocationType::Young);
        self.heap.set_map(object, map.object());
        let js_object = JSObject::unchecked_cast(object);
        js_object.set_raw_properties_or_hash(&mut self.heap, self.roots.empty_property_array);
        js_object.set_elements(&mut self.heap, self.roots.empty_fixed_array);

        let undefined = self.undefined_value();
        let inobject = map.inobject_properties(&self.heap);
        for i in 0..inobject {
            js_object.set_in_object_property_at(&mut self.heap, map, i, undefined);
        }
        js_object
    }

    pub fn new_js_object(&mut self) -> JSObject {
        let map = self.roots.js_object_map();
        self.new_js_object_from_map(map)
    }

    pub fn grow_property_array(&mut self, object: JSObject, length: i32) {
        let existing = object.property_array(&self.heap);
        let existing_length = existing.length(&self.heap);
        if existing_length >= length {
            return;
        }
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let existing_handle = self.create_object_handle(existing.object());
        let grown = self.new_property_array(length);
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let existing = PropertyArray::unchecked_cast(existing_handle.object(self));
        for i in 0..existing_length {
            let value = existing.get(&self.heap, i);
            grown.set(&mut self.heap, i, value);
        }
        object.set_raw_properties_or_hash(&mut self.heap, grown.tagged());
        self.close_handle_scope(scope);
    }

    // -- code ---------------------------------------------------------------

    pub fn new_bytecode_array(
        &mut self,
        data: &crate::interpreter::bytecode_array_builder::BytecodeArrayData,
    ) -> crate::objects::bytecode_array::BytecodeArray {
        use crate::objects::bytecode_array::BytecodeArray;

        let constant_pool = self.new_constant_pool(&data.constants);
        let scope = self.handle_scope();
        let constant_pool = self.create_object_handle(constant_pool.object());

        let length = data.bytecodes.len() as i32;
        let map = self
            .roots
            .bytecode_array_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            BytecodeArray::size_for(length) as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let array = BytecodeArray::unchecked_cast(object);
        self.create_object_handle(object);
        array.set_length(&mut self.heap, length);
        let pool = constant_pool.get(self);
        array.set_constant_pool(&mut self.heap, pool);
        array.set_source_position_table(&mut self.heap, self.roots.empty_fixed_array);
        array.set_handler_table(&mut self.heap, self.roots.empty_fixed_array);
        array.set_register_count(&mut self.heap, data.register_count);
        array.set_parameter_count(&mut self.heap, data.parameter_count);
        array.set_max_arguments(&mut self.heap, 0);
        let encoded = encode_handler_table(&data.handler_table);
        let handler_table = if encoded.is_empty() {
            self.roots.empty_fixed_array
        } else {
            let table = self.new_fixed_array_tenured(encoded.len() as i32);
            for (index, value) in encoded.iter().enumerate() {
                table.set(&mut self.heap, index as i32, Smi::from_int(*value));
            }
            table.tagged()
        };
        let array = BytecodeArray::unchecked_cast(
            self.handles[self.handles.len() - 1]
                .to_strong_heap_object()
                .unwrap(),
        );
        array.set_handler_table(&mut self.heap, handler_table);
        self.close_handle_scope(scope);
        let bytes = data.bytecodes.clone();
        self.heap
            .write_bytes(array.first_bytecode_address(), &bytes);
        array
    }

    fn new_constant_pool(
        &mut self,
        entries: &[crate::interpreter::constant_array_builder::ConstantEntry],
    ) -> FixedArray {
        use crate::interpreter::constant_array_builder::ConstantEntry;

        let pool = self.new_fixed_array_tenured(entries.len() as i32);
        let scope = self.handle_scope();
        let pool_handle = self.create_object_handle(pool.object());
        for (index, entry) in entries.iter().enumerate() {
            let value = match entry {
                ConstantEntry::Smi(value) => Smi::from_int(*value),
                ConstantEntry::Number(value) => self.new_heap_number_tenured(*value).tagged(),
                ConstantEntry::String(value) => {
                    let string = value.clone();
                    self.new_string_tenured(&string).tagged()
                }
                ConstantEntry::Name(value) => {
                    let name = value.clone();
                    self.internalize_string(&name).tagged()
                }
                ConstantEntry::FunctionLiteral(code_index) => Smi::from_int(*code_index as i32),
                ConstantEntry::ScopeInfo(names) => {
                    let names = names.clone();
                    self.new_scope_info(&names).tagged()
                }
                ConstantEntry::TheHole => self.the_hole_value(),
            };
            let pool = FixedArray::unchecked_cast(pool_handle.object(self));
            pool.set(&mut self.heap, index as i32, value);
        }
        let pool = FixedArray::unchecked_cast(pool_handle.object(self));
        self.close_handle_scope(scope);
        pool
    }

    /// `Factory::NewScopeInfo`
    pub fn new_scope_info(&mut self, names: &[String]) -> crate::objects::scope_info::ScopeInfo {
        use crate::objects::scope_info::ScopeInfo;
        let count = names.len() as i32;
        let map = self.roots.scope_info_map.to_strong_heap_object().unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            ScopeInfo::size_for(count) as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let info = ScopeInfo::unchecked_cast(object);
        info.set_context_local_count(&mut self.heap, count);
        for (index, name) in names.iter().enumerate() {
            let name = name.clone();
            let interned = self.internalize_string(&name).tagged();
            info.set_context_local_name(&mut self.heap, index as i32, interned);
        }
        info
    }

    pub fn new_function_context(
        &mut self,
        slots: i32,
        previous: Tagged,
    ) -> crate::objects::contexts::Context {
        use crate::objects::contexts::Context;
        let length = Context::MIN_CONTEXT_SLOTS + slots;
        let scope = self.handle_scope();
        let previous = self.create_handle(previous);
        let map = self
            .roots
            .function_context_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            Context::size_for(length) as usize,
            AllocationType::Young,
        );
        let previous = previous.get(self);
        self.close_handle_scope(scope);
        self.heap.set_map(object, map);
        let context = Context::unchecked_cast(object);
        context.set_length(&mut self.heap, length);
        let undefined = self.undefined_value();
        for index in 0..length {
            context.set(&mut self.heap, index, undefined);
        }
        context.set_previous(&mut self.heap, previous);
        context
    }

    pub fn new_property_cell(
        &mut self,
        name: Tagged,
        value: Tagged,
    ) -> crate::objects::property_cell::PropertyCell {
        use crate::objects::property_cell::{initial_cell_type, PropertyCell};
        use crate::objects::property_details::{PropertyDetails, PropertyKind, NONE};

        let map = self
            .roots
            .property_cell_map
            .to_strong_heap_object()
            .unwrap();
        let object = self
            .allocate_raw_with_retry_or_fail(PropertyCell::K_SIZE as usize, AllocationType::Old);
        self.heap.set_map(object, map);
        let cell = PropertyCell::unchecked_cast(object);
        cell.set_name(&mut self.heap, name);
        cell.set_value(&mut self.heap, value);
        let cell_type = initial_cell_type(value, self.roots.undefined_value);
        cell.set_property_details(
            &mut self.heap,
            PropertyDetails::new_cell(PropertyKind::Data, NONE, cell_type),
        );
        cell.set_dependent_code(&mut self.heap, self.roots.undefined_value);
        cell
    }

    pub fn new_feedback_metadata(
        &mut self,
        kinds: &[crate::objects::feedback_vector::FeedbackSlotKind],
    ) -> crate::objects::feedback_vector::FeedbackMetadata {
        use crate::objects::feedback_vector::FeedbackMetadata;
        let slot_count = kinds.len() as i32;
        let map = self
            .roots
            .feedback_metadata_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            FeedbackMetadata::size_for(slot_count) as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let metadata = FeedbackMetadata::unchecked_cast(object);
        metadata.set_slot_count(&mut self.heap, slot_count);
        for (slot, kind) in kinds.iter().enumerate() {
            metadata.set_kind(&mut self.heap, slot as i32, *kind);
        }
        metadata
    }

    pub fn new_feedback_vector(
        &mut self,
        shared: crate::objects::shared_function_info::SharedFunctionInfo,
    ) -> crate::objects::feedback_vector::FeedbackVector {
        use crate::objects::feedback_vector::{FeedbackMetadata, FeedbackVector};

        let metadata = shared.feedback_metadata(&self.heap);
        let length = match metadata.to_strong_heap_object() {
            Some(object) => FeedbackMetadata::unchecked_cast(object).slot_count(&self.heap),
            None => 0,
        };
        let map = self
            .roots
            .feedback_vector_map
            .to_strong_heap_object()
            .unwrap();
        let scope = self.handle_scope();
        let shared_handle = self.create_object_handle(shared.object());
        let metadata_handle = self.create_handle(metadata);
        let object = self.allocate_raw_with_retry_or_fail(
            FeedbackVector::size_for(length) as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let vector = FeedbackVector::unchecked_cast(object);
        vector.set_length(&mut self.heap, length);
        vector.set_invocation_count(&mut self.heap, 0);
        let shared_value = shared_handle.get(self);
        let metadata = metadata_handle.get(self);
        self.close_handle_scope(scope);
        vector.set_shared_function_info(&mut self.heap, shared_value);
        vector.set_metadata(&mut self.heap, metadata);
        let uninitialized = self.roots.uninitialized_value;
        for slot in 0..length {
            vector.set(&mut self.heap, slot, uninitialized);
        }
        vector
    }

    /// `Factory::NewSharedFunctionInfo`
    pub fn new_shared_function_info(
        &mut self,
        name: &str,
        bytecode: Option<crate::objects::bytecode_array::BytecodeArray>,
        declared_parameter_count: i32,
    ) -> crate::objects::shared_function_info::SharedFunctionInfo {
        use crate::objects::shared_function_info::SharedFunctionInfo;

        let name = self.internalize_string(name).tagged();
        let map = self
            .roots
            .shared_function_info_map
            .to_strong_heap_object()
            .unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            SharedFunctionInfo::K_SIZE as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let info = SharedFunctionInfo::unchecked_cast(object);
        info.set_name(&mut self.heap, name);
        info.set_length(&mut self.heap, declared_parameter_count);
        info.set_formal_parameter_count(&mut self.heap, declared_parameter_count + 1);
        info.set_flags(&mut self.heap, 0);
        info.set_feedback_metadata(&mut self.heap, self.roots.undefined_value);
        match bytecode {
            Some(bytecode) => info.set_function_data(&mut self.heap, bytecode.tagged()),
            None => info.set_function_data(&mut self.heap, self.roots.undefined_value),
        }
        info
    }

    pub fn new_native_shared_function_info(
        &mut self,
        name: &str,
        builtin_id: i32,
        declared_parameter_count: i32,
    ) -> crate::objects::shared_function_info::SharedFunctionInfo {
        let info = self.new_shared_function_info(name, None, declared_parameter_count);
        info.set_is_native(&mut self.heap, true);
        info.set_function_data(&mut self.heap, Smi::from_int(builtin_id));
        info
    }

    /// `Factory::NewFunctionFromSharedFunctionInfo`
    pub fn new_js_function(
        &mut self,
        shared: crate::objects::shared_function_info::SharedFunctionInfo,
    ) -> crate::objects::js_function::JSFunction {
        use crate::objects::js_function::JSFunction;

        let is_native = shared.is_native(&self.heap);
        let map = if is_native {
            self.roots.js_native_function_map
        } else {
            self.roots.js_function_map
        };
        let map = Map::unchecked_cast(map.to_strong_heap_object().unwrap());
        let scope = self.handle_scope();
        let shared_handle = self.create_object_handle(shared.object());
        let object = self.new_js_object_from_map(map);
        let function_handle = self.create_object_handle(object.object());
        let function = JSFunction::unchecked_cast(function_handle.object(self));
        let shared_value = shared_handle.get(self);
        function.set_shared_function_info(&mut self.heap, shared_value);
        function.set_context(&mut self.heap, self.roots.undefined_value);
        if is_native {
            function.set_feedback_vector(&mut self.heap, self.roots.undefined_value);
        } else {
            let shared = crate::objects::shared_function_info::SharedFunctionInfo::unchecked_cast(
                shared_handle.object(self),
            );
            let vector = self.new_feedback_vector(shared).tagged();
            let function = JSFunction::unchecked_cast(function_handle.object(self));
            function.set_feedback_vector(&mut self.heap, vector);
        }
        let function = JSFunction::unchecked_cast(function_handle.object(self));
        self.close_handle_scope(scope);
        function
    }

    pub fn new_fixed_array_tenured(&mut self, length: i32) -> FixedArray {
        if length == 0 {
            return self.empty_fixed_array();
        }
        let map = self.roots.fixed_array_map.to_strong_heap_object().unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            FixedArray::size_for(length) as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let array = FixedArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        let undefined = self.undefined_value();
        for i in 0..length {
            array.set(&mut self.heap, i, undefined);
        }
        array
    }

    pub fn new_heap_number_tenured(&mut self, value: f64) -> HeapNumber {
        let map = self.roots.heap_number_map.to_strong_heap_object().unwrap();
        let object =
            self.allocate_raw_with_retry_or_fail(HeapNumber::K_SIZE as usize, AllocationType::Old);
        self.heap.set_map(object, map);
        let number = HeapNumber::unchecked_cast(object);
        number.set_value(&mut self.heap, value);
        number
    }

    fn new_string_tenured(&mut self, s: &str) -> VxString {
        let one_byte = VxString::is_one_byte_content(s);
        let map = if one_byte {
            self.roots.seq_one_byte_string_map
        } else {
            self.roots.seq_two_byte_string_map
        };
        self.allocate_string_raw(s, map, one_byte, AllocationType::Old)
    }

    pub fn object_size(&self, object: crate::objects::tagged::HeapObject) -> i32 {
        use crate::objects::instance_type as it;
        let map = self.heap.map_of(object);
        let instance_size = map.instance_size(&self.heap);
        if instance_size > 0 {
            return instance_size;
        }
        let instance_type = map.instance_type(&self.heap);
        if it::is_string(instance_type) {
            let string = VxString::unchecked_cast(object);
            let length = string.length(&self.heap);
            return if it::is_one_byte_string(instance_type) {
                VxString::size_for_one_byte(length)
            } else {
                VxString::size_for_two_byte(length)
            };
        }
        match instance_type {
            it::FIXED_ARRAY_TYPE | it::NUMBER_DICTIONARY_TYPE | it::NAME_DICTIONARY_TYPE => {
                FixedArray::size_for(FixedArray::unchecked_cast(object).length(&self.heap))
            }
            it::PROPERTY_ARRAY_TYPE => {
                PropertyArray::size_for(PropertyArray::unchecked_cast(object).length(&self.heap))
            }
            it::FIXED_DOUBLE_ARRAY_TYPE => FixedDoubleArray::size_for(
                FixedDoubleArray::unchecked_cast(object).length(&self.heap),
            ),
            it::TRANSITION_ARRAY_TYPE => {
                use crate::objects::fixed_array::WeakFixedArray;
                WeakFixedArray::size_for(WeakFixedArray::unchecked_cast(object).length(&self.heap))
            }
            it::FEEDBACK_VECTOR_TYPE => {
                use crate::objects::feedback_vector::FeedbackVector;
                FeedbackVector::size_for(FeedbackVector::unchecked_cast(object).length(&self.heap))
            }
            it::FEEDBACK_METADATA_TYPE => {
                use crate::objects::feedback_vector::FeedbackMetadata;
                FeedbackMetadata::size_for(
                    FeedbackMetadata::unchecked_cast(object).slot_count(&self.heap),
                )
            }
            it::DESCRIPTOR_ARRAY_TYPE => DescriptorArray::size_for(
                DescriptorArray::unchecked_cast(object).number_of_all_descriptors(&self.heap),
            ),
            it::BYTECODE_ARRAY_TYPE => {
                use crate::objects::bytecode_array::BytecodeArray;
                BytecodeArray::size_for(BytecodeArray::unchecked_cast(object).length(&self.heap))
            }
            it::CONTEXT_TYPE => {
                use crate::objects::contexts::Context;
                Context::size_for(Context::unchecked_cast(object).length(&self.heap))
            }
            it::SCOPE_INFO_TYPE => {
                use crate::objects::scope_info::ScopeInfo;
                ScopeInfo::size_for(
                    ScopeInfo::unchecked_cast(object).context_local_count(&self.heap),
                )
            }
            it::MAP_TYPE => Map::K_SIZE,
            other => panic!(
                "object at 0x{:x} has variable-size map but unknown instance type {} ({})",
                object.address(),
                other,
                it::instance_type_name(other)
            ),
        }
    }
}

pub const JS_OBJECT_HEADER_WORDS: i32 = JSObject::K_HEADER_SIZE / K_TAGGED_SIZE;

/// `HandlerTableBuilder::ToHandlerTable`
fn encode_handler_table(table: &crate::codegen::handler_table::HandlerTable) -> Vec<i32> {
    use crate::codegen::handler_table::HandlerTable;
    let mut encoded = Vec::new();
    for index in 0..table.number_of_range_entries() {
        let entry = table.entry(index as i32);
        encoded.push(entry.offset_start);
        encoded.push(entry.offset_end);
        encoded.push(HandlerTable::encode_handler(
            entry.offset_target,
            entry.prediction,
        ));
        encoded.push(entry.context.index());
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::heap_number::number_value;
    use crate::objects::instance_type;

    #[test]
    fn new_number_prefers_smis() {
        let mut isolate = Isolate::new();
        let smi = isolate.new_number(42.0);
        assert!(smi.is_smi());
        assert_eq!(Smi::to_int(smi), 42);

        let boxed = isolate.new_number(1.5);
        assert!(boxed.is_heap_object());
        assert_eq!(number_value(&isolate.heap, boxed), Some(1.5));

        let big = isolate.new_number(1e10);
        assert!(big.is_heap_object(), "beyond Smi range");
        assert_eq!(number_value(&isolate.heap, big), Some(1e10));

        let negative_zero = isolate.new_number(-0.0);
        assert!(negative_zero.is_heap_object(), "-0 must stay boxed");
        assert!(number_value(&isolate.heap, negative_zero)
            .unwrap()
            .is_sign_negative());
    }

    #[test]
    fn literal_strings_are_not_internalized_but_names_are() {
        let mut isolate = Isolate::new();
        let literal_a = isolate.new_string("hello");
        let literal_b = isolate.new_string("hello");
        assert_ne!(literal_a, literal_b, "two literals are two objects");
        assert!(!literal_a.is_internalized(&isolate.heap));
        assert!(literal_a.equals(&isolate.heap, literal_b));

        let name_a = isolate.internalize_string("hello");
        let name_b = isolate.internalize_string("hello");
        assert_eq!(name_a, name_b);
        assert!(name_a.is_internalized(&isolate.heap));
    }

    #[test]
    fn literal_strings_are_young_and_names_are_immortal() {
        let mut isolate = Isolate::new();
        let literal = isolate.new_string("transient");
        let name = isolate.internalize_string("permanent");
        assert!(isolate.heap.in_young_generation(literal.object()));
        assert!(isolate.heap.in_read_only_space(name.object()));
    }

    #[test]
    fn fresh_arrays_are_filled_with_undefined() {
        let mut isolate = Isolate::new();
        let array = isolate.new_fixed_array(3);
        let undefined = isolate.undefined_value();
        assert_eq!(array.length(&isolate.heap), 3);
        for i in 0..3 {
            assert_eq!(array.get(&isolate.heap, i), undefined);
        }
        assert_eq!(isolate.new_fixed_array(0), isolate.empty_fixed_array());
    }

    #[test]
    fn a_fresh_object_has_the_initial_map_and_no_properties() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let map = object.map(&isolate.heap);

        assert_eq!(map, isolate.roots.js_object_map());
        assert_eq!(map.number_of_own_descriptors(&isolate.heap), 0);
        assert_eq!(
            object.raw_properties_or_hash(&isolate.heap),
            isolate.roots.empty_property_array,
            "no backing store until the inline slots run out"
        );
        assert_eq!(
            object.elements(&isolate.heap),
            isolate.roots.empty_fixed_array
        );
        assert!(isolate.heap.in_young_generation(object.object()));
    }

    #[test]
    fn a_fresh_objects_inline_slots_hold_undefined_not_garbage() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let map = object.map(&isolate.heap);
        let undefined = isolate.undefined_value();
        for i in 0..map.inobject_properties(&isolate.heap) {
            assert_eq!(
                object.in_object_property_at(&isolate.heap, map, i),
                undefined,
                "slot {i} must be traceable before it is assigned"
            );
        }
    }

    #[test]
    fn two_fresh_objects_share_one_map() {
        let mut isolate = Isolate::new();
        let a = isolate.new_js_object();
        let b = isolate.new_js_object();
        assert_ne!(a, b);
        assert_eq!(
            a.map(&isolate.heap),
            b.map(&isolate.heap),
            "identical shapes must share a hidden class"
        );
    }

    #[test]
    fn copy_map_shell_keeps_the_shape_but_not_the_properties() {
        let mut isolate = Isolate::new();
        let source = isolate.roots.js_object_map();
        let copy = isolate.copy_map_shell(source);

        assert_ne!(copy, source);
        assert_eq!(
            copy.instance_size(&isolate.heap),
            source.instance_size(&isolate.heap)
        );
        assert_eq!(
            copy.inobject_properties(&isolate.heap),
            source.inobject_properties(&isolate.heap)
        );
        assert_eq!(
            copy.instance_type(&isolate.heap),
            source.instance_type(&isolate.heap)
        );
        assert_eq!(
            copy.prototype(&isolate.heap),
            source.prototype(&isolate.heap)
        );
        assert_eq!(copy.number_of_own_descriptors(&isolate.heap), 0);
        assert_eq!(
            copy.raw_transitions(&isolate.heap),
            Tagged::ZERO,
            "a copy starts with no transitions of its own"
        );
    }

    #[test]
    fn object_size_handles_fixed_and_variable_sized_objects() {
        let mut isolate = Isolate::new();

        let object = isolate.new_js_object();
        assert_eq!(
            isolate.object_size(object.object()),
            object.map(&isolate.heap).instance_size(&isolate.heap)
        );

        let number = isolate.new_heap_number(1.5);
        assert_eq!(isolate.object_size(number.object()), HeapNumber::K_SIZE);

        let short = isolate.new_string("abc");
        assert_eq!(
            isolate.object_size(short.object()),
            VxString::size_for_one_byte(3)
        );
        let wide = isolate.new_string("☃☃");
        assert_eq!(
            isolate.object_size(wide.object()),
            VxString::size_for_two_byte(2)
        );

        let array = isolate.new_fixed_array(5);
        assert_eq!(isolate.object_size(array.object()), FixedArray::size_for(5));

        let descriptors = isolate.new_descriptor_array(3);
        assert_eq!(
            isolate.object_size(descriptors.object()),
            DescriptorArray::size_for(3)
        );

        let map = isolate.roots.js_object_map();
        assert_eq!(isolate.object_size(map.object()), Map::K_SIZE);
        assert_eq!(
            isolate.heap.instance_type(map.object()),
            instance_type::MAP_TYPE
        );
    }

    #[test]
    fn growing_a_property_array_preserves_its_contents() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        isolate.handles.push(object.tagged());
        let rooted = |isolate: &Isolate| {
            JSObject::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap())
        };
        isolate.grow_property_array(rooted(&isolate), 2);
        let array = rooted(&isolate).property_array(&isolate.heap);
        array.set(&mut isolate.heap, 0, Smi::from_int(7));
        array.set(&mut isolate.heap, 1, Smi::from_int(8));

        isolate.grow_property_array(rooted(&isolate), 5);
        let grown = rooted(&isolate).property_array(&isolate.heap);
        assert_eq!(grown.length(&isolate.heap), 5);
        assert_eq!(Smi::to_int(grown.get(&isolate.heap, 0)), 7);
        assert_eq!(Smi::to_int(grown.get(&isolate.heap, 1)), 8);
        assert_eq!(
            grown.get(&isolate.heap, 4),
            isolate.undefined_value(),
            "new slots start as undefined"
        );

        isolate.grow_property_array(rooted(&isolate), 1);
        assert_eq!(
            rooted(&isolate)
                .property_array(&isolate.heap)
                .length(&isolate.heap),
            5
        );
    }
}

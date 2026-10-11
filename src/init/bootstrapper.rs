use crate::builtins::builtins::Builtin;
use crate::execution::isolate::Isolate;
use crate::handles::handles::*;
use crate::interpreter::bytecode_generator::{BytecodeGenerator, CompiledProgram};
use crate::objects::js_function::JSFunction;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::tagged::Tagged;
use crate::parsing::parser::Parser;

impl Isolate {
    pub fn setup_global_object(&mut self) {
        if self.roots.global_object.is_strong() {
            return;
        }
        let map = Map::unchecked_cast(
            self.roots
                .js_global_object_map
                .to_strong_heap_object()
                .unwrap(),
        );
        let global = self.new_js_object_from_map(map);
        self.roots.global_object = global.tagged();
        let scope = self.handle_scope();

        let undefined = self.undefined_value();
        self.set_property_by_str(self.global_object(), "undefined", undefined);
        let nan = self.new_heap_number(f64::NAN).tagged();
        self.set_property_by_str(self.global_object(), "NaN", nan);
        let infinity = self.new_heap_number(f64::INFINITY).tagged();
        self.set_property_by_str(self.global_object(), "Infinity", infinity);

        // `console.log`
        let console = self.new_js_object();
        let console = self.create_object_handle(console.object());
        self.install_builtin(console, "log", Builtin::ConsoleLog, 1);
        let console = console.get(self);
        self.set_property_by_str(self.global_object(), "console", console);

        // `Math`
        let math = self.new_js_object();
        let math = self.create_object_handle(math.object());
        self.install_builtin(math, "sqrt", Builtin::MathSqrt, 1);
        self.install_builtin(math, "floor", Builtin::MathFloor, 1);
        self.install_builtin(math, "abs", Builtin::MathAbs, 1);
        self.install_builtin(math, "max", Builtin::MathMax, 2);
        self.install_builtin(math, "min", Builtin::MathMin, 2);
        self.install_builtin(math, "pow", Builtin::MathPow, 2);
        let math = math.get(self);
        self.set_property_by_str(self.global_object(), "Math", math);

        let global = self.roots.global_object;
        self.set_property_by_str(self.global_object(), "globalThis", global);

        self.initialize_native_context();
        self.close_handle_scope(scope);
    }

    fn initialize_native_context(&mut self) {
        use crate::objects::contexts::NativeContext;
        use crate::objects::elements_kind::FAST_ELEMENTS_KIND_SEQUENCE;

        let undefined = self.undefined_value();
        let context = self.new_function_context(
            NativeContext::NATIVE_CONTEXT_SLOTS
                - crate::objects::contexts::Context::MIN_CONTEXT_SLOTS,
            undefined,
        );
        let context = NativeContext::unchecked_cast(context.object());
        self.roots.native_context = context.tagged();

        let scope = self.handle_scope();

        let object_prototype = self.new_js_object();
        let object_prototype = self.create_object_handle(object_prototype.object());
        self.install_builtin(
            object_prototype,
            "hasOwnProperty",
            Builtin::ObjectPrototypeHasOwnProperty,
            1,
        );
        self.install_builtin(
            object_prototype,
            "toString",
            Builtin::ObjectPrototypeToString,
            0,
        );

        let array_prototype = self.new_js_object();
        let array_prototype = self.create_object_handle(array_prototype.object());
        self.install_builtin(array_prototype, "push", Builtin::ArrayPrototypePush, 1);
        self.install_builtin(array_prototype, "pop", Builtin::ArrayPrototypePop, 0);
        self.install_builtin(array_prototype, "join", Builtin::ArrayPrototypeJoin, 1);
        self.install_builtin(
            array_prototype,
            "indexOf",
            Builtin::ArrayPrototypeIndexOf,
            1,
        );

        let string_prototype = self.new_js_object();
        let string_prototype = self.create_object_handle(string_prototype.object());
        self.install_builtin(
            string_prototype,
            "charAt",
            Builtin::StringPrototypeCharAt,
            1,
        );
        self.install_builtin(
            string_prototype,
            "indexOf",
            Builtin::StringPrototypeIndexOf,
            1,
        );
        self.install_builtin(string_prototype, "slice", Builtin::StringPrototypeSlice, 2);
        self.install_builtin(
            string_prototype,
            "toUpperCase",
            Builtin::StringPrototypeToUpperCase,
            0,
        );
        self.install_builtin(
            string_prototype,
            "toLowerCase",
            Builtin::StringPrototypeToLowerCase,
            0,
        );

        let number_prototype = self.new_js_object();
        let number_prototype = self.create_object_handle(number_prototype.object());
        self.install_builtin(
            number_prototype,
            "toString",
            Builtin::NumberPrototypeToString,
            0,
        );

        let function_prototype = self.new_js_object();
        let function_prototype = self.create_object_handle(function_prototype.object());
        self.install_builtin(
            function_prototype,
            "call",
            Builtin::FunctionPrototypeCall,
            1,
        );

        let object_prototype_value = object_prototype.get(self);
        self.roots.object_prototype = object_prototype_value;
        for prototype in [
            array_prototype,
            string_prototype,
            number_prototype,
            function_prototype,
        ] {
            let map = self.heap.map_of(prototype.object(self));
            map.set_prototype(&mut self.heap, object_prototype_value);
        }

        let object_maps = [self.roots.js_object_map, self.roots.js_global_object_map];
        for map in object_maps {
            let map =
                crate::objects::map::Map::unchecked_cast(map.to_strong_heap_object().unwrap());
            map.set_prototype(&mut self.heap, object_prototype_value);
        }

        let array_prototype_value = array_prototype.get(self);
        self.roots.array_prototype = array_prototype_value;
        for kind in FAST_ELEMENTS_KIND_SEQUENCE {
            let map = crate::objects::map::Map::unchecked_cast(
                self.roots
                    .get_initial_js_array_map(kind)
                    .to_strong_heap_object()
                    .unwrap(),
            );
            map.set_prototype(&mut self.heap, array_prototype_value);
        }
        let dictionary_array_map = crate::objects::map::Map::unchecked_cast(
            self.roots
                .js_array_dictionary_elements_map
                .to_strong_heap_object()
                .unwrap(),
        );
        dictionary_array_map.set_prototype(&mut self.heap, array_prototype_value);

        let function_prototype_value = function_prototype.get(self);
        self.roots.function_prototype = function_prototype_value;
        for map in [
            self.roots.js_function_map,
            self.roots.js_native_function_map,
        ] {
            let map =
                crate::objects::map::Map::unchecked_cast(map.to_strong_heap_object().unwrap());
            map.set_prototype(&mut self.heap, function_prototype_value);
        }

        let context = NativeContext::unchecked_cast(
            self.roots.native_context.to_strong_heap_object().unwrap(),
        );
        let string_prototype_value = string_prototype.get(self);
        let number_prototype_value = number_prototype.get(self);
        context.set(
            &mut self.heap,
            NativeContext::INITIAL_OBJECT_PROTOTYPE_INDEX,
            object_prototype_value,
        );
        context.set(
            &mut self.heap,
            NativeContext::INITIAL_ARRAY_PROTOTYPE_INDEX,
            array_prototype_value,
        );
        context.set(
            &mut self.heap,
            NativeContext::INITIAL_STRING_PROTOTYPE_INDEX,
            string_prototype_value,
        );
        context.set(
            &mut self.heap,
            NativeContext::INITIAL_NUMBER_PROTOTYPE_INDEX,
            number_prototype_value,
        );
        context.set(
            &mut self.heap,
            NativeContext::INITIAL_FUNCTION_PROTOTYPE_INDEX,
            function_prototype_value,
        );

        let object_constructor = self.new_js_object();
        let object_constructor = self.create_object_handle(object_constructor.object());
        self.install_builtin(object_constructor, "keys", Builtin::ObjectKeys, 1);
        let object_constructor_value = object_constructor.get(self);
        self.set_property_by_str(self.global_object(), "Object", object_constructor_value);

        self.close_handle_scope(scope);
    }

    /// `Genesis::SimpleInstallFunction`
    fn install_builtin(
        &mut self,
        target: Handle,
        name: &str,
        builtin: Builtin,
        parameter_count: i32,
    ) {
        let scope = self.handle_scope();
        let shared = self.new_native_shared_function_info(name, builtin.id(), parameter_count);
        let shared = self.create_object_handle(shared.object());
        let shared = crate::objects::shared_function_info::SharedFunctionInfo::unchecked_cast(
            shared.object(self),
        );
        let function = self.new_js_function(shared).tagged();
        let target = JSObject::unchecked_cast(target.object(self));
        self.add_property_by_str_with_attributes(
            target,
            name,
            function,
            crate::objects::property_details::DONT_ENUM,
        );
        self.close_handle_scope(scope);
    }

    /// `Isolate::global_object`
    pub fn global_object(&self) -> JSObject {
        JSObject::unchecked_cast(
            self.roots
                .global_object
                .to_strong_heap_object()
                .expect("the global object has not been set up"),
        )
    }

    pub(crate) fn closure_for(&mut self, index: usize) -> Tagged {
        let undefined = self.undefined_value();
        self.closure_for_with_context(index, undefined)
    }

    pub(crate) fn closure_for_with_context(&mut self, index: usize, context: Tagged) -> Tagged {
        let scope = self.handle_scope();
        let context = self.create_handle(context);
        let shared = self.shared_function_infos[index];
        let shared = crate::objects::shared_function_info::SharedFunctionInfo::unchecked_cast(
            shared.to_strong_heap_object().expect("a compiled function"),
        );
        let function = self.new_js_function(shared);
        let function_handle = self.create_object_handle(function.object());
        let context = context.get(self);
        let function =
            crate::objects::js_function::JSFunction::unchecked_cast(function_handle.object(self));
        function.set_context(&mut self.heap, context);
        let result = function.tagged();
        self.close_handle_scope(scope);
        result
    }

    pub fn script_feedback_vector(
        &self,
    ) -> Option<crate::objects::feedback_vector::FeedbackVector> {
        let function = self.script_function.to_strong_heap_object()?;
        let function = JSFunction::unchecked_cast(function);
        crate::objects::feedback_vector::try_cast_feedback_vector(
            &self.heap,
            function.feedback_vector(&self.heap),
        )
    }

    pub fn trace_ic(&self) -> Vec<String> {
        let Some(vector) = self.script_feedback_vector() else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        let mut slot = 0;
        let length = vector.length(&self.heap);
        while slot < length {
            let kind = vector.get_kind(&self.heap, slot);
            if kind != crate::objects::feedback_vector::FeedbackSlotKind::Invalid {
                let nexus =
                    crate::objects::feedback_vector::FeedbackNexus::new(&self.heap, vector, slot);
                if self.ic_state(nexus) != crate::common::globals::InlineCacheState::Uninitialized {
                    lines.push(self.describe_ic(vector, slot));
                }
            }
            slot += 1;
        }
        lines
    }

    pub(crate) fn write_output(&mut self, line: &str) {
        self.output.push(line.to_string());
    }

    pub fn output(&self) -> &[String] {
        &self.output
    }

    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }

    pub fn run_script(&mut self, source: &str) -> Result<Tagged, String> {
        self.setup_global_object();
        let program = Parser::parse_program(source).map_err(|error| error.display(source))?;
        let compiled =
            BytecodeGenerator::generate(&program).map_err(|error| error.message.clone())?;
        self.run_compiled(&compiled)
    }

    pub fn run_compiled(&mut self, compiled: &CompiledProgram) -> Result<Tagged, String> {
        self.setup_global_object();

        let base = self.shared_function_infos.len();
        assert_eq!(base, 0);
        for function in &compiled.functions {
            let scope = self.handle_scope();
            let bytecode = self.new_bytecode_array(&function.data);
            let bytecode = self.create_object_handle(bytecode.object());
            let bytecode = crate::objects::bytecode_array::BytecodeArray::unchecked_cast(
                bytecode.object(self),
            );
            let shared = self.new_shared_function_info(
                &function.name,
                Some(bytecode),
                function.declared_parameter_count,
            );
            let shared = self.create_object_handle(shared.object());
            let kinds = function.data.feedback_slot_kinds.clone();
            let metadata = self.new_feedback_metadata(&kinds).tagged();
            let shared = crate::objects::shared_function_info::SharedFunctionInfo::unchecked_cast(
                shared.object(self),
            );
            shared.set_feedback_metadata(&mut self.heap, metadata);
            self.shared_function_infos.push(shared.tagged());
            self.close_handle_scope(scope);
        }

        let undefined = self.undefined_value();
        for name in &compiled.global_declarations {
            let key = self.internalize_string(name).tagged();
            let global = self.global_object();
            if self.get_own_property(global, key).is_none() {
                self.set_property(global, key, undefined);
            }
        }

        let script = self.closure_for(0);
        self.script_function = script;
        let script = JSFunction::unchecked_cast(script.to_strong_heap_object().unwrap());
        let receiver = self.undefined_value();
        match self.execute(script, receiver, &[]) {
            Ok(value) => Ok(value),
            Err(thrown) => Err(self.to_string(thrown.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_global_object_carries_the_builtins() {
        let mut isolate = Isolate::new();
        isolate.setup_global_object();
        let global = isolate.global_object();

        let math = isolate.get_property_by_str(global, "Math");
        assert!(math.is_strong(), "Math should be installed");
        let math = JSObject::unchecked_cast(math.to_strong_heap_object().unwrap());
        let sqrt = isolate.get_property_by_str(math, "sqrt");
        assert!(sqrt.is_strong());
        assert!(crate::objects::instance_type::is_callable(
            isolate
                .heap
                .instance_type(sqrt.to_strong_heap_object().unwrap())
        ));

        let global_this = isolate.get_property_by_str(global, "globalThis");
        assert_eq!(global_this, global.tagged());
    }

    #[test]
    fn setting_up_twice_is_a_no_op() {
        let mut isolate = Isolate::new();
        isolate.setup_global_object();
        let global = isolate.global_object();
        isolate.setup_global_object();
        assert_eq!(isolate.global_object(), global);
    }

    #[test]
    fn console_log_output_is_captured() {
        let mut isolate = Isolate::new();
        isolate.run_script("console.log('hello', 1 + 1);").unwrap();
        assert_eq!(isolate.output(), &["hello 2".to_string()]);
    }

    #[test]
    fn a_syntax_error_is_reported_with_its_position() {
        let mut isolate = Isolate::new();
        let error = isolate.run_script("var a = ;").unwrap_err();
        assert!(error.contains("SyntaxError"), "{error}");
        assert!(error.contains("line 1:"), "{error}");
    }

    #[test]
    fn unimplemented_syntax_is_reported_rather_than_mis_run() {
        let mut isolate = Isolate::new();
        let error = isolate.run_script("class A {}").unwrap_err();
        assert!(error.contains("classes"), "{error}");
    }
}

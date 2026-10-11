//! Ported from `src/init/bootstrapper.cc`.
//!
//! Builds the world a script runs in: the global object, and the functions
//! and objects reachable from it.
//!
//! V8 does this once and serialises the result into a snapshot, so a real
//! isolate starts by deserialising rather than by running this. `vx` builds
//! it every time.
//!
//! Deviation: V8 installs the whole of the standard library here, through a
//! `NativeContext` that also holds the prototypes, the constructors and the
//! realm's intrinsics. `vx` has a global object with a handful of builtins
//! and no context, so `globalThis`, realms and prototypes-per-realm do not
//! exist. See `PORTING.md`.

use crate::builtins::builtins::Builtin;
use crate::execution::isolate::Isolate;
use crate::interpreter::bytecode_generator::{BytecodeGenerator, CompiledProgram};
use crate::objects::js_function::JSFunction;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::tagged::Tagged;
use crate::parsing::parser::Parser;

impl Isolate {
    /// `Genesis::CreateRoots` plus `Genesis::InstallGlobalThisBinding`.
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

        // `undefined`, `NaN` and `Infinity` are *properties of the global
        // object*, not keywords -- which is why `typeof undefined` is a
        // global load and why `undefined` was shadowable before ES5.
        let undefined = self.undefined_value();
        self.set_property_by_str(global, "undefined", undefined);
        let nan = self.new_heap_number(f64::NAN).tagged();
        self.set_property_by_str(global, "NaN", nan);
        let infinity = self.new_heap_number(f64::INFINITY).tagged();
        self.set_property_by_str(global, "Infinity", infinity);

        // `console.log`
        let console = self.new_js_object();
        self.install_builtin(console, "log", Builtin::ConsoleLog, 1);
        self.set_property_by_str(global, "console", console.tagged());

        // `Math`
        let math = self.new_js_object();
        self.install_builtin(math, "sqrt", Builtin::MathSqrt, 1);
        self.install_builtin(math, "floor", Builtin::MathFloor, 1);
        self.install_builtin(math, "abs", Builtin::MathAbs, 1);
        self.install_builtin(math, "max", Builtin::MathMax, 2);
        self.install_builtin(math, "min", Builtin::MathMin, 2);
        self.install_builtin(math, "pow", Builtin::MathPow, 2);
        self.set_property_by_str(global, "Math", math.tagged());

        // `globalThis`, which is just the global object under a name.
        self.set_property_by_str(global, "globalThis", global.tagged());
    }

    /// `Genesis::SimpleInstallFunction`
    fn install_builtin(
        &mut self,
        target: JSObject,
        name: &str,
        builtin: Builtin,
        parameter_count: i32,
    ) {
        let shared = self.new_native_shared_function_info(name, builtin.id(), parameter_count);
        let function = self.new_js_function(shared);
        self.set_property_by_str(target, name, function.tagged());
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

    /// A closure over the compiled function at `index`.
    pub(crate) fn closure_for(&mut self, index: usize) -> Tagged {
        let shared = self.shared_function_infos[index];
        let shared = crate::objects::shared_function_info::SharedFunctionInfo::unchecked_cast(
            shared.to_strong_heap_object().expect("a compiled function"),
        );
        self.new_js_function(shared).tagged()
    }

    pub(crate) fn write_output(&mut self, line: &str) {
        self.output.push(line.to_string());
    }

    /// Everything `console.log` has printed.
    pub fn output(&self) -> &[String] {
        &self.output
    }

    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }

    /// `Compiler::Compile` followed by `Execution::Call`: the whole pipeline.
    ///
    /// Returns the script's completion value -- the value of its last
    /// expression statement, which is what `eval` yields.
    pub fn run_script(&mut self, source: &str) -> Result<Tagged, String> {
        self.setup_global_object();
        let program = Parser::parse_program(source).map_err(|error| error.display(source))?;
        let compiled =
            BytecodeGenerator::generate(&program).map_err(|error| error.message.clone())?;
        self.run_compiled(&compiled)
    }

    /// Materialises a compiled program and runs its top level.
    pub fn run_compiled(&mut self, compiled: &CompiledProgram) -> Result<Tagged, String> {
        self.setup_global_object();

        // Materialise every function, so that `CreateClosure` can name one
        // by index -- including a function that has not been reached yet.
        let base = self.shared_function_infos.len();
        assert_eq!(base, 0, "one script per isolate for now");
        for function in &compiled.functions {
            let bytecode = self.new_bytecode_array(&function.data);
            let shared = self.new_shared_function_info(
                &function.name,
                Some(bytecode),
                function.declared_parameter_count,
            );
            self.shared_function_infos.push(shared.tagged());
        }

        // Hoisted top-level `var`s exist, holding undefined, before the
        // first statement runs.
        let global = self.global_object();
        let undefined = self.undefined_value();
        for name in &compiled.global_declarations {
            let key = self.internalize_string(name).tagged();
            if self.get_own_property(global, key).is_none() {
                self.set_property(global, key, undefined);
            }
        }

        let script = self.closure_for(0);
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

        // `globalThis` is the global object itself.
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

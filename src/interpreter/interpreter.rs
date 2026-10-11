//! Ported from `src/interpreter/interpreter.h` and the bytecode handlers in
//! `src/interpreter/interpreter-generator.cc`.
//!
//! The dispatch loop. V8's handlers are not a loop at all: each bytecode has
//! its own machine-code handler, generated from CSA, and each one ends by
//! jumping straight to the next handler through a dispatch table. `vx`'s is a
//! `match` in a loop -- the same semantics, one indirect branch instead of a
//! tail call per bytecode.
//!
//! The frame
//! ---------
//! Registers live on [`Isolate::stack`], so the collector traces them as
//! roots: an object held only in a register across an allocation still
//! survives.
//!
//! ```text
//!   stack: [ ... | receiver | arg0 | arg1 | r0 | r1 | ... ]
//!                 ^params_base              ^registers_base
//! ```
//!
//! A parameter's register index is negative, so the two halves are addressed
//! from the same signed index -- see [`crate::interpreter::bytecode_register`].
//!
//! Deviations, tracked in `PORTING.md`: the fixed frame slots (the closure,
//! the context, the feedback vector) are fields of the Rust frame rather than
//! stack slots, because nothing reads them through a register yet. A call
//! recurses into this function rather than pushing a machine frame, so
//! JavaScript recursion is bounded by Rust's stack and is checked explicitly.
//! Keyed property access converts its key to a string name, so an array is
//! an object with `"0"`, `"1"` ... properties and has no `length`.

use crate::builtins::builtins::Builtin;
use crate::execution::isolate::Isolate;
use crate::interpreter::bytecode_operands::OperandScale;
use crate::interpreter::bytecode_register::Register;
use crate::interpreter::bytecodes::Bytecode;
use crate::objects::bytecode_array::BytecodeArray;
use crate::objects::js_function::JSFunction;
use crate::objects::js_objects::JSObject;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

/// A thrown value, propagating up. V8 unwinds through the handler table;
/// `vx` has no `try`, so this always reaches the top.
#[derive(Copy, Clone, Debug)]
pub struct Thrown(pub Tagged);

pub type ExecutionResult = Result<Tagged, Thrown>;

/// How deep JavaScript calls may nest. Each one is a Rust frame, so this
/// stands in for V8's stack guard.
const K_MAX_CALL_DEPTH: usize = 180;

/// The part of the interpreter frame that is not on the value stack.
struct InterpreterFrame {
    bytecode: BytecodeArray,
    /// This closure's feedback vector, where the inline caches live.
    /// `None` for a function compiled without one.
    feedback_vector: Option<crate::objects::feedback_vector::FeedbackVector>,
    /// Where the receiver sits on `isolate.stack`; parameter *i* is at
    /// `params_base + i`.
    params_base: usize,
    params_count: usize,
    /// Where `r0` sits.
    registers_base: usize,
}

impl Isolate {
    /// `Interpreter::Execute`: runs `function` and returns its value.
    ///
    /// `args` excludes the receiver. Missing parameters become `undefined`
    /// and extra arguments are dropped, as the specification requires.
    pub fn execute(
        &mut self,
        function: JSFunction,
        receiver: Tagged,
        args: &[Tagged],
    ) -> ExecutionResult {
        if self.call_depth >= K_MAX_CALL_DEPTH {
            let message = self.new_string("Maximum call stack size exceeded").tagged();
            return Err(Thrown(message));
        }

        let shared = function.shared(&self.heap);
        if shared.is_native(&self.heap) {
            let builtin = Builtin::from_id(shared.builtin_id(&self.heap))
                .expect("a native function names a builtin");
            self.call_depth += 1;
            let result = self.call_builtin(builtin, args);
            self.call_depth -= 1;
            return result;
        }

        let bytecode = shared.bytecode_array(&self.heap);
        let params_count = bytecode.parameter_count(&self.heap) as usize;
        let register_count = bytecode.register_count(&self.heap) as usize;

        // Lay the frame out on the value stack, so everything in it is a
        // root for the collector.
        let params_base = self.stack.len();
        self.stack.push(receiver);
        let undefined = self.undefined_value();
        for index in 0..params_count.saturating_sub(1) {
            self.stack
                .push(args.get(index).copied().unwrap_or(undefined));
        }
        let registers_base = self.stack.len();
        for _ in 0..register_count {
            self.stack.push(undefined);
        }

        // `--trace-ic` and, in V8, the tier-up decision read this.
        let feedback_vector = crate::objects::feedback_vector::try_cast_feedback_vector(
            &self.heap,
            function.feedback_vector(&self.heap),
        );
        if let Some(vector) = feedback_vector {
            vector.increment_invocation_count(&mut self.heap);
        }

        let frame = InterpreterFrame {
            bytecode,
            feedback_vector,
            params_base,
            params_count,
            registers_base,
        };

        self.call_depth += 1;
        let result = self.dispatch(&frame);
        self.call_depth -= 1;

        // Pop the frame whether or not the call threw.
        self.stack.truncate(params_base);
        result
    }

    /// Where a register lives on the value stack.
    #[inline]
    fn register_slot(&self, frame: &InterpreterFrame, register: Register) -> usize {
        if register.is_parameter() {
            let index = register.to_parameter_index() as usize;
            debug_assert!(index < frame.params_count, "parameter {index} out of range");
            frame.params_base + index
        } else {
            frame.registers_base + register.index() as usize
        }
    }

    #[inline]
    fn read_register(&self, frame: &InterpreterFrame, register: Register) -> Tagged {
        self.stack[self.register_slot(frame, register)]
    }

    #[inline]
    fn write_register(&mut self, frame: &InterpreterFrame, register: Register, value: Tagged) {
        let slot = self.register_slot(frame, register);
        self.stack[slot] = value;
    }

    /// The dispatch loop proper.
    fn dispatch(&mut self, frame: &InterpreterFrame) -> ExecutionResult {
        let bytecode_array = frame.bytecode;
        let length = bytecode_array.length(&self.heap);
        let mut accumulator = self.undefined_value();
        let mut offset: i32 = 0;

        while offset < length {
            // Resolve the prefix: a Wide/ExtraWide byte scales every
            // scalable operand of the instruction that follows it.
            let mut scale = OperandScale::Single;
            let mut opcode_offset = offset;
            let mut bytecode = Bytecode::from_byte(bytecode_array.get(&self.heap, offset))
                .unwrap_or(Bytecode::Illegal);
            if let Some(prefixed) = bytecode.prefix_to_operand_scale() {
                scale = prefixed;
                opcode_offset = offset + 1;
                bytecode = Bytecode::from_byte(bytecode_array.get(&self.heap, opcode_offset))
                    .unwrap_or(Bytecode::Illegal);
            }

            let operand = |isolate: &Isolate, index: usize| -> i32 {
                let at = opcode_offset + bytecode.operand_offset(index, scale) as i32;
                let operand_type = bytecode.operand_type(index);
                let size = operand_type.size(scale);
                let mut bytes = [0u8; 4];
                for i in 0..size.bytes() {
                    bytes[i] = bytecode_array.get(&isolate.heap, at + i as i32);
                }
                crate::interpreter::bytecode_array_iterator::read_operand(
                    &bytes,
                    0,
                    size,
                    operand_type.is_unsigned(),
                )
            };
            let register_operand =
                |isolate: &Isolate, index: usize| Register::from_operand(operand(isolate, index));

            // Advance past the instruction; a jump overwrites this.
            let instruction_size = (opcode_offset - offset) + bytecode.size(scale) as i32;
            let mut next_offset = offset + instruction_size;

            match bytecode {
                // -- loads ------------------------------------------------
                Bytecode::LdaZero => accumulator = Smi::from_int(0),
                Bytecode::LdaSmi => accumulator = Smi::from_int(operand(self, 0)),
                Bytecode::LdaUndefined => accumulator = self.undefined_value(),
                Bytecode::LdaNull => accumulator = self.null_value(),
                Bytecode::LdaTheHole => accumulator = self.the_hole_value(),
                Bytecode::LdaTrue => accumulator = self.true_value(),
                Bytecode::LdaFalse => accumulator = self.false_value(),
                Bytecode::LdaConstant => {
                    let index = operand(self, 0);
                    accumulator = bytecode_array
                        .constant_pool(&self.heap)
                        .get(&self.heap, index);
                }
                Bytecode::Ldar => {
                    let register = register_operand(self, 0);
                    accumulator = self.read_register(frame, register);
                }
                Bytecode::Star => {
                    let register = register_operand(self, 0);
                    self.write_register(frame, register, accumulator);
                }
                Bytecode::Mov => {
                    let from = register_operand(self, 0);
                    let to = register_operand(self, 1);
                    let value = self.read_register(frame, from);
                    self.write_register(frame, to, value);
                }

                // -- globals ----------------------------------------------
                Bytecode::LdaGlobal => {
                    let name = self.constant_name(bytecode_array, operand(self, 0));
                    let global = self.global_object();
                    accumulator = match self.get_own_property(global, name) {
                        Some(value) => value,
                        None => {
                            let text = self.to_string(name);
                            let message =
                                self.new_string(&format!("{text} is not defined")).tagged();
                            return Err(Thrown(message));
                        }
                    };
                }
                Bytecode::StaGlobal => {
                    let name = self.constant_name(bytecode_array, operand(self, 0));
                    let global = self.global_object();
                    self.set_property(global, name, accumulator);
                }

                // -- properties -------------------------------------------
                Bytecode::GetNamedProperty => {
                    let object = self.read_register(frame, register_operand(self, 0));
                    let name = self.constant_name(bytecode_array, operand(self, 1));
                    let slot = operand(self, 2);
                    accumulator = self.load_named_property_ic(frame, object, name, slot)?;
                }
                Bytecode::GetKeyedProperty => {
                    let object = self.read_register(frame, register_operand(self, 0));
                    let key = self.key_to_name(accumulator);
                    accumulator = self.load_property(object, key)?;
                }
                Bytecode::SetNamedProperty | Bytecode::DefineNamedOwnProperty => {
                    let object = self.read_register(frame, register_operand(self, 0));
                    let name = self.constant_name(bytecode_array, operand(self, 1));
                    let slot = operand(self, 2);
                    self.store_named_property_ic(frame, object, name, accumulator, slot)?;
                    // Both bytecodes clobber the accumulator.
                    accumulator = self.undefined_value();
                }
                Bytecode::SetKeyedProperty => {
                    let object = self.read_register(frame, register_operand(self, 0));
                    let key = self.read_register(frame, register_operand(self, 1));
                    let key = self.key_to_name(key);
                    self.store_property(object, key, accumulator)?;
                    accumulator = self.undefined_value();
                }

                // -- binary operators -------------------------------------
                Bytecode::Add
                | Bytecode::Sub
                | Bytecode::Mul
                | Bytecode::Div
                | Bytecode::Mod
                | Bytecode::Exp
                | Bytecode::BitwiseOr
                | Bytecode::BitwiseXor
                | Bytecode::BitwiseAnd
                | Bytecode::ShiftLeft
                | Bytecode::ShiftRight
                | Bytecode::ShiftRightLogical => {
                    // `acc = reg op acc`: the register is the left operand.
                    let left = self.read_register(frame, register_operand(self, 0));
                    accumulator = self.apply_binary_op(bytecode, left, accumulator);
                }
                Bytecode::AddSmi
                | Bytecode::SubSmi
                | Bytecode::MulSmi
                | Bytecode::DivSmi
                | Bytecode::ModSmi => {
                    // `acc = acc op imm`, the other way round.
                    let right = Smi::from_int(operand(self, 0));
                    accumulator = self.apply_binary_op(bytecode, accumulator, right);
                }

                // -- unary operators --------------------------------------
                Bytecode::Inc => {
                    let one = Smi::from_int(1);
                    accumulator = self.add(accumulator, one);
                }
                Bytecode::Dec => {
                    let one = Smi::from_int(1);
                    accumulator = self.subtract(accumulator, one);
                }
                Bytecode::Negate => {
                    let value = self.to_number(accumulator);
                    accumulator = self.new_number(-value);
                }
                Bytecode::BitwiseNot => accumulator = self.bitwise_not(accumulator),
                Bytecode::ToBooleanLogicalNot => {
                    let truthy = self.to_boolean(accumulator);
                    accumulator = self.boolean_value(!truthy);
                }
                Bytecode::LogicalNot => {
                    // The operand is already a boolean, so this is identity
                    // on the other oddball rather than a coercion.
                    accumulator = self.boolean_value(accumulator != self.true_value());
                }
                Bytecode::TypeOf => accumulator = self.type_of(accumulator),

                // -- conversions ------------------------------------------
                Bytecode::ToBoolean => {
                    let truthy = self.to_boolean(accumulator);
                    accumulator = self.boolean_value(truthy);
                }
                Bytecode::ToNumber => {
                    let value = self.to_number(accumulator);
                    accumulator = self.new_number(value);
                }
                Bytecode::ToString => {
                    let text = self.to_string(accumulator);
                    accumulator = self.new_string(&text).tagged();
                }
                Bytecode::ToName => accumulator = self.key_to_name(accumulator),

                // -- comparisons ------------------------------------------
                Bytecode::TestEqual
                | Bytecode::TestEqualStrict
                | Bytecode::TestLessThan
                | Bytecode::TestGreaterThan
                | Bytecode::TestLessThanOrEqual
                | Bytecode::TestGreaterThanOrEqual
                | Bytecode::TestIn => {
                    let left = self.read_register(frame, register_operand(self, 0));
                    let result = self.apply_comparison(bytecode, left, accumulator);
                    accumulator = self.boolean_value(result);
                }
                Bytecode::TestReferenceEqual => {
                    let left = self.read_register(frame, register_operand(self, 0));
                    accumulator = self.boolean_value(left == accumulator);
                }
                Bytecode::TestUndetectable => {
                    // True for `null` and `undefined`; in V8 also for
                    // `document.all`, which `vx` has no way to create.
                    let null = self.null_value();
                    let undefined = self.undefined_value();
                    accumulator =
                        self.boolean_value(accumulator == null || accumulator == undefined);
                }
                Bytecode::TestNull => {
                    let null = self.null_value();
                    accumulator = self.boolean_value(accumulator == null);
                }
                Bytecode::TestUndefined => {
                    let undefined = self.undefined_value();
                    accumulator = self.boolean_value(accumulator == undefined);
                }
                Bytecode::TestInstanceOf => {
                    let message = self.new_string("`instanceof` is not implemented").tagged();
                    return Err(Thrown(message));
                }

                // -- literals ---------------------------------------------
                Bytecode::CreateEmptyObjectLiteral => {
                    accumulator = self.new_js_object().tagged();
                }
                Bytecode::CreateEmptyArrayLiteral => {
                    // Deviation: an array is a plain object here, so it has
                    // no `length`. See the module docs.
                    accumulator = self.new_js_object().tagged();
                }
                Bytecode::CreateClosure => {
                    let index = operand(self, 0);
                    let shared_index = bytecode_array
                        .constant_pool(&self.heap)
                        .get(&self.heap, index);
                    accumulator = self.closure_for(Smi::to_int(shared_index) as usize);
                }

                // -- calls ------------------------------------------------
                Bytecode::CallUndefinedReceiver | Bytecode::CallProperty => {
                    let callee = self.read_register(frame, register_operand(self, 0));
                    let first_argument = register_operand(self, 1);
                    let argument_count = operand(self, 2);

                    let mut arguments = Vec::with_capacity(argument_count as usize);
                    for index in 0..argument_count {
                        let register = Register::new(first_argument.index() + index);
                        arguments.push(self.read_register(frame, register));
                    }

                    let (receiver, arguments) = if bytecode == Bytecode::CallProperty {
                        // The receiver is the first register of the list.
                        let receiver = arguments.remove(0);
                        (receiver, arguments)
                    } else {
                        (self.undefined_value(), arguments)
                    };

                    let Some(callee_object) = callee.to_strong_heap_object() else {
                        let message = self.not_a_function_message(callee);
                        return Err(Thrown(message));
                    };
                    if !crate::objects::instance_type::is_callable(
                        self.heap.instance_type(callee_object),
                    ) {
                        let message = self.not_a_function_message(callee);
                        return Err(Thrown(message));
                    }
                    let callee_function = JSFunction::unchecked_cast(callee_object);
                    accumulator = self.execute(callee_function, receiver, &arguments)?;
                }

                // -- control flow -----------------------------------------
                Bytecode::Jump
                | Bytecode::JumpIfTrue
                | Bytecode::JumpIfFalse
                | Bytecode::JumpIfToBooleanTrue
                | Bytecode::JumpIfToBooleanFalse
                | Bytecode::JumpIfNull
                | Bytecode::JumpIfNotNull
                | Bytecode::JumpIfUndefined
                | Bytecode::JumpIfNotUndefined
                | Bytecode::JumpIfUndefinedOrNull
                | Bytecode::JumpConstant
                | Bytecode::JumpIfTrueConstant
                | Bytecode::JumpIfFalseConstant
                | Bytecode::JumpIfToBooleanTrueConstant
                | Bytecode::JumpIfToBooleanFalseConstant
                | Bytecode::JumpIfNullConstant
                | Bytecode::JumpIfNotNullConstant
                | Bytecode::JumpIfUndefinedConstant
                | Bytecode::JumpIfNotUndefinedConstant
                | Bytecode::JumpIfUndefinedOrNullConstant => {
                    // The offset is either the operand or, for the
                    // `*Constant` forms, a Smi in the constant pool.
                    let delta = if bytecode.is_jump_constant() {
                        let index = operand(self, 0);
                        Smi::to_int(
                            bytecode_array
                                .constant_pool(&self.heap)
                                .get(&self.heap, index),
                        )
                    } else {
                        operand(self, 0)
                    };
                    if self.jump_condition_holds(bytecode, accumulator) {
                        // Relative to the opcode byte, not to the prefix.
                        next_offset = opcode_offset + delta;
                    }
                }
                Bytecode::JumpLoop => {
                    let delta = operand(self, 0);
                    next_offset = opcode_offset - delta;
                }

                Bytecode::Return => return Ok(accumulator),
                Bytecode::Throw | Bytecode::ReThrow => return Err(Thrown(accumulator)),

                Bytecode::Wide | Bytecode::ExtraWide => {
                    unreachable!("a prefix should have been consumed above")
                }
                other => {
                    let message = self
                        .new_string(&format!("{other} is not implemented"))
                        .tagged();
                    return Err(Thrown(message));
                }
            }

            offset = next_offset;
        }

        // Control fell off the end. The generator appends an explicit
        // `return undefined`, so this is unreachable in generated code.
        Ok(self.undefined_value())
    }

    /// Whether a conditional jump is taken.
    fn jump_condition_holds(&mut self, bytecode: Bytecode, accumulator: Tagged) -> bool {
        use Bytecode::*;
        match bytecode {
            Jump | JumpConstant => true,
            JumpIfTrue | JumpIfTrueConstant => accumulator == self.true_value(),
            JumpIfFalse | JumpIfFalseConstant => accumulator == self.false_value(),
            JumpIfToBooleanTrue | JumpIfToBooleanTrueConstant => self.to_boolean(accumulator),
            JumpIfToBooleanFalse | JumpIfToBooleanFalseConstant => !self.to_boolean(accumulator),
            JumpIfNull | JumpIfNullConstant => accumulator == self.null_value(),
            JumpIfNotNull | JumpIfNotNullConstant => accumulator != self.null_value(),
            JumpIfUndefined | JumpIfUndefinedConstant => accumulator == self.undefined_value(),
            JumpIfNotUndefined | JumpIfNotUndefinedConstant => {
                accumulator != self.undefined_value()
            }
            JumpIfUndefinedOrNull | JumpIfUndefinedOrNullConstant => {
                accumulator == self.undefined_value() || accumulator == self.null_value()
            }
            other => unreachable!("{other} is not a conditional jump"),
        }
    }

    fn apply_binary_op(&mut self, bytecode: Bytecode, left: Tagged, right: Tagged) -> Tagged {
        use Bytecode::*;
        match bytecode {
            Add => self.add(left, right),
            Sub | SubSmi => self.subtract(left, right),
            Mul | MulSmi => self.multiply(left, right),
            Div | DivSmi => self.divide(left, right),
            Mod | ModSmi => self.modulus(left, right),
            Exp => self.exponentiate(left, right),
            BitwiseOr => self.bitwise_or(left, right),
            BitwiseXor => self.bitwise_xor(left, right),
            BitwiseAnd => self.bitwise_and(left, right),
            ShiftLeft => self.shift_left(left, right),
            ShiftRight => self.shift_right(left, right),
            ShiftRightLogical => self.shift_right_logical(left, right),
            AddSmi => self.add(left, right),
            other => unreachable!("{other} is not a binary operator"),
        }
    }

    fn apply_comparison(&mut self, bytecode: Bytecode, left: Tagged, right: Tagged) -> bool {
        use Bytecode::*;
        match bytecode {
            TestEqual => self.loose_equals(left, right),
            TestEqualStrict => self.strict_equals(left, right),
            TestLessThan => self.less_than(left, right),
            TestGreaterThan => self.greater_than(left, right),
            TestLessThanOrEqual => self.less_than_or_equal(left, right),
            TestGreaterThanOrEqual => self.greater_than_or_equal(left, right),
            TestIn => self.has_property(right, left),
            other => unreachable!("{other} is not a comparison"),
        }
    }

    fn constant_name(&self, bytecode_array: BytecodeArray, index: i32) -> Tagged {
        bytecode_array
            .constant_pool(&self.heap)
            .get(&self.heap, index)
    }

    /// Any value used as a property key becomes an internalized name, which
    /// is what lets the lookup compare it by address.
    fn key_to_name(&mut self, key: Tagged) -> Tagged {
        if let Some(string) = crate::objects::string::String::try_cast(&self.heap, key) {
            if string.is_internalized(&self.heap) {
                return key;
            }
        }
        let text = self.to_string(key);
        self.internalize_string(&text).tagged()
    }

    /// `LoadIC`: the cached path when this site has a feedback vector, and
    /// the plain lookup when it does not.
    fn load_named_property_ic(
        &mut self,
        frame: &InterpreterFrame,
        object: Tagged,
        name: Tagged,
        slot: i32,
    ) -> ExecutionResult {
        let Some(vector) = frame.feedback_vector else {
            return self.load_property(object, name);
        };
        match object.to_strong_heap_object() {
            Some(receiver)
                if crate::objects::instance_type::is_js_receiver(
                    self.heap.instance_type(receiver),
                ) =>
            {
                Ok(self.load_ic(JSObject::unchecked_cast(receiver), name, vector, slot))
            }
            // Not an object: the error path, which no cache helps with.
            _ => self.load_property(object, name),
        }
    }

    /// `StoreIC`
    fn store_named_property_ic(
        &mut self,
        frame: &InterpreterFrame,
        object: Tagged,
        name: Tagged,
        value: Tagged,
        slot: i32,
    ) -> ExecutionResult {
        let Some(vector) = frame.feedback_vector else {
            return self.store_property(object, name, value);
        };
        match object.to_strong_heap_object() {
            Some(receiver)
                if crate::objects::instance_type::is_js_receiver(
                    self.heap.instance_type(receiver),
                ) =>
            {
                self.store_ic(
                    JSObject::unchecked_cast(receiver),
                    name,
                    value,
                    vector,
                    slot,
                );
                Ok(value)
            }
            _ => self.store_property(object, name, value),
        }
    }

    fn load_property(&mut self, object: Tagged, name: Tagged) -> ExecutionResult {
        match object.to_strong_heap_object() {
            Some(receiver)
                if crate::objects::instance_type::is_js_receiver(
                    self.heap.instance_type(receiver),
                ) =>
            {
                Ok(self.get_property(JSObject::unchecked_cast(receiver), name))
            }
            _ => {
                // Reading a property of undefined or null is a TypeError,
                // and the message naming both is why it is built here.
                let object_text = self.to_string(object);
                let name_text = self.to_string(name);
                let message = self
                    .new_string(&format!(
                        "Cannot read property '{name_text}' of {object_text}"
                    ))
                    .tagged();
                Err(Thrown(message))
            }
        }
    }

    fn store_property(&mut self, object: Tagged, name: Tagged, value: Tagged) -> ExecutionResult {
        match object.to_strong_heap_object() {
            Some(receiver)
                if crate::objects::instance_type::is_js_receiver(
                    self.heap.instance_type(receiver),
                ) =>
            {
                self.set_property(JSObject::unchecked_cast(receiver), name, value);
                Ok(value)
            }
            _ => {
                let object_text = self.to_string(object);
                let name_text = self.to_string(name);
                let message = self
                    .new_string(&format!(
                        "Cannot set property '{name_text}' of {object_text}"
                    ))
                    .tagged();
                Err(Thrown(message))
            }
        }
    }

    fn not_a_function_message(&mut self, callee: Tagged) -> Tagged {
        let text = self.to_string(callee);
        self.new_string(&format!("{text} is not a function"))
            .tagged()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a script and returns what the last expression statement produced.
    fn run(source: &str) -> std::result::Result<String, String> {
        let mut isolate = Isolate::new();
        isolate.setup_global_object();
        match isolate.run_script(source) {
            Ok(value) => Ok(isolate.to_string(value)),
            Err(message) => Err(message),
        }
    }

    fn evaluate(source: &str) -> String {
        match run(source) {
            Ok(value) => value,
            Err(message) => panic!("{source:?} failed: {message}"),
        }
    }

    #[test]
    fn arithmetic() {
        assert_eq!(evaluate("1 + 2"), "3");
        assert_eq!(evaluate("2 * 3 + 1"), "7");
        assert_eq!(evaluate("1 + 2 * 3"), "7");
        assert_eq!(evaluate("(1 + 2) * 3"), "9");
        assert_eq!(evaluate("7 / 2"), "3.5");
        assert_eq!(evaluate("7 % 3"), "1");
        assert_eq!(evaluate("2 ** 10"), "1024");
        assert_eq!(evaluate("-5 + 3"), "-2");
    }

    #[test]
    fn string_concatenation_and_coercion() {
        assert_eq!(evaluate("'a' + 'b'"), "ab");
        assert_eq!(evaluate("1 + '1'"), "11");
        assert_eq!(evaluate("'1' + 1"), "11");
        assert_eq!(evaluate("1 - '1'"), "0", "only + concatenates");
    }

    #[test]
    fn comparisons_and_equality() {
        assert_eq!(evaluate("1 < 2"), "true");
        assert_eq!(evaluate("2 <= 2"), "true");
        assert_eq!(evaluate("1 == '1'"), "true");
        assert_eq!(evaluate("1 === '1'"), "false");
        assert_eq!(evaluate("1 != 2"), "true");
        assert_eq!(evaluate("1 !== 1"), "false");
        assert_eq!(evaluate("null == undefined"), "true");
        assert_eq!(evaluate("null === undefined"), "false");
    }

    #[test]
    fn logical_operators_return_an_operand_not_a_boolean() {
        assert_eq!(evaluate("0 || 'fallback'"), "fallback");
        assert_eq!(evaluate("'value' || 'fallback'"), "value");
        assert_eq!(evaluate("1 && 2"), "2");
        assert_eq!(evaluate("0 && 2"), "0");
        assert_eq!(evaluate("null ?? 'default'"), "default");
        assert_eq!(evaluate("0 ?? 'default'"), "0", "?? only catches nullish");
    }

    #[test]
    fn variables_and_assignment() {
        assert_eq!(evaluate("var x = 1; x + 1"), "2");
        assert_eq!(evaluate("var x = 1; x = x + 5; x"), "6");
        assert_eq!(evaluate("var x = 1; x += 5; x"), "6");
        assert_eq!(evaluate("var x = 10; x -= 3; x *= 2; x"), "14");
        assert_eq!(
            evaluate("var x; x"),
            "undefined",
            "a hoisted var is undefined"
        );
    }

    #[test]
    fn increment_and_decrement_differ_in_their_value() {
        assert_eq!(evaluate("var x = 1; ++x"), "2");
        assert_eq!(
            evaluate("var x = 1; x++"),
            "1",
            "postfix yields the old value"
        );
        assert_eq!(evaluate("var x = 1; x++; x"), "2", "but still increments");
        assert_eq!(evaluate("var x = 1; x--; x"), "0");
    }

    #[test]
    fn conditionals() {
        assert_eq!(
            evaluate("var x; if (true) { x = 1; } else { x = 2; } x"),
            "1"
        );
        assert_eq!(
            evaluate("var x; if (false) { x = 1; } else { x = 2; } x"),
            "2"
        );
        assert_eq!(evaluate("var x = 0; if (x) { x = 1; } x"), "0");
        assert_eq!(evaluate("true ? 'yes' : 'no'"), "yes");
        assert_eq!(evaluate("0 ? 'yes' : 'no'"), "no");
    }

    #[test]
    fn short_circuiting_in_a_condition_does_not_evaluate_the_rest() {
        // `a && b` as a *test* never materialises a boolean, and must not
        // evaluate `b` when `a` is falsy.
        assert_eq!(
            evaluate("var ran = 0; function mark() { ran = 1; return true; } if (false && mark()) { } ran"),
            "0"
        );
        assert_eq!(
            evaluate("var ran = 0; function mark() { ran = 1; return true; } if (true || mark()) { } ran"),
            "0"
        );
    }

    #[test]
    fn while_loops() {
        assert_eq!(evaluate("var i = 0; while (i < 5) { i = i + 1; } i"), "5");
        assert_eq!(evaluate("var i = 0; while (false) { i = 1; } i"), "0");
        assert_eq!(
            evaluate("var i = 0; var total = 0; while (i < 4) { total += i; i++; } total"),
            "6"
        );
    }

    #[test]
    fn do_while_runs_the_body_at_least_once() {
        assert_eq!(
            evaluate("var i = 10; do { i = i + 1; } while (i < 5) i"),
            "11"
        );
    }

    #[test]
    fn for_loops() {
        assert_eq!(
            evaluate("var total = 0; for (var i = 0; i < 5; i++) { total += i; } total"),
            "10"
        );
        assert_eq!(
            evaluate("var count = 0; for (;;) { count++; if (count > 3) { break; } } count"),
            "4"
        );
    }

    #[test]
    fn break_and_continue() {
        assert_eq!(
            evaluate("var i = 0; while (true) { i++; if (i > 3) { break; } } i"),
            "4"
        );
        // `continue` must still run the increment, or this never terminates.
        assert_eq!(
            evaluate("var total = 0; for (var i = 0; i < 5; i++) { if (i == 2) { continue; } total += i; } total"),
            "8"
        );
    }

    #[test]
    fn functions_and_calls() {
        assert_eq!(evaluate("function f() { return 1; } f()"), "1");
        assert_eq!(
            evaluate("function add(a, b) { return a + b; } add(2, 3)"),
            "5"
        );
        assert_eq!(
            evaluate("function f() { } f()"),
            "undefined",
            "a function with no return yields undefined"
        );
        // Callable before its definition, because declarations are hoisted.
        assert_eq!(evaluate("var r = f(); function f() { return 7; } r"), "7");
        // Missing arguments are undefined, extra ones dropped.
        assert_eq!(evaluate("function f(a, b) { return b; } f(1)"), "undefined");
        assert_eq!(evaluate("function f(a) { return a; } f(1, 2)"), "1");
    }

    #[test]
    fn recursion() {
        assert_eq!(
            evaluate(
                "function fact(n) { if (n <= 1) { return 1; } return n * fact(n - 1); } fact(10)"
            ),
            "3628800"
        );
        assert_eq!(
            evaluate("function fib(n) { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); } fib(15)"),
            "610"
        );
    }

    #[test]
    fn locals_do_not_leak_between_calls() {
        assert_eq!(
            evaluate("function counter() { var n = 0; n = n + 1; return n; } counter(); counter()"),
            "1",
            "each call gets a fresh frame"
        );
    }

    #[test]
    fn objects_and_properties() {
        assert_eq!(evaluate("var o = {}; o.x = 1; o.x"), "1");
        assert_eq!(evaluate("var o = {x: 1, y: 2}; o.x + o.y"), "3");
        assert_eq!(evaluate("var o = {}; o.missing"), "undefined");
        assert_eq!(evaluate("var o = {x: 1}; o['x']"), "1");
        assert_eq!(evaluate("var o = {}; var k = 'key'; o[k] = 5; o.key"), "5");
        assert_eq!(evaluate("var o = {a: {b: 7}}; o.a.b"), "7");
    }

    #[test]
    fn methods_receive_their_object_as_the_receiver() {
        assert_eq!(
            evaluate("var o = {n: 42}; o.get = function () { return this.n; }; o.get()"),
            "42"
        );
    }

    #[test]
    fn typeof_reports_the_type() {
        assert_eq!(evaluate("typeof 1"), "number");
        assert_eq!(evaluate("typeof 'a'"), "string");
        assert_eq!(evaluate("typeof true"), "boolean");
        assert_eq!(evaluate("typeof undefined"), "undefined");
        assert_eq!(evaluate("typeof null"), "object");
        assert_eq!(evaluate("typeof {}"), "object");
        assert_eq!(evaluate("function f() { } typeof f"), "function");
    }

    #[test]
    fn errors_are_thrown_not_silently_wrong() {
        assert!(run("undeclared").unwrap_err().contains("is not defined"));
        assert!(run("var x = 1; x()")
            .unwrap_err()
            .contains("is not a function"));
        assert!(run("var o; o.x")
            .unwrap_err()
            .contains("Cannot read property"));
        assert!(run("function f() { return f(); } f()")
            .unwrap_err()
            .contains("call stack"));
    }

    #[test]
    fn builtins_are_callable() {
        assert_eq!(evaluate("Math.sqrt(16)"), "4");
        assert_eq!(evaluate("Math.floor(1.7)"), "1");
        assert_eq!(evaluate("Math.abs(-3)"), "3");
        assert_eq!(evaluate("Math.max(1, 5, 3)"), "5");
        assert_eq!(evaluate("Math.pow(2, 8)"), "256");
    }

    #[test]
    fn a_program_that_exercises_the_whole_pipeline() {
        let source = "
            function factorial(n) {
              if (n <= 1) { return 1; }
              return n * factorial(n - 1);
            }
            var total = 0;
            for (var i = 1; i <= 6; i++) {
              total += factorial(i);
            }
            var report = { total: total, label: 'factorials' };
            report.label + ': ' + report.total
        ";
        assert_eq!(evaluate(source), "factorials: 873");
    }
}

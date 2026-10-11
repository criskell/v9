use crate::builtins::builtins::Builtin;
use crate::common::globals::StoreOrigin;
use crate::execution::isolate::Isolate;
use crate::interpreter::bytecode_operands::OperandScale;
use crate::interpreter::bytecode_register::Register;
use crate::interpreter::bytecodes::Bytecode;
use crate::objects::bytecode_array::BytecodeArray;
use crate::objects::js_function::JSFunction;
use crate::objects::js_objects::JSObject;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

#[derive(Copy, Clone, Debug)]
pub struct Thrown(pub Tagged);

pub type ExecutionResult = Result<Tagged, Thrown>;

const K_MAX_CALL_DEPTH: usize = 180;

const K_FIXED_SLOT_COUNT: usize = 5;

struct InterpreterFrame {
    bytecode: BytecodeArray,
    feedback_vector: Option<crate::objects::feedback_vector::FeedbackVector>,
    params_base: usize,
    params_count: usize,
    fixed_base: usize,
    registers_base: usize,
}

impl Isolate {
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

        let params_base = self.stack.len();
        self.stack.push(receiver);
        let undefined = self.undefined_value();
        for index in 0..params_count.saturating_sub(1) {
            self.stack
                .push(args.get(index).copied().unwrap_or(undefined));
        }
        let fixed_base = self.stack.len();
        for _ in 0..K_FIXED_SLOT_COUNT {
            self.stack.push(undefined);
        }

        let registers_base = self.stack.len();
        for _ in 0..register_count {
            self.stack.push(undefined);
        }

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
            fixed_base,
            registers_base,
        };

        let context = function.context(&self.heap);
        self.write_register(&frame, Register::current_context(), context);
        self.write_register(&frame, Register::function_closure(), function.tagged());

        self.call_depth += 1;
        let result = self.dispatch(&frame);
        self.call_depth -= 1;

        self.stack.truncate(params_base);
        result
    }

    #[inline]
    fn register_slot(&self, frame: &InterpreterFrame, register: Register) -> usize {
        let index = register.index();
        if index >= 0 {
            return frame.registers_base + index as usize;
        }
        let parameter = register.to_parameter_index();
        if parameter >= 0 {
            debug_assert!(
                (parameter as usize) < frame.params_count,
                "parameter {parameter} out of range"
            );
            return frame.params_base + parameter as usize;
        }
        let fixed = (-index - 1) as usize;
        debug_assert!(fixed < K_FIXED_SLOT_COUNT, "unknown fixed slot {index}");
        frame.fixed_base + fixed
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

    fn dispatch(&mut self, frame: &InterpreterFrame) -> ExecutionResult {
        let bytecode_array = frame.bytecode;
        let length = bytecode_array.length(&self.heap);
        let mut accumulator = self.undefined_value();
        let mut offset: i32 = 0;

        while offset < length {
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

                // -- contexts ---------------------------------------------
                Bytecode::CreateFunctionContext => {
                    let index = operand(self, 0);
                    let slots = operand(self, 1);
                    let scope_info = bytecode_array
                        .constant_pool(&self.heap)
                        .get(&self.heap, index);
                    let previous = self.read_register(frame, Register::current_context());
                    let context = self.new_function_context(slots, previous);
                    context.set_scope_info(&mut self.heap, scope_info);
                    accumulator = context.tagged();
                }
                Bytecode::PushContext => {
                    let register = register_operand(self, 0);
                    let current = self.read_register(frame, Register::current_context());
                    self.write_register(frame, register, current);
                    self.write_register(frame, Register::current_context(), accumulator);
                }
                Bytecode::PopContext => {
                    let register = register_operand(self, 0);
                    let saved = self.read_register(frame, register);
                    self.write_register(frame, Register::current_context(), saved);
                }
                Bytecode::LdaCurrentContextSlot => {
                    let slot = operand(self, 0);
                    accumulator = self.context_slot(frame, Register::current_context(), 0, slot)?;
                }
                Bytecode::LdaContextSlot => {
                    let base = register_operand(self, 0);
                    let slot = operand(self, 1);
                    let depth = operand(self, 2);
                    accumulator = self.context_slot(frame, base, depth, slot)?;
                }
                Bytecode::StaCurrentContextSlot => {
                    let slot = operand(self, 0);
                    self.set_context_slot(
                        frame,
                        Register::current_context(),
                        0,
                        slot,
                        accumulator,
                    )?;
                }
                Bytecode::StaContextSlot => {
                    let base = register_operand(self, 0);
                    let slot = operand(self, 1);
                    let depth = operand(self, 2);
                    self.set_context_slot(frame, base, depth, slot, accumulator)?;
                }

                // -- globals ----------------------------------------------
                Bytecode::LdaGlobal => {
                    let name = self.constant_name(bytecode_array, operand(self, 0));
                    let slot = operand(self, 1);
                    let value = match frame.feedback_vector {
                        Some(vector) => self.load_global_ic(name, vector, slot),
                        None => self.load_global(name),
                    };
                    accumulator = match value {
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
                    let slot = operand(self, 1);
                    match frame.feedback_vector {
                        Some(vector) => self.store_global_ic(name, accumulator, vector, slot),
                        None => self.store_global(name, accumulator),
                    }
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
                    accumulator = self.get_keyed_property(object, accumulator)?;
                }
                Bytecode::SetNamedProperty | Bytecode::DefineNamedOwnProperty => {
                    let object = self.read_register(frame, register_operand(self, 0));
                    let name = self.constant_name(bytecode_array, operand(self, 1));
                    let slot = operand(self, 2);
                    self.store_named_property_ic(frame, object, name, accumulator, slot)?;
                    accumulator = self.undefined_value();
                }
                Bytecode::SetKeyedProperty => {
                    let object = self.read_register(frame, register_operand(self, 0));
                    let key = self.read_register(frame, register_operand(self, 1));
                    self.set_keyed_property(object, key, accumulator)?;
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
                    let left = self.read_register(frame, register_operand(self, 0));
                    accumulator = self.apply_binary_op(bytecode, left, accumulator);
                }
                Bytecode::AddSmi
                | Bytecode::SubSmi
                | Bytecode::MulSmi
                | Bytecode::DivSmi
                | Bytecode::ModSmi => {
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
                    accumulator = self.boolean_value(accumulator != self.true_value());
                }
                Bytecode::TypeOf => accumulator = self.type_of(accumulator),

                Bytecode::DeletePropertyStrict | Bytecode::DeletePropertySloppy => {
                    let object = self.read_register(frame, register_operand(self, 0));
                    let key = self.key_to_name(accumulator);
                    let deleted = self.delete_property_or_element(object, key)?;
                    accumulator = self.boolean_value(deleted);
                }

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
                    accumulator = self.new_js_array(0).tagged();
                }
                Bytecode::CreateClosure => {
                    let index = operand(self, 0);
                    let shared_index = bytecode_array
                        .constant_pool(&self.heap)
                        .get(&self.heap, index);
                    let context = self.read_register(frame, Register::current_context());
                    accumulator =
                        self.closure_for_with_context(Smi::to_int(shared_index) as usize, context);
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

        Ok(self.undefined_value())
    }

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

    fn context_slot(
        &mut self,
        frame: &InterpreterFrame,
        base: Register,
        depth: i32,
        slot: i32,
    ) -> ExecutionResult {
        let context = self.context_at(frame, base, depth)?;
        Ok(context.get(&self.heap, slot))
    }

    fn set_context_slot(
        &mut self,
        frame: &InterpreterFrame,
        base: Register,
        depth: i32,
        slot: i32,
        value: Tagged,
    ) -> ExecutionResult {
        let context = self.context_at(frame, base, depth)?;
        context.set(&mut self.heap, slot, value);
        Ok(value)
    }

    fn context_at(
        &mut self,
        frame: &InterpreterFrame,
        base: Register,
        depth: i32,
    ) -> Result<crate::objects::contexts::Context, Thrown> {
        use crate::objects::contexts::Context;
        let value = self.read_register(frame, base);
        match value.to_strong_heap_object() {
            Some(object)
                if self.heap.instance_type(object)
                    == crate::objects::instance_type::CONTEXT_TYPE =>
            {
                Ok(Context::unchecked_cast(object).at_depth(&self.heap, depth))
            }
            _ => {
                let message = self
                    .new_string("internal: no context in this frame")
                    .tagged();
                Err(Thrown(message))
            }
        }
    }

    fn constant_name(&self, bytecode_array: BytecodeArray, index: i32) -> Tagged {
        bytecode_array
            .constant_pool(&self.heap)
            .get(&self.heap, index)
    }

    fn key_to_name(&mut self, key: Tagged) -> Tagged {
        if let Some(string) = crate::objects::string::String::try_cast(&self.heap, key) {
            if string.is_internalized(&self.heap) {
                return key;
            }
        }
        let text = self.to_string(key);
        self.internalize_string(&text).tagged()
    }

    /// `KeyedLoadIC::Load`
    fn get_keyed_property(&mut self, object: Tagged, key: Tagged) -> ExecutionResult {
        if let Some(index) = self.to_array_index(key) {
            if let Some(value) = self.load_element(object, index)? {
                return Ok(value);
            }
        }
        let name = self.key_to_name(key);
        self.load_property(object, name)
    }

    /// `KeyedStoreIC::Store`
    fn set_keyed_property(
        &mut self,
        object: Tagged,
        key: Tagged,
        value: Tagged,
    ) -> ExecutionResult {
        if let Some(index) = self.to_array_index(key) {
            if self.store_element(object, index, value)? {
                return Ok(value);
            }
        }
        let name = self.key_to_name(key);
        self.store_property_with_origin(object, name, value, StoreOrigin::MaybeKeyed)
    }

    /// `JSObject::GetElement`
    fn load_element(&mut self, object: Tagged, index: u32) -> Result<Option<Tagged>, Thrown> {
        use crate::objects::elements::ElementsAccessor;
        let Some(receiver) = object.to_strong_heap_object() else {
            return Ok(None);
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(receiver)) {
            return Ok(None);
        }
        let holder = JSObject::unchecked_cast(receiver);
        let kind = holder.get_elements_kind(&self.heap);
        let accessor = ElementsAccessor::for_kind(kind);
        match accessor.get(self, holder, index) {
            Some(value) => Ok(Some(value)),
            None if self.is_js_array(holder) => Ok(Some(self.undefined_value())),
            None => Ok(None),
        }
    }

    /// `JSObject::SetElement`
    fn store_element(&mut self, object: Tagged, index: u32, value: Tagged) -> Result<bool, Thrown> {
        let Some(receiver) = object.to_strong_heap_object() else {
            return Ok(false);
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(receiver)) {
            return Ok(false);
        }
        let holder = JSObject::unchecked_cast(receiver);
        if !self.is_js_array(holder) {
            return Ok(false);
        }
        self.add_data_element(holder, index, value);
        Ok(true)
    }

    /// `JSReceiver::DeletePropertyOrElement`
    fn delete_property_or_element(&mut self, object: Tagged, name: Tagged) -> Result<bool, Thrown> {
        let Some(receiver) = object.to_strong_heap_object() else {
            return Ok(true);
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(receiver)) {
            return Ok(true);
        }
        let holder = JSObject::unchecked_cast(receiver);
        if let Some(index) = self.string_as_array_index(name) {
            return Ok(self.delete_element(holder, index));
        }
        Ok(self.delete_property(holder, name))
    }

    fn delete_element(&mut self, holder: JSObject, index: u32) -> bool {
        use crate::objects::elements::ElementsAccessor;
        let kind = holder.get_elements_kind(&self.heap);
        let accessor = ElementsAccessor::for_kind(kind);
        if !accessor.has_element(&self.heap, holder, index) {
            return true;
        }
        self.delete_element_entry(holder, index);
        true
    }

    fn is_js_array(&self, object: JSObject) -> bool {
        self.heap.instance_type(object.object()) == crate::objects::instance_type::JS_ARRAY_TYPE
    }

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
        self.store_property_with_origin(object, name, value, StoreOrigin::Named)
    }

    fn store_property_with_origin(
        &mut self,
        object: Tagged,
        name: Tagged,
        value: Tagged,
        store_origin: StoreOrigin,
    ) -> ExecutionResult {
        match object.to_strong_heap_object() {
            Some(receiver)
                if crate::objects::instance_type::is_js_receiver(
                    self.heap.instance_type(receiver),
                ) =>
            {
                self.set_property_with_origin(
                    JSObject::unchecked_cast(receiver),
                    name,
                    value,
                    store_origin,
                );
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
        assert_eq!(evaluate("var r = f(); function f() { return 7; } r"), "7");
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

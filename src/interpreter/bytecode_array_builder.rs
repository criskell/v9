use crate::ast::ast::Literal;
use crate::interpreter::bytecode_array_writer::BytecodeArrayWriter;
use crate::interpreter::bytecode_label::{BytecodeLabel, BytecodeLabels, BytecodeLoopHeader};
use crate::interpreter::bytecode_node::BytecodeNode;
use crate::interpreter::bytecode_register::{Register, RegisterList};
use crate::interpreter::bytecode_register_allocator::BytecodeRegisterAllocator;
use crate::interpreter::bytecodes::Bytecode;
use crate::interpreter::constant_array_builder::ConstantEntry;
use crate::objects::feedback_vector::{FeedbackSlotKind, FeedbackVectorSpec};
use crate::parsing::token::Token;

pub const K_UNINITIALIZED_EMBEDDED_FEEDBACK: i32 = 0;

pub const K_FEEDBACK_IS_EMBEDDED: i32 = -1;

#[derive(Clone, Debug)]
pub struct BytecodeArrayData {
    pub bytecodes: Vec<u8>,
    pub constants: Vec<ConstantEntry>,
    pub register_count: i32,
    pub parameter_count: i32,
    pub feedback_slot_count: i32,
    pub feedback_slot_kinds: Vec<FeedbackSlotKind>,
    pub source_position_table: Vec<(usize, usize)>,
    pub handler_table: crate::codegen::handler_table::HandlerTable,
}

pub struct BytecodeArrayBuilder {
    bytecode_array_writer: BytecodeArrayWriter,
    register_allocator: BytecodeRegisterAllocator,
    parameter_count: i32,
    local_register_count: i32,
    feedback_vector_spec: FeedbackVectorSpec,
    pending_source_position: Option<usize>,
    handler_table_builder: crate::interpreter::handler_table_builder::HandlerTableBuilder,
}

impl BytecodeArrayBuilder {
    pub fn new(parameter_count: i32, local_register_count: i32) -> BytecodeArrayBuilder {
        BytecodeArrayBuilder {
            bytecode_array_writer: BytecodeArrayWriter::new(),
            register_allocator: BytecodeRegisterAllocator::new(local_register_count),
            parameter_count,
            local_register_count,
            feedback_vector_spec: FeedbackVectorSpec::new(),
            pending_source_position: None,
            handler_table_builder:
                crate::interpreter::handler_table_builder::HandlerTableBuilder::new(),
        }
    }


    #[inline]
    pub fn parameter_count(&self) -> i32 {
        self.parameter_count
    }

    #[inline]
    pub fn parameter(&self, index: i32) -> Register {
        debug_assert!(index < self.parameter_count - 1);
        Register::from_parameter_index(index + 1)
    }

    #[inline]
    pub fn receiver(&self) -> Register {
        Register::receiver()
    }

    #[inline]
    pub fn local(&self, index: i32) -> Register {
        debug_assert!(index < self.local_register_count);
        Register::new(index)
    }

    #[inline]
    pub fn register_allocator(&mut self) -> &mut BytecodeRegisterAllocator {
        &mut self.register_allocator
    }

    pub fn allocate_feedback_slot(&mut self, kind: FeedbackSlotKind) -> i32 {
        self.feedback_vector_spec.add_slot(kind)
    }

    pub fn feedback_vector_spec(&self) -> &FeedbackVectorSpec {
        &self.feedback_vector_spec
    }

    pub fn set_source_position(&mut self, position: usize) -> &mut Self {
        self.pending_source_position = Some(position);
        self
    }

    pub fn offset(&self) -> usize {
        self.bytecode_array_writer.offset()
    }


    fn output(&mut self, bytecode: Bytecode, operands: &[i32]) -> &mut Self {
        let mut node = BytecodeNode::new(bytecode, operands);
        if let Some(position) = self.pending_source_position.take() {
            node = node.with_source_position(position);
        }
        self.bytecode_array_writer.write(&node);
        self
    }

    pub fn load_literal_smi(&mut self, value: i32) -> &mut Self {
        if value == 0 {
            self.output(Bytecode::LdaZero, &[])
        } else {
            self.output(Bytecode::LdaSmi, &[value])
        }
    }

    pub fn load_literal_number(&mut self, value: f64) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_number(value) as i32;
        self.output(Bytecode::LdaConstant, &[index])
    }

    pub fn load_literal_string(&mut self, value: &str) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_string(value) as i32;
        self.output(Bytecode::LdaConstant, &[index])
    }

    pub fn load_class_boilerplate(&mut self, members: &[(i32, Option<String>)]) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_class_boilerplate(members) as i32;
        self.output(Bytecode::LdaConstant, &[index])
    }

    pub fn load_literal_name(&mut self, value: &str) -> &mut Self {
        let index = self.insert_name(value);
        self.output(Bytecode::LdaConstant, &[index])
    }

    pub fn load_undefined(&mut self) -> &mut Self {
        self.output(Bytecode::LdaUndefined, &[])
    }

    pub fn load_null(&mut self) -> &mut Self {
        self.output(Bytecode::LdaNull, &[])
    }

    pub fn load_the_hole(&mut self) -> &mut Self {
        self.output(Bytecode::LdaTheHole, &[])
    }

    pub fn load_true(&mut self) -> &mut Self {
        self.output(Bytecode::LdaTrue, &[])
    }

    pub fn load_false(&mut self) -> &mut Self {
        self.output(Bytecode::LdaFalse, &[])
    }

    pub fn load_literal(&mut self, literal: &Literal) -> &mut Self {
        match literal {
            Literal::Smi(value) => self.load_literal_smi(*value),
            Literal::Number(value) => self.load_literal_number(*value),
            Literal::String(value) => self.load_literal_string(value),
            Literal::Boolean(true) => self.load_true(),
            Literal::Boolean(false) => self.load_false(),
            Literal::Null => self.load_null(),
            Literal::Undefined => self.load_undefined(),
        }
    }

    pub fn load_accumulator_with_register(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::Ldar, &[register.to_operand()])
    }

    pub fn store_accumulator_in_register(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::Star, &[register.to_operand()])
    }

    pub fn move_register(&mut self, from: Register, to: Register) -> &mut Self {
        debug_assert_ne!(from, to);
        self.output(Bytecode::Mov, &[from.to_operand(), to.to_operand()])
    }


    pub fn create_function_context(&mut self, names: &[String], slots: i32) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_scope_info(names) as i32;
        self.output(Bytecode::CreateFunctionContext, &[index, slots])
    }

    pub fn push_context(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::PushContext, &[register.to_operand()])
    }

    pub fn pop_context(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::PopContext, &[register.to_operand()])
    }

    pub fn load_current_context_slot(&mut self, slot: i32) -> &mut Self {
        self.output(Bytecode::LdaCurrentContextSlot, &[slot])
    }

    pub fn load_context_slot(&mut self, slot: i32, depth: i32) -> &mut Self {
        self.output(
            Bytecode::LdaContextSlot,
            &[Register::current_context().to_operand(), slot, depth],
        )
    }

    pub fn store_current_context_slot(&mut self, slot: i32) -> &mut Self {
        self.output(Bytecode::StaCurrentContextSlot, &[slot])
    }

    pub fn store_context_slot(&mut self, slot: i32, depth: i32) -> &mut Self {
        self.output(
            Bytecode::StaContextSlot,
            &[Register::current_context().to_operand(), slot, depth],
        )
    }


    pub fn load_global(&mut self, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadGlobalNotInsideTypeof);
        self.output(Bytecode::LdaGlobal, &[index, slot])
    }

    pub fn store_global(&mut self, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::StoreGlobalStrict);
        self.output(Bytecode::StaGlobal, &[index, slot])
    }


    pub fn store_class_fields_initializer(&mut self, constructor: Register) -> &mut Self {
        let index = self.class_fields_symbol_constant_pool_entry();
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::DefineNamedOwn);
        self.output(
            Bytecode::DefineNamedOwnProperty,
            &[constructor.to_operand(), index, slot],
        )
    }

    pub fn load_class_fields_initializer(&mut self, constructor: Register) -> &mut Self {
        let index = self.class_fields_symbol_constant_pool_entry();
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadProperty);
        self.output(
            Bytecode::GetNamedProperty,
            &[constructor.to_operand(), index, slot],
        )
    }

    fn class_fields_symbol_constant_pool_entry(&mut self) -> i32 {
        self.bytecode_array_writer
            .constant_array_builder_mut()
            .insert_class_fields_symbol() as i32
    }

    pub fn load_named_property_from_super(&mut self, receiver: Register, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadProperty);
        self.output(
            Bytecode::GetNamedPropertyFromSuper,
            &[receiver.to_operand(), index, slot],
        )
    }

    pub fn load_named_property(&mut self, object: Register, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadProperty);
        self.output(
            Bytecode::GetNamedProperty,
            &[object.to_operand(), index, slot],
        )
    }

    pub fn load_keyed_property(&mut self, object: Register) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadKeyed);
        self.output(Bytecode::GetKeyedProperty, &[object.to_operand(), slot])
    }

    pub fn set_named_property(&mut self, object: Register, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::SetNamedStrict);
        self.output(
            Bytecode::SetNamedProperty,
            &[object.to_operand(), index, slot],
        )
    }

    pub fn define_named_own_property(&mut self, object: Register, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::DefineNamedOwn);
        self.output(
            Bytecode::DefineNamedOwnProperty,
            &[object.to_operand(), index, slot],
        )
    }

    pub fn define_keyed_own_property(&mut self, object: Register, key: Register) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::DefineKeyedOwn);
        self.output(
            Bytecode::DefineKeyedOwnProperty,
            &[object.to_operand(), key.to_operand(), 0, slot],
        )
    }

    pub fn set_keyed_property(&mut self, object: Register, key: Register) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::SetKeyedStrict);
        self.output(
            Bytecode::SetKeyedProperty,
            &[object.to_operand(), key.to_operand(), slot],
        )
    }


    pub fn binary_operation(&mut self, op: Token, register: Register) -> &mut Self {
        let bytecode = match op {
            Token::Add => Bytecode::Add,
            Token::Sub => Bytecode::Sub,
            Token::Mul => Bytecode::Mul,
            Token::Div => Bytecode::Div,
            Token::Mod => Bytecode::Mod,
            Token::Exp => Bytecode::Exp,
            Token::BitOr => Bytecode::BitwiseOr,
            Token::BitXor => Bytecode::BitwiseXor,
            Token::BitAnd => Bytecode::BitwiseAnd,
            Token::Shl => Bytecode::ShiftLeft,
            Token::Sar => Bytecode::ShiftRight,
            Token::Shr => Bytecode::ShiftRightLogical,
            other => unreachable!("{other} is not a binary operator"),
        };
        self.output(
            bytecode,
            &[register.to_operand(), K_UNINITIALIZED_EMBEDDED_FEEDBACK],
        )
    }

    pub fn binary_operation_smi_literal(&mut self, op: Token, literal: i32) -> &mut Self {
        let bytecode = match op {
            Token::Add => Bytecode::AddSmi,
            Token::Sub => Bytecode::SubSmi,
            Token::Mul => Bytecode::MulSmi,
            Token::Div => Bytecode::DivSmi,
            Token::Mod => Bytecode::ModSmi,
            other => unreachable!("{other} has no Smi-literal form"),
        };
        self.output(bytecode, &[literal, K_UNINITIALIZED_EMBEDDED_FEEDBACK])
    }

    pub fn unary_operation(&mut self, op: Token) -> &mut Self {
        match op {
            Token::Add => {
                let slot = self.allocate_feedback_slot(FeedbackSlotKind::BinaryOp);
                self.output(Bytecode::ToNumber, &[slot])
            }
            Token::Sub => self.output(Bytecode::Negate, &[K_UNINITIALIZED_EMBEDDED_FEEDBACK]),
            Token::BitNot => {
                self.output(Bytecode::BitwiseNot, &[K_UNINITIALIZED_EMBEDDED_FEEDBACK])
            }
            other => unreachable!("{other} is not a unary operator"),
        }
    }

    pub fn increment(&mut self) -> &mut Self {
        self.output(Bytecode::Inc, &[K_UNINITIALIZED_EMBEDDED_FEEDBACK])
    }

    pub fn decrement(&mut self) -> &mut Self {
        self.output(Bytecode::Dec, &[K_UNINITIALIZED_EMBEDDED_FEEDBACK])
    }

    pub fn logical_not(&mut self) -> &mut Self {
        self.output(Bytecode::LogicalNot, &[])
    }

    pub fn to_boolean_logical_not(&mut self) -> &mut Self {
        self.output(Bytecode::ToBooleanLogicalNot, &[])
    }

    pub fn type_of(&mut self) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::TypeOf);
        self.output(Bytecode::TypeOf, &[slot])
    }

    pub fn construct(&mut self, constructor: Register, args: RegisterList) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Call);
        self.output(
            Bytecode::Construct,
            &[
                constructor.to_operand(),
                args.first_register().to_operand(),
                args.register_count(),
                slot,
            ],
        )
    }

    pub fn for_in_enumerate(&mut self, receiver: Register) -> &mut Self {
        self.output(Bytecode::ForInEnumerate, &[receiver.to_operand()])
    }

    pub fn for_in_prepare(&mut self, cache_info_triple: RegisterList) -> &mut Self {
        debug_assert_eq!(cache_info_triple.register_count(), 3);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::ForIn);
        self.output(
            Bytecode::ForInPrepare,
            &[cache_info_triple.first_register().to_operand(), slot],
        )
    }

    pub fn for_in_next(
        &mut self,
        receiver: Register,
        index: Register,
        cache_type_array_pair: RegisterList,
    ) -> &mut Self {
        debug_assert_eq!(cache_type_array_pair.register_count(), 2);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::ForIn);
        self.output(
            Bytecode::ForInNext,
            &[
                receiver.to_operand(),
                index.to_operand(),
                cache_type_array_pair.first_register().to_operand(),
                slot,
            ],
        )
    }

    pub fn for_in_step(&mut self, index: Register) -> &mut Self {
        self.output(Bytecode::ForInStep, &[index.to_operand()])
    }

    pub fn for_in_done(&mut self, index: Register, cache_length: Register) -> &mut Self {
        self.load_accumulator_with_register(index);
        self.compare_operation(Token::EqStrict, cache_length)
    }

    pub fn delete(&mut self, object: Register, strict: bool) -> &mut Self {
        let bytecode = if strict {
            Bytecode::DeletePropertyStrict
        } else {
            Bytecode::DeletePropertySloppy
        };
        self.output(bytecode, &[object.to_operand()])
    }

    pub fn compare_operation(&mut self, op: Token, register: Register) -> &mut Self {
        let bytecode = match op {
            Token::Eq => Bytecode::TestEqual,
            Token::EqStrict => Bytecode::TestEqualStrict,
            Token::LessThan => Bytecode::TestLessThan,
            Token::GreaterThan => Bytecode::TestGreaterThan,
            Token::LessThanEq => Bytecode::TestLessThanOrEqual,
            Token::GreaterThanEq => Bytecode::TestGreaterThanOrEqual,
            Token::InstanceOf => Bytecode::TestInstanceOf,
            Token::In => Bytecode::TestIn,
            other => unreachable!("{other} has no comparison bytecode"),
        };
        let operands = if matches!(op, Token::InstanceOf | Token::In) {
            let slot = self.allocate_feedback_slot(FeedbackSlotKind::InstanceOf);
            [register.to_operand(), slot]
        } else {
            [register.to_operand(), K_UNINITIALIZED_EMBEDDED_FEEDBACK]
        };
        self.output(bytecode, &operands)
    }

    pub fn compare_null(&mut self) -> &mut Self {
        self.output(Bytecode::TestNull, &[])
    }

    pub fn compare_undefined(&mut self) -> &mut Self {
        self.output(Bytecode::TestUndefined, &[])
    }

    pub fn compare_undetectable(&mut self) -> &mut Self {
        self.output(Bytecode::TestUndetectable, &[])
    }

    pub fn compare_reference_equal(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::TestReferenceEqual, &[register.to_operand()])
    }


    pub fn to_boolean(&mut self) -> &mut Self {
        self.output(Bytecode::ToBoolean, &[])
    }

    pub fn to_name(&mut self) -> &mut Self {
        self.output(Bytecode::ToName, &[])
    }

    pub fn to_string(&mut self) -> &mut Self {
        self.output(Bytecode::ToString, &[])
    }

    pub fn to_number(&mut self) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::BinaryOp);
        self.output(Bytecode::ToNumber, &[slot])
    }


    pub fn create_empty_object_literal(&mut self) -> &mut Self {
        self.output(Bytecode::CreateEmptyObjectLiteral, &[])
    }

    pub fn create_empty_array_literal(&mut self) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Literal);
        self.output(Bytecode::CreateEmptyArrayLiteral, &[slot])
    }

    pub fn create_closure(&mut self, function_index: usize) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_function_literal(function_index) as i32;
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Literal);
        self.output(Bytecode::CreateClosure, &[index, slot, 0])
    }


    pub fn call_undefined_receiver(&mut self, callee: Register, args: RegisterList) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Call);
        self.output(
            Bytecode::CallUndefinedReceiver,
            &[
                callee.to_operand(),
                args.first_register().to_operand(),
                args.register_count(),
                slot,
            ],
        )
    }

    pub fn call_runtime(
        &mut self,
        function_id: crate::runtime::runtime::FunctionId,
        args: RegisterList,
    ) -> &mut Self {
        debug_assert!(
            function_id.function().nargs == -1
                || function_id.function().nargs == args.register_count()
        );
        self.output(
            Bytecode::CallRuntime,
            &[
                function_id.to_u16() as i32,
                args.first_register().to_operand(),
                args.register_count(),
            ],
        )
    }

    pub fn construct_forward_all_args(&mut self, constructor: Register) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Call);
        self.output(
            Bytecode::ConstructForwardAllArgs,
            &[constructor.to_operand(), slot],
        )
    }

    pub fn get_super_constructor(&mut self, out: Register) -> &mut Self {
        self.output(Bytecode::GetSuperConstructor, &[out.to_operand()])
    }

    pub fn call_property(&mut self, callee: Register, args: RegisterList) -> &mut Self {
        debug_assert!(args.register_count() >= 1);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Call);
        self.output(
            Bytecode::CallProperty,
            &[
                callee.to_operand(),
                args.first_register().to_operand(),
                args.register_count(),
                slot,
            ],
        )
    }


    pub fn jump(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::Jump, label)
    }

    pub fn jump_if_true(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfTrue, label)
    }

    pub fn jump_if_false(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfFalse, label)
    }

    pub fn jump_if_to_boolean_true(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfToBooleanTrue, label)
    }

    pub fn jump_if_to_boolean_false(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfToBooleanFalse, label)
    }

    pub fn jump_if_null(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfNull, label)
    }

    pub fn jump_if_undefined(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfUndefined, label)
    }

    pub fn jump_if_undefined_or_null(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfUndefinedOrNull, label)
    }

    fn output_jump(&mut self, bytecode: Bytecode, label: &mut BytecodeLabel) -> &mut Self {
        let mut node = BytecodeNode::new(bytecode, &[0]);
        if let Some(position) = self.pending_source_position.take() {
            node = node.with_source_position(position);
        }
        self.bytecode_array_writer.write_jump(&node, label);
        self
    }

    pub fn jump_loop(&mut self, loop_header: &BytecodeLoopHeader) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::JumpLoop);
        let node = BytecodeNode::new(Bytecode::JumpLoop, &[0, 0, slot]);
        self.bytecode_array_writer
            .write_jump_loop(&node, loop_header);
        self
    }

    pub fn bind(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.bytecode_array_writer.bind_label(label);
        self
    }

    pub fn bind_labels(&mut self, labels: &mut BytecodeLabels) -> &mut Self {
        self.bytecode_array_writer.bind_labels(labels);
        self
    }

    pub fn bind_loop_header(&mut self, loop_header: &mut BytecodeLoopHeader) -> &mut Self {
        self.bytecode_array_writer.bind_loop_header(loop_header);
        self
    }

    pub fn emit_return(&mut self) -> &mut Self {
        self.output(Bytecode::Return, &[])
    }

    pub fn emit_rethrow(&mut self) -> &mut Self {
        self.output(Bytecode::ReThrow, &[])
    }

    pub fn emit_throw(&mut self) -> &mut Self {
        self.output(Bytecode::Throw, &[])
    }

    #[cfg(test)]
    pub fn start_basic_block_for_test(&mut self) {
        self.bytecode_array_writer.start_basic_block();
    }

    #[inline]
    pub fn exit_seen_in_block(&self) -> bool {
        self.bytecode_array_writer.exit_seen_in_block()
    }


    fn insert_name(&mut self, name: &str) -> i32 {
        self.bytecode_array_writer
            .constant_array_builder_mut()
            .insert_name(name) as i32
    }

    pub fn to_bytecode_array(mut self) -> BytecodeArrayData {
        if !self.exit_seen_in_block() {
            self.load_undefined();
            self.emit_return();
        }
        let register_count = self.register_allocator.maximum_register_count();
        let parameter_count = self.parameter_count;
        let feedback_slot_count = self.feedback_vector_spec.slot_count();
        let feedback_slot_kinds = self.feedback_vector_spec.to_slot_kinds();
        let (bytecodes, constant_array_builder, source_position_table) =
            self.bytecode_array_writer.finish();
        BytecodeArrayData {
            bytecodes,
            constants: constant_array_builder.entries(),
            register_count,
            parameter_count,
            feedback_slot_count,
            feedback_slot_kinds,
            source_position_table,
            handler_table: self.handler_table_builder.to_handler_table(),
        }
    }

    pub fn new_handler_entry(&mut self) -> i32 {
        self.handler_table_builder.new_handler_entry()
    }

    pub fn mark_try_begin(&mut self, handler_id: i32, context: Register) {
        let offset = self.bytecode_array_writer.offset();
        self.handler_table_builder
            .set_try_region_start(handler_id, offset);
        self.handler_table_builder
            .set_context_register(handler_id, context);
    }

    pub fn mark_try_end(&mut self, handler_id: i32) {
        let offset = self.bytecode_array_writer.offset();
        self.handler_table_builder
            .set_try_region_end(handler_id, offset);
    }

    pub fn mark_handler(
        &mut self,
        handler_id: i32,
        prediction: crate::codegen::handler_table::CatchPrediction,
    ) {
        self.bytecode_array_writer.mark_handler();
        let offset = self.bytecode_array_writer.offset();
        self.handler_table_builder
            .set_handler_target(handler_id, offset);
        self.handler_table_builder
            .set_prediction(handler_id, prediction);
    }

    pub fn drop_handler_entry(&mut self, handler_id: i32) {
        self.handler_table_builder.drop_handler_entry(handler_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builder() -> BytecodeArrayBuilder {
        BytecodeArrayBuilder::new(1, 0)
    }

    #[test]
    fn zero_gets_its_own_bytecode() {
        let mut builder = builder();
        builder.load_literal_smi(0);
        builder.load_literal_smi(1);
        let data = builder.to_bytecode_array();
        assert_eq!(data.bytecodes[0], Bytecode::LdaZero.to_byte());
        assert_eq!(data.bytecodes[1], Bytecode::LdaSmi.to_byte());
        assert_eq!(data.bytecodes[2], 1);
    }

    #[test]
    fn a_function_that_falls_off_the_end_returns_undefined() {
        let data = builder().to_bytecode_array();
        assert_eq!(
            data.bytecodes,
            vec![Bytecode::LdaUndefined.to_byte(), Bytecode::Return.to_byte()]
        );
    }

    #[test]
    fn an_explicit_return_is_not_doubled() {
        let mut builder = builder();
        builder.load_literal_smi(1);
        builder.emit_return();
        let data = builder.to_bytecode_array();
        assert_eq!(
            data.bytecodes,
            vec![Bytecode::LdaSmi.to_byte(), 1, Bytecode::Return.to_byte()],
            "the implicit return should be elided after an explicit one"
        );
    }

    #[test]
    fn a_binary_operation_reads_a_register_and_the_accumulator() {
        let mut builder = BytecodeArrayBuilder::new(1, 2);
        let a = builder.local(0);
        let b = builder.local(1);
        builder.load_accumulator_with_register(b);
        builder.binary_operation(Token::Add, a);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[0], Bytecode::Ldar.to_byte());
        assert_eq!(data.bytecodes[1], b.to_operand() as u8);
        assert_eq!(data.bytecodes[2], Bytecode::Add.to_byte());
        assert_eq!(data.bytecodes[3], a.to_operand() as u8);
        assert_eq!(
            data.bytecodes[4], K_UNINITIALIZED_EMBEDDED_FEEDBACK as u8,
            "the feedback byte starts empty"
        );
    }

    #[test]
    fn literals_are_shared_through_the_constant_pool() {
        let mut builder = builder();
        builder.load_literal_string("hello");
        builder.load_literal_string("hello");
        builder.load_literal_number(1.5);
        let data = builder.to_bytecode_array();
        assert_eq!(data.constants.len(), 2, "the repeated string is one entry");
        assert_eq!(
            data.constants[0],
            ConstantEntry::String("hello".to_string())
        );
        assert_eq!(data.constants[1], ConstantEntry::Number(1.5));
        assert_eq!(data.bytecodes[1], 0);
        assert_eq!(data.bytecodes[3], 0);
    }

    #[test]
    fn property_names_become_internalized_constants() {
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        let object = builder.local(0);
        builder.load_named_property(object, "x");
        let data = builder.to_bytecode_array();
        assert_eq!(data.constants[0], ConstantEntry::Name("x".to_string()));
        assert_eq!(data.bytecodes[0], Bytecode::GetNamedProperty.to_byte());
    }

    #[test]
    fn each_property_access_gets_its_own_feedback_slot() {
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        let object = builder.local(0);
        builder.load_named_property(object, "x");
        builder.load_named_property(object, "x");
        let data = builder.to_bytecode_array();
        assert_eq!(data.feedback_slot_count, 4);
        assert_eq!(data.bytecodes[3], 0, "first site: slot 0");
        assert_eq!(data.bytecodes[7], 2, "second site: slot 2");
        assert_eq!(
            data.feedback_slot_kinds,
            vec![
                FeedbackSlotKind::LoadProperty,
                FeedbackSlotKind::Invalid,
                FeedbackSlotKind::LoadProperty,
                FeedbackSlotKind::Invalid,
            ]
        );
    }

    #[test]
    fn the_frame_size_is_the_peak_register_use() {
        let mut builder = BytecodeArrayBuilder::new(1, 2);
        let mark = builder.register_allocator().next_register_index();
        let list = builder.register_allocator().new_register_list(4);
        builder.load_accumulator_with_register(list.get(0));
        builder.register_allocator().release_registers(mark);
        builder.register_allocator().new_register();
        let data = builder.to_bytecode_array();
        assert_eq!(
            data.register_count, 6,
            "two locals plus the four temporaries at the peak"
        );
    }

    #[test]
    fn a_forward_branch_is_resolved_when_bound() {
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        let mut end = BytecodeLabel::new();
        builder.load_accumulator_with_register(builder.local(0));
        builder.jump_if_to_boolean_false(&mut end);
        builder.load_literal_smi(1);
        builder.bind(&mut end);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[2], Bytecode::JumpIfToBooleanFalse.to_byte());
        assert_eq!(data.bytecodes[3], 4);
        assert!(data.constants.is_empty(), "a short jump needs no constant");
    }

    #[test]
    fn a_loop_jumps_backwards_to_its_header() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut header = BytecodeLoopHeader::new();
        builder.bind_loop_header(&mut header);
        builder.load_literal_smi(1);
        builder.jump_loop(&header);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[0], Bytecode::LdaSmi.to_byte());
        assert_eq!(data.bytecodes[2], Bytecode::JumpLoop.to_byte());
        assert_eq!(data.bytecodes[3], 2, "two bytes back to the header");
    }

    #[test]
    fn several_branches_can_share_an_exit() {
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        let mut exits = BytecodeLabels::new();
        let first = exits.new_label();
        builder.load_accumulator_with_register(builder.local(0));
        builder.bytecode_array_writer.write_jump(
            &BytecodeNode::new(Bytecode::JumpIfTrue, &[0]),
            exits.label_mut(first),
        );
        let second = exits.new_label();
        builder.bytecode_array_writer.write_jump(
            &BytecodeNode::new(Bytecode::JumpIfFalse, &[0]),
            exits.label_mut(second),
        );
        builder.load_literal_smi(7);
        builder.bind_labels(&mut exits);
        let data = builder.to_bytecode_array();
        assert!(exits.is_bound());
        assert_eq!(data.bytecodes[3], 6, "first branch skips 6 bytes");
        assert_eq!(data.bytecodes[5], 4, "second branch skips 4");
    }

    #[test]
    fn a_call_passes_its_arguments_as_a_contiguous_list() {
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        let callee = builder.local(0);
        let mut args = builder.register_allocator().new_growable_register_list();
        for value in [1, 2] {
            builder.load_literal_smi(value);
            let register = builder.register_allocator().grow_register_list(&mut args);
            builder.store_accumulator_in_register(register);
        }
        builder.call_undefined_receiver(callee, args);
        let data = builder.to_bytecode_array();

        let call_offset = data
            .bytecodes
            .iter()
            .position(|byte| *byte == Bytecode::CallUndefinedReceiver.to_byte())
            .expect("the call should have been emitted");
        assert_eq!(data.bytecodes[call_offset + 1], callee.to_operand() as u8);
        assert_eq!(
            data.bytecodes[call_offset + 2],
            args.first_register().to_operand() as u8
        );
        assert_eq!(data.bytecodes[call_offset + 3], 2, "two arguments");
    }

    #[test]
    fn source_positions_attach_to_the_next_bytecode() {
        let mut builder = builder();
        builder.set_source_position(5);
        builder.load_literal_smi(1);
        builder.load_literal_smi(2);
        builder.set_source_position(9);
        builder.emit_return();
        let data = builder.to_bytecode_array();
        assert_eq!(data.source_position_table, vec![(0, 5), (4, 9)]);
    }

    #[test]
    fn parameters_and_locals_live_in_different_parts_of_the_frame() {
        let builder = BytecodeArrayBuilder::new(3, 2);
        assert_eq!(builder.receiver(), Register::receiver());
        assert!(builder.parameter(0).is_parameter());
        assert!(builder.parameter(1).is_parameter());
        assert!(!builder.local(0).is_parameter());
        assert_eq!(builder.local(0), Register::new(0));
        assert_eq!(builder.local(1), Register::new(1));
    }
}

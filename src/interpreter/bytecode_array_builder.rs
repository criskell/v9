//! Ported from `src/interpreter/bytecode-array-builder.h` /
//! `bytecode-array-builder.cc`.
//!
//! The interface the bytecode generator writes against: one method per
//! operation it wants, each one choosing the bytecode, allocating the
//! feedback slot and handing a [`BytecodeNode`] to the writer. Fluent, as in
//! V8, so a sequence of emissions reads as one statement.
//!
//! It owns the register allocator and the writer, which is what lets
//! [`Self::binary_operation`] decide between `Add` and `AddSmi` or
//! [`Self::load_literal_smi`] decide between `LdaZero` and `LdaSmi` --
//! peephole choices that need to know what is being emitted *and* what is
//! already there.
//!
//! Deviation, tracked in `PORTING.md`: feedback slots are counted here
//! rather than described by a `FeedbackVectorSpec` from
//! `src/objects/feedback-vector.h`. The slot *indices* in the bytecode are
//! therefore the ones V8 would use, but nothing allocates a feedback vector
//! to go with them yet, so the inline caches have somewhere to live and
//! nothing living there.

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

/// `BytecodeArrayBuilder::kUninitializedEmbeddedFeedback`: what a binary
/// operation's inline feedback byte starts as.
pub const K_UNINITIALIZED_EMBEDDED_FEEDBACK: i32 = 0;

/// `BytecodeArrayBuilder::kFeedbackIsEmbedded`: the sentinel V8 passes where
/// a feedback slot would go for operations whose feedback is in the bytecode.
pub const K_FEEDBACK_IS_EMBEDDED: i32 = -1;

/// What the builder produces: everything a `BytecodeArray` needs, before it
/// is materialised in the heap. V8's `BytecodeArrayBuilder::ToBytecodeArray`
/// allocates directly; `vx` separates the two so that generation never
/// allocates (and so can never trigger a collection half way through a
/// function).
#[derive(Clone, Debug)]
pub struct BytecodeArrayData {
    pub bytecodes: Vec<u8>,
    pub constants: Vec<ConstantEntry>,
    /// Registers the frame needs, which is the allocator's high-water mark.
    pub register_count: i32,
    /// Including the receiver.
    pub parameter_count: i32,
    pub feedback_slot_count: i32,
    /// The kind of each feedback site, in allocation order. The vector's
    /// layout cannot be recovered from the bytecode alone, because a slot's
    /// width depends on its kind.
    pub feedback_slot_kinds: Vec<FeedbackSlotKind>,
    pub source_position_table: Vec<(usize, usize)>,
}

pub struct BytecodeArrayBuilder {
    bytecode_array_writer: BytecodeArrayWriter,
    register_allocator: BytecodeRegisterAllocator,
    parameter_count: i32,
    local_register_count: i32,
    /// `BytecodeArrayBuilder::feedback_vector_spec_`: the sites, in the
    /// order they were allocated.
    feedback_vector_spec: FeedbackVectorSpec,
    /// The source position to attach to the next bytecode emitted.
    pending_source_position: Option<usize>,
}

impl BytecodeArrayBuilder {
    /// `BytecodeArrayBuilder::BytecodeArrayBuilder`.
    ///
    /// `parameter_count` includes the receiver. Locals occupy registers
    /// `r0..r(locals-1)`, and temporaries are allocated above them.
    pub fn new(parameter_count: i32, local_register_count: i32) -> BytecodeArrayBuilder {
        BytecodeArrayBuilder {
            bytecode_array_writer: BytecodeArrayWriter::new(),
            register_allocator: BytecodeRegisterAllocator::new(local_register_count),
            parameter_count,
            local_register_count,
            feedback_vector_spec: FeedbackVectorSpec::new(),
            pending_source_position: None,
        }
    }

    // -- frame ---------------------------------------------------------------

    #[inline]
    pub fn parameter_count(&self) -> i32 {
        self.parameter_count
    }

    /// `BytecodeArrayBuilder::Parameter`: the register holding declared
    /// parameter `index` (0 is the first after the receiver).
    #[inline]
    pub fn parameter(&self, index: i32) -> Register {
        debug_assert!(index < self.parameter_count - 1);
        Register::from_parameter_index(index + 1)
    }

    #[inline]
    pub fn receiver(&self) -> Register {
        Register::receiver()
    }

    /// The register holding local `index`.
    #[inline]
    pub fn local(&self, index: i32) -> Register {
        debug_assert!(index < self.local_register_count);
        Register::new(index)
    }

    #[inline]
    pub fn register_allocator(&mut self) -> &mut BytecodeRegisterAllocator {
        &mut self.register_allocator
    }

    /// `FeedbackVectorSpec::AddSlot`.
    ///
    /// The *kind* has to be declared here and not later, because it decides
    /// how wide the slot is -- so the vector's layout is fixed by the time
    /// the bytecode referring to it exists.
    pub fn allocate_feedback_slot(&mut self, kind: FeedbackSlotKind) -> i32 {
        self.feedback_vector_spec.add_slot(kind)
    }

    pub fn feedback_vector_spec(&self) -> &FeedbackVectorSpec {
        &self.feedback_vector_spec
    }

    /// `BytecodeArrayBuilder::SetStatementPosition` /
    /// `SetExpressionPosition`: the position lands on the next bytecode.
    pub fn set_source_position(&mut self, position: usize) -> &mut Self {
        self.pending_source_position = Some(position);
        self
    }

    pub fn offset(&self) -> usize {
        self.bytecode_array_writer.offset()
    }

    // -- emission ------------------------------------------------------------

    /// `BytecodeArrayBuilder::Output`
    fn output(&mut self, bytecode: Bytecode, operands: &[i32]) -> &mut Self {
        let mut node = BytecodeNode::new(bytecode, operands);
        if let Some(position) = self.pending_source_position.take() {
            node = node.with_source_position(position);
        }
        self.bytecode_array_writer.write(&node);
        self
    }

    // -- loading the accumulator ---------------------------------------------

    /// `BytecodeArrayBuilder::LoadLiteral(Smi)`
    pub fn load_literal_smi(&mut self, value: i32) -> &mut Self {
        // `LdaZero` exists because zero is the single most common literal in
        // any program, and it saves the operand byte.
        if value == 0 {
            self.output(Bytecode::LdaZero, &[])
        } else {
            self.output(Bytecode::LdaSmi, &[value])
        }
    }

    /// `BytecodeArrayBuilder::LoadLiteral(double)`
    pub fn load_literal_number(&mut self, value: f64) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_number(value) as i32;
        self.output(Bytecode::LdaConstant, &[index])
    }

    /// `BytecodeArrayBuilder::LoadLiteral(AstRawString)`
    pub fn load_literal_string(&mut self, value: &str) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_string(value) as i32;
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

    /// Loads any AST literal, picking the cheapest encoding.
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

    /// `BytecodeArrayBuilder::LoadAccumulatorWithRegister`
    pub fn load_accumulator_with_register(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::Ldar, &[register.to_operand()])
    }

    /// `BytecodeArrayBuilder::StoreAccumulatorInRegister`
    pub fn store_accumulator_in_register(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::Star, &[register.to_operand()])
    }

    /// `BytecodeArrayBuilder::MoveRegister`
    pub fn move_register(&mut self, from: Register, to: Register) -> &mut Self {
        debug_assert_ne!(from, to, "a move to itself is not emitted");
        self.output(Bytecode::Mov, &[from.to_operand(), to.to_operand()])
    }

    // -- globals -------------------------------------------------------------

    /// `BytecodeArrayBuilder::LoadGlobal`
    pub fn load_global(&mut self, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadGlobalNotInsideTypeof);
        self.output(Bytecode::LdaGlobal, &[index, slot])
    }

    /// `BytecodeArrayBuilder::StoreGlobal`
    pub fn store_global(&mut self, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::StoreGlobalStrict);
        self.output(Bytecode::StaGlobal, &[index, slot])
    }

    // -- properties ----------------------------------------------------------

    /// `BytecodeArrayBuilder::LoadNamedProperty`
    pub fn load_named_property(&mut self, object: Register, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadProperty);
        self.output(
            Bytecode::GetNamedProperty,
            &[object.to_operand(), index, slot],
        )
    }

    /// `BytecodeArrayBuilder::LoadKeyedProperty`: the key is in the
    /// accumulator.
    pub fn load_keyed_property(&mut self, object: Register) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::LoadKeyed);
        self.output(Bytecode::GetKeyedProperty, &[object.to_operand(), slot])
    }

    /// `BytecodeArrayBuilder::SetNamedProperty`: the value is in the
    /// accumulator.
    pub fn set_named_property(&mut self, object: Register, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::SetNamedStrict);
        self.output(
            Bytecode::SetNamedProperty,
            &[object.to_operand(), index, slot],
        )
    }

    /// `BytecodeArrayBuilder::DefineNamedOwnProperty`: for an object literal,
    /// where the property is known not to exist yet and no setter on the
    /// prototype chain may intercept it.
    pub fn define_named_own_property(&mut self, object: Register, name: &str) -> &mut Self {
        let index = self.insert_name(name);
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::DefineNamedOwn);
        self.output(
            Bytecode::DefineNamedOwnProperty,
            &[object.to_operand(), index, slot],
        )
    }

    /// `BytecodeArrayBuilder::SetKeyedProperty`
    pub fn set_keyed_property(&mut self, object: Register, key: Register) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::SetKeyedStrict);
        self.output(
            Bytecode::SetKeyedProperty,
            &[object.to_operand(), key.to_operand(), slot],
        )
    }

    // -- operators -----------------------------------------------------------

    /// `BytecodeArrayBuilder::BinaryOperation`: `acc = reg op acc`.
    ///
    /// The feedback is embedded in the instruction rather than kept in a
    /// slot, which is why no slot is allocated here.
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

    /// `BytecodeArrayBuilder::BinaryOperationSmiLiteral`: `acc = acc op imm`,
    /// which saves a register and a load for the very common `x + 1`.
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

    /// `BytecodeArrayBuilder::UnaryOperation`
    pub fn unary_operation(&mut self, op: Token) -> &mut Self {
        match op {
            Token::Add => {
                // Unary plus is ToNumber, nothing more.
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

    /// `BytecodeArrayBuilder::LogicalNot`: negates a value already known to
    /// be a boolean.
    pub fn logical_not(&mut self) -> &mut Self {
        self.output(Bytecode::LogicalNot, &[])
    }

    /// `BytecodeArrayBuilder::ToBooleanLogicalNot`: `!x` for any `x`, which
    /// has to coerce first.
    pub fn to_boolean_logical_not(&mut self) -> &mut Self {
        self.output(Bytecode::ToBooleanLogicalNot, &[])
    }

    /// `BytecodeArrayBuilder::TypeOf`
    pub fn type_of(&mut self) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::TypeOf);
        self.output(Bytecode::TypeOf, &[slot])
    }

    /// `BytecodeArrayBuilder::CompareOperation`: `acc = reg op acc`.
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
            // `!=` and `!==` have no bytecode of their own: the generator
            // emits the positive test and negates it, which is why V8's
            // token list has them adjacent to their positive forms.
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

    /// `BytecodeArrayBuilder::CompareUndefined` / `CompareNull`: the
    /// comparisons against the oddballs get their own operand-free bytecodes,
    /// because `x == null` is common and needs no feedback.
    pub fn compare_null(&mut self) -> &mut Self {
        self.output(Bytecode::TestNull, &[])
    }

    pub fn compare_undefined(&mut self) -> &mut Self {
        self.output(Bytecode::TestUndefined, &[])
    }

    /// `BytecodeArrayBuilder::CompareTypeOf`-adjacent: `TestUndetectable` is
    /// true for `null`, `undefined` and (in V8) `document.all`.
    ///
    /// This is what `x == null` compiles to. The *strict* comparisons get
    /// `TestNull`/`TestUndefined` instead, because `null === undefined` is
    /// false while `null == undefined` is true.
    pub fn compare_undetectable(&mut self) -> &mut Self {
        self.output(Bytecode::TestUndetectable, &[])
    }

    pub fn compare_reference_equal(&mut self, register: Register) -> &mut Self {
        self.output(Bytecode::TestReferenceEqual, &[register.to_operand()])
    }

    // -- conversions ---------------------------------------------------------

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

    // -- literals ------------------------------------------------------------

    pub fn create_empty_object_literal(&mut self) -> &mut Self {
        self.output(Bytecode::CreateEmptyObjectLiteral, &[])
    }

    pub fn create_empty_array_literal(&mut self) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Literal);
        self.output(Bytecode::CreateEmptyArrayLiteral, &[slot])
    }

    /// `BytecodeArrayBuilder::CreateClosure`
    pub fn create_closure(&mut self, function_index: usize) -> &mut Self {
        let index = self
            .bytecode_array_writer
            .constant_array_builder_mut()
            .insert_function_literal(function_index) as i32;
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::Literal);
        self.output(Bytecode::CreateClosure, &[index, slot, 0])
    }

    // -- calls ---------------------------------------------------------------

    /// `BytecodeArrayBuilder::CallUndefinedReceiver`: `f(a, b)`, where there
    /// is no receiver to pass.
    ///
    /// `args` must be the registers holding the arguments, contiguously --
    /// which is why the generator builds them with a growable register list.
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

    /// `BytecodeArrayBuilder::CallProperty`: `o.f(a)`, where `o` is the
    /// receiver. The receiver is the first register of `args`.
    pub fn call_property(&mut self, callee: Register, args: RegisterList) -> &mut Self {
        debug_assert!(
            args.register_count() >= 1,
            "a property call passes at least the receiver"
        );
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

    // -- control flow --------------------------------------------------------

    /// `BytecodeArrayBuilder::Jump`
    pub fn jump(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::Jump, label)
    }

    pub fn jump_if_true(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfTrue, label)
    }

    pub fn jump_if_false(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.output_jump(Bytecode::JumpIfFalse, label)
    }

    /// `BytecodeArrayBuilder::JumpIfToBooleanTrue`: branches on any value,
    /// coercing it. Saves the generator emitting a `ToBoolean` before every
    /// `if`, which is why these bytecodes exist at all.
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

    /// `BytecodeArrayBuilder::JumpLoop`: the backward jump that closes a
    /// loop. Its own bytecode because it is where the interpreter checks for
    /// interrupts and, in V8, where it decides to tier up.
    pub fn jump_loop(&mut self, loop_header: &BytecodeLoopHeader) -> &mut Self {
        let slot = self.allocate_feedback_slot(FeedbackSlotKind::JumpLoop);
        let node = BytecodeNode::new(Bytecode::JumpLoop, &[0, 0, slot]);
        self.bytecode_array_writer
            .write_jump_loop(&node, loop_header);
        self
    }

    /// `BytecodeArrayBuilder::Bind(BytecodeLabel*)`
    pub fn bind(&mut self, label: &mut BytecodeLabel) -> &mut Self {
        self.bytecode_array_writer.bind_label(label);
        self
    }

    /// `BytecodeArrayBuilder::Bind(BytecodeLabels*)`
    pub fn bind_labels(&mut self, labels: &mut BytecodeLabels) -> &mut Self {
        self.bytecode_array_writer.bind_labels(labels);
        self
    }

    /// `BytecodeArrayBuilder::Bind(BytecodeLoopHeader*)`
    pub fn bind_loop_header(&mut self, loop_header: &mut BytecodeLoopHeader) -> &mut Self {
        self.bytecode_array_writer.bind_loop_header(loop_header);
        self
    }

    /// `BytecodeArrayBuilder::Return`
    pub fn emit_return(&mut self) -> &mut Self {
        self.output(Bytecode::Return, &[])
    }

    /// `BytecodeArrayBuilder::Throw`
    pub fn emit_throw(&mut self) -> &mut Self {
        self.output(Bytecode::Throw, &[])
    }

    /// Marks the next bytecode as reachable. The generator gets this
    /// implicitly by binding a label; tests sometimes need it directly.
    #[cfg(test)]
    pub fn start_basic_block_for_test(&mut self) {
        self.bytecode_array_writer.start_basic_block();
    }

    #[inline]
    pub fn exit_seen_in_block(&self) -> bool {
        self.bytecode_array_writer.exit_seen_in_block()
    }

    // -- finishing -----------------------------------------------------------

    fn insert_name(&mut self, name: &str) -> i32 {
        self.bytecode_array_writer
            .constant_array_builder_mut()
            .insert_name(name) as i32
    }

    /// `BytecodeArrayBuilder::ToBytecodeArray`.
    ///
    /// Appends the implicit `return undefined` that every function ends with
    /// if control can fall off the end, which is why a JavaScript function
    /// without a `return` evaluates to `undefined`.
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
        }
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
        // `a + b` compiles to: load a into a register, load b into the
        // accumulator, then Add.
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
        // Both loads name index 0.
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
        // Two sites that could cache different maps must not share a slot.
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        let object = builder.local(0);
        builder.load_named_property(object, "x");
        builder.load_named_property(object, "x");
        let data = builder.to_bytecode_array();
        // A load site is two slots wide -- one for the map, one for the
        // handler -- so the second site starts at 2.
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
        // if (x) { } -- jump over the empty body.
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        let mut end = BytecodeLabel::new();
        builder.load_accumulator_with_register(builder.local(0));
        builder.jump_if_to_boolean_false(&mut end);
        builder.load_literal_smi(1);
        builder.bind(&mut end);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[2], Bytecode::JumpIfToBooleanFalse.to_byte());
        // The jump is 2 bytes, LdaSmi is 2, so the target is 4 on.
        assert_eq!(data.bytecodes[3], 4);
        assert!(data.constants.is_empty(), "a short jump needs no constant");
    }

    #[test]
    fn a_loop_jumps_backwards_to_its_header() {
        // while (true) { }
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

        // Locate the call rather than computing its offset: it has four
        // operands, so the arithmetic is easy to get wrong and says nothing.
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

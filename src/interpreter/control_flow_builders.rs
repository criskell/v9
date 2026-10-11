use crate::codegen::handler_table::CatchPrediction;
use crate::interpreter::bytecode_array_builder::BytecodeArrayBuilder;
use crate::interpreter::bytecode_label::{BytecodeLabel, BytecodeLabels, BytecodeLoopHeader};
use crate::interpreter::bytecode_register::Register;

#[derive(Default)]
pub struct BreakableControlFlowBuilder {
    break_labels: BytecodeLabels,
}

impl BreakableControlFlowBuilder {
    pub fn new() -> BreakableControlFlowBuilder {
        BreakableControlFlowBuilder::default()
    }

    /// `BreakableControlFlowBuilder::Break`
    pub fn emit_break(&mut self, builder: &mut BytecodeArrayBuilder) {
        let label = self.break_labels.new_label();
        builder.jump(self.break_labels.label_mut(label));
    }

    /// `BreakableControlFlowBuilder::BindBreakTarget`
    pub fn bind_break_target(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.bind_labels(&mut self.break_labels);
    }

    pub fn break_labels(&mut self) -> &mut BytecodeLabels {
        &mut self.break_labels
    }
}

#[derive(Default)]
pub struct BlockBuilder {
    breakable: BreakableControlFlowBuilder,
}

impl BlockBuilder {
    pub fn new() -> BlockBuilder {
        BlockBuilder::default()
    }

    pub fn emit_break(&mut self, builder: &mut BytecodeArrayBuilder) {
        self.breakable.emit_break(builder);
    }

    pub fn finish(&mut self, builder: &mut BytecodeArrayBuilder) {
        self.breakable.bind_break_target(builder);
    }
}

#[derive(Default)]
pub struct LoopBuilder {
    breakable: BreakableControlFlowBuilder,
    continue_labels: BytecodeLabels,
    loop_header: BytecodeLoopHeader,
}

impl LoopBuilder {
    pub fn new() -> LoopBuilder {
        LoopBuilder::default()
    }

    /// `LoopBuilder::LoopHeader`
    pub fn loop_header(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.bind_loop_header(&mut self.loop_header);
    }

    pub fn jump_to_header(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.jump_loop(&self.loop_header);
    }

    /// `LoopBuilder::BindContinueTarget`
    pub fn bind_continue_target(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.bind_labels(&mut self.continue_labels);
    }

    /// `LoopBuilder::Continue`
    pub fn emit_continue(&mut self, builder: &mut BytecodeArrayBuilder) {
        let label = self.continue_labels.new_label();
        builder.jump(self.continue_labels.label_mut(label));
    }

    pub fn emit_break(&mut self, builder: &mut BytecodeArrayBuilder) {
        self.breakable.emit_break(builder);
    }

    pub fn break_labels(&mut self) -> &mut BytecodeLabels {
        self.breakable.break_labels()
    }

    pub fn continue_labels(&mut self) -> &mut BytecodeLabels {
        &mut self.continue_labels
    }

    pub fn finish(&mut self, builder: &mut BytecodeArrayBuilder) {
        self.breakable.bind_break_target(builder);
    }
}

#[derive(Default)]
pub struct ConditionalControlFlowBuilder {
    then_labels: BytecodeLabels,
    else_labels: BytecodeLabels,
    end_labels: BytecodeLabels,
}

impl ConditionalControlFlowBuilder {
    pub fn new() -> ConditionalControlFlowBuilder {
        ConditionalControlFlowBuilder::default()
    }

    pub fn then_labels(&mut self) -> &mut BytecodeLabels {
        &mut self.then_labels
    }

    pub fn else_labels(&mut self) -> &mut BytecodeLabels {
        &mut self.else_labels
    }

    /// `ConditionalControlFlowBuilder::Then`
    pub fn then_branch(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.bind_labels(&mut self.then_labels);
    }

    /// `ConditionalControlFlowBuilder::Else`
    pub fn else_branch(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.bind_labels(&mut self.else_labels);
    }

    pub fn jump_to_end(&mut self, builder: &mut BytecodeArrayBuilder) {
        let label = self.end_labels.new_label();
        builder.jump(self.end_labels.label_mut(label));
    }

    pub fn finish(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.bind_labels(&mut self.end_labels);
    }
}

/// `TryCatchBuilder`
pub struct TryCatchBuilder {
    handler_id: i32,
    catch_prediction: CatchPrediction,
    exit: BytecodeLabel,
}

impl TryCatchBuilder {
    pub fn new(
        builder: &mut BytecodeArrayBuilder,
        catch_prediction: CatchPrediction,
    ) -> TryCatchBuilder {
        TryCatchBuilder {
            handler_id: builder.new_handler_entry(),
            catch_prediction,
            exit: BytecodeLabel::new(),
        }
    }

    /// `TryCatchBuilder::BeginTry`
    pub fn begin_try(&mut self, builder: &mut BytecodeArrayBuilder, context: Register) {
        builder.mark_try_begin(self.handler_id, context);
    }

    /// `TryCatchBuilder::EndTry`
    pub fn end_try(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.mark_try_end(self.handler_id);
        builder.jump(&mut self.exit);
        builder.mark_handler(self.handler_id, self.catch_prediction);
    }

    /// `TryCatchBuilder::EndCatch`
    pub fn end_catch(&mut self, builder: &mut BytecodeArrayBuilder) {
        builder.bind(&mut self.exit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::bytecodes::Bytecode;

    #[test]
    fn a_break_jumps_to_where_the_loop_ends() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut loop_builder = LoopBuilder::new();
        loop_builder.loop_header(&mut builder);
        loop_builder.emit_break(&mut builder);
        loop_builder.bind_continue_target(&mut builder);
        loop_builder.jump_to_header(&mut builder);
        loop_builder.finish(&mut builder);
        builder.load_literal_smi(1);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[0], Bytecode::Jump.to_byte());
        assert_eq!(data.bytecodes[1], 6);
        assert_eq!(data.bytecodes[2], Bytecode::JumpLoop.to_byte());
        assert_eq!(data.bytecodes[6], Bytecode::LdaSmi.to_byte());
    }

    #[test]
    fn a_continue_lands_before_the_backward_jump_not_on_the_header() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut loop_builder = LoopBuilder::new();
        loop_builder.loop_header(&mut builder);
        loop_builder.emit_continue(&mut builder);
        loop_builder.bind_continue_target(&mut builder);
        builder.load_literal_smi(1);
        loop_builder.jump_to_header(&mut builder);
        loop_builder.finish(&mut builder);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[0], Bytecode::Jump.to_byte());
        assert_eq!(
            data.bytecodes[1], 2,
            "continue skips to the increment, which is two bytes on"
        );
        assert_eq!(data.bytecodes[2], Bytecode::LdaSmi.to_byte());
        assert_eq!(data.bytecodes[4], Bytecode::JumpLoop.to_byte());
    }

    #[test]
    fn several_breaks_share_the_exit() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut loop_builder = LoopBuilder::new();
        loop_builder.loop_header(&mut builder);
        loop_builder.emit_break(&mut builder);
        builder.start_basic_block_for_test();
        builder.load_literal_smi(1);
        loop_builder.emit_break(&mut builder);
        loop_builder.finish(&mut builder);
        builder.load_literal_smi(2);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[0], Bytecode::Jump.to_byte());
        assert_eq!(
            data.bytecodes[1], 6,
            "first break: over LdaSmi and the other Jump"
        );
        assert_eq!(data.bytecodes[4], Bytecode::Jump.to_byte());
        assert_eq!(data.bytecodes[5], 2, "second break: just past itself");
    }

    #[test]
    fn a_conditional_binds_both_arms_and_the_join() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut conditional = ConditionalControlFlowBuilder::new();
        let else_label = conditional.else_labels().new_label();
        builder.load_true();
        builder.jump_if_false(conditional.else_labels().label_mut(else_label));

        conditional.then_branch(&mut builder);
        builder.load_literal_smi(1);
        conditional.jump_to_end(&mut builder);

        conditional.else_branch(&mut builder);
        builder.load_literal_smi(2);

        conditional.finish(&mut builder);
        let data = builder.to_bytecode_array();

        assert_eq!(data.bytecodes[1], Bytecode::JumpIfFalse.to_byte());
        assert_eq!(data.bytecodes[2], 6, "the else arm starts 6 bytes on");
        assert_eq!(data.bytecodes[5], Bytecode::Jump.to_byte());
        assert_eq!(data.bytecodes[6], 4, "the join is 4 bytes past the jump");
    }
}

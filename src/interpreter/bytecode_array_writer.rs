use crate::interpreter::bytecode_label::{BytecodeLabel, BytecodeLabels, BytecodeLoopHeader};
use crate::interpreter::bytecode_node::BytecodeNode;
use crate::interpreter::bytecode_operands::{OperandScale, OperandSize};
use crate::interpreter::bytecodes::Bytecode;
use crate::interpreter::constant_array_builder::ConstantArrayBuilder;

const K_JUMP_PLACEHOLDER_BYTE: u8 = 0x7f;

pub struct BytecodeArrayWriter {
    bytecodes: Vec<u8>,
    constant_array_builder: ConstantArrayBuilder,
    unbound_jumps: usize,
    exit_seen_in_block: bool,
    source_position_table: Vec<(usize, usize)>,
}

impl Default for BytecodeArrayWriter {
    fn default() -> BytecodeArrayWriter {
        BytecodeArrayWriter::new()
    }
}

impl BytecodeArrayWriter {
    pub fn new() -> BytecodeArrayWriter {
        BytecodeArrayWriter {
            bytecodes: Vec::new(),
            constant_array_builder: ConstantArrayBuilder::new(),
            unbound_jumps: 0,
            exit_seen_in_block: false,
            source_position_table: Vec::new(),
        }
    }

    #[inline]
    pub fn bytecodes(&self) -> &[u8] {
        &self.bytecodes
    }

    #[inline]
    pub fn constant_array_builder(&self) -> &ConstantArrayBuilder {
        &self.constant_array_builder
    }

    #[inline]
    pub fn constant_array_builder_mut(&mut self) -> &mut ConstantArrayBuilder {
        &mut self.constant_array_builder
    }

    #[inline]
    pub fn source_position_table(&self) -> &[(usize, usize)] {
        &self.source_position_table
    }

    #[inline]
    pub fn offset(&self) -> usize {
        self.bytecodes.len()
    }

    pub fn unbound_jumps(&self) -> usize {
        self.unbound_jumps
    }

    #[inline]
    /// `BytecodeArrayWriter::StartBasicBlock`
    pub fn mark_handler(&mut self) {
        self.exit_seen_in_block = false;
    }

    pub fn exit_seen_in_block(&self) -> bool {
        self.exit_seen_in_block
    }

    pub fn start_basic_block(&mut self) {
        self.exit_seen_in_block = false;
    }

    /// `BytecodeArrayWriter::Write`
    pub fn write(&mut self, node: &BytecodeNode) {
        debug_assert!(!node.bytecode().is_jump());
        if self.exit_seen_in_block {
            return;
        }
        self.update_exit_seen_in_block(node.bytecode());
        self.emit_bytecode(node);
    }

    pub fn write_jump(&mut self, node: &BytecodeNode, label: &mut BytecodeLabel) {
        debug_assert!(node.bytecode().is_forward_jump());
        debug_assert_eq!(node.operand(0), 0);
        if self.exit_seen_in_block {
            return;
        }

        self.emit_jump(node, label);
        self.update_exit_seen_in_block(node.bytecode());
    }

    pub fn write_jump_loop(&mut self, node: &BytecodeNode, loop_header: &BytecodeLoopHeader) {
        debug_assert_eq!(node.bytecode(), Bytecode::JumpLoop);
        if self.exit_seen_in_block {
            return;
        }
        self.emit_jump_loop(node, loop_header);
        self.update_exit_seen_in_block(node.bytecode());
    }

    /// `BytecodeArrayWriter::BindLabel`
    pub fn bind_label(&mut self, label: &mut BytecodeLabel) {
        if !label.has_referrer_jump() {
            label.bind();
            self.start_basic_block();
            return;
        }
        let current_offset = self.offset();
        self.patch_jump(current_offset, label.jump_offset());
        label.bind();
        self.unbound_jumps -= 1;
        self.start_basic_block();
    }

    pub fn bind_labels(&mut self, labels: &mut BytecodeLabels) {
        for index in labels.pending() {
            let jump_offset = labels.label(index).jump_offset();
            let current_offset = self.offset();
            self.patch_jump(current_offset, jump_offset);
            labels.label_mut(index).bind();
            self.unbound_jumps -= 1;
        }
        labels.mark_bound();
        self.start_basic_block();
    }

    /// `BytecodeArrayWriter::BindLoopHeader`
    pub fn bind_loop_header(&mut self, loop_header: &mut BytecodeLoopHeader) {
        loop_header.bind_to(self.offset());
        if self.exit_seen_in_block {
            return;
        }
        self.start_basic_block();
    }

    // -- emission -----------------------------------------------------------

    /// `BytecodeArrayWriter::EmitBytecode`
    fn emit_bytecode(&mut self, node: &BytecodeNode) {
        debug_assert_ne!(node.bytecode(), Bytecode::Illegal);
        let bytecode = node.bytecode();
        let scale = node.operand_scale();

        if let Some(position) = node.source_position() {
            self.source_position_table.push((self.offset(), position));
        }

        if let Some(prefix) = Bytecode::operand_scale_to_prefix(scale) {
            self.bytecodes.push(prefix.to_byte());
        }
        self.bytecodes.push(bytecode.to_byte());

        for (index, value) in node.operands().iter().enumerate() {
            match bytecode.operand_size(index, scale) {
                OperandSize::None => unreachable!("an operand with no size"),
                OperandSize::Byte => self.bytecodes.push(*value as u8),
                OperandSize::Short => self
                    .bytecodes
                    .extend_from_slice(&(*value as u16).to_le_bytes()),
                OperandSize::Quad => self
                    .bytecodes
                    .extend_from_slice(&(*value as u32).to_le_bytes()),
            }
        }
    }

    /// `BytecodeArrayWriter::EmitJump`
    fn emit_jump(&mut self, node: &BytecodeNode, label: &mut BytecodeLabel) {
        let current_offset = self.offset();
        self.unbound_jumps += 1;
        label.set_referrer(current_offset);

        let reserved = self
            .constant_array_builder
            .create_reserved_entry(operand_size_for_scale(node.operand_scale()));

        let mut node = *node;
        node.update_operand0(placeholder_for(reserved));
        debug_assert_eq!(operand_size_for_scale(node.operand_scale()), reserved);
        self.emit_bytecode(&node);
    }

    /// `BytecodeArrayWriter::EmitJumpLoop`
    fn emit_jump_loop(&mut self, node: &BytecodeNode, loop_header: &BytecodeLoopHeader) {
        let current_offset = self.offset();
        let header_offset = loop_header.offset();
        debug_assert!(current_offset >= header_offset);

        let mut delta = (current_offset - header_offset) as i32;

        let mut node = *node;
        let emits_prefix = node.operand_scale() != OperandScale::Single
            || scale_for_unsigned(delta as u32) != OperandScale::Single;
        if emits_prefix {
            const K_PREFIX_BYTECODE_SIZE: i32 = 1;
            delta += K_PREFIX_BYTECODE_SIZE;
        }
        node.update_operand0(delta);
        debug_assert_eq!(node.operand_scale() != OperandScale::Single, emits_prefix);
        self.emit_bytecode(&node);
    }

    // -- patching -----------------------------------------------------------

    fn patch_jump(&mut self, jump_target: usize, jump_location: usize) {
        let first_byte = Bytecode::from_byte(self.bytecodes[jump_location])
            .expect("a jump instruction at the recorded location");
        let mut delta = (jump_target - jump_location) as i32;
        let mut prefix_offset = 0;
        let mut operand_scale = OperandScale::Single;

        if first_byte.is_prefix_scaling_bytecode() {
            delta -= 1;
            prefix_offset = 1;
            operand_scale = first_byte
                .prefix_to_operand_scale()
                .expect("a prefix has a scale");
        }

        debug_assert!(
            Bytecode::from_byte(self.bytecodes[jump_location + prefix_offset])
                .expect("a jump opcode")
                .is_jump()
        );

        match operand_scale {
            OperandScale::Single => self.patch_jump_with_8bit_operand(jump_location, delta),
            OperandScale::Double => {
                self.patch_jump_with_16bit_operand(jump_location + prefix_offset, delta)
            }
            OperandScale::Quadruple => {
                self.patch_jump_with_32bit_operand(jump_location + prefix_offset, delta)
            }
        }
    }

    /// `BytecodeArrayWriter::PatchJumpWith8BitOperand`
    fn patch_jump_with_8bit_operand(&mut self, jump_location: usize, delta: i32) {
        debug_assert!(delta > 0);
        let operand_location = jump_location + 1;
        debug_assert_eq!(self.bytecodes[operand_location], K_JUMP_PLACEHOLDER_BYTE);

        if scale_for_unsigned(delta as u32) == OperandScale::Single {
            self.constant_array_builder
                .discard_reserved_entry(OperandSize::Byte);
            self.bytecodes[operand_location] = delta as u8;
            return;
        }
        let entry = self
            .constant_array_builder
            .commit_reserved_entry(OperandSize::Byte, delta);
        debug_assert!(entry <= u8::MAX as usize);
        let jump_bytecode = Bytecode::from_byte(self.bytecodes[jump_location])
            .expect("a jump")
            .jump_with_constant_operand();
        self.bytecodes[jump_location] = jump_bytecode.to_byte();
        self.bytecodes[operand_location] = entry as u8;
    }

    /// `BytecodeArrayWriter::PatchJumpWith16BitOperand`
    fn patch_jump_with_16bit_operand(&mut self, jump_location: usize, delta: i32) {
        debug_assert!(delta > 0);
        let operand_location = jump_location + 1;
        let bytes = if scale_for_unsigned(delta as u32) <= OperandScale::Double {
            self.constant_array_builder
                .discard_reserved_entry(OperandSize::Short);
            (delta as u16).to_le_bytes()
        } else {
            let entry = self
                .constant_array_builder
                .commit_reserved_entry(OperandSize::Short, delta);
            debug_assert!(entry <= u16::MAX as usize);
            let jump_bytecode = Bytecode::from_byte(self.bytecodes[jump_location])
                .expect("a jump")
                .jump_with_constant_operand();
            self.bytecodes[jump_location] = jump_bytecode.to_byte();
            (entry as u16).to_le_bytes()
        };
        self.bytecodes[operand_location..operand_location + 2].copy_from_slice(&bytes);
    }

    /// `BytecodeArrayWriter::PatchJumpWith32BitOperand`
    fn patch_jump_with_32bit_operand(&mut self, jump_location: usize, delta: i32) {
        debug_assert!(delta > 0);
        let operand_location = jump_location + 1;
        self.constant_array_builder
            .discard_reserved_entry(OperandSize::Quad);
        self.bytecodes[operand_location..operand_location + 4]
            .copy_from_slice(&(delta as u32).to_le_bytes());
    }

    /// `BytecodeArrayWriter::UpdateExitSeenInBlock`
    fn update_exit_seen_in_block(&mut self, bytecode: Bytecode) {
        if bytecode.is_without_fall_through() {
            self.exit_seen_in_block = true;
        }
    }

    pub fn finish(self) -> (Vec<u8>, ConstantArrayBuilder, Vec<(usize, usize)>) {
        assert_eq!(self.unbound_jumps, 0);
        assert_eq!(self.constant_array_builder.outstanding_reservations(), 0);
        (
            self.bytecodes,
            self.constant_array_builder,
            self.source_position_table,
        )
    }
}

fn operand_size_for_scale(scale: OperandScale) -> OperandSize {
    match scale {
        OperandScale::Single => OperandSize::Byte,
        OperandScale::Double => OperandSize::Short,
        OperandScale::Quadruple => OperandSize::Quad,
    }
}

fn placeholder_for(size: OperandSize) -> i32 {
    match size {
        OperandSize::None => unreachable!(),
        OperandSize::Byte => K_JUMP_PLACEHOLDER_BYTE as i32,
        OperandSize::Short => {
            i32::from_le_bytes([K_JUMP_PLACEHOLDER_BYTE, K_JUMP_PLACEHOLDER_BYTE, 0, 0])
        }
        OperandSize::Quad => i32::from_le_bytes([
            K_JUMP_PLACEHOLDER_BYTE,
            K_JUMP_PLACEHOLDER_BYTE,
            K_JUMP_PLACEHOLDER_BYTE,
            0x7f,
        ]),
    }
}

/// `Bytecodes::ScaleForUnsignedOperand`
fn scale_for_unsigned(value: u32) -> OperandScale {
    if value <= u8::MAX as u32 {
        OperandScale::Single
    } else if value <= u16::MAX as u32 {
        OperandScale::Double
    } else {
        OperandScale::Quadruple
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::bytecode_register::Register;
    use crate::interpreter::constant_array_builder::{ConstantEntry, K_8BIT_CAPACITY};

    fn node(bytecode: Bytecode, operands: &[i32]) -> BytecodeNode {
        BytecodeNode::new(bytecode, operands)
    }

    #[test]
    fn a_bytecode_with_no_operands_is_one_byte() {
        let mut writer = BytecodeArrayWriter::new();
        writer.write(&node(Bytecode::LdaZero, &[]));
        assert_eq!(writer.bytecodes(), &[Bytecode::LdaZero.to_byte()]);
    }

    #[test]
    fn operands_are_little_endian_and_truncated_to_their_width() {
        let mut writer = BytecodeArrayWriter::new();
        writer.write(&node(Bytecode::LdaSmi, &[0x12]));
        writer.write(&node(Bytecode::LdaSmi, &[0x1234]));
        let bytes = writer.bytecodes();
        assert_eq!(bytes[0], Bytecode::LdaSmi.to_byte());
        assert_eq!(bytes[1], 0x12);
        assert_eq!(bytes[2], Bytecode::Wide.to_byte());
        assert_eq!(bytes[3], Bytecode::LdaSmi.to_byte());
        assert_eq!(bytes[4], 0x34, "low byte first");
        assert_eq!(bytes[5], 0x12);
    }

    #[test]
    fn negative_operands_are_written_as_twos_complement() {
        let mut writer = BytecodeArrayWriter::new();
        writer.write(&node(Bytecode::Ldar, &[Register::new(1).to_operand()]));
        assert_eq!(writer.bytecodes()[1], 0xff);
    }

    #[test]
    fn a_short_forward_jump_is_patched_in_place() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();
        writer.write_jump(&node(Bytecode::JumpIfFalse, &[0]), &mut label);
        assert_eq!(writer.unbound_jumps(), 1);
        let jump_offset = label.jump_offset();

        writer.write(&node(Bytecode::LdaZero, &[]));
        writer.bind_label(&mut label);

        assert!(label.is_bound());
        assert_eq!(writer.unbound_jumps(), 0);
        let bytes = writer.bytecodes();
        assert_eq!(
            bytes[jump_offset],
            Bytecode::JumpIfFalse.to_byte(),
            "the offset fitted, so the bytecode is unchanged"
        );
        assert_eq!(bytes[jump_offset + 1], 3);
        assert_eq!(writer.constant_array_builder().size(), 0);
    }

    #[test]
    fn an_unconditional_jump_makes_what_it_skips_unreachable() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();
        writer.write_jump(&node(Bytecode::Jump, &[0]), &mut label);
        writer.write(&node(Bytecode::LdaZero, &[]));
        assert_eq!(writer.bytecodes().len(), 2, "the LdaZero was dropped");
        writer.bind_label(&mut label);
        assert_eq!(
            writer.bytecodes()[1],
            2,
            "so the jump lands right after itself"
        );
    }

    #[test]
    fn a_long_forward_jump_becomes_the_constant_form() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();
        writer.write_jump(&node(Bytecode::JumpIfFalse, &[0]), &mut label);
        let jump_offset = label.jump_offset();

        for _ in 0..300 {
            writer.write(&node(Bytecode::LdaZero, &[]));
        }
        writer.bind_label(&mut label);

        let bytes = writer.bytecodes();
        assert_eq!(
            bytes[jump_offset],
            Bytecode::JumpIfFalseConstant.to_byte(),
            "the offset did not fit, so the jump should read it from the pool"
        );
        let index = bytes[jump_offset + 1] as usize;
        assert_eq!(
            writer.constant_array_builder().at(index),
            ConstantEntry::Smi(302),
            "the committed offset counts the jump itself"
        );
        assert!(
            index < K_8BIT_CAPACITY,
            "the index must fit the byte operand"
        );
        assert_eq!(writer.unbound_jumps(), 0);
    }

    #[test]
    fn a_backward_jump_needs_no_patching() {
        let mut writer = BytecodeArrayWriter::new();
        let mut header = BytecodeLoopHeader::new();
        writer.bind_loop_header(&mut header);
        assert_eq!(header.offset(), 0);

        writer.write(&node(Bytecode::LdaZero, &[]));
        writer.write_jump_loop(&node(Bytecode::JumpLoop, &[0, 0, 0]), &header);

        let bytes = writer.bytecodes();
        assert_eq!(bytes[0], Bytecode::LdaZero.to_byte());
        assert_eq!(bytes[1], Bytecode::JumpLoop.to_byte());
        assert_eq!(bytes[2], 1);
        assert_eq!(writer.unbound_jumps(), 0, "nothing was left unbound");
    }

    #[test]
    fn several_jumps_can_share_a_target() {
        let mut writer = BytecodeArrayWriter::new();
        let mut labels = BytecodeLabels::new();

        let first = labels.new_label();
        writer.write_jump(&node(Bytecode::JumpIfTrue, &[0]), labels.label_mut(first));
        writer.start_basic_block();
        let second = labels.new_label();
        writer.write_jump(&node(Bytecode::JumpIfFalse, &[0]), labels.label_mut(second));
        writer.start_basic_block();
        assert_eq!(writer.unbound_jumps(), 2);

        writer.write(&node(Bytecode::LdaZero, &[]));
        writer.bind_labels(&mut labels);

        assert!(labels.is_bound());
        assert_eq!(writer.unbound_jumps(), 0);
        let bytes = writer.bytecodes();
        assert_eq!(bytes[1], 5, "first jump: 2 + 2 + 1 bytes away");
        assert_eq!(bytes[3], 3, "second jump: 2 + 1 bytes away");
    }

    #[test]
    fn code_after_a_return_is_dropped() {
        let mut writer = BytecodeArrayWriter::new();
        writer.write(&node(Bytecode::LdaZero, &[]));
        writer.write(&node(Bytecode::Return, &[]));
        assert!(writer.exit_seen_in_block());

        writer.write(&node(Bytecode::LdaSmi, &[1]));
        writer.write(&node(Bytecode::Return, &[]));
        assert_eq!(writer.bytecodes().len(), 2);

        writer.start_basic_block();
        writer.write(&node(Bytecode::LdaSmi, &[1]));
        assert_eq!(writer.bytecodes().len(), 4);
    }

    #[test]
    fn binding_a_label_makes_the_block_live_again() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();
        writer.write_jump(&node(Bytecode::JumpIfFalse, &[0]), &mut label);
        writer.write(&node(Bytecode::Return, &[]));
        assert!(writer.exit_seen_in_block());

        writer.bind_label(&mut label);
        assert!(!writer.exit_seen_in_block());
        writer.write(&node(Bytecode::LdaZero, &[]));
        assert_eq!(
            *writer.bytecodes().last().unwrap(),
            Bytecode::LdaZero.to_byte()
        );
    }

    #[test]
    fn source_positions_are_recorded_against_offsets() {
        let mut writer = BytecodeArrayWriter::new();
        writer.write(&node(Bytecode::LdaZero, &[]).with_source_position(10));
        writer.write(&node(Bytecode::LdaSmi, &[1]));
        writer.write(&node(Bytecode::Return, &[]).with_source_position(20));
        assert_eq!(writer.source_position_table(), &[(0, 10), (3, 20)]);
    }

    #[test]
    #[should_panic]
    fn an_unbound_jump_is_refused_at_the_end() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();
        writer.write_jump(&node(Bytecode::Jump, &[0]), &mut label);
        writer.finish();
    }

    #[test]
    fn a_jump_over_a_wide_instruction_measures_from_the_opcode() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();

        for i in 0..K_8BIT_CAPACITY {
            writer.constant_array_builder_mut().insert_smi(i as i32);
        }

        writer.write_jump(&node(Bytecode::JumpIfFalse, &[0]), &mut label);
        let jump_offset = label.jump_offset();
        assert_eq!(
            writer.bytecodes()[jump_offset],
            Bytecode::Wide.to_byte(),
            "a 16-bit reservation forces a prefix"
        );

        writer.write(&node(Bytecode::LdaZero, &[]));
        writer.bind_label(&mut label);

        let bytes = writer.bytecodes();
        let operand = u16::from_le_bytes([bytes[jump_offset + 2], bytes[jump_offset + 3]]);
        assert_eq!(operand, 4, "the offset is measured from the opcode byte");
    }
}

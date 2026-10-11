//! Ported from `src/interpreter/bytecode-array-writer.h` /
//! `bytecode-array-writer.cc`.
//!
//! Turns [`BytecodeNode`]s into bytes, and resolves jumps.
//!
//! How a forward jump gets encoded
//! ------------------------------
//! The distance is unknown when the jump is emitted, but the width of its
//! operand has to be decided *now*. V8's answer is to reserve a constant
//! pool entry:
//!
//! 1. [`Self::write_jump`] calls `create_reserved_entry`, which claims a slot
//!    in the narrowest constant pool slice with room and reports how wide an
//!    index into that slice is. The jump is emitted with an operand that
//!    wide, filled with a placeholder.
//! 2. When the label binds, [`Self::patch_jump`] computes the real offset. If
//!    it fits in the operand already emitted, the reservation is discarded
//!    and the offset is written in place.
//! 3. If it does not fit, the reservation is committed: the offset goes into
//!    the constant pool and the jump is rewritten to its `*Constant` form,
//!    whose operand is the index -- which fits by construction, because the
//!    slot came from the slice with exactly that index width.
//!
//! Two details that are easy to get wrong, and both come straight from the
//! original:
//!
//! * **The offset is relative to the opcode byte, not to the start of the
//!   instruction.** A jump that needed a `Wide` prefix therefore has one
//!   subtracted from its delta, because the interpreter resumes from the
//!   opcode.
//! * **A label holds exactly one jump.** Several jumps to one target are a
//!   [`BytecodeLabels`] set, bound together.

use crate::interpreter::bytecode_label::{BytecodeLabel, BytecodeLabels, BytecodeLoopHeader};
use crate::interpreter::bytecode_node::BytecodeNode;
use crate::interpreter::bytecode_operands::{OperandScale, OperandSize};
use crate::interpreter::bytecodes::Bytecode;
use crate::interpreter::constant_array_builder::ConstantArrayBuilder;

/// `BytecodeArrayWriter::k8BitJumpPlaceholder` and friends. The value is
/// arbitrary -- it is always overwritten by patching -- but a recognisable
/// one makes a half-patched array obvious in a dump.
const K_JUMP_PLACEHOLDER_BYTE: u8 = 0x7f;

pub struct BytecodeArrayWriter {
    bytecodes: Vec<u8>,
    constant_array_builder: ConstantArrayBuilder,
    /// How many forward jumps are still waiting for their label. Must be zero
    /// before the array is handed over.
    unbound_jumps: usize,
    /// `BytecodeArrayWriter::exit_seen_in_block_`.
    ///
    /// Set once this basic block has emitted something control cannot pass
    /// (a `Return`, a `Throw`, an unconditional jump). Everything after it is
    /// unreachable, so the writer drops it rather than encoding dead bytes.
    exit_seen_in_block: bool,
    /// Offset -> source position, V8's `SourcePositionTableBuilder`.
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

    /// The offset the next bytecode will be written at.
    #[inline]
    pub fn offset(&self) -> usize {
        self.bytecodes.len()
    }

    pub fn unbound_jumps(&self) -> usize {
        self.unbound_jumps
    }

    /// Whether control can reach the next bytecode written.
    #[inline]
    pub fn exit_seen_in_block(&self) -> bool {
        self.exit_seen_in_block
    }

    /// `BytecodeArrayWriter::StartBasicBlock`: control can arrive here from
    /// somewhere else, so whatever was unreachable before is reachable now.
    pub fn start_basic_block(&mut self) {
        self.exit_seen_in_block = false;
    }

    /// `BytecodeArrayWriter::Write`
    pub fn write(&mut self, node: &BytecodeNode) {
        debug_assert!(!node.bytecode().is_jump(), "use write_jump");
        if self.exit_seen_in_block {
            // Unreachable: V8 calls this "eliding" and so does the comment in
            // `UpdateExitSeenInBlock`. Dropping it is not an optimisation,
            // it is what keeps the offsets honest after a `return`.
            return;
        }
        self.update_exit_seen_in_block(node.bytecode());
        self.emit_bytecode(node);
    }

    /// `BytecodeArrayWriter::WriteJump`: a forward jump to an unbound label.
    pub fn write_jump(&mut self, node: &BytecodeNode, label: &mut BytecodeLabel) {
        debug_assert!(node.bytecode().is_forward_jump());
        debug_assert_eq!(node.operand(0), 0, "the offset is filled in by patching");
        if self.exit_seen_in_block {
            return;
        }

        self.emit_jump(node, label);
        self.update_exit_seen_in_block(node.bytecode());
    }

    /// `BytecodeArrayWriter::WriteJumpLoop`: a backward jump to a header that
    /// is already bound, so the distance is known and nothing is patched.
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
            // Nothing jumped here -- the branch was elided as unreachable.
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

    /// `BytecodeLabels::Bind`: bind every jump in the set to here.
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
        // V8 does not start a basic block when the whole loop is dead.
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
            // Little-endian, truncated to the operand's width. A signed
            // operand is written as its two's complement, which the reader
            // sign-extends back.
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

        // Reserve a constant pool slot. Its slice's index width is the width
        // the operand must be emitted at -- see the module docs.
        let reserved = self
            .constant_array_builder
            .create_reserved_entry(operand_size_for_scale(node.operand_scale()));

        let mut node = *node;
        // The placeholder is as wide as the reservation demands, which is
        // what forces the instruction to that scale.
        node.update_operand0(placeholder_for(reserved));
        debug_assert_eq!(
            operand_size_for_scale(node.operand_scale()),
            reserved,
            "the placeholder must hold the operand at the reserved width"
        );
        self.emit_bytecode(&node);
    }

    /// `BytecodeArrayWriter::EmitJumpLoop`
    fn emit_jump_loop(&mut self, node: &BytecodeNode, loop_header: &BytecodeLoopHeader) {
        let current_offset = self.offset();
        let header_offset = loop_header.offset();
        debug_assert!(current_offset >= header_offset);

        let mut delta = (current_offset - header_offset) as i32;

        // This JumpLoop may itself need a prefix, and if it does, the offset
        // the interpreter jumps from moves forward by one byte. Both the
        // scale already implied by the other operands and the scale the delta
        // itself needs can force one.
        let mut node = *node;
        let emits_prefix = node.operand_scale() != OperandScale::Single
            || scale_for_unsigned(delta as u32) != OperandScale::Single;
        if emits_prefix {
            const K_PREFIX_BYTECODE_SIZE: i32 = 1;
            delta += K_PREFIX_BYTECODE_SIZE;
        }
        node.update_operand0(delta);
        debug_assert_eq!(
            node.operand_scale() != OperandScale::Single,
            emits_prefix,
            "the prefix prediction must match what the node ends up needing"
        );
        self.emit_bytecode(&node);
    }

    // -- patching -----------------------------------------------------------

    /// `BytecodeArrayWriter::PatchJump`
    ///
    /// `jump_location` is where the jump instruction starts, prefix included;
    /// `jump_target` is where it should land.
    fn patch_jump(&mut self, jump_target: usize, jump_location: usize) {
        let first_byte = Bytecode::from_byte(self.bytecodes[jump_location])
            .expect("a jump instruction at the recorded location");
        let mut delta = (jump_target - jump_location) as i32;
        let mut prefix_offset = 0;
        let mut operand_scale = OperandScale::Single;

        if first_byte.is_prefix_scaling_bytecode() {
            // The interpreter resumes from the opcode, not from the prefix,
            // so the offset it adds is one less.
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
        debug_assert!(delta > 0, "a forward jump goes forward");
        let operand_location = jump_location + 1;
        debug_assert_eq!(self.bytecodes[operand_location], K_JUMP_PLACEHOLDER_BYTE);

        if scale_for_unsigned(delta as u32) == OperandScale::Single {
            // It fits: cancel the reservation and jump directly.
            self.constant_array_builder
                .discard_reserved_entry(OperandSize::Byte);
            self.bytecodes[operand_location] = delta as u8;
            return;
        }
        // It does not: put the offset in the constant pool and switch the
        // jump to its `*Constant` form. The index is a byte by construction.
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
        // A 32-bit operand can hold any offset a bytecode array can have, so
        // the reservation is always discarded rather than committed.
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

    /// Hands over the encoded bytes. Asserts that every jump found its
    /// target, because a bytecode array with an unpatched jump would run
    /// into a placeholder.
    pub fn finish(self) -> (Vec<u8>, ConstantArrayBuilder, Vec<(usize, usize)>) {
        assert_eq!(
            self.unbound_jumps, 0,
            "{} jump(s) were never bound",
            self.unbound_jumps
        );
        assert_eq!(
            self.constant_array_builder.outstanding_reservations(),
            0,
            "a constant pool reservation was never resolved"
        );
        (
            self.bytecodes,
            self.constant_array_builder,
            self.source_position_table,
        )
    }
}

/// V8 writes `static_cast<OperandSize>(node->operand_scale())`: the two
/// enumerations are numbered so that Single/Double/Quadruple line up with
/// Byte/Short/Quad. Spelled out here rather than cast.
fn operand_size_for_scale(scale: OperandScale) -> OperandSize {
    match scale {
        OperandScale::Single => OperandSize::Byte,
        OperandScale::Double => OperandSize::Short,
        OperandScale::Quadruple => OperandSize::Quad,
    }
}

/// A placeholder wide enough to force `size`, built from the placeholder byte
/// repeated -- which is what V8's `k16BitJumpPlaceholder` is.
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
        // The second needs two bytes, so it is prefixed.
        assert_eq!(bytes[2], Bytecode::Wide.to_byte());
        assert_eq!(bytes[3], Bytecode::LdaSmi.to_byte());
        assert_eq!(bytes[4], 0x34, "low byte first");
        assert_eq!(bytes[5], 0x12);
    }

    #[test]
    fn negative_operands_are_written_as_twos_complement() {
        let mut writer = BytecodeArrayWriter::new();
        // Register r1 encodes as operand -1.
        writer.write(&node(Bytecode::Ldar, &[Register::new(1).to_operand()]));
        assert_eq!(writer.bytecodes()[1], 0xff);
    }

    #[test]
    fn a_short_forward_jump_is_patched_in_place() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();
        // A conditional jump, so the code it skips over is still reachable by
        // falling through -- an unconditional one would make it dead.
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
        // JumpIfFalse(2 bytes) + LdaZero(1) = the target is 3 bytes on.
        assert_eq!(bytes[jump_offset + 1], 3);
        // Nothing went into the constant pool.
        assert_eq!(writer.constant_array_builder().size(), 0);
    }

    #[test]
    fn an_unconditional_jump_makes_what_it_skips_unreachable() {
        // Which is why the test above uses a conditional one: after `Jump`
        // there is no fall-through path, so the writer drops what follows
        // until something branches back in.
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

        // Put more than 255 bytes between the jump and its target.
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
        // The JumpLoop starts one byte in, so the distance back is 1.
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
        // Both land on the same offset, from different distances.
        assert_eq!(bytes[1], 5, "first jump: 2 + 2 + 1 bytes away");
        assert_eq!(bytes[3], 3, "second jump: 2 + 1 bytes away");
    }

    #[test]
    fn code_after_a_return_is_dropped() {
        let mut writer = BytecodeArrayWriter::new();
        writer.write(&node(Bytecode::LdaZero, &[]));
        writer.write(&node(Bytecode::Return, &[]));
        assert!(writer.exit_seen_in_block());

        // Unreachable, so it never reaches the array.
        writer.write(&node(Bytecode::LdaSmi, &[1]));
        writer.write(&node(Bytecode::Return, &[]));
        assert_eq!(writer.bytecodes().len(), 2);

        // Until something can branch here again.
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

        // The label is a branch target, so what follows it is reachable.
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
    #[should_panic(expected = "never bound")]
    fn an_unbound_jump_is_refused_at_the_end() {
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();
        writer.write_jump(&node(Bytecode::Jump, &[0]), &mut label);
        writer.finish();
    }

    #[test]
    fn a_jump_over_a_wide_instruction_measures_from_the_opcode() {
        // The `delta -= 1` case: the jump itself needs a prefix, so the
        // offset the interpreter adds is relative to the opcode byte.
        let mut writer = BytecodeArrayWriter::new();
        let mut label = BytecodeLabel::new();

        // Fill the byte slice of the constant pool so the reservation has to
        // come from the 16-bit slice, forcing a Wide jump.
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
        // The instruction is Wide(1) + JumpIfFalse(1) + operand(2) = 4 bytes,
        // then LdaZero(1). The target is 5 bytes from the prefix -- but the
        // interpreter resumes from the opcode, so the stored offset is 4.
        let operand = u16::from_le_bytes([bytes[jump_offset + 2], bytes[jump_offset + 3]]);
        assert_eq!(operand, 4, "the offset is measured from the opcode byte");
    }
}

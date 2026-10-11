use crate::interpreter::bytecode_operands::OperandScale;
use crate::interpreter::bytecodes::Bytecode;

pub const K_MAX_OPERANDS: usize = 5;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct BytecodeNode {
    bytecode: Bytecode,
    operands: [i32; K_MAX_OPERANDS],
    operand_count: usize,
    operand_scale: OperandScale,
    source_position: Option<usize>,
}

impl BytecodeNode {
    pub fn new(bytecode: Bytecode, operands: &[i32]) -> BytecodeNode {
        assert_eq!(operands.len(), bytecode.number_of_operands());
        let mut stored = [0i32; K_MAX_OPERANDS];
        stored[..operands.len()].copy_from_slice(operands);
        BytecodeNode {
            bytecode,
            operands: stored,
            operand_count: operands.len(),
            operand_scale: bytecode.scale_for_operands(operands),
            source_position: None,
        }
    }

    pub fn with_source_position(mut self, position: usize) -> BytecodeNode {
        self.source_position = Some(position);
        self
    }

    #[inline]
    pub fn bytecode(self) -> Bytecode {
        self.bytecode
    }

    #[inline]
    pub fn operand(self, index: usize) -> i32 {
        debug_assert!(index < self.operand_count);
        self.operands[index]
    }

    #[inline]
    pub fn operand_count(self) -> usize {
        self.operand_count
    }

    #[inline]
    pub fn operands(&self) -> &[i32] {
        &self.operands[..self.operand_count]
    }

    #[inline]
    pub fn operand_scale(self) -> OperandScale {
        self.operand_scale
    }

    #[inline]
    pub fn source_position(self) -> Option<usize> {
        self.source_position
    }

    pub fn update_operand0(&mut self, value: i32) {
        debug_assert!(self.operand_count > 0);
        self.operands[0] = value;
        self.operand_scale = self.bytecode.scale_for_operands(self.operands());
    }

    pub fn set_bytecode(&mut self, bytecode: Bytecode) {
        debug_assert_eq!(bytecode.number_of_operands(), self.operand_count);
        self.bytecode = bytecode;
    }

    pub fn size(self) -> usize {
        let prefix = if self.operand_scale == OperandScale::Single {
            0
        } else {
            1
        };
        prefix + self.bytecode.size(self.operand_scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::bytecode_register::Register;

    #[test]
    fn the_scale_is_derived_from_the_operands() {
        let small = BytecodeNode::new(Bytecode::Ldar, &[Register::new(1).to_operand()]);
        assert_eq!(small.operand_scale(), OperandScale::Single);
        assert_eq!(small.size(), 2, "opcode + one byte");

        let large = BytecodeNode::new(Bytecode::Ldar, &[Register::new(1000).to_operand()]);
        assert_eq!(large.operand_scale(), OperandScale::Double);
        assert_eq!(large.size(), 4, "prefix + opcode + two bytes");
    }

    #[test]
    fn updating_a_jump_offset_can_widen_the_instruction() {
        let mut node = BytecodeNode::new(Bytecode::Jump, &[0]);
        assert_eq!(node.operand_scale(), OperandScale::Single);
        assert_eq!(node.size(), 2);

        node.update_operand0(1000);
        assert_eq!(node.operand_scale(), OperandScale::Double);
        assert_eq!(node.size(), 4);
        assert_eq!(node.operand(0), 1000);
    }

    #[test]
    #[should_panic]
    fn the_operand_count_must_match_the_bytecode() {
        BytecodeNode::new(Bytecode::Ldar, &[]);
    }

    #[test]
    fn source_positions_are_optional() {
        let node = BytecodeNode::new(Bytecode::LdaZero, &[]);
        assert_eq!(node.source_position(), None);
        assert_eq!(node.with_source_position(42).source_position(), Some(42));
    }
}

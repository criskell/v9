use crate::interpreter::bytecode_operands::{OperandScale, OperandSize, OperandType};
use crate::interpreter::bytecode_register::Register;
use crate::interpreter::bytecodes::Bytecode;

/// `BytecodeArrayIterator`
pub struct BytecodeArrayIterator<'a> {
    bytecodes: &'a [u8],
    offset: usize,
    bytecode: Bytecode,
    operand_scale: OperandScale,
    opcode_offset: usize,
}

impl<'a> BytecodeArrayIterator<'a> {
    pub fn new(bytecodes: &'a [u8]) -> BytecodeArrayIterator<'a> {
        let mut iterator = BytecodeArrayIterator {
            bytecodes,
            offset: 0,
            bytecode: Bytecode::Illegal,
            operand_scale: OperandScale::Single,
            opcode_offset: 0,
        };
        if !bytecodes.is_empty() {
            iterator.update_operand_scale();
        }
        iterator
    }

    /// `BytecodeArrayIterator::done`
    #[inline]
    pub fn done(&self) -> bool {
        self.offset >= self.bytecodes.len()
    }

    /// `BytecodeArrayIterator::current_bytecode`
    #[inline]
    pub fn current_bytecode(&self) -> Bytecode {
        self.bytecode
    }

    /// `BytecodeArrayIterator::current_offset`
    #[inline]
    pub fn current_offset(&self) -> usize {
        self.offset
    }

    #[inline]
    pub fn opcode_offset(&self) -> usize {
        self.opcode_offset
    }

    /// `BytecodeArrayIterator::current_operand_scale`
    #[inline]
    pub fn current_operand_scale(&self) -> OperandScale {
        self.operand_scale
    }

    pub fn current_bytecode_size(&self) -> usize {
        (self.opcode_offset - self.offset) + self.bytecode.size(self.operand_scale)
    }

    /// `BytecodeArrayIterator::Advance`
    pub fn advance(&mut self) {
        self.offset += self.current_bytecode_size();
        if !self.done() {
            self.update_operand_scale();
        }
    }

    pub fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
        if !self.done() {
            self.update_operand_scale();
        }
    }

    /// `BytecodeArrayIterator::UpdateOperandScale`
    fn update_operand_scale(&mut self) {
        let first = Bytecode::from_byte(self.bytecodes[self.offset]).unwrap_or(Bytecode::Illegal);
        if let Some(scale) = first.prefix_to_operand_scale() {
            self.operand_scale = scale;
            self.opcode_offset = self.offset + 1;
            self.bytecode = Bytecode::from_byte(self.bytecodes[self.opcode_offset])
                .unwrap_or(Bytecode::Illegal);
        } else {
            self.operand_scale = OperandScale::Single;
            self.opcode_offset = self.offset;
            self.bytecode = first;
        }
    }

    pub fn operand(&self, index: usize) -> i32 {
        let offset = self.opcode_offset + self.bytecode.operand_offset(index, self.operand_scale);
        let operand_type = self.bytecode.operand_type(index);
        let size = operand_type.size(self.operand_scale);
        let unsigned = operand_type.is_unsigned();
        read_operand(self.bytecodes, offset, size, unsigned)
    }

    /// `BytecodeArrayIterator::GetRegisterOperand`
    pub fn register_operand(&self, index: usize) -> Register {
        debug_assert!(self.bytecode.operand_type(index).is_register_operand_type());
        Register::from_operand(self.operand(index))
    }

    /// `BytecodeArrayIterator::GetIndexOperand`
    pub fn index_operand(&self, index: usize) -> usize {
        self.operand(index) as usize
    }

    /// `BytecodeArrayIterator::GetJumpTargetOffset`.
    /// backwards.
    pub fn jump_target_offset(&self) -> usize {
        debug_assert!(self.bytecode.is_jump());
        let delta = self.operand(0);
        if self.bytecode == Bytecode::JumpLoop {
            self.opcode_offset - delta as usize
        } else {
            self.opcode_offset + delta as usize
        }
    }
}

pub fn read_operand(bytes: &[u8], offset: usize, size: OperandSize, unsigned: bool) -> i32 {
    match size {
        OperandSize::None => 0,
        OperandSize::Byte => {
            let raw = bytes[offset];
            if unsigned {
                raw as i32
            } else {
                raw as i8 as i32
            }
        }
        OperandSize::Short => {
            let raw = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            if unsigned {
                raw as i32
            } else {
                raw as i16 as i32
            }
        }
        OperandSize::Quad => i32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]),
    }
}

pub fn disassemble(bytecodes: &[u8], constants: &[String]) -> String {
    let mut out = String::new();
    let mut iterator = BytecodeArrayIterator::new(bytecodes);
    while !iterator.done() {
        let bytecode = iterator.current_bytecode();
        out.push_str(&format!("{:>5} : ", iterator.current_offset()));

        let size = iterator.current_bytecode_size();
        let raw: Vec<String> = bytecodes
            [iterator.current_offset()..iterator.current_offset() + size]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        out.push_str(&format!("{:<20} ", raw.join(" ")));

        if iterator.current_operand_scale() != OperandScale::Single {
            out.push_str(&format!(
                "{} ",
                Bytecode::operand_scale_to_prefix(iterator.current_operand_scale())
                    .map(Bytecode::to_string)
                    .unwrap_or("")
            ));
        }
        out.push_str(bytecode.to_string());

        let mut operands = Vec::new();
        for index in 0..bytecode.number_of_operands() {
            let operand_type = bytecode.operand_type(index);
            let value = iterator.operand(index);
            operands.push(match operand_type {
                OperandType::Reg | OperandType::RegOut | OperandType::RegList => {
                    format!("{}", Register::from_operand(value))
                }
                OperandType::ConstantPoolIndex => match constants.get(value as usize) {
                    Some(description) => format!("[{value}] ({description})"),
                    None => format!("[{value}]"),
                },
                OperandType::FeedbackSlot => format!("#{value}"),
                _ => format!("{value}"),
            });
        }
        if !operands.is_empty() {
            out.push_str(&format!(" {}", operands.join(", ")));
        }

        if bytecode.is_jump() {
            out.push_str(&format!("  -> {}", iterator.jump_target_offset()));
        }
        out.push('\n');
        iterator.advance();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::bytecode_array_builder::BytecodeArrayBuilder;
    use crate::interpreter::bytecode_label::BytecodeLabel;
    use crate::parsing::token::Token;

    #[test]
    fn it_walks_instruction_by_instruction() {
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        builder.load_literal_smi(7);
        builder.store_accumulator_in_register(Register::new(0));
        builder.emit_return();
        let data = builder.to_bytecode_array();

        let mut iterator = BytecodeArrayIterator::new(&data.bytecodes);
        assert_eq!(iterator.current_bytecode(), Bytecode::LdaSmi);
        assert_eq!(iterator.operand(0), 7);
        iterator.advance();
        assert_eq!(iterator.current_bytecode(), Bytecode::Star);
        assert_eq!(iterator.register_operand(0), Register::new(0));
        iterator.advance();
        assert_eq!(iterator.current_bytecode(), Bytecode::Return);
        iterator.advance();
        assert!(iterator.done());
    }

    #[test]
    fn a_prefix_is_resolved_and_the_operand_read_at_the_wider_size() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        builder.load_literal_smi(1000);
        let data = builder.to_bytecode_array();

        let iterator = BytecodeArrayIterator::new(&data.bytecodes);
        assert_eq!(iterator.current_bytecode(), Bytecode::LdaSmi);
        assert_eq!(iterator.current_operand_scale(), OperandScale::Double);
        assert_eq!(iterator.operand(0), 1000);
        assert_eq!(iterator.current_offset(), 0);
        assert_eq!(iterator.opcode_offset(), 1, "the prefix comes first");
        assert_eq!(iterator.current_bytecode_size(), 4);
    }

    #[test]
    fn signed_operands_are_sign_extended() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        builder.load_literal_smi(-5);
        let data = builder.to_bytecode_array();
        let iterator = BytecodeArrayIterator::new(&data.bytecodes);
        assert_eq!(iterator.operand(0), -5);
    }

    #[test]
    fn a_register_operand_decodes_back_to_the_register() {
        let mut builder = BytecodeArrayBuilder::new(3, 1);
        builder.load_accumulator_with_register(builder.parameter(0));
        builder.store_accumulator_in_register(Register::new(0));
        let data = builder.to_bytecode_array();

        let mut iterator = BytecodeArrayIterator::new(&data.bytecodes);
        assert!(iterator.register_operand(0).is_parameter());
        assert_eq!(iterator.register_operand(0).to_parameter_index(), 1);
        iterator.advance();
        assert_eq!(iterator.register_operand(0), Register::new(0));
    }

    #[test]
    fn a_forward_jump_target_is_relative_to_the_opcode() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut label = BytecodeLabel::new();
        builder.load_true();
        builder.jump_if_false(&mut label);
        builder.load_literal_smi(1);
        builder.bind(&mut label);
        builder.emit_return();
        let data = builder.to_bytecode_array();

        let mut iterator = BytecodeArrayIterator::new(&data.bytecodes);
        iterator.advance(); // past LdaTrue
        assert_eq!(iterator.current_bytecode(), Bytecode::JumpIfFalse);
        assert_eq!(iterator.jump_target_offset(), 5);
    }

    #[test]
    fn a_loop_jump_target_goes_backwards() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut header = crate::interpreter::bytecode_label::BytecodeLoopHeader::new();
        builder.bind_loop_header(&mut header);
        builder.load_literal_smi(1);
        builder.jump_loop(&header);
        let data = builder.to_bytecode_array();

        let mut iterator = BytecodeArrayIterator::new(&data.bytecodes);
        iterator.advance();
        assert_eq!(iterator.current_bytecode(), Bytecode::JumpLoop);
        assert_eq!(iterator.jump_target_offset(), 0);
    }

    #[test]
    fn setting_the_offset_is_how_a_jump_is_taken() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        builder.load_literal_smi(1);
        builder.load_literal_smi(2);
        builder.emit_return();
        let data = builder.to_bytecode_array();

        let mut iterator = BytecodeArrayIterator::new(&data.bytecodes);
        iterator.set_offset(4);
        assert_eq!(iterator.current_bytecode(), Bytecode::Return);
        assert!(!iterator.done());
        iterator.advance();
        assert!(iterator.done());
    }

    #[test]
    fn disassembly_names_the_operands() {
        let mut builder = BytecodeArrayBuilder::new(1, 1);
        builder.load_literal_smi(2);
        builder.store_accumulator_in_register(Register::new(0));
        builder.load_literal_smi(3);
        builder.binary_operation(Token::Add, Register::new(0));
        builder.emit_return();
        let data = builder.to_bytecode_array();

        let listing = disassemble(&data.bytecodes, &[]);
        assert!(listing.contains("LdaSmi 2"), "{listing}");
        assert!(listing.contains("Star r0"), "{listing}");
        assert!(listing.contains("Add r0"), "{listing}");
        assert!(listing.contains("Return"), "{listing}");
        assert!(listing.starts_with("    0 : "), "{listing}");
    }

    #[test]
    fn disassembly_resolves_jump_targets_and_constants() {
        let mut builder = BytecodeArrayBuilder::new(1, 0);
        let mut label = BytecodeLabel::new();
        builder.load_literal_string("hi");
        builder.jump_if_to_boolean_false(&mut label);
        builder.load_literal_smi(1);
        builder.bind(&mut label);
        let data = builder.to_bytecode_array();

        let listing = disassemble(&data.bytecodes, &["\"hi\"".to_string()]);
        assert!(listing.contains("LdaConstant [0] (\"hi\")"), "{listing}");
        assert!(listing.contains("-> 6"), "{listing}");
    }
}

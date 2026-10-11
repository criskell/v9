use crate::interpreter::bytecode_operands::{
    ImplicitRegisterUse, OperandScale, OperandSize, OperandType,
};

macro_rules! bytecode_list {
    ($($name:ident, $use:expr $(, $operand:ident)* ;)*) => {
        /// `Bytecode`
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(u8)]
        pub enum Bytecode { $($name,)* }

        impl Bytecode {
            pub fn to_string(self) -> &'static str {
                match self { $(Bytecode::$name => stringify!($name),)* }
            }

            /// `Bytecodes::GetOperandTypes`
            pub fn operand_types(self) -> &'static [OperandType] {
                match self {
                    $(Bytecode::$name => &[$(OperandType::$operand),*],)*
                }
            }

            /// `Bytecodes::GetImplicitRegisterUse`
            pub fn implicit_register_use(self) -> ImplicitRegisterUse {
                match self { $(Bytecode::$name => $use,)* }
            }

            pub const ALL: &'static [Bytecode] = &[$(Bytecode::$name,)*];
        }
    };
}

use ImplicitRegisterUse as Use;

bytecode_list! {
    Wide,                     Use::NONE;
    ExtraWide,                Use::NONE;

    Ldar,                     Use::WRITE_ACCUMULATOR, Reg;
    LdaZero,                  Use::WRITE_ACCUMULATOR;
    LdaSmi,                   Use::WRITE_ACCUMULATOR, Imm;
    LdaUndefined,             Use::WRITE_ACCUMULATOR;
    LdaNull,                  Use::WRITE_ACCUMULATOR;
    LdaTheHole,               Use::WRITE_ACCUMULATOR;
    LdaTrue,                  Use::WRITE_ACCUMULATOR;
    LdaFalse,                 Use::WRITE_ACCUMULATOR;
    LdaConstant,              Use::WRITE_ACCUMULATOR, ConstantPoolIndex;
    LdaContextSlot,           Use::WRITE_ACCUMULATOR, Reg, ContextSlot, UImm;
    LdaCurrentContextSlot,    Use::WRITE_ACCUMULATOR, ContextSlot;

    Star,                     Use::READ_ACCUMULATOR, RegOut;
    Mov,                      Use::NONE, Reg, RegOut;
    PushContext,              Use::READ_ACCUMULATOR, RegOut;
    PopContext,               Use::NONE, Reg;

    TestReferenceEqual,       Use::READ_WRITE_ACCUMULATOR, Reg;
    TestUndetectable,         Use::READ_WRITE_ACCUMULATOR;
    TestNull,                 Use::READ_WRITE_ACCUMULATOR;
    TestUndefined,            Use::READ_WRITE_ACCUMULATOR;
    TestTypeOf,               Use::READ_WRITE_ACCUMULATOR, Flag8;

    // -- Globals
    LdaGlobal,                Use::WRITE_ACCUMULATOR, ConstantPoolIndex, FeedbackSlot;
    StaGlobal,                Use::READ_AND_CLOBBER_ACCUMULATOR, ConstantPoolIndex, FeedbackSlot;
    StaContextSlot,           Use::READ_ACCUMULATOR, Reg, ContextSlot, UImm;
    StaCurrentContextSlot,    Use::READ_ACCUMULATOR, ContextSlot;

    GetNamedProperty,         Use::WRITE_ACCUMULATOR, Reg, ConstantPoolIndex, FeedbackSlot;
    GetKeyedProperty,         Use::READ_WRITE_ACCUMULATOR, Reg, FeedbackSlot;
    SetNamedProperty,         Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, ConstantPoolIndex, FeedbackSlot;
    DefineNamedOwnProperty,   Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, ConstantPoolIndex, FeedbackSlot;
    SetKeyedProperty,         Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, Reg, FeedbackSlot;
    DefineKeyedOwnProperty,   Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, Reg, Flag8, FeedbackSlot;

    Add,                      Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    Sub,                      Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    Mul,                      Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    Div,                      Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    Mod,                      Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    Exp,                      Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    BitwiseOr,                Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    BitwiseXor,               Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    BitwiseAnd,               Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    ShiftLeft,                Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    ShiftRight,               Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    ShiftRightLogical,        Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;

    AddSmi,                   Use::READ_WRITE_ACCUMULATOR, Imm, EmbeddedFeedback;
    SubSmi,                   Use::READ_WRITE_ACCUMULATOR, Imm, EmbeddedFeedback;
    MulSmi,                   Use::READ_WRITE_ACCUMULATOR, Imm, EmbeddedFeedback;
    DivSmi,                   Use::READ_WRITE_ACCUMULATOR, Imm, EmbeddedFeedback;
    ModSmi,                   Use::READ_WRITE_ACCUMULATOR, Imm, EmbeddedFeedback;

    // -- Unary operators
    Inc,                      Use::READ_WRITE_ACCUMULATOR, EmbeddedFeedback;
    Dec,                      Use::READ_WRITE_ACCUMULATOR, EmbeddedFeedback;
    Negate,                   Use::READ_WRITE_ACCUMULATOR, EmbeddedFeedback;
    BitwiseNot,               Use::READ_WRITE_ACCUMULATOR, EmbeddedFeedback;
    ToBooleanLogicalNot,      Use::READ_WRITE_ACCUMULATOR;
    LogicalNot,               Use::READ_WRITE_ACCUMULATOR;
    TypeOf,                   Use::READ_WRITE_ACCUMULATOR, FeedbackSlot;
    DeletePropertyStrict,     Use::READ_WRITE_ACCUMULATOR, Reg;
    DeletePropertySloppy,     Use::READ_WRITE_ACCUMULATOR, Reg;

    // -- Calls
    CallAnyReceiver,          Use::WRITE_ACCUMULATOR, Reg, RegList, RegCount, FeedbackSlot;
    CallProperty,             Use::WRITE_ACCUMULATOR, Reg, RegList, RegCount, FeedbackSlot;
    CallUndefinedReceiver,    Use::WRITE_ACCUMULATOR, Reg, RegList, RegCount, FeedbackSlot;

    // -- Comparisons
    TestEqual,                Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    TestEqualStrict,          Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    TestLessThan,             Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    TestGreaterThan,          Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    TestLessThanOrEqual,      Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    TestGreaterThanOrEqual,   Use::READ_WRITE_ACCUMULATOR, Reg, EmbeddedFeedback;
    TestInstanceOf,           Use::READ_WRITE_ACCUMULATOR, Reg, FeedbackSlot;
    TestIn,                   Use::READ_WRITE_ACCUMULATOR, Reg, FeedbackSlot;

    // -- Conversions
    ToName,                   Use::READ_WRITE_ACCUMULATOR;
    ToNumber,                 Use::READ_WRITE_ACCUMULATOR, FeedbackSlot;
    ToString,                 Use::READ_WRITE_ACCUMULATOR;
    ToBoolean,                Use::READ_WRITE_ACCUMULATOR;

    // -- Literals
    CreateArrayLiteral,       Use::WRITE_ACCUMULATOR, ConstantPoolIndex, FeedbackSlot, Flag8;
    CreateEmptyArrayLiteral,  Use::WRITE_ACCUMULATOR, FeedbackSlot;
    CreateObjectLiteral,      Use::WRITE_ACCUMULATOR, ConstantPoolIndex, FeedbackSlot, Flag8;
    CreateEmptyObjectLiteral, Use::WRITE_ACCUMULATOR;
    CreateClosure,            Use::WRITE_ACCUMULATOR, ConstantPoolIndex, FeedbackSlot, Flag8;
    CreateFunctionContext,    Use::WRITE_ACCUMULATOR, ConstantPoolIndex, UImm;

    JumpLoop,                      Use::CLOBBER_ACCUMULATOR, UImm, Imm, FeedbackSlot;
    Jump,                          Use::NONE, UImm;
    JumpConstant,                  Use::NONE, ConstantPoolIndex;
    JumpIfNullConstant,            Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfNotNullConstant,         Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfUndefinedConstant,       Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfNotUndefinedConstant,    Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfUndefinedOrNullConstant, Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfTrueConstant,            Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfFalseConstant,           Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfToBooleanTrueConstant,   Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfToBooleanFalseConstant,  Use::READ_ACCUMULATOR, ConstantPoolIndex;
    JumpIfToBooleanTrue,           Use::READ_ACCUMULATOR, UImm;
    JumpIfToBooleanFalse,          Use::READ_ACCUMULATOR, UImm;
    JumpIfTrue,                    Use::READ_ACCUMULATOR, UImm;
    JumpIfFalse,                   Use::READ_ACCUMULATOR, UImm;
    JumpIfNull,                    Use::READ_ACCUMULATOR, UImm;
    JumpIfNotNull,                 Use::READ_ACCUMULATOR, UImm;
    JumpIfUndefined,               Use::READ_ACCUMULATOR, UImm;
    JumpIfNotUndefined,            Use::READ_ACCUMULATOR, UImm;
    JumpIfUndefinedOrNull,         Use::READ_ACCUMULATOR, UImm;

    Return,                   Use::READ_ACCUMULATOR;
    Throw,                    Use::READ_ACCUMULATOR;
    ReThrow,                  Use::READ_ACCUMULATOR;

    Debugger,                 Use::CLOBBER_ACCUMULATOR;
    Illegal,                  Use::NONE;
}

impl Bytecode {
    pub const LAST: Bytecode = Bytecode::Illegal;

    /// `Bytecodes::ToByte`
    #[inline]
    pub const fn to_byte(self) -> u8 {
        self as u8
    }

    /// `Bytecodes::FromByte`
    #[inline]
    pub fn from_byte(byte: u8) -> Option<Bytecode> {
        Bytecode::ALL.get(byte as usize).copied()
    }

    /// `Bytecodes::NumberOfOperands`
    #[inline]
    pub fn number_of_operands(self) -> usize {
        self.operand_types().len()
    }

    /// `Bytecodes::GetOperandType`
    #[inline]
    pub fn operand_type(self, index: usize) -> OperandType {
        self.operand_types()[index]
    }

    /// `Bytecodes::ReadsAccumulator`
    #[inline]
    pub fn reads_accumulator(self) -> bool {
        self.implicit_register_use().reads_accumulator()
    }

    /// `Bytecodes::WritesAccumulator`
    #[inline]
    pub fn writes_accumulator(self) -> bool {
        self.implicit_register_use().writes_accumulator()
    }

    /// `Bytecodes::ClobbersAccumulator`
    #[inline]
    pub fn clobbers_accumulator(self) -> bool {
        self.implicit_register_use().clobbers_accumulator()
    }

    /// `Bytecodes::GetOperandSize`
    #[inline]
    pub fn operand_size(self, index: usize, scale: OperandScale) -> OperandSize {
        self.operand_type(index).size(scale)
    }

    pub fn operand_offset(self, index: usize, scale: OperandScale) -> usize {
        let mut offset = 1; // the opcode itself
        for i in 0..index {
            offset += self.operand_size(i, scale).bytes();
        }
        offset
    }

    pub fn size(self, scale: OperandScale) -> usize {
        self.operand_offset(self.number_of_operands(), scale)
    }

    /// `Bytecodes::IsPrefixScalingBytecode`
    #[inline]
    pub fn is_prefix_scaling_bytecode(self) -> bool {
        matches!(self, Bytecode::Wide | Bytecode::ExtraWide)
    }

    /// `Bytecodes::PrefixBytecodeToOperandScale`
    pub fn prefix_to_operand_scale(self) -> Option<OperandScale> {
        match self {
            Bytecode::Wide => Some(OperandScale::Double),
            Bytecode::ExtraWide => Some(OperandScale::Quadruple),
            _ => None,
        }
    }

    /// `Bytecodes::OperandScaleToPrefixBytecode`
    pub fn operand_scale_to_prefix(scale: OperandScale) -> Option<Bytecode> {
        match scale {
            OperandScale::Single => None,
            OperandScale::Double => Some(Bytecode::Wide),
            OperandScale::Quadruple => Some(Bytecode::ExtraWide),
        }
    }

    /// `Bytecodes::IsJump`
    #[inline]
    pub fn is_jump(self) -> bool {
        self >= Bytecode::JumpLoop && self <= Bytecode::JumpIfUndefinedOrNull
    }

    #[inline]
    pub fn is_conditional_jump(self) -> bool {
        self >= Bytecode::JumpIfNullConstant && self <= Bytecode::JumpIfUndefinedOrNull
    }

    /// `Bytecodes::IsConditionalJumpImmediate`
    #[inline]
    pub fn is_conditional_jump_immediate(self) -> bool {
        self >= Bytecode::JumpIfToBooleanTrue && self <= Bytecode::JumpIfUndefinedOrNull
    }

    /// `Bytecodes::IsConditionalJumpConstant`
    #[inline]
    pub fn is_conditional_jump_constant(self) -> bool {
        self >= Bytecode::JumpIfNullConstant && self <= Bytecode::JumpIfToBooleanFalseConstant
    }

    /// `Bytecodes::IsUnconditionalJump`
    #[inline]
    pub fn is_unconditional_jump(self) -> bool {
        self >= Bytecode::JumpLoop && self <= Bytecode::JumpConstant
    }

    #[inline]
    pub fn is_jump_immediate(self) -> bool {
        matches!(self, Bytecode::Jump | Bytecode::JumpLoop) || self.is_conditional_jump_immediate()
    }

    #[inline]
    pub fn is_jump_constant(self) -> bool {
        self >= Bytecode::JumpConstant && self <= Bytecode::JumpIfToBooleanFalseConstant
    }

    #[inline]
    pub fn is_jump_if_to_boolean(self) -> bool {
        self >= Bytecode::JumpIfToBooleanTrueConstant && self <= Bytecode::JumpIfToBooleanFalse
    }

    #[inline]
    pub fn is_forward_jump(self) -> bool {
        self >= Bytecode::Jump && self <= Bytecode::JumpIfUndefinedOrNull
    }

    pub fn jump_with_constant_operand(self) -> Bytecode {
        match self {
            Bytecode::Jump => Bytecode::JumpConstant,
            Bytecode::JumpIfNull => Bytecode::JumpIfNullConstant,
            Bytecode::JumpIfNotNull => Bytecode::JumpIfNotNullConstant,
            Bytecode::JumpIfUndefined => Bytecode::JumpIfUndefinedConstant,
            Bytecode::JumpIfNotUndefined => Bytecode::JumpIfNotUndefinedConstant,
            Bytecode::JumpIfUndefinedOrNull => Bytecode::JumpIfUndefinedOrNullConstant,
            Bytecode::JumpIfTrue => Bytecode::JumpIfTrueConstant,
            Bytecode::JumpIfFalse => Bytecode::JumpIfFalseConstant,
            Bytecode::JumpIfToBooleanTrue => Bytecode::JumpIfToBooleanTrueConstant,
            Bytecode::JumpIfToBooleanFalse => Bytecode::JumpIfToBooleanFalseConstant,
            other => unreachable!("{other} has no constant-operand form"),
        }
    }

    /// `Bytecodes::Returns`
    #[inline]
    pub fn returns(self) -> bool {
        matches!(self, Bytecode::Return)
    }

    /// `Bytecodes::UnconditionallyThrows`
    #[inline]
    pub fn unconditionally_throws(self) -> bool {
        matches!(self, Bytecode::Throw | Bytecode::ReThrow)
    }

    #[inline]
    pub fn is_without_fall_through(self) -> bool {
        self.returns() || self.unconditionally_throws() || self.is_unconditional_jump()
    }

    /// `Bytecodes::IsAccumulatorLoadWithoutEffects`
    #[inline]
    pub fn is_accumulator_load_without_effects(self) -> bool {
        matches!(
            self,
            Bytecode::Ldar
                | Bytecode::LdaZero
                | Bytecode::LdaSmi
                | Bytecode::LdaUndefined
                | Bytecode::LdaNull
                | Bytecode::LdaTheHole
                | Bytecode::LdaTrue
                | Bytecode::LdaFalse
                | Bytecode::LdaConstant
        )
    }

    pub fn scale_for_operands(self, operands: &[i32]) -> OperandScale {
        debug_assert_eq!(operands.len(), self.number_of_operands());
        let mut scale = OperandScale::Single;
        for (index, value) in operands.iter().enumerate() {
            let operand_type = self.operand_type(index);
            if !operand_type.is_scalable() {
                continue;
            }
            let needed = if operand_type.is_unsigned() {
                scale_for_unsigned(*value as u32)
            } else {
                scale_for_signed(*value)
            };
            if needed > scale {
                scale = needed;
            }
        }
        scale
    }
}

fn scale_for_signed(value: i32) -> OperandScale {
    if value >= i8::MIN as i32 && value <= i8::MAX as i32 {
        OperandScale::Single
    } else if value >= i16::MIN as i32 && value <= i16::MAX as i32 {
        OperandScale::Double
    } else {
        OperandScale::Quadruple
    }
}

fn scale_for_unsigned(value: u32) -> OperandScale {
    if value <= u8::MAX as u32 {
        OperandScale::Single
    } else if value <= u16::MAX as u32 {
        OperandScale::Double
    } else {
        OperandScale::Quadruple
    }
}

impl std::fmt::Display for Bytecode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Bytecode::to_string(*self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_byte_encoding_round_trips_for_every_bytecode() {
        assert!(Bytecode::ALL.len() <= 256, "the opcode must fit in a byte");
        for (index, bytecode) in Bytecode::ALL.iter().enumerate() {
            assert_eq!(bytecode.to_byte() as usize, index);
            assert_eq!(Bytecode::from_byte(bytecode.to_byte()), Some(*bytecode));
        }
        assert_eq!(Bytecode::from_byte(255), None);
    }

    #[test]
    fn sizes_add_up_from_the_opcode_and_the_operands() {
        assert_eq!(Bytecode::LdaZero.size(OperandScale::Single), 1);
        assert_eq!(Bytecode::Return.size(OperandScale::Single), 1);

        assert_eq!(Bytecode::Ldar.size(OperandScale::Single), 2);
        assert_eq!(Bytecode::LdaSmi.size(OperandScale::Single), 2);

        assert_eq!(Bytecode::Add.size(OperandScale::Single), 3);

        // Three operands.
        assert_eq!(Bytecode::GetNamedProperty.size(OperandScale::Single), 4);
    }

    #[test]
    fn a_prefix_widens_scalable_operands_but_not_fixed_ones() {
        assert_eq!(Bytecode::Ldar.size(OperandScale::Single), 2);
        assert_eq!(Bytecode::Ldar.size(OperandScale::Double), 3);
        assert_eq!(Bytecode::Ldar.size(OperandScale::Quadruple), 5);

        assert_eq!(Bytecode::Add.size(OperandScale::Single), 3);
        assert_eq!(Bytecode::Add.size(OperandScale::Double), 4);
        assert_eq!(Bytecode::Add.size(OperandScale::Quadruple), 6);
    }

    #[test]
    fn operand_offsets_skip_the_preceding_operands() {
        let bytecode = Bytecode::GetNamedProperty;
        assert_eq!(bytecode.operand_offset(0, OperandScale::Single), 1);
        assert_eq!(bytecode.operand_offset(1, OperandScale::Single), 2);
        assert_eq!(bytecode.operand_offset(2, OperandScale::Single), 3);
        assert_eq!(bytecode.operand_offset(3, OperandScale::Single), 4);

        assert_eq!(bytecode.operand_offset(0, OperandScale::Double), 1);
        assert_eq!(bytecode.operand_offset(1, OperandScale::Double), 3);
        assert_eq!(bytecode.operand_offset(2, OperandScale::Double), 5);
    }

    #[test]
    fn prefixes_map_to_scales_and_back() {
        assert!(Bytecode::Wide.is_prefix_scaling_bytecode());
        assert!(Bytecode::ExtraWide.is_prefix_scaling_bytecode());
        assert!(!Bytecode::Ldar.is_prefix_scaling_bytecode());

        assert_eq!(
            Bytecode::Wide.prefix_to_operand_scale(),
            Some(OperandScale::Double)
        );
        assert_eq!(
            Bytecode::ExtraWide.prefix_to_operand_scale(),
            Some(OperandScale::Quadruple)
        );
        assert_eq!(Bytecode::Ldar.prefix_to_operand_scale(), None);

        assert_eq!(
            Bytecode::operand_scale_to_prefix(OperandScale::Single),
            None,
            "the single scale needs no prefix -- that is the point"
        );
        assert_eq!(
            Bytecode::operand_scale_to_prefix(OperandScale::Double),
            Some(Bytecode::Wide)
        );
    }

    #[test]
    fn jump_ranges_hold_exactly_the_jumps() {
        let jumps = [
            Bytecode::JumpLoop,
            Bytecode::Jump,
            Bytecode::JumpConstant,
            Bytecode::JumpIfNullConstant,
            Bytecode::JumpIfNotNullConstant,
            Bytecode::JumpIfUndefinedConstant,
            Bytecode::JumpIfNotUndefinedConstant,
            Bytecode::JumpIfUndefinedOrNullConstant,
            Bytecode::JumpIfTrueConstant,
            Bytecode::JumpIfFalseConstant,
            Bytecode::JumpIfToBooleanTrueConstant,
            Bytecode::JumpIfToBooleanFalseConstant,
            Bytecode::JumpIfToBooleanTrue,
            Bytecode::JumpIfToBooleanFalse,
            Bytecode::JumpIfTrue,
            Bytecode::JumpIfFalse,
            Bytecode::JumpIfNull,
            Bytecode::JumpIfNotNull,
            Bytecode::JumpIfUndefined,
            Bytecode::JumpIfNotUndefined,
            Bytecode::JumpIfUndefinedOrNull,
        ];
        for bytecode in Bytecode::ALL {
            assert_eq!(
                bytecode.is_jump(),
                jumps.contains(bytecode),
                "{bytecode} is on the wrong side of the jump range"
            );
        }

        assert!(Bytecode::Jump.is_unconditional_jump());
        assert!(Bytecode::JumpLoop.is_unconditional_jump());
        assert!(!Bytecode::JumpIfTrue.is_unconditional_jump());
        assert!(Bytecode::JumpIfTrue.is_conditional_jump());
        assert!(!Bytecode::Jump.is_conditional_jump());

        assert!(!Bytecode::JumpLoop.is_forward_jump());
        assert!(Bytecode::Jump.is_forward_jump());
        assert!(Bytecode::JumpIfFalse.is_forward_jump());

        for bytecode in Bytecode::ALL {
            if !bytecode.is_jump() {
                continue;
            }
            assert_ne!(
                bytecode.is_jump_immediate(),
                bytecode.is_jump_constant(),
                "{bytecode} must take exactly one of an offset or an index"
            );
            let operand = bytecode.operand_type(0);
            if bytecode.is_jump_constant() {
                assert_eq!(operand, OperandType::ConstantPoolIndex, "{bytecode}");
            } else {
                assert_eq!(operand, OperandType::UImm, "{bytecode}");
            }
        }

        for bytecode in Bytecode::ALL {
            if !bytecode.is_conditional_jump_immediate() && *bytecode != Bytecode::Jump {
                continue;
            }
            let constant = bytecode.jump_with_constant_operand();
            assert!(constant.is_jump_constant(), "{bytecode} -> {constant}");
            assert_eq!(
                constant.implicit_register_use(),
                bytecode.implicit_register_use(),
                "{bytecode} and {constant} must agree on the accumulator"
            );
            assert_eq!(
                constant.to_string(),
                format!("{bytecode}Constant"),
                "the naming convention is load-bearing for readers"
            );
        }

        assert!(Bytecode::JumpIfToBooleanTrue.is_jump_if_to_boolean());
        assert!(Bytecode::JumpIfToBooleanFalseConstant.is_jump_if_to_boolean());
        assert!(!Bytecode::JumpIfTrue.is_jump_if_to_boolean());
        assert!(!Bytecode::Jump.is_jump_if_to_boolean());
    }

    #[test]
    fn accumulator_use_matches_what_the_bytecodes_mean() {
        assert!(Bytecode::LdaZero.writes_accumulator());
        assert!(!Bytecode::LdaZero.reads_accumulator());
        assert!(Bytecode::Star.reads_accumulator());
        assert!(!Bytecode::Star.writes_accumulator());
        assert!(Bytecode::Add.reads_accumulator());
        assert!(Bytecode::Add.writes_accumulator());
        assert!(Bytecode::SetNamedProperty.reads_accumulator());
        assert!(!Bytecode::SetNamedProperty.writes_accumulator());
        assert!(Bytecode::SetNamedProperty.clobbers_accumulator());
        assert!(!Bytecode::Mov.reads_accumulator());
        assert!(!Bytecode::Mov.writes_accumulator());
        assert!(Bytecode::JumpIfFalse.reads_accumulator());
        assert!(!Bytecode::Jump.reads_accumulator());
        assert!(Bytecode::Return.reads_accumulator());
    }

    #[test]
    fn control_flow_predicates() {
        assert!(Bytecode::Return.returns());
        assert!(!Bytecode::Jump.returns());
        assert!(Bytecode::Throw.unconditionally_throws());
        assert!(Bytecode::ReThrow.unconditionally_throws());

        for bytecode in [
            Bytecode::Return,
            Bytecode::Throw,
            Bytecode::Jump,
            Bytecode::JumpLoop,
        ] {
            assert!(bytecode.is_without_fall_through(), "{bytecode}");
        }
        for bytecode in [Bytecode::Add, Bytecode::JumpIfTrue, Bytecode::Star] {
            assert!(!bytecode.is_without_fall_through(), "{bytecode}");
        }
    }

    #[test]
    fn the_scale_needed_follows_the_widest_scalable_operand() {
        assert_eq!(
            Bytecode::Ldar.scale_for_operands(&[Register::new(0).to_operand()]),
            OperandScale::Single
        );
        assert_eq!(
            Bytecode::Ldar.scale_for_operands(&[Register::new(1000).to_operand()]),
            OperandScale::Double
        );
        assert_eq!(
            Bytecode::LdaSmi.scale_for_operands(&[100_000]),
            OperandScale::Quadruple
        );
        assert_eq!(
            Bytecode::Add.scale_for_operands(&[0, 255]),
            OperandScale::Single
        );
        assert_eq!(
            Bytecode::GetNamedProperty.scale_for_operands(&[0, 300, 1]),
            OperandScale::Double
        );
    }

    use crate::interpreter::bytecode_register::Register;

    #[test]
    fn display_agrees_with_the_name_and_does_not_recurse() {
        for bytecode in Bytecode::ALL {
            assert_eq!(format!("{bytecode}"), Bytecode::to_string(*bytecode));
        }
        assert_eq!(format!("{}", Bytecode::Add), "Add");
        assert_eq!(
            format!("{}", Bytecode::JumpIfToBooleanFalse),
            "JumpIfToBooleanFalse"
        );
    }

    #[test]
    fn every_bytecode_has_a_name_and_a_consistent_operand_list() {
        for bytecode in Bytecode::ALL {
            assert!(!Bytecode::to_string(*bytecode).is_empty());
            assert_eq!(
                bytecode.number_of_operands(),
                bytecode.operand_types().len()
            );
            assert!(bytecode.size(OperandScale::Single) >= 1, "{bytecode}");
            for index in 0..bytecode.number_of_operands() {
                let operand = bytecode.operand_type(index);
                if operand.is_register_output_operand_type() {
                    assert!(
                        !bytecode.writes_accumulator() || bytecode == &Bytecode::Mov,
                        "{bytecode} writes both a register and the accumulator"
                    );
                }
            }
        }
    }
}

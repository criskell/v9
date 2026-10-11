//! Ported from `src/interpreter/bytecodes.h` / `bytecodes.cc`.
//!
//! The instruction set Ignition executes. A register machine with an
//! accumulator: most bytecodes take their second operand implicitly from the
//! accumulator and leave their result there, which is why `a + b` is
//! `Ldar a; Add b` and not a three-operand instruction.
//!
//! The declaration below is V8's `BYTECODE_LIST` for the subset `vx`
//! implements -- each entry is a name, how it uses the accumulator, and its
//! explicit operand types, transcribed from V8. The *order* matters for the
//! same reason the token enum's does: `IsJump` and friends are range checks.
//!
//! Encoding
//! --------
//! One opcode byte, then the operands, each as wide as its type says. Operand
//! widths scale together: if any scalable operand does not fit in a byte, the
//! whole bytecode is prefixed with [`Bytecode::Wide`] or
//! [`Bytecode::ExtraWide`] and every scalable operand in it doubles or
//! quadruples. The common case therefore pays one byte per operand.
//!
//! Deviations, tracked in `PORTING.md`: no debug-break bytecodes, no context
//! slots (no closures yet), no `*Constant` jump variants (a jump whose offset
//! lives in the constant pool, for jumps too long for a quadruple operand),
//! no generators, no `for...in`, no class or template literals, no runtime
//! calls or intrinsics.

use crate::interpreter::bytecode_operands::{
    ImplicitRegisterUse, OperandScale, OperandSize, OperandType,
};

/// `BYTECODE_LIST`: name, accumulator use, operand types.
macro_rules! bytecode_list {
    ($($name:ident, $use:expr $(, $operand:ident)* ;)*) => {
        /// `Bytecode`
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(u8)]
        pub enum Bytecode { $($name,)* }

        impl Bytecode {
            /// `Bytecodes::ToString`.
            ///
            /// Note this shadows `ToString::to_string` for a `Bytecode`
            /// value but not for a reference, which is V8's name at the cost
            /// of a Rust sharp edge; `Display` is implemented in terms of it.
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

            /// Every bytecode, in declaration order.
            pub const ALL: &'static [Bytecode] = &[$(Bytecode::$name,)*];
        }
    };
}

use ImplicitRegisterUse as Use;

bytecode_list! {
    // Extended width operands. These are prefixes, not instructions: they
    // scale the operands of the bytecode that follows.
    Wide,                     Use::NONE;
    ExtraWide,                Use::NONE;

    // -- Loading the accumulator
    Ldar,                     Use::WRITE_ACCUMULATOR, Reg;
    LdaZero,                  Use::WRITE_ACCUMULATOR;
    LdaSmi,                   Use::WRITE_ACCUMULATOR, Imm;
    LdaUndefined,             Use::WRITE_ACCUMULATOR;
    LdaNull,                  Use::WRITE_ACCUMULATOR;
    LdaTheHole,               Use::WRITE_ACCUMULATOR;
    LdaTrue,                  Use::WRITE_ACCUMULATOR;
    LdaFalse,                 Use::WRITE_ACCUMULATOR;
    LdaConstant,              Use::WRITE_ACCUMULATOR, ConstantPoolIndex;
    // Captured variables. The `Reg` holds the context to start from and the
    // `UImm` is the depth -- how many `previous` links to walk. Depth zero
    // is overwhelmingly the common case, so it gets its own operand-free
    // form.
    LdaContextSlot,           Use::WRITE_ACCUMULATOR, Reg, ContextSlot, UImm;
    LdaCurrentContextSlot,    Use::WRITE_ACCUMULATOR, ContextSlot;

    // -- Register loads
    Star,                     Use::READ_ACCUMULATOR, RegOut;
    Mov,                      Use::NONE, Reg, RegOut;
    // `PushContext` saves the current context into a register and installs
    // the one in the accumulator; `PopContext` puts the saved one back.
    PushContext,              Use::READ_ACCUMULATOR, RegOut;
    PopContext,               Use::NONE, Reg;

    // -- Side-effect-free tests
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

    // -- Property access. The feedback slot is what an inline cache keys off.
    GetNamedProperty,         Use::WRITE_ACCUMULATOR, Reg, ConstantPoolIndex, FeedbackSlot;
    GetKeyedProperty,         Use::READ_WRITE_ACCUMULATOR, Reg, FeedbackSlot;
    SetNamedProperty,         Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, ConstantPoolIndex, FeedbackSlot;
    DefineNamedOwnProperty,   Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, ConstantPoolIndex, FeedbackSlot;
    SetKeyedProperty,         Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, Reg, FeedbackSlot;
    DefineKeyedOwnProperty,   Use::READ_AND_CLOBBER_ACCUMULATOR, Reg, Reg, Flag8, FeedbackSlot;

    // -- Binary operators. `acc = reg <op> acc`: the left operand comes from
    //    a register and the right from the accumulator.
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

    // -- Binary operators with a Smi immediate, which save a register and a
    //    load for the overwhelmingly common `x + 1`.
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

    // -- Control flow.
    //
    //    The order is V8's, and every jump predicate below is a range check
    //    over it. Each conditional jump comes in two forms: one whose operand
    //    is the offset itself (`UImm`) and one whose operand is a constant
    //    pool index holding the offset (`ConstantPoolIndex`).
    //
    //    The second form is not for jumps that are "too long". It exists
    //    because a *forward* jump has to be emitted before its distance is
    //    known, so its operand width must be chosen in advance. The writer
    //    reserves a constant pool entry to fix that width; if the real offset
    //    turns out not to fit, it commits the reservation and rewrites the
    //    bytecode to the `*Constant` form, whose index fits by construction.
    //    See `bytecode_array_writer`.
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

    /// `Bytecodes::GetOperandOffset`: how far into the bytecode operand
    /// `index` starts, counting the opcode byte.
    pub fn operand_offset(self, index: usize, scale: OperandScale) -> usize {
        let mut offset = 1; // the opcode itself
        for i in 0..index {
            offset += self.operand_size(i, scale).bytes();
        }
        offset
    }

    /// `Bytecodes::Size`: total length in bytes, excluding any prefix.
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

    /// `Bytecodes::IsConditionalJump`, either operand form.
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

    /// `Bytecodes::IsJumpImmediate`: the offset is the operand.
    #[inline]
    pub fn is_jump_immediate(self) -> bool {
        matches!(self, Bytecode::Jump | Bytecode::JumpLoop) || self.is_conditional_jump_immediate()
    }

    /// `Bytecodes::IsJumpConstant`: the operand is a constant pool index
    /// holding the offset.
    #[inline]
    pub fn is_jump_constant(self) -> bool {
        self >= Bytecode::JumpConstant && self <= Bytecode::JumpIfToBooleanFalseConstant
    }

    /// `Bytecodes::IsJumpIfToBoolean`: jumps that coerce the accumulator to a
    /// boolean themselves, so the generator does not have to emit a
    /// `ToBoolean` before every `if`.
    #[inline]
    pub fn is_jump_if_to_boolean(self) -> bool {
        self >= Bytecode::JumpIfToBooleanTrueConstant && self <= Bytecode::JumpIfToBooleanFalse
    }

    /// `Bytecodes::IsForwardJump`: everything but `JumpLoop`, which is the
    /// only backwards jump the generator emits -- and the reason it is a
    /// separate bytecode is so the interpreter knows where to check for
    /// interrupts and where to consider tiering up.
    #[inline]
    pub fn is_forward_jump(self) -> bool {
        self >= Bytecode::Jump && self <= Bytecode::JumpIfUndefinedOrNull
    }

    /// `Bytecodes::GetJumpWithConstantOperand`: the `*Constant` form of a
    /// jump whose offset did not fit in its reserved operand.
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

    /// Whether control can continue past this bytecode. The bytecode writer
    /// uses it to know that the code after a `Return` is unreachable.
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

    /// The scale a bytecode needs to encode the given operand values.
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
        // Called as an associated function on purpose. `self.to_string()`
        // would resolve to `ToString::to_string`, which is this impl --
        // method lookup prefers the trait method because its receiver is
        // already `&Bytecode` and the inherent one needs a deref.
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
        // No operands: just the opcode byte.
        assert_eq!(Bytecode::LdaZero.size(OperandScale::Single), 1);
        assert_eq!(Bytecode::Return.size(OperandScale::Single), 1);

        // One byte-sized operand.
        assert_eq!(Bytecode::Ldar.size(OperandScale::Single), 2);
        assert_eq!(Bytecode::LdaSmi.size(OperandScale::Single), 2);

        // Two operands, one of which is fixed-width feedback.
        assert_eq!(Bytecode::Add.size(OperandScale::Single), 3);

        // Three operands.
        assert_eq!(Bytecode::GetNamedProperty.size(OperandScale::Single), 4);
    }

    #[test]
    fn a_prefix_widens_scalable_operands_but_not_fixed_ones() {
        // Ldar's single register operand doubles.
        assert_eq!(Bytecode::Ldar.size(OperandScale::Single), 2);
        assert_eq!(Bytecode::Ldar.size(OperandScale::Double), 3);
        assert_eq!(Bytecode::Ldar.size(OperandScale::Quadruple), 5);

        // Add is Reg (scalable) + EmbeddedFeedback (fixed byte), so only the
        // register grows: 1 + 1 + 1 -> 1 + 2 + 1 -> 1 + 4 + 1.
        assert_eq!(Bytecode::Add.size(OperandScale::Single), 3);
        assert_eq!(Bytecode::Add.size(OperandScale::Double), 4);
        assert_eq!(Bytecode::Add.size(OperandScale::Quadruple), 6);
    }

    #[test]
    fn operand_offsets_skip_the_preceding_operands() {
        // GetNamedProperty reg, name, slot
        let bytecode = Bytecode::GetNamedProperty;
        assert_eq!(bytecode.operand_offset(0, OperandScale::Single), 1);
        assert_eq!(bytecode.operand_offset(1, OperandScale::Single), 2);
        assert_eq!(bytecode.operand_offset(2, OperandScale::Single), 3);
        assert_eq!(bytecode.operand_offset(3, OperandScale::Single), 4);

        // Doubled, every operand is two bytes wide.
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

        // JumpLoop is the only backward jump, which is why it is its own
        // bytecode: it is where the interrupt check lives.
        assert!(!Bytecode::JumpLoop.is_forward_jump());
        assert!(Bytecode::Jump.is_forward_jump());
        assert!(Bytecode::JumpIfFalse.is_forward_jump());

        // The two operand forms partition the jumps.
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

        // Every immediate conditional jump has a constant counterpart, and
        // the pair agrees on everything but the operand.
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

        // The ToBoolean-coercing jumps are their own contiguous run.
        assert!(Bytecode::JumpIfToBooleanTrue.is_jump_if_to_boolean());
        assert!(Bytecode::JumpIfToBooleanFalseConstant.is_jump_if_to_boolean());
        assert!(!Bytecode::JumpIfTrue.is_jump_if_to_boolean());
        assert!(!Bytecode::Jump.is_jump_if_to_boolean());
    }

    #[test]
    fn accumulator_use_matches_what_the_bytecodes_mean() {
        // A load writes it without reading it.
        assert!(Bytecode::LdaZero.writes_accumulator());
        assert!(!Bytecode::LdaZero.reads_accumulator());
        // Star reads it and writes a register.
        assert!(Bytecode::Star.reads_accumulator());
        assert!(!Bytecode::Star.writes_accumulator());
        // A binary operator does both: `acc = reg op acc`.
        assert!(Bytecode::Add.reads_accumulator());
        assert!(Bytecode::Add.writes_accumulator());
        // A store reads it and leaves it unspecified.
        assert!(Bytecode::SetNamedProperty.reads_accumulator());
        assert!(!Bytecode::SetNamedProperty.writes_accumulator());
        assert!(Bytecode::SetNamedProperty.clobbers_accumulator());
        // Mov touches neither.
        assert!(!Bytecode::Mov.reads_accumulator());
        assert!(!Bytecode::Mov.writes_accumulator());
        // A conditional jump reads it to decide.
        assert!(Bytecode::JumpIfFalse.reads_accumulator());
        assert!(!Bytecode::Jump.reads_accumulator());
        // And Return reads the value being returned.
        assert!(Bytecode::Return.reads_accumulator());
    }

    #[test]
    fn control_flow_predicates() {
        assert!(Bytecode::Return.returns());
        assert!(!Bytecode::Jump.returns());
        assert!(Bytecode::Throw.unconditionally_throws());
        assert!(Bytecode::ReThrow.unconditionally_throws());

        // What ends a basic block.
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
        // Ldar r0: fits in a byte.
        assert_eq!(
            Bytecode::Ldar.scale_for_operands(&[Register::new(0).to_operand()]),
            OperandScale::Single
        );
        // Ldar r1000: the register operand needs two bytes.
        assert_eq!(
            Bytecode::Ldar.scale_for_operands(&[Register::new(1000).to_operand()]),
            OperandScale::Double
        );
        // A large Smi immediate needs four.
        assert_eq!(
            Bytecode::LdaSmi.scale_for_operands(&[100_000]),
            OperandScale::Quadruple
        );
        // A fixed operand never forces a prefix, however large.
        assert_eq!(
            Bytecode::Add.scale_for_operands(&[0, 255]),
            OperandScale::Single
        );
        // The widest operand wins.
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
            // The size must be at least the opcode byte.
            assert!(bytecode.size(OperandScale::Single) >= 1, "{bytecode}");
            // A register output operand implies the bytecode writes somewhere
            // other than the accumulator.
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

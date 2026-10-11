#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum OperandScale {
    Single = 1,
    Double = 2,
    Quadruple = 4,
}

impl OperandScale {
    pub const LAST: OperandScale = OperandScale::Quadruple;

    #[inline]
    pub fn factor(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum OperandSize {
    None = 0,
    Byte = 1,
    Short = 2,
    Quad = 4,
}

impl OperandSize {
    #[inline]
    pub fn bytes(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OperandTypeInfo {
    None,
    ScalableSignedByte,
    ScalableUnsignedByte,
    FixedUnsignedByte,
    FixedUnsignedShort,
}

impl OperandTypeInfo {
    pub fn is_scalable(self) -> bool {
        matches!(
            self,
            OperandTypeInfo::ScalableSignedByte | OperandTypeInfo::ScalableUnsignedByte
        )
    }

    pub fn is_unsigned(self) -> bool {
        matches!(
            self,
            OperandTypeInfo::ScalableUnsignedByte
                | OperandTypeInfo::FixedUnsignedByte
                | OperandTypeInfo::FixedUnsignedShort
        )
    }

    pub fn unscaled_size(self) -> OperandSize {
        match self {
            OperandTypeInfo::None => OperandSize::None,
            OperandTypeInfo::ScalableSignedByte
            | OperandTypeInfo::ScalableUnsignedByte
            | OperandTypeInfo::FixedUnsignedByte => OperandSize::Byte,
            OperandTypeInfo::FixedUnsignedShort => OperandSize::Short,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum OperandType {
    None,
    Flag8,
    Flag16,
    IntrinsicId,
    RuntimeId,
    NativeContextIndex,
    AbortReason,
    EmbeddedFeedback,
    ConstantPoolIndex,
    FeedbackSlot,
    ContextSlot,
    CoverageSlot,
    UImm,
    RegCount,
    Imm,
    Reg,
    RegList,
    RegPair,
    RegOut,
    RegOutList,
    RegOutPair,
    RegOutTriple,
    RegInOut,
}

impl OperandType {
    pub fn type_info(self) -> OperandTypeInfo {
        use OperandType::*;
        match self {
            None => OperandTypeInfo::None,
            Flag8 | IntrinsicId | NativeContextIndex | AbortReason | EmbeddedFeedback => {
                OperandTypeInfo::FixedUnsignedByte
            }
            Flag16 | RuntimeId => OperandTypeInfo::FixedUnsignedShort,
            ConstantPoolIndex | FeedbackSlot | ContextSlot | CoverageSlot | UImm | RegCount => {
                OperandTypeInfo::ScalableUnsignedByte
            }
            Imm | Reg | RegList | RegPair | RegOut | RegOutList | RegOutPair | RegOutTriple
            | RegInOut => OperandTypeInfo::ScalableSignedByte,
        }
    }

    /// `Bytecodes::IsRegisterOperandType`
    #[inline]
    pub fn is_register_operand_type(self) -> bool {
        self >= OperandType::Reg
    }

    /// `Bytecodes::IsRegisterInputOperandType`
    #[inline]
    pub fn is_register_input_operand_type(self) -> bool {
        matches!(
            self,
            OperandType::Reg | OperandType::RegList | OperandType::RegPair | OperandType::RegInOut
        )
    }

    /// `Bytecodes::IsRegisterOutputOperandType`
    #[inline]
    pub fn is_register_output_operand_type(self) -> bool {
        matches!(
            self,
            OperandType::RegOut
                | OperandType::RegOutList
                | OperandType::RegOutPair
                | OperandType::RegOutTriple
                | OperandType::RegInOut
        )
    }

    #[inline]
    pub fn is_unsigned(self) -> bool {
        self.type_info().is_unsigned()
    }

    #[inline]
    pub fn is_scalable(self) -> bool {
        self.type_info().is_scalable()
    }

    pub fn size(self, scale: OperandScale) -> OperandSize {
        let info = self.type_info();
        let unscaled = info.unscaled_size();
        if !info.is_scalable() {
            return unscaled;
        }
        match unscaled.bytes() * scale.factor() {
            0 => OperandSize::None,
            1 => OperandSize::Byte,
            2 => OperandSize::Short,
            _ => OperandSize::Quad,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ImplicitRegisterUse(u8);

impl ImplicitRegisterUse {
    pub const NONE: ImplicitRegisterUse = ImplicitRegisterUse(0);
    pub const READ_ACCUMULATOR: ImplicitRegisterUse = ImplicitRegisterUse(1 << 0);
    pub const WRITE_ACCUMULATOR: ImplicitRegisterUse = ImplicitRegisterUse(1 << 1);
    pub const CLOBBER_ACCUMULATOR: ImplicitRegisterUse = ImplicitRegisterUse(1 << 2);
    pub const WRITE_SHORT_STAR: ImplicitRegisterUse = ImplicitRegisterUse(1 << 3);

    pub const READ_WRITE_ACCUMULATOR: ImplicitRegisterUse =
        ImplicitRegisterUse(Self::READ_ACCUMULATOR.0 | Self::WRITE_ACCUMULATOR.0);
    pub const READ_AND_CLOBBER_ACCUMULATOR: ImplicitRegisterUse =
        ImplicitRegisterUse(Self::READ_ACCUMULATOR.0 | Self::CLOBBER_ACCUMULATOR.0);
    pub const READ_ACCUMULATOR_WRITE_SHORT_STAR: ImplicitRegisterUse =
        ImplicitRegisterUse(Self::READ_ACCUMULATOR.0 | Self::WRITE_SHORT_STAR.0);

    #[inline]
    pub const fn reads_accumulator(self) -> bool {
        self.0 & Self::READ_ACCUMULATOR.0 != 0
    }

    #[inline]
    pub const fn writes_accumulator(self) -> bool {
        self.0 & Self::WRITE_ACCUMULATOR.0 != 0
    }

    #[inline]
    pub const fn clobbers_accumulator(self) -> bool {
        self.0 & Self::CLOBBER_ACCUMULATOR.0 != 0
    }

    #[inline]
    pub const fn writes_short_star(self) -> bool {
        self.0 & Self::WRITE_SHORT_STAR.0 != 0
    }

    #[inline]
    pub const fn writes_or_clobbers_accumulator(self) -> bool {
        self.writes_accumulator() || self.clobbers_accumulator()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_widens_scalable_operands_only() {
        assert_eq!(
            OperandType::Reg.size(OperandScale::Single),
            OperandSize::Byte
        );
        assert_eq!(
            OperandType::Reg.size(OperandScale::Double),
            OperandSize::Short
        );
        assert_eq!(
            OperandType::Reg.size(OperandScale::Quadruple),
            OperandSize::Quad
        );

        for operand in [
            OperandType::ConstantPoolIndex,
            OperandType::FeedbackSlot,
            OperandType::UImm,
            OperandType::Imm,
        ] {
            assert_eq!(operand.size(OperandScale::Single), OperandSize::Byte);
            assert_eq!(operand.size(OperandScale::Quadruple), OperandSize::Quad);
        }

        assert_eq!(
            OperandType::Flag8.size(OperandScale::Single),
            OperandSize::Byte
        );
        assert_eq!(
            OperandType::Flag8.size(OperandScale::Quadruple),
            OperandSize::Byte,
            "a fixed operand ignores the prefix"
        );
        assert_eq!(
            OperandType::RuntimeId.size(OperandScale::Quadruple),
            OperandSize::Short
        );
        assert_eq!(
            OperandType::EmbeddedFeedback.size(OperandScale::Double),
            OperandSize::Byte
        );
    }

    #[test]
    fn signedness_matches_what_the_operand_holds() {
        assert!(!OperandType::Reg.is_unsigned());
        assert!(!OperandType::RegOut.is_unsigned());
        assert!(!OperandType::Imm.is_unsigned());
        assert!(OperandType::ConstantPoolIndex.is_unsigned());
        assert!(OperandType::FeedbackSlot.is_unsigned());
        assert!(OperandType::UImm.is_unsigned());
        assert!(OperandType::Flag8.is_unsigned());
    }

    #[test]
    fn register_operand_ranges_are_range_checks() {
        let registers = [
            OperandType::Reg,
            OperandType::RegList,
            OperandType::RegPair,
            OperandType::RegOut,
            OperandType::RegOutList,
            OperandType::RegOutPair,
            OperandType::RegOutTriple,
            OperandType::RegInOut,
        ];
        for operand in registers {
            assert!(operand.is_register_operand_type(), "{operand:?}");
        }
        for operand in [
            OperandType::None,
            OperandType::Flag8,
            OperandType::ConstantPoolIndex,
            OperandType::FeedbackSlot,
            OperandType::Imm,
            OperandType::UImm,
        ] {
            assert!(!operand.is_register_operand_type(), "{operand:?}");
        }

        assert!(OperandType::Reg.is_register_input_operand_type());
        assert!(!OperandType::Reg.is_register_output_operand_type());
        assert!(OperandType::RegOut.is_register_output_operand_type());
        assert!(!OperandType::RegOut.is_register_input_operand_type());
        assert!(OperandType::RegInOut.is_register_input_operand_type());
        assert!(OperandType::RegInOut.is_register_output_operand_type());
    }

    #[test]
    fn implicit_register_use_combinations() {
        let read_write = ImplicitRegisterUse::READ_WRITE_ACCUMULATOR;
        assert!(read_write.reads_accumulator());
        assert!(read_write.writes_accumulator());
        assert!(!read_write.clobbers_accumulator());

        let read_clobber = ImplicitRegisterUse::READ_AND_CLOBBER_ACCUMULATOR;
        assert!(read_clobber.reads_accumulator());
        assert!(!read_clobber.writes_accumulator());
        assert!(read_clobber.clobbers_accumulator());
        assert!(
            read_clobber.writes_or_clobbers_accumulator(),
            "either way the old value is gone"
        );

        let none = ImplicitRegisterUse::NONE;
        assert!(!none.reads_accumulator());
        assert!(!none.writes_or_clobbers_accumulator());
    }

    #[test]
    fn operand_scale_factors() {
        assert_eq!(OperandScale::Single.factor(), 1);
        assert_eq!(OperandScale::Double.factor(), 2);
        assert_eq!(OperandScale::Quadruple.factor(), 4);
        assert_eq!(OperandScale::LAST, OperandScale::Quadruple);
    }
}

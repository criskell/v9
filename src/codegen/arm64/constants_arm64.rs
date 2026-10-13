#![allow(non_upper_case_globals)]

pub type Instr = u32;

pub const kInstrSize: u8 = 4;
pub const kInstrSizeLog2: u8 = 2;
pub const kLoadLiteralScaleLog2: u8 = 2;
pub const kLoadLiteralScale: u8 = 1 << kLoadLiteralScaleLog2;

pub const kNumberOfRegisters: i32 = 32;
pub const kNumberOfVRegisters: i32 = 32;
pub const kWRegSizeInBits: i32 = 32;
pub const kWRegSizeInBitsLog2: i32 = 5;
pub const kWRegSize: i32 = kWRegSizeInBits >> 3;
pub const kWRegSizeLog2: i32 = kWRegSizeInBitsLog2 - 3;
pub const kXRegSizeInBits: i32 = 64;
pub const kXRegSizeInBitsLog2: i32 = 6;
pub const kXRegSize: i32 = kXRegSizeInBits >> 3;
pub const kXRegSizeLog2: i32 = kXRegSizeInBitsLog2 - 3;
pub const kSRegSizeInBits: i32 = 32;
pub const kSRegSizeInBitsLog2: i32 = 5;
pub const kSRegSize: i32 = kSRegSizeInBits >> 3;
pub const kSRegSizeLog2: i32 = kSRegSizeInBitsLog2 - 3;
pub const kDRegSizeInBits: i32 = 64;
pub const kDRegSizeInBitsLog2: i32 = 6;
pub const kDRegSize: i32 = kDRegSizeInBits >> 3;
pub const kDRegSizeLog2: i32 = kDRegSizeInBitsLog2 - 3;
pub const kDRegSizeInBytesLog2: i32 = kDRegSizeInBitsLog2 - 3;
pub const kBRegSizeInBits: i32 = 8;
pub const kBRegSize: i32 = kBRegSizeInBits >> 3;
pub const kHRegSizeInBits: i32 = 16;
pub const kHRegSize: i32 = kHRegSizeInBits >> 3;
pub const kQRegSizeInBits: i32 = 128;
pub const kQRegSizeInBitsLog2: i32 = 7;
pub const kQRegSize: i32 = kQRegSizeInBits >> 3;
pub const kQRegSizeLog2: i32 = kQRegSizeInBitsLog2 - 3;
pub const kVRegSizeInBits: i32 = kQRegSizeInBits;
pub const kVRegSize: i32 = kVRegSizeInBits >> 3;
pub const kMaxLanesPerVector: i32 = 16;

pub const kDoubleMantissaBits: u32 = 52;
pub const kDoubleExponentBits: u32 = 11;
pub const kFloatMantissaBits: u32 = 23;
pub const kFloatExponentBits: u32 = 8;
pub const kWRegMask: i64 = 0x00000000ffffffff;
pub const kZeroRegCode: i32 = 31;
pub const kSPRegInternalCode: i32 = 63;
pub const kRegCodeMask: u32 = 0x1f;

pub const Rd_offset: u32 = 0;
pub const Rd_width: u32 = 5;
pub const Rn_offset: u32 = 5;
pub const Rn_width: u32 = 5;
pub const Rm_offset: u32 = 16;
pub const Rm_width: u32 = 5;
pub const Rt_offset: u32 = 0;
pub const Rt_width: u32 = 5;

pub const FlagsUpdate_offset: u32 = 29;

pub const ShiftDP_offset: u32 = 22;
pub const ImmDPShift_offset: u32 = 10;

pub const ImmAddSub_offset: u32 = 10;
pub const ShiftAddSub_offset: u32 = 22;

pub const ImmMoveWide_offset: u32 = 5;
pub const ShiftMoveWide_offset: u32 = 21;

pub const ImmTestBranch_offset: u32 = 5;
pub const ImmTestBranch_width: u32 = 14;
pub const ImmTestBranch_mask: u32 = ((1 << ImmTestBranch_width) - 1) << ImmTestBranch_offset;

pub const ConditionBranch_offset: u32 = 0;
pub const ImmCondBranch_offset: u32 = 5;
pub const ImmCondBranch_width: u32 = 19;
pub const ImmCondBranch_mask: u32 = ((1 << ImmCondBranch_width) - 1) << ImmCondBranch_offset;

pub const ImmLS_offset: u32 = 12;
pub const ImmLSUnsigned_offset: u32 = 10;
pub const LSOpc_offset: u32 = 22;
pub const LSOpc_mask: u32 = 0x3 << LSOpc_offset;
pub const LSVector_offset: u32 = 26;
pub const LSVector_mask: u32 = 0x1 << LSVector_offset;
pub const LSSize_offset: u32 = 30;

pub const ImmUncondBranch_offset: u32 = 0;
pub const ImmUncondBranch_width: u32 = 26;
pub const ImmUncondBranch_mask: u32 =
    (((1u64 << ImmUncondBranch_width) - 1) as u32) << ImmUncondBranch_offset;
pub const ImmCmpBranch_offset: u32 = 5;
pub const ImmCmpBranch_width: u32 = 19;
pub const ImmCmpBranch_mask: u32 = ((1 << ImmCmpBranch_width) - 1) << ImmCmpBranch_offset;
pub const ImmException_offset: u32 = 5;
pub const ImmHint_offset: u32 = 5;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Condition {
    Eq = 0,
    Ne = 1,
    Hs = 2,
    Lo = 3,
    Mi = 4,
    Pl = 5,
    Vs = 6,
    Vc = 7,
    Hi = 8,
    Ls = 9,
    Ge = 10,
    Lt = 11,
    Gt = 12,
    Le = 13,
    Al = 14,
    Nv = 15,
}

pub const cs: Condition = Condition::Hs;
pub const cc: Condition = Condition::Lo;

pub fn condition_from_bits(bits: u32) -> Condition {
    match bits {
        0 => Condition::Eq,
        1 => Condition::Ne,
        2 => Condition::Hs,
        3 => Condition::Lo,
        4 => Condition::Mi,
        5 => Condition::Pl,
        6 => Condition::Vs,
        7 => Condition::Vc,
        8 => Condition::Hi,
        9 => Condition::Ls,
        10 => Condition::Ge,
        11 => Condition::Lt,
        12 => Condition::Gt,
        13 => Condition::Le,
        14 => Condition::Al,
        _ => Condition::Nv,
    }
}

pub fn negate_condition(cond: Condition) -> Condition {
    debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
    condition_from_bits(cond as u32 ^ 1)
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FlagsUpdate {
    SetFlags = 1,
    LeaveFlags = 0,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Shift {
    NoShift = -1,
    Lsl = 0x0,
    Lsr = 0x1,
    Asr = 0x2,
    Ror = 0x3,
    Msl = 0x4,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Extend {
    NoExtend = -1,
    Uxtb = 0,
    Uxth = 1,
    Uxtw = 2,
    Uxtx = 3,
    Sxtb = 4,
    Sxth = 5,
    Sxtw = 6,
    Sxtx = 7,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SystemHint {
    Nop = 0,
    Yield = 1,
    Wfe = 2,
    Wfi = 3,
    Sev = 4,
    Sevl = 5,
    Csdb = 20,
    Bti = 32,
    BtiC = 34,
    BtiJ = 36,
    BtiJc = 38,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ImmBranchType {
    UnknownBranchType = 0,
    CondBranchType = 1,
    UncondBranchType = 2,
    CompareBranchType = 3,
    TestBranchType = 4,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AddrMode {
    Offset,
    PreIndex,
    PostIndex,
}

pub const SixtyFourBits: Instr = 0x80000000;
pub const ThirtyTwoBits: Instr = 0x00000000;

pub const kSFOffset: u32 = 31;
pub const AddSubSetFlagsBit: Instr = 0x20000000;
pub const ADD: Instr = 0x00000000;
pub const ADDS: Instr = ADD | AddSubSetFlagsBit;
pub const SUB: Instr = 0x40000000;
pub const SUBS: Instr = SUB | AddSubSetFlagsBit;

pub const AddSubImmediateFixed: Instr = 0x11000000;
pub const AddSubShiftedFixed: Instr = 0x0B000000;
pub const AddSubExtendedFixed: Instr = 0x0B200000;

pub const NOT: Instr = 0x00200000;
pub const AND: Instr = 0x00000000;
pub const BIC: Instr = AND | NOT;
pub const ORR: Instr = 0x20000000;
pub const ORN: Instr = ORR | NOT;
pub const EOR: Instr = 0x40000000;
pub const EON: Instr = EOR | NOT;
pub const ANDS: Instr = 0x60000000;
pub const BICS: Instr = ANDS | NOT;

pub const LogicalShiftedFixed: Instr = 0x0A000000;

pub const MoveWideImmediateFixed: Instr = 0x12800000;
pub const MOVN: Instr = 0x00000000;
pub const MOVZ: Instr = 0x40000000;
pub const MOVK: Instr = 0x60000000;

pub const UnconditionalBranchFixed: Instr = 0x14000000;
pub const UnconditionalBranchFMask: Instr = 0x7C000000;
pub const B: Instr = UnconditionalBranchFixed | 0x00000000;
pub const BL: Instr = UnconditionalBranchFixed | 0x80000000;

pub const UnconditionalBranchToRegisterFixed: Instr = 0xD6000000;
pub const BR: Instr = UnconditionalBranchToRegisterFixed | 0x001F0000;
pub const BLR: Instr = UnconditionalBranchToRegisterFixed | 0x003F0000;
pub const RET: Instr = UnconditionalBranchToRegisterFixed | 0x005F0000;

pub const CompareBranchFixed: Instr = 0x34000000;
pub const CompareBranchFMask: Instr = 0x7E000000;
pub const CBZ: Instr = CompareBranchFixed | 0x00000000;
pub const CBNZ: Instr = CompareBranchFixed | 0x01000000;

pub const ConditionalBranchFixed: Instr = 0x54000000;
pub const ConditionalBranchFMask: Instr = 0xFE000000;
pub const B_cond: Instr = ConditionalBranchFixed | 0x00000000;

pub const TestBranchFixed: Instr = 0x36000000;
pub const TestBranchFMask: Instr = 0x7E000000;

pub const PCRelAddressingFixed: Instr = 0x10000000;
pub const PCRelAddressingFMask: Instr = 0x1F000000;
pub const PCRelAddressingMask: Instr = 0x9F000000;

pub const SystemHintFixed: Instr = 0xD503201F;
pub const HINT: Instr = SystemHintFixed | 0x00000000;

pub const ExceptionFixed: Instr = 0xD4000000;
pub const HLT: Instr = ExceptionFixed | 0x00400000;
pub const BRK: Instr = ExceptionFixed | 0x00200000;

pub const LoadStoreUnscaledOffsetFixed: Instr = 0x38000000;
pub const LoadStoreUnsignedOffsetFixed: Instr = 0x39000000;
pub const LoadStorePostIndexFixed: Instr = 0x38000400;
pub const LoadStorePreIndexFixed: Instr = 0x38000C00;

pub const STRB_w: Instr = 0x00000000;
pub const STRH_w: Instr = 0x40000000;
pub const STR_w: Instr = 0x80000000;
pub const STR_x: Instr = 0xC0000000;
pub const LDRB_w: Instr = 0x00400000;
pub const LDRH_w: Instr = 0x40400000;
pub const LDR_w: Instr = 0x80400000;
pub const LDR_x: Instr = 0xC0400000;
pub const LDRSB_x: Instr = 0x00800000;
pub const LDRSH_x: Instr = 0x40800000;
pub const LDRSW_x: Instr = 0x80800000;
pub const LDRSB_w: Instr = 0x00C00000;
pub const LDRSH_w: Instr = 0x40C00000;
pub const STR_b: Instr = 0x04000000;
pub const STR_h: Instr = 0x44000000;
pub const STR_s: Instr = 0x84000000;
pub const STR_d: Instr = 0xC4000000;
pub const STR_q: Instr = 0x04800000;
pub const LDR_b: Instr = 0x04400000;
pub const LDR_h: Instr = 0x44400000;
pub const LDR_s: Instr = 0x84400000;
pub const LDR_d: Instr = 0xC4400000;
pub const LDR_q: Instr = 0x04C00000;
pub const LoadStoreRegisterOffsetFixed: Instr = 0x38200800;

pub const kBitfieldNOffset: u32 = 22;

pub const Ra_offset: u32 = 10;
pub const Rt2_offset: u32 = 10;
pub const BitN_offset: u32 = 22;
pub const ImmRotate_offset: u32 = 16;
pub const ImmSetBits_offset: u32 = 10;
pub const ImmR_offset: u32 = 16;
pub const ImmS_offset: u32 = 10;
pub const Condition_offset: u32 = 12;
pub const Nzcv_offset: u32 = 0;
pub const ImmCondCmp_offset: u32 = 16;
pub const ImmLSPair_offset: u32 = 15;
pub const Flags_offset: u32 = 28;

pub const ImmTestBranchBit40_offset: u32 = 19;
pub const ImmTestBranchBit40_mask: u32 = 0x1f << ImmTestBranchBit40_offset;
pub const ImmTestBranchBit5_offset: u32 = 31;
pub const ImmTestBranchBit5_mask: u32 = 0x1 << ImmTestBranchBit5_offset;

pub const ImmPCRelHi_offset: u32 = 5;
pub const ImmPCRelHi_width: u32 = 19;
pub const ImmPCRelHi_mask: u32 = ((1 << ImmPCRelHi_width) - 1) << ImmPCRelHi_offset;
pub const ImmPCRelLo_offset: u32 = 29;
pub const ImmPCRelLo_width: u32 = 2;
pub const ImmPCRelLo_mask: u32 = ((1 << ImmPCRelLo_width) - 1) << ImmPCRelLo_offset;
pub const ImmPCRel_mask: u32 = ImmPCRelLo_mask | ImmPCRelHi_mask;

pub const N_mask: u32 = 0x1 << 31;
pub const Z_mask: u32 = 0x1 << 30;
pub const C_mask: u32 = 0x1 << 29;
pub const V_mask: u32 = 0x1 << 28;

pub const NoFlag: u32 = 0;
pub const NFlag: u32 = N_mask;
pub const ZFlag: u32 = Z_mask;
pub const CFlag: u32 = C_mask;
pub const VFlag: u32 = V_mask;
pub const NZFlag: u32 = NFlag | ZFlag;
pub const NCFlag: u32 = NFlag | CFlag;
pub const NVFlag: u32 = NFlag | VFlag;
pub const ZCFlag: u32 = ZFlag | CFlag;
pub const ZVFlag: u32 = ZFlag | VFlag;
pub const CVFlag: u32 = CFlag | VFlag;
pub const NZCFlag: u32 = NFlag | ZFlag | CFlag;
pub const NZVFlag: u32 = NFlag | ZFlag | VFlag;
pub const NCVFlag: u32 = NFlag | CFlag | VFlag;
pub const ZCVFlag: u32 = ZFlag | CFlag | VFlag;
pub const NZCVFlag: u32 = NFlag | ZFlag | CFlag | VFlag;

pub const ADR: Instr = PCRelAddressingFixed | 0x00000000;
pub const ADRP: Instr = PCRelAddressingFixed | 0x80000000;

pub const AddSubWithCarryFixed: Instr = 0x1A000000;
pub const ADC: Instr = AddSubWithCarryFixed | ADD;
pub const ADCS_w: Instr = AddSubWithCarryFixed | ADDS;
pub const SBC: Instr = AddSubWithCarryFixed | SUB;
pub const SBCS_w: Instr = AddSubWithCarryFixed | SUBS;

pub const LogicalImmediateFixed: Instr = 0x12000000;

pub const BitfieldFixed: Instr = 0x13000000;
pub const SBFM: Instr = BitfieldFixed | 0x00000000;
pub const BFM: Instr = BitfieldFixed | 0x20000000;
pub const UBFM: Instr = BitfieldFixed | 0x40000000;

pub const ExtractFixed: Instr = 0x13800000;
pub const EXTR: Instr = ExtractFixed | 0x00000000;

pub const TBZ: Instr = TestBranchFixed | 0x00000000;
pub const TBNZ: Instr = TestBranchFixed | 0x01000000;

pub const LoadStorePairLBit: Instr = 1 << 22;
pub const STP_w: Instr = 0x00000000;
pub const LDP_w: Instr = 0x00400000;
pub const LDPSW_x: Instr = 0x40400000;
pub const STP_x: Instr = 0x80000000;
pub const LDP_x: Instr = 0x80400000;
pub const STP_s: Instr = 0x04000000;
pub const LDP_s: Instr = 0x04400000;
pub const STP_d: Instr = 0x44000000;
pub const LDP_d: Instr = 0x44400000;
pub const STP_q: Instr = 0x84000000;
pub const LDP_q: Instr = 0x84400000;
pub const LoadStorePairPostIndexFixed: Instr = 0x28800000;
pub const LoadStorePairPreIndexFixed: Instr = 0x29800000;
pub const LoadStorePairOffsetFixed: Instr = 0x29000000;

pub const ConditionalCompareRegisterFixed: Instr = 0x1A400000;
pub const ConditionalCompareImmediateFixed: Instr = 0x1A400800;
pub const CCMN: Instr = 0x20000000;
pub const CCMP: Instr = 0x60000000;

pub const ConditionalSelectFixed: Instr = 0x1A800000;
pub const CSEL: Instr = ConditionalSelectFixed | 0x00000000;
pub const CSINC: Instr = ConditionalSelectFixed | 0x00000400;
pub const CSINV: Instr = ConditionalSelectFixed | 0x40000000;
pub const CSNEG: Instr = ConditionalSelectFixed | 0x40000400;

pub const DataProcessing1SourceFixed: Instr = 0x5AC00000;
pub const RBIT: Instr = DataProcessing1SourceFixed | 0x00000000;
pub const REV16: Instr = DataProcessing1SourceFixed | 0x00000400;
pub const REV: Instr = DataProcessing1SourceFixed | 0x00000800;
pub const REV_w: Instr = REV;
pub const REV_x: Instr = DataProcessing1SourceFixed | SixtyFourBits | 0x00000C00;
pub const CLZ: Instr = DataProcessing1SourceFixed | 0x00001000;
pub const CLS: Instr = DataProcessing1SourceFixed | 0x00001400;

pub const DataProcessing2SourceFixed: Instr = 0x1AC00000;
pub const UDIV: Instr = DataProcessing2SourceFixed | 0x00000800;
pub const SDIV: Instr = DataProcessing2SourceFixed | 0x00000C00;
pub const LSLV: Instr = DataProcessing2SourceFixed | 0x00002000;
pub const LSRV: Instr = DataProcessing2SourceFixed | 0x00002400;
pub const ASRV: Instr = DataProcessing2SourceFixed | 0x00002800;
pub const RORV: Instr = DataProcessing2SourceFixed | 0x00002C00;

pub const DataProcessing3SourceFixed: Instr = 0x1B000000;
pub const MADD: Instr = DataProcessing3SourceFixed | 0x00000000;
pub const MSUB: Instr = DataProcessing3SourceFixed | 0x00008000;
pub const SMADDL_x: Instr = DataProcessing3SourceFixed | 0x80200000;
pub const SMSUBL_x: Instr = DataProcessing3SourceFixed | 0x80208000;
pub const SMULH_x: Instr = DataProcessing3SourceFixed | 0x80400000;
pub const UMADDL_x: Instr = DataProcessing3SourceFixed | 0x80A00000;
pub const UMSUBL_x: Instr = DataProcessing3SourceFixed | 0x80A08000;
pub const UMULH_x: Instr = DataProcessing3SourceFixed | 0x80C00000;

pub const FP32: Instr = 0x00000000;
pub const FP64: Instr = 0x00400000;

pub const ImmFP_offset: u32 = 13;
pub const FPScale_offset: u32 = 10;

pub const FPCompareFixed: Instr = 0x1E202000;
pub const FCMP_s: Instr = FPCompareFixed | 0x00000000;
pub const FCMP: Instr = FCMP_s;
pub const FCMP_s_zero: Instr = FPCompareFixed | 0x00000008;
pub const FCMP_zero: Instr = FCMP_s_zero;

pub const FPImmediateFixed: Instr = 0x1E201000;
pub const FMOV_s_imm: Instr = FPImmediateFixed | 0x00000000;
pub const FMOV_d_imm: Instr = FPImmediateFixed | FP64 | 0x00000000;

pub const FPDataProcessing1SourceFixed: Instr = 0x1E204000;
pub const FMOV: Instr = FPDataProcessing1SourceFixed | 0x00000000;
pub const FABS: Instr = FPDataProcessing1SourceFixed | 0x00008000;
pub const FNEG: Instr = FPDataProcessing1SourceFixed | 0x00010000;
pub const FSQRT: Instr = FPDataProcessing1SourceFixed | 0x00018000;
pub const FCVT_ds: Instr = FPDataProcessing1SourceFixed | 0x00028000;
pub const FCVT_sd: Instr = FPDataProcessing1SourceFixed | FP64 | 0x00020000;
pub const FCVT_hs: Instr = FPDataProcessing1SourceFixed | 0x00038000;
pub const FCVT_hd: Instr = FPDataProcessing1SourceFixed | FP64 | 0x00038000;
pub const FCVT_sh: Instr = FPDataProcessing1SourceFixed | 0x00C20000;
pub const FCVT_dh: Instr = FPDataProcessing1SourceFixed | 0x00C28000;
pub const FRINTN: Instr = FPDataProcessing1SourceFixed | 0x00040000;
pub const FRINTP: Instr = FPDataProcessing1SourceFixed | 0x00048000;
pub const FRINTM: Instr = FPDataProcessing1SourceFixed | 0x00050000;
pub const FRINTZ: Instr = FPDataProcessing1SourceFixed | 0x00058000;
pub const FRINTA: Instr = FPDataProcessing1SourceFixed | 0x00060000;
pub const FRINTX: Instr = FPDataProcessing1SourceFixed | 0x00070000;
pub const FRINTI: Instr = FPDataProcessing1SourceFixed | 0x00078000;

pub const FPDataProcessing2SourceFixed: Instr = 0x1E200800;
pub const FMUL: Instr = FPDataProcessing2SourceFixed | 0x00000000;
pub const FDIV: Instr = FPDataProcessing2SourceFixed | 0x00001000;
pub const FADD: Instr = FPDataProcessing2SourceFixed | 0x00002000;
pub const FSUB: Instr = FPDataProcessing2SourceFixed | 0x00003000;
pub const FMAX: Instr = FPDataProcessing2SourceFixed | 0x00004000;
pub const FMIN: Instr = FPDataProcessing2SourceFixed | 0x00005000;
pub const FMAXNM: Instr = FPDataProcessing2SourceFixed | 0x00006000;
pub const FMINNM: Instr = FPDataProcessing2SourceFixed | 0x00007000;
pub const FNMUL_s: Instr = FPDataProcessing2SourceFixed | 0x00008000;
pub const FNMUL_d: Instr = FNMUL_s | FP64;

pub const FPDataProcessing3SourceFixed: Instr = 0x1F000000;
pub const FMADD_s: Instr = FPDataProcessing3SourceFixed | 0x00000000;
pub const FMSUB_s: Instr = FPDataProcessing3SourceFixed | 0x00008000;
pub const FNMADD_s: Instr = FPDataProcessing3SourceFixed | 0x00200000;
pub const FNMSUB_s: Instr = FPDataProcessing3SourceFixed | 0x00208000;
pub const FMADD_d: Instr = FPDataProcessing3SourceFixed | 0x00400000;
pub const FMSUB_d: Instr = FPDataProcessing3SourceFixed | 0x00408000;
pub const FNMADD_d: Instr = FPDataProcessing3SourceFixed | 0x00600000;
pub const FNMSUB_d: Instr = FPDataProcessing3SourceFixed | 0x00608000;

pub const FPIntegerConvertFixed: Instr = 0x1E200000;
pub const FCVTNS: Instr = FPIntegerConvertFixed | 0x00000000;
pub const FCVTNU: Instr = FPIntegerConvertFixed | 0x00010000;
pub const SCVTF: Instr = FPIntegerConvertFixed | 0x00020000;
pub const UCVTF: Instr = FPIntegerConvertFixed | 0x00030000;
pub const FCVTAS: Instr = FPIntegerConvertFixed | 0x00040000;
pub const FCVTAU: Instr = FPIntegerConvertFixed | 0x00050000;
pub const FMOV_ws: Instr = FPIntegerConvertFixed | 0x00060000;
pub const FMOV_sw: Instr = FPIntegerConvertFixed | 0x00070000;
pub const FMOV_xd: Instr = FMOV_ws | SixtyFourBits | FP64;
pub const FMOV_dx: Instr = FMOV_sw | SixtyFourBits | FP64;
pub const FCVTPS: Instr = FPIntegerConvertFixed | 0x00080000;
pub const FCVTPU: Instr = FPIntegerConvertFixed | 0x00090000;
pub const FCVTMS: Instr = FPIntegerConvertFixed | 0x00100000;
pub const FCVTMU: Instr = FPIntegerConvertFixed | 0x00110000;
pub const FCVTZS: Instr = FPIntegerConvertFixed | 0x00180000;
pub const FCVTZU: Instr = FPIntegerConvertFixed | 0x00190000;
pub const FJCVTZS: Instr = FPIntegerConvertFixed | FP64 | 0x001E0000;

pub const FPFixedPointConvertFixed: Instr = 0x1E000000;
pub const SCVTF_fixed: Instr = FPFixedPointConvertFixed | 0x00020000;
pub const UCVTF_fixed: Instr = FPFixedPointConvertFixed | 0x00030000;
pub const FCVTZS_fixed: Instr = FPFixedPointConvertFixed | 0x00180000;
pub const FCVTZU_fixed: Instr = FPFixedPointConvertFixed | 0x00190000;

pub const kInt32Size: i32 = 4;
pub const kInt64Size: i32 = 8;
pub const kMaxInt: i32 = 0x7fffffff;

pub const ImmLLiteral_offset: u32 = 5;
pub const ImmLLiteral_width: u32 = 19;
pub const ImmLLiteral_mask: u32 = ((1 << ImmLLiteral_width) - 1) << ImmLLiteral_offset;

pub const LoadLiteralFixed: Instr = 0x18000000;
pub const LoadLiteralFMask: Instr = 0x3B000000;
pub const LoadLiteralMask: Instr = 0xFF000000;
pub const LDR_w_lit: Instr = LoadLiteralFixed | 0x00000000;
pub const LDR_x_lit: Instr = LoadLiteralFixed | 0x40000000;
pub const LDRSW_x_lit: Instr = LoadLiteralFixed | 0x80000000;
pub const LDR_s_lit: Instr = LoadLiteralFixed | 0x04000000;
pub const LDR_d_lit: Instr = LoadLiteralFixed | 0x44000000;

pub const AddSubImmediateFMask: Instr = 0x1F000000;
pub const AddSubExtendedFMask: Instr = 0x1F200000;
pub const AddSubShiftedFMask: Instr = 0x1F200000;
pub const AddSubOpMask: Instr = 0x60000000;
pub const LogicalImmediateFMask: Instr = 0x1F800000;
pub const LogicalImmediateMask: Instr = 0xFF800000;
pub const LogicalOpMask: Instr = 0x60200000;
pub const LoadStoreMask: Instr = 0xC4C00000;
pub const LoadStorePairMask: Instr = 0xC4400000;
pub const LoadStoreAnyFMask: Instr = 0x0a000000;
pub const LoadStoreAnyFixed: Instr = 0x08000000;
pub const LoadStorePairAnyFMask: Instr = 0x3a000000;
pub const LoadStorePairAnyFixed: Instr = 0x28000000;
pub const MoveWideImmediateMask: Instr = 0xFF800000;
pub const UnconditionalBranchMask: Instr = 0xFC000000;
pub const UnconditionalBranchToRegisterMask: Instr = 0xFFFFFC1F;
pub const CompareBranchMask: Instr = 0xFF000000;
pub const ConditionalBranchMask: Instr = 0xFF000010;
pub const TestBranchMask: Instr = 0x7F000000;
pub const ConditionalSelectMask: Instr = 0xFFE00C00;
pub const ConditionalCompareMask: Instr = 0x60000000;
pub const DataProcessing1SourceMask: Instr = 0xFFFFFC00;
pub const DataProcessing2SourceMask: Instr = 0xFFE0FC00;
pub const DataProcessing3SourceMask: Instr = 0xFFE08000;
pub const BitfieldMask: Instr = 0xFF800000;
pub const ExtractMask: Instr = 0xFFA00000;
pub const ExceptionMask: Instr = 0xFFE0001F;

pub const CBZ_w: Instr = CompareBranchFixed | 0x00000000;
pub const CBZ_x: Instr = CompareBranchFixed | 0x80000000;
pub const CBNZ_w: Instr = CompareBranchFixed | 0x01000000;
pub const CBNZ_x: Instr = CompareBranchFixed | 0x81000000;
pub const CSEL_w: Instr = ConditionalSelectFixed | 0x00000000;
pub const CSEL_x: Instr = ConditionalSelectFixed | 0x80000000;
pub const CSINC_w: Instr = ConditionalSelectFixed | 0x00000400;
pub const CSINC_x: Instr = ConditionalSelectFixed | 0x80000400;
pub const CSINV_w: Instr = ConditionalSelectFixed | 0x40000000;
pub const CSINV_x: Instr = ConditionalSelectFixed | 0xC0000000;
pub const CSNEG_w: Instr = ConditionalSelectFixed | 0x40000400;
pub const CSNEG_x: Instr = ConditionalSelectFixed | 0xC0000400;
pub const MOVN_w: Instr = MoveWideImmediateFixed | MOVN;
pub const MOVN_x: Instr = MoveWideImmediateFixed | MOVN | SixtyFourBits;
pub const MOVZ_w: Instr = MoveWideImmediateFixed | MOVZ;
pub const MOVZ_x: Instr = MoveWideImmediateFixed | MOVZ | SixtyFourBits;
pub const MOVK_w: Instr = MoveWideImmediateFixed | MOVK;
pub const MOVK_x: Instr = MoveWideImmediateFixed | MOVK | SixtyFourBits;
pub const SBFM_w: Instr = BitfieldFixed | 0x00000000;
pub const SBFM_x: Instr = BitfieldFixed | 0x80000000;
pub const BFM_w: Instr = BitfieldFixed | 0x20000000;
pub const BFM_x: Instr = BitfieldFixed | 0xA0000000;
pub const UBFM_w: Instr = BitfieldFixed | 0x40000000;
pub const UBFM_x: Instr = BitfieldFixed | 0xC0000000;
pub const RBIT_w: Instr = RBIT;
pub const RBIT_x: Instr = RBIT | SixtyFourBits;
pub const REV16_w: Instr = REV16;
pub const REV16_x: Instr = REV16 | SixtyFourBits;
pub const REV32_x: Instr = REV | SixtyFourBits;
pub const CLZ_w: Instr = CLZ;
pub const CLZ_x: Instr = CLZ | SixtyFourBits;
pub const CLS_w: Instr = CLS;
pub const CLS_x: Instr = CLS | SixtyFourBits;
pub const UDIV_w: Instr = UDIV;
pub const UDIV_x: Instr = UDIV | SixtyFourBits;
pub const SDIV_w: Instr = SDIV;
pub const SDIV_x: Instr = SDIV | SixtyFourBits;
pub const LSLV_w: Instr = LSLV;
pub const LSLV_x: Instr = LSLV | SixtyFourBits;
pub const LSRV_w: Instr = LSRV;
pub const LSRV_x: Instr = LSRV | SixtyFourBits;
pub const ASRV_w: Instr = ASRV;
pub const ASRV_x: Instr = ASRV | SixtyFourBits;
pub const RORV_w: Instr = RORV;
pub const RORV_x: Instr = RORV | SixtyFourBits;
pub const MADD_w: Instr = MADD;
pub const MADD_x: Instr = MADD | SixtyFourBits;
pub const MSUB_w: Instr = MSUB;
pub const MSUB_x: Instr = MSUB | SixtyFourBits;

pub const kByteMask: i64 = 0xff;
pub const kHalfWordMask: i64 = 0xffff;
pub const kWordMask: i64 = 0xffffffff;
pub const kShiftAmountWRegMask: u32 = 0x1f;
pub const kShiftAmountXRegMask: u32 = 0x3f;
pub const kLinkRegCode: u32 = 30;

pub const FPDataProcessing1SourceMask: Instr = 0xFFFFFC00;
pub const FPDataProcessing2SourceMask: Instr = 0xFFE0FC00;
pub const FPDataProcessing3SourceMask: Instr = 0xFFE08000;
pub const FPCompareMask: Instr = 0xFFE0FC1F;
pub const FPImmediateMask: Instr = 0xFFE01C00;
pub const FPIntegerConvertMask: Instr = 0xFFFFFC00;
pub const FPFixedPointConvertMask: Instr = 0xFFFF0000;

pub const FMOV_s: Instr = FPDataProcessing1SourceFixed | 0x00000000;
pub const FMOV_d: Instr = FPDataProcessing1SourceFixed | FP64 | 0x00000000;
pub const FABS_s: Instr = FPDataProcessing1SourceFixed | 0x00008000;
pub const FABS_d: Instr = FPDataProcessing1SourceFixed | FP64 | 0x00008000;
pub const FNEG_s: Instr = FPDataProcessing1SourceFixed | 0x00010000;
pub const FNEG_d: Instr = FPDataProcessing1SourceFixed | FP64 | 0x00010000;
pub const FSQRT_s: Instr = FPDataProcessing1SourceFixed | 0x00018000;
pub const FSQRT_d: Instr = FPDataProcessing1SourceFixed | FP64 | 0x00018000;
pub const FRINTN_s: Instr = FRINTN;
pub const FRINTN_d: Instr = FRINTN | FP64;
pub const FRINTP_s: Instr = FRINTP;
pub const FRINTP_d: Instr = FRINTP | FP64;
pub const FRINTM_s: Instr = FRINTM;
pub const FRINTM_d: Instr = FRINTM | FP64;
pub const FRINTZ_s: Instr = FRINTZ;
pub const FRINTZ_d: Instr = FRINTZ | FP64;
pub const FRINTA_s: Instr = FRINTA;
pub const FRINTA_d: Instr = FRINTA | FP64;
pub const FRINTX_s: Instr = FRINTX;
pub const FRINTX_d: Instr = FRINTX | FP64;
pub const FRINTI_s: Instr = FRINTI;
pub const FRINTI_d: Instr = FRINTI | FP64;

pub const FMUL_s: Instr = FMUL;
pub const FMUL_d: Instr = FMUL | FP64;
pub const FDIV_s: Instr = FDIV;
pub const FDIV_d: Instr = FDIV | FP64;
pub const FADD_s: Instr = FADD;
pub const FADD_d: Instr = FADD | FP64;
pub const FSUB_s: Instr = FSUB;
pub const FSUB_d: Instr = FSUB | FP64;
pub const FMAX_s: Instr = FMAX;
pub const FMAX_d: Instr = FMAX | FP64;
pub const FMIN_s: Instr = FMIN;
pub const FMIN_d: Instr = FMIN | FP64;
pub const FMAXNM_s: Instr = FMAXNM;
pub const FMAXNM_d: Instr = FMAXNM | FP64;
pub const FMINNM_s: Instr = FMINNM;
pub const FMINNM_d: Instr = FMINNM | FP64;

pub const FCMP_d: Instr = FPCompareFixed | FP64 | 0x00000000;
pub const FCMP_d_zero: Instr = FPCompareFixed | FP64 | 0x00000008;

pub const FCVTZS_ws: Instr = FCVTZS;
pub const FCVTZS_xs: Instr = FCVTZS | SixtyFourBits;
pub const FCVTZS_wd: Instr = FCVTZS | FP64;
pub const FCVTZS_xd: Instr = FCVTZS | SixtyFourBits | FP64;
pub const FCVTZU_ws: Instr = FCVTZU;
pub const FCVTZU_xs: Instr = FCVTZU | SixtyFourBits;
pub const FCVTZU_wd: Instr = FCVTZU | FP64;
pub const FCVTZU_xd: Instr = FCVTZU | SixtyFourBits | FP64;
pub const SCVTF_sw: Instr = SCVTF;
pub const SCVTF_sx: Instr = SCVTF | SixtyFourBits;
pub const SCVTF_dw: Instr = SCVTF | FP64;
pub const SCVTF_dx: Instr = SCVTF | SixtyFourBits | FP64;
pub const UCVTF_sw: Instr = UCVTF;
pub const UCVTF_sx: Instr = UCVTF | SixtyFourBits;
pub const UCVTF_dw: Instr = UCVTF | FP64;
pub const UCVTF_dx: Instr = UCVTF | SixtyFourBits | FP64;
pub const FCVTNS_ws: Instr = FCVTNS;
pub const FCVTNS_xs: Instr = FCVTNS | SixtyFourBits;
pub const FCVTNS_wd: Instr = FCVTNS | FP64;
pub const FCVTNS_xd: Instr = FCVTNS | SixtyFourBits | FP64;
pub const FCVTMS_wd: Instr = FCVTMS | FP64;
pub const FCVTMS_xd: Instr = FCVTMS | SixtyFourBits | FP64;
pub const FCVTAS_wd: Instr = FCVTAS | FP64;
pub const FCVTAS_xd: Instr = FCVTAS | SixtyFourBits | FP64;

pub const SCVTF_sw_fixed: Instr = SCVTF_fixed;
pub const SCVTF_sx_fixed: Instr = SCVTF_fixed | SixtyFourBits;
pub const SCVTF_dw_fixed: Instr = SCVTF_fixed | FP64;
pub const SCVTF_dx_fixed: Instr = SCVTF_fixed | SixtyFourBits | FP64;
pub const UCVTF_sw_fixed: Instr = UCVTF_fixed;
pub const UCVTF_sx_fixed: Instr = UCVTF_fixed | SixtyFourBits;
pub const UCVTF_dw_fixed: Instr = UCVTF_fixed | FP64;
pub const UCVTF_dx_fixed: Instr = UCVTF_fixed | SixtyFourBits | FP64;
pub const FCVTZS_ws_fixed: Instr = FCVTZS_fixed;
pub const FCVTZS_xs_fixed: Instr = FCVTZS_fixed | SixtyFourBits;
pub const FCVTZS_wd_fixed: Instr = FCVTZS_fixed | FP64;
pub const FCVTZS_xd_fixed: Instr = FCVTZS_fixed | SixtyFourBits | FP64;
pub const FCVTZU_ws_fixed: Instr = FCVTZU_fixed;
pub const FCVTZU_xs_fixed: Instr = FCVTZU_fixed | SixtyFourBits;
pub const FCVTZU_wd_fixed: Instr = FCVTZU_fixed | FP64;
pub const FCVTZU_xd_fixed: Instr = FCVTZU_fixed | SixtyFourBits | FP64;

pub const kFP64PositiveInfinity: u64 = 0x7FF0000000000000;
pub const kFP64NegativeInfinity: u64 = 0xFFF0000000000000;

pub const FPEqualFlag: u32 = ZFlag | CFlag;
pub const FPLessThanFlag: u32 = NFlag;
pub const FPGreaterThanFlag: u32 = CFlag;
pub const FPUnorderedFlag: u32 = CFlag | VFlag;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FPRounding {
    FPTieEven = 0x0,
    FPPositiveInfinity = 0x1,
    FPNegativeInfinity = 0x2,
    FPZero = 0x3,
    FPTieAway,
}

pub const ExtendMode_offset: u32 = 13;
pub const ImmExtendShift_offset: u32 = 10;
pub const ImmShiftLS_offset: u32 = 12;

pub const LoadStoreAcquireReleaseFixed: Instr = 0x08000000;
pub const LoadStoreAcquireReleaseFMask: Instr = 0x3F000000;
pub const LoadStoreAcquireReleaseMask: Instr = 0xCFE08000;
pub const STLXR_b: Instr = LoadStoreAcquireReleaseFixed | 0x00008000;
pub const LDAXR_b: Instr = LoadStoreAcquireReleaseFixed | 0x00408000;
pub const STLR_b: Instr = LoadStoreAcquireReleaseFixed | 0x00808000;
pub const LDAR_b: Instr = LoadStoreAcquireReleaseFixed | 0x00C08000;
pub const STLXR_h: Instr = LoadStoreAcquireReleaseFixed | 0x40008000;
pub const LDAXR_h: Instr = LoadStoreAcquireReleaseFixed | 0x40408000;
pub const STLR_h: Instr = LoadStoreAcquireReleaseFixed | 0x40808000;
pub const LDAR_h: Instr = LoadStoreAcquireReleaseFixed | 0x40C08000;
pub const STLXR_w: Instr = LoadStoreAcquireReleaseFixed | 0x80008000;
pub const LDAXR_w: Instr = LoadStoreAcquireReleaseFixed | 0x80408000;
pub const STLR_w: Instr = LoadStoreAcquireReleaseFixed | 0x80808000;
pub const LDAR_w: Instr = LoadStoreAcquireReleaseFixed | 0x80C08000;
pub const STLXR_x: Instr = LoadStoreAcquireReleaseFixed | 0xC0008000;
pub const LDAXR_x: Instr = LoadStoreAcquireReleaseFixed | 0xC0408000;
pub const STLR_x: Instr = LoadStoreAcquireReleaseFixed | 0xC0808000;
pub const LDAR_x: Instr = LoadStoreAcquireReleaseFixed | 0xC0C08000;
pub const LSEBit_l: Instr = 0x00400000;
pub const LSEBit_o0: Instr = 0x00008000;
pub const LSEBit_sz: Instr = 0x40000000;
pub const CASFixed: Instr = LoadStoreAcquireReleaseFixed | 0x80A00000;
pub const CASBFixed: Instr = LoadStoreAcquireReleaseFixed | 0x00A00000;
pub const CASHFixed: Instr = LoadStoreAcquireReleaseFixed | 0x40A00000;
pub const CASPFixed: Instr = LoadStoreAcquireReleaseFixed | 0x00200000;
pub const CAS_w: Instr = CASFixed;
pub const CAS_x: Instr = CASFixed | LSEBit_sz;
pub const CASA_w: Instr = CASFixed | LSEBit_l;
pub const CASA_x: Instr = CASFixed | LSEBit_l | LSEBit_sz;
pub const CASL_w: Instr = CASFixed | LSEBit_o0;
pub const CASL_x: Instr = CASFixed | LSEBit_o0 | LSEBit_sz;
pub const CASAL_w: Instr = CASFixed | LSEBit_l | LSEBit_o0;
pub const CASAL_x: Instr = CASFixed | LSEBit_l | LSEBit_o0 | LSEBit_sz;
pub const CASP_w: Instr = CASPFixed;
pub const CASP_x: Instr = CASPFixed | LSEBit_sz;
pub const CASPA_w: Instr = CASPFixed | LSEBit_l;
pub const CASPA_x: Instr = CASPFixed | LSEBit_l | LSEBit_sz;
pub const CASPL_w: Instr = CASPFixed | LSEBit_o0;
pub const CASPL_x: Instr = CASPFixed | LSEBit_o0 | LSEBit_sz;
pub const CASPAL_w: Instr = CASPFixed | LSEBit_l | LSEBit_o0;
pub const CASPAL_x: Instr = CASPFixed | LSEBit_l | LSEBit_o0 | LSEBit_sz;
pub const CASB: Instr = CASBFixed;
pub const CASAB: Instr = CASBFixed | LSEBit_l;
pub const CASLB: Instr = CASBFixed | LSEBit_o0;
pub const CASALB: Instr = CASBFixed | LSEBit_l | LSEBit_o0;
pub const CASH: Instr = CASHFixed;
pub const CASAH: Instr = CASHFixed | LSEBit_l;
pub const CASLH: Instr = CASHFixed | LSEBit_o0;
pub const CASALH: Instr = CASHFixed | LSEBit_l | LSEBit_o0;

pub const AtomicMemoryFixed: Instr = 0x38200000;
pub const AtomicMemoryFMask: Instr = 0x3B200C00;
pub const AtomicMemoryMask: Instr = 0xFFE0FC00;
pub const SWPB: Instr = AtomicMemoryFixed | 0x00008000;
pub const SWPAB: Instr = AtomicMemoryFixed | 0x00808000;
pub const SWPLB: Instr = AtomicMemoryFixed | 0x00408000;
pub const SWPALB: Instr = AtomicMemoryFixed | 0x00C08000;
pub const SWPH: Instr = AtomicMemoryFixed | 0x40008000;
pub const SWPAH: Instr = AtomicMemoryFixed | 0x40808000;
pub const SWPLH: Instr = AtomicMemoryFixed | 0x40408000;
pub const SWPALH: Instr = AtomicMemoryFixed | 0x40C08000;
pub const SWP_w: Instr = AtomicMemoryFixed | 0x80008000;
pub const SWPA_w: Instr = AtomicMemoryFixed | 0x80808000;
pub const SWPL_w: Instr = AtomicMemoryFixed | 0x80408000;
pub const SWPAL_w: Instr = AtomicMemoryFixed | 0x80C08000;
pub const SWP_x: Instr = AtomicMemoryFixed | 0xC0008000;
pub const SWPA_x: Instr = AtomicMemoryFixed | 0xC0808000;
pub const SWPL_x: Instr = AtomicMemoryFixed | 0xC0408000;
pub const SWPAL_x: Instr = AtomicMemoryFixed | 0xC0C08000;
pub const AtomicMemorySimpleFMask: Instr = 0x3B208C00;
pub const AtomicMemorySimpleOpMask: Instr = 0x00007000;
pub const LDADDOp: Instr = 0x00000000;
pub const LDADDB: Instr = AtomicMemoryFixed | LDADDOp | 0x00000000;
pub const LDADDAB: Instr = AtomicMemoryFixed | LDADDOp | 0x00800000;
pub const LDADDLB: Instr = AtomicMemoryFixed | LDADDOp | 0x00400000;
pub const LDADDALB: Instr = AtomicMemoryFixed | LDADDOp | 0x00C00000;
pub const LDADDH: Instr = AtomicMemoryFixed | LDADDOp | 0x40000000;
pub const LDADDAH: Instr = AtomicMemoryFixed | LDADDOp | 0x40800000;
pub const LDADDLH: Instr = AtomicMemoryFixed | LDADDOp | 0x40400000;
pub const LDADDALH: Instr = AtomicMemoryFixed | LDADDOp | 0x40C00000;
pub const LDADD_w: Instr = AtomicMemoryFixed | LDADDOp | 0x80000000;
pub const LDADDA_w: Instr = AtomicMemoryFixed | LDADDOp | 0x80800000;
pub const LDADDL_w: Instr = AtomicMemoryFixed | LDADDOp | 0x80400000;
pub const LDADDAL_w: Instr = AtomicMemoryFixed | LDADDOp | 0x80C00000;
pub const LDADD_x: Instr = AtomicMemoryFixed | LDADDOp | 0xC0000000;
pub const LDADDA_x: Instr = AtomicMemoryFixed | LDADDOp | 0xC0800000;
pub const LDADDL_x: Instr = AtomicMemoryFixed | LDADDOp | 0xC0400000;
pub const LDADDAL_x: Instr = AtomicMemoryFixed | LDADDOp | 0xC0C00000;
pub const LDCLROp: Instr = 0x00001000;
pub const LDCLRB: Instr = AtomicMemoryFixed | LDCLROp | 0x00000000;
pub const LDCLRAB: Instr = AtomicMemoryFixed | LDCLROp | 0x00800000;
pub const LDCLRLB: Instr = AtomicMemoryFixed | LDCLROp | 0x00400000;
pub const LDCLRALB: Instr = AtomicMemoryFixed | LDCLROp | 0x00C00000;
pub const LDCLRH: Instr = AtomicMemoryFixed | LDCLROp | 0x40000000;
pub const LDCLRAH: Instr = AtomicMemoryFixed | LDCLROp | 0x40800000;
pub const LDCLRLH: Instr = AtomicMemoryFixed | LDCLROp | 0x40400000;
pub const LDCLRALH: Instr = AtomicMemoryFixed | LDCLROp | 0x40C00000;
pub const LDCLR_w: Instr = AtomicMemoryFixed | LDCLROp | 0x80000000;
pub const LDCLRA_w: Instr = AtomicMemoryFixed | LDCLROp | 0x80800000;
pub const LDCLRL_w: Instr = AtomicMemoryFixed | LDCLROp | 0x80400000;
pub const LDCLRAL_w: Instr = AtomicMemoryFixed | LDCLROp | 0x80C00000;
pub const LDCLR_x: Instr = AtomicMemoryFixed | LDCLROp | 0xC0000000;
pub const LDCLRA_x: Instr = AtomicMemoryFixed | LDCLROp | 0xC0800000;
pub const LDCLRL_x: Instr = AtomicMemoryFixed | LDCLROp | 0xC0400000;
pub const LDCLRAL_x: Instr = AtomicMemoryFixed | LDCLROp | 0xC0C00000;
pub const LDEOROp: Instr = 0x00002000;
pub const LDEORB: Instr = AtomicMemoryFixed | LDEOROp | 0x00000000;
pub const LDEORAB: Instr = AtomicMemoryFixed | LDEOROp | 0x00800000;
pub const LDEORLB: Instr = AtomicMemoryFixed | LDEOROp | 0x00400000;
pub const LDEORALB: Instr = AtomicMemoryFixed | LDEOROp | 0x00C00000;
pub const LDEORH: Instr = AtomicMemoryFixed | LDEOROp | 0x40000000;
pub const LDEORAH: Instr = AtomicMemoryFixed | LDEOROp | 0x40800000;
pub const LDEORLH: Instr = AtomicMemoryFixed | LDEOROp | 0x40400000;
pub const LDEORALH: Instr = AtomicMemoryFixed | LDEOROp | 0x40C00000;
pub const LDEOR_w: Instr = AtomicMemoryFixed | LDEOROp | 0x80000000;
pub const LDEORA_w: Instr = AtomicMemoryFixed | LDEOROp | 0x80800000;
pub const LDEORL_w: Instr = AtomicMemoryFixed | LDEOROp | 0x80400000;
pub const LDEORAL_w: Instr = AtomicMemoryFixed | LDEOROp | 0x80C00000;
pub const LDEOR_x: Instr = AtomicMemoryFixed | LDEOROp | 0xC0000000;
pub const LDEORA_x: Instr = AtomicMemoryFixed | LDEOROp | 0xC0800000;
pub const LDEORL_x: Instr = AtomicMemoryFixed | LDEOROp | 0xC0400000;
pub const LDEORAL_x: Instr = AtomicMemoryFixed | LDEOROp | 0xC0C00000;
pub const LDSETOp: Instr = 0x00003000;
pub const LDSETB: Instr = AtomicMemoryFixed | LDSETOp | 0x00000000;
pub const LDSETAB: Instr = AtomicMemoryFixed | LDSETOp | 0x00800000;
pub const LDSETLB: Instr = AtomicMemoryFixed | LDSETOp | 0x00400000;
pub const LDSETALB: Instr = AtomicMemoryFixed | LDSETOp | 0x00C00000;
pub const LDSETH: Instr = AtomicMemoryFixed | LDSETOp | 0x40000000;
pub const LDSETAH: Instr = AtomicMemoryFixed | LDSETOp | 0x40800000;
pub const LDSETLH: Instr = AtomicMemoryFixed | LDSETOp | 0x40400000;
pub const LDSETALH: Instr = AtomicMemoryFixed | LDSETOp | 0x40C00000;
pub const LDSET_w: Instr = AtomicMemoryFixed | LDSETOp | 0x80000000;
pub const LDSETA_w: Instr = AtomicMemoryFixed | LDSETOp | 0x80800000;
pub const LDSETL_w: Instr = AtomicMemoryFixed | LDSETOp | 0x80400000;
pub const LDSETAL_w: Instr = AtomicMemoryFixed | LDSETOp | 0x80C00000;
pub const LDSET_x: Instr = AtomicMemoryFixed | LDSETOp | 0xC0000000;
pub const LDSETA_x: Instr = AtomicMemoryFixed | LDSETOp | 0xC0800000;
pub const LDSETL_x: Instr = AtomicMemoryFixed | LDSETOp | 0xC0400000;
pub const LDSETAL_x: Instr = AtomicMemoryFixed | LDSETOp | 0xC0C00000;
pub const LDSMAXOp: Instr = 0x00004000;
pub const LDSMAXB: Instr = AtomicMemoryFixed | LDSMAXOp | 0x00000000;
pub const LDSMAXAB: Instr = AtomicMemoryFixed | LDSMAXOp | 0x00800000;
pub const LDSMAXLB: Instr = AtomicMemoryFixed | LDSMAXOp | 0x00400000;
pub const LDSMAXALB: Instr = AtomicMemoryFixed | LDSMAXOp | 0x00C00000;
pub const LDSMAXH: Instr = AtomicMemoryFixed | LDSMAXOp | 0x40000000;
pub const LDSMAXAH: Instr = AtomicMemoryFixed | LDSMAXOp | 0x40800000;
pub const LDSMAXLH: Instr = AtomicMemoryFixed | LDSMAXOp | 0x40400000;
pub const LDSMAXALH: Instr = AtomicMemoryFixed | LDSMAXOp | 0x40C00000;
pub const LDSMAX_w: Instr = AtomicMemoryFixed | LDSMAXOp | 0x80000000;
pub const LDSMAXA_w: Instr = AtomicMemoryFixed | LDSMAXOp | 0x80800000;
pub const LDSMAXL_w: Instr = AtomicMemoryFixed | LDSMAXOp | 0x80400000;
pub const LDSMAXAL_w: Instr = AtomicMemoryFixed | LDSMAXOp | 0x80C00000;
pub const LDSMAX_x: Instr = AtomicMemoryFixed | LDSMAXOp | 0xC0000000;
pub const LDSMAXA_x: Instr = AtomicMemoryFixed | LDSMAXOp | 0xC0800000;
pub const LDSMAXL_x: Instr = AtomicMemoryFixed | LDSMAXOp | 0xC0400000;
pub const LDSMAXAL_x: Instr = AtomicMemoryFixed | LDSMAXOp | 0xC0C00000;
pub const LDSMINOp: Instr = 0x00005000;
pub const LDSMINB: Instr = AtomicMemoryFixed | LDSMINOp | 0x00000000;
pub const LDSMINAB: Instr = AtomicMemoryFixed | LDSMINOp | 0x00800000;
pub const LDSMINLB: Instr = AtomicMemoryFixed | LDSMINOp | 0x00400000;
pub const LDSMINALB: Instr = AtomicMemoryFixed | LDSMINOp | 0x00C00000;
pub const LDSMINH: Instr = AtomicMemoryFixed | LDSMINOp | 0x40000000;
pub const LDSMINAH: Instr = AtomicMemoryFixed | LDSMINOp | 0x40800000;
pub const LDSMINLH: Instr = AtomicMemoryFixed | LDSMINOp | 0x40400000;
pub const LDSMINALH: Instr = AtomicMemoryFixed | LDSMINOp | 0x40C00000;
pub const LDSMIN_w: Instr = AtomicMemoryFixed | LDSMINOp | 0x80000000;
pub const LDSMINA_w: Instr = AtomicMemoryFixed | LDSMINOp | 0x80800000;
pub const LDSMINL_w: Instr = AtomicMemoryFixed | LDSMINOp | 0x80400000;
pub const LDSMINAL_w: Instr = AtomicMemoryFixed | LDSMINOp | 0x80C00000;
pub const LDSMIN_x: Instr = AtomicMemoryFixed | LDSMINOp | 0xC0000000;
pub const LDSMINA_x: Instr = AtomicMemoryFixed | LDSMINOp | 0xC0800000;
pub const LDSMINL_x: Instr = AtomicMemoryFixed | LDSMINOp | 0xC0400000;
pub const LDSMINAL_x: Instr = AtomicMemoryFixed | LDSMINOp | 0xC0C00000;
pub const LDUMAXOp: Instr = 0x00006000;
pub const LDUMAXB: Instr = AtomicMemoryFixed | LDUMAXOp | 0x00000000;
pub const LDUMAXAB: Instr = AtomicMemoryFixed | LDUMAXOp | 0x00800000;
pub const LDUMAXLB: Instr = AtomicMemoryFixed | LDUMAXOp | 0x00400000;
pub const LDUMAXALB: Instr = AtomicMemoryFixed | LDUMAXOp | 0x00C00000;
pub const LDUMAXH: Instr = AtomicMemoryFixed | LDUMAXOp | 0x40000000;
pub const LDUMAXAH: Instr = AtomicMemoryFixed | LDUMAXOp | 0x40800000;
pub const LDUMAXLH: Instr = AtomicMemoryFixed | LDUMAXOp | 0x40400000;
pub const LDUMAXALH: Instr = AtomicMemoryFixed | LDUMAXOp | 0x40C00000;
pub const LDUMAX_w: Instr = AtomicMemoryFixed | LDUMAXOp | 0x80000000;
pub const LDUMAXA_w: Instr = AtomicMemoryFixed | LDUMAXOp | 0x80800000;
pub const LDUMAXL_w: Instr = AtomicMemoryFixed | LDUMAXOp | 0x80400000;
pub const LDUMAXAL_w: Instr = AtomicMemoryFixed | LDUMAXOp | 0x80C00000;
pub const LDUMAX_x: Instr = AtomicMemoryFixed | LDUMAXOp | 0xC0000000;
pub const LDUMAXA_x: Instr = AtomicMemoryFixed | LDUMAXOp | 0xC0800000;
pub const LDUMAXL_x: Instr = AtomicMemoryFixed | LDUMAXOp | 0xC0400000;
pub const LDUMAXAL_x: Instr = AtomicMemoryFixed | LDUMAXOp | 0xC0C00000;
pub const LDUMINOp: Instr = 0x00007000;
pub const LDUMINB: Instr = AtomicMemoryFixed | LDUMINOp | 0x00000000;
pub const LDUMINAB: Instr = AtomicMemoryFixed | LDUMINOp | 0x00800000;
pub const LDUMINLB: Instr = AtomicMemoryFixed | LDUMINOp | 0x00400000;
pub const LDUMINALB: Instr = AtomicMemoryFixed | LDUMINOp | 0x00C00000;
pub const LDUMINH: Instr = AtomicMemoryFixed | LDUMINOp | 0x40000000;
pub const LDUMINAH: Instr = AtomicMemoryFixed | LDUMINOp | 0x40800000;
pub const LDUMINLH: Instr = AtomicMemoryFixed | LDUMINOp | 0x40400000;
pub const LDUMINALH: Instr = AtomicMemoryFixed | LDUMINOp | 0x40C00000;
pub const LDUMIN_w: Instr = AtomicMemoryFixed | LDUMINOp | 0x80000000;
pub const LDUMINA_w: Instr = AtomicMemoryFixed | LDUMINOp | 0x80800000;
pub const LDUMINL_w: Instr = AtomicMemoryFixed | LDUMINOp | 0x80400000;
pub const LDUMINAL_w: Instr = AtomicMemoryFixed | LDUMINOp | 0x80C00000;
pub const LDUMIN_x: Instr = AtomicMemoryFixed | LDUMINOp | 0xC0000000;
pub const LDUMINA_x: Instr = AtomicMemoryFixed | LDUMINOp | 0xC0800000;
pub const LDUMINL_x: Instr = AtomicMemoryFixed | LDUMINOp | 0xC0400000;
pub const LDUMINAL_x: Instr = AtomicMemoryFixed | LDUMINOp | 0xC0C00000;

pub const ImmNEONImmhImmb_offset: u32 = 16;
pub const ImmNEONImmhImmb_width: u32 = 7;
pub const ImmNEONImmh_offset: u32 = 19;
pub const ImmNEONImmh_width: u32 = 4;
pub const ImmNEONImmb_offset: u32 = 16;
pub const ImmNEONImmb_width: u32 = 3;
pub const Rs_offset: u32 = 16;
pub const Rt2_width: u32 = 5;
pub const Rt2_mask: Instr = ((1 << Rt2_width) - 1) << Rt2_offset;

pub const NEONFormatFieldMask: Instr = 0x40C00000;
pub const NEON_Q: Instr = 0x40000000;
pub const NEON_sz: Instr = 0x00400000;
pub const NEON_8B: Instr = 0x00000000;
pub const NEON_16B: Instr = NEON_8B | NEON_Q;
pub const NEON_4H: Instr = 0x00400000;
pub const NEON_8H: Instr = NEON_4H | NEON_Q;
pub const NEON_2S: Instr = 0x00800000;
pub const NEON_4S: Instr = NEON_2S | NEON_Q;
pub const NEON_1D: Instr = 0x00C00000;
pub const NEON_2D: Instr = 0x00C00000 | NEON_Q;

pub const NEONFPFormatFieldMask: Instr = 0x40400000;
pub const NEON_FP_4H: Instr = 0x00000000;
pub const NEON_FP_8H: Instr = NEON_Q;
pub const NEON_FP_2S: Instr = FP32;
pub const NEON_FP_4S: Instr = FP32 | NEON_Q;
pub const NEON_FP_2D: Instr = FP64 | NEON_Q;

pub const NEONLSFormatFieldMask: Instr = 0x40000C00;
pub const LS_NEON_8B: Instr = 0x00000000;
pub const LS_NEON_16B: Instr = LS_NEON_8B | NEON_Q;
pub const LS_NEON_4H: Instr = 0x00000400;
pub const LS_NEON_8H: Instr = LS_NEON_4H | NEON_Q;
pub const LS_NEON_2S: Instr = 0x00000800;
pub const LS_NEON_4S: Instr = LS_NEON_2S | NEON_Q;
pub const LS_NEON_1D: Instr = 0x00000C00;
pub const LS_NEON_2D: Instr = LS_NEON_1D | NEON_Q;

pub const NEONScalarFormatFieldMask: Instr = 0x00C00000;
pub const NEONScalar: Instr = 0x10000000;
pub const NEON_B: Instr = 0x00000000;
pub const NEON_H: Instr = 0x00400000;
pub const NEON_S: Instr = 0x00800000;
pub const NEON_D: Instr = 0x00C00000;

pub const NEON3SameFixed: Instr = 0x0E200400;
pub const NEON3SameFMask: Instr = 0x9F200400;
pub const NEON3SameMask: Instr = 0xBF20FC00;
pub const NEON3SameUBit: Instr = 0x20000000;
pub const NEON_ADD: Instr = NEON3SameFixed | 0x00008000;
pub const NEON_ADDP: Instr = NEON3SameFixed | 0x0000B800;
pub const NEON_SHADD: Instr = NEON3SameFixed | 0x00000000;
pub const NEON_SHSUB: Instr = NEON3SameFixed | 0x00002000;
pub const NEON_SRHADD: Instr = NEON3SameFixed | 0x00001000;
pub const NEON_CMEQ: Instr = NEON3SameFixed | NEON3SameUBit | 0x00008800;
pub const NEON_CMGE: Instr = NEON3SameFixed | 0x00003800;
pub const NEON_CMGT: Instr = NEON3SameFixed | 0x00003000;
pub const NEON_CMHI: Instr = NEON3SameFixed | NEON3SameUBit | NEON_CMGT;
pub const NEON_CMHS: Instr = NEON3SameFixed | NEON3SameUBit | NEON_CMGE;
pub const NEON_CMTST: Instr = NEON3SameFixed | 0x00008800;
pub const NEON_MLA: Instr = NEON3SameFixed | 0x00009000;
pub const NEON_MLS: Instr = NEON3SameFixed | 0x20009000;
pub const NEON_MUL: Instr = NEON3SameFixed | 0x00009800;
pub const NEON_PMUL: Instr = NEON3SameFixed | 0x20009800;
pub const NEON_SRSHL: Instr = NEON3SameFixed | 0x00005000;
pub const NEON_SQSHL: Instr = NEON3SameFixed | 0x00004800;
pub const NEON_SQRSHL: Instr = NEON3SameFixed | 0x00005800;
pub const NEON_SSHL: Instr = NEON3SameFixed | 0x00004000;
pub const NEON_SMAX: Instr = NEON3SameFixed | 0x00006000;
pub const NEON_SMAXP: Instr = NEON3SameFixed | 0x0000A000;
pub const NEON_SMIN: Instr = NEON3SameFixed | 0x00006800;
pub const NEON_SMINP: Instr = NEON3SameFixed | 0x0000A800;
pub const NEON_SABD: Instr = NEON3SameFixed | 0x00007000;
pub const NEON_SABA: Instr = NEON3SameFixed | 0x00007800;
pub const NEON_UABD: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SABD;
pub const NEON_UABA: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SABA;
pub const NEON_SQADD: Instr = NEON3SameFixed | 0x00000800;
pub const NEON_SQSUB: Instr = NEON3SameFixed | 0x00002800;
pub const NEON_SUB: Instr = NEON3SameFixed | NEON3SameUBit | 0x00008000;
pub const NEON_UHADD: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SHADD;
pub const NEON_UHSUB: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SHSUB;
pub const NEON_URHADD: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SRHADD;
pub const NEON_UMAX: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SMAX;
pub const NEON_UMAXP: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SMAXP;
pub const NEON_UMIN: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SMIN;
pub const NEON_UMINP: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SMINP;
pub const NEON_URSHL: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SRSHL;
pub const NEON_UQADD: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SQADD;
pub const NEON_UQRSHL: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SQRSHL;
pub const NEON_UQSHL: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SQSHL;
pub const NEON_UQSUB: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SQSUB;
pub const NEON_USHL: Instr = NEON3SameFixed | NEON3SameUBit | NEON_SSHL;
pub const NEON_SQDMULH: Instr = NEON3SameFixed | 0x0000B000;
pub const NEON_SQRDMULH: Instr = NEON3SameFixed | 0x2000B000;

pub const NEON3SameLogicalFixed: Instr = NEON3SameFixed | 0x00001800;
pub const NEON3SameLogicalMask: Instr = 0xBFE0FC00;
pub const NEON3SameLogicalFormatMask: Instr = NEON_Q;
pub const NEON_AND: Instr = NEON3SameLogicalFixed | 0x00000000;
pub const NEON_ORR: Instr = NEON3SameLogicalFixed | 0x00A00000;
pub const NEON_ORN: Instr = NEON3SameLogicalFixed | 0x00C00000;
pub const NEON_EOR: Instr = NEON3SameLogicalFixed | 0x20000000;
pub const NEON_BIC: Instr = NEON3SameLogicalFixed | 0x00400000;
pub const NEON_BIF: Instr = NEON3SameLogicalFixed | 0x20C00000;
pub const NEON_BIT: Instr = NEON3SameLogicalFixed | 0x20800000;
pub const NEON_BSL: Instr = NEON3SameLogicalFixed | 0x20400000;

pub const NEON3SameFPFixed: Instr = NEON3SameFixed | 0x0000C000;
pub const NEON_FADD: Instr = NEON3SameFixed | 0x0000D000;
pub const NEON_FSUB: Instr = NEON3SameFixed | 0x0080D000;
pub const NEON_FMUL: Instr = NEON3SameFixed | 0x2000D800;
pub const NEON_FDIV: Instr = NEON3SameFixed | 0x2000F800;
pub const NEON_FMAX: Instr = NEON3SameFixed | 0x0000F000;
pub const NEON_FMAXNM: Instr = NEON3SameFixed | 0x0000C000;
pub const NEON_FMAXP: Instr = NEON3SameFixed | 0x2000F000;
pub const NEON_FMAXNMP: Instr = NEON3SameFixed | 0x2000C000;
pub const NEON_FMIN: Instr = NEON3SameFixed | 0x0080F000;
pub const NEON_FMINNM: Instr = NEON3SameFixed | 0x0080C000;
pub const NEON_FMINP: Instr = NEON3SameFixed | 0x2080F000;
pub const NEON_FMINNMP: Instr = NEON3SameFixed | 0x2080C000;
pub const NEON_FMLA: Instr = NEON3SameFixed | 0x0000C800;
pub const NEON_FMLS: Instr = NEON3SameFixed | 0x0080C800;
pub const NEON_FMULX: Instr = NEON3SameFixed | 0x0000D800;
pub const NEON_FRECPS: Instr = NEON3SameFixed | 0x0000F800;
pub const NEON_FRSQRTS: Instr = NEON3SameFixed | 0x0080F800;
pub const NEON_FABD: Instr = NEON3SameFixed | 0x2080D000;
pub const NEON_FADDP: Instr = NEON3SameFixed | 0x2000D000;
pub const NEON_FCMEQ: Instr = NEON3SameFixed | 0x0000E000;
pub const NEON_FCMGE: Instr = NEON3SameFixed | 0x2000E000;
pub const NEON_FCMGT: Instr = NEON3SameFixed | 0x2080E000;
pub const NEON_FACGE: Instr = NEON3SameFixed | 0x2000E800;
pub const NEON_FACGT: Instr = NEON3SameFixed | 0x2080E800;

pub const NEON3DifferentFixed: Instr = 0x0E200000;
pub const NEON3DifferentFMask: Instr = 0x9F200C00;
pub const NEON3DifferentMask: Instr = 0xFF20FC00;
pub const NEON_ADDHN: Instr = NEON3DifferentFixed | 0x00004000;
pub const NEON_ADDHN2: Instr = NEON_ADDHN | NEON_Q;
pub const NEON_PMULL: Instr = NEON3DifferentFixed | 0x0000E000;
pub const NEON_PMULL2: Instr = NEON_PMULL | NEON_Q;
pub const NEON_RADDHN: Instr = NEON3DifferentFixed | 0x20004000;
pub const NEON_RADDHN2: Instr = NEON_RADDHN | NEON_Q;
pub const NEON_RSUBHN: Instr = NEON3DifferentFixed | 0x20006000;
pub const NEON_RSUBHN2: Instr = NEON_RSUBHN | NEON_Q;
pub const NEON_SABAL: Instr = NEON3DifferentFixed | 0x00005000;
pub const NEON_SABAL2: Instr = NEON_SABAL | NEON_Q;
pub const NEON_SABDL: Instr = NEON3DifferentFixed | 0x00007000;
pub const NEON_SABDL2: Instr = NEON_SABDL | NEON_Q;
pub const NEON_SADDL: Instr = NEON3DifferentFixed | 0x00000000;
pub const NEON_SADDL2: Instr = NEON_SADDL | NEON_Q;
pub const NEON_SADDW: Instr = NEON3DifferentFixed | 0x00001000;
pub const NEON_SADDW2: Instr = NEON_SADDW | NEON_Q;
pub const NEON_SMLAL: Instr = NEON3DifferentFixed | 0x00008000;
pub const NEON_SMLAL2: Instr = NEON_SMLAL | NEON_Q;
pub const NEON_SMLSL: Instr = NEON3DifferentFixed | 0x0000A000;
pub const NEON_SMLSL2: Instr = NEON_SMLSL | NEON_Q;
pub const NEON_SMULL: Instr = NEON3DifferentFixed | 0x0000C000;
pub const NEON_SMULL2: Instr = NEON_SMULL | NEON_Q;
pub const NEON_SSUBL: Instr = NEON3DifferentFixed | 0x00002000;
pub const NEON_SSUBL2: Instr = NEON_SSUBL | NEON_Q;
pub const NEON_SSUBW: Instr = NEON3DifferentFixed | 0x00003000;
pub const NEON_SSUBW2: Instr = NEON_SSUBW | NEON_Q;
pub const NEON_SQDMLAL: Instr = NEON3DifferentFixed | 0x00009000;
pub const NEON_SQDMLAL2: Instr = NEON_SQDMLAL | NEON_Q;
pub const NEON_SQDMLSL: Instr = NEON3DifferentFixed | 0x0000B000;
pub const NEON_SQDMLSL2: Instr = NEON_SQDMLSL | NEON_Q;
pub const NEON_SQDMULL: Instr = NEON3DifferentFixed | 0x0000D000;
pub const NEON_SQDMULL2: Instr = NEON_SQDMULL | NEON_Q;
pub const NEON_SUBHN: Instr = NEON3DifferentFixed | 0x00006000;
pub const NEON_SUBHN2: Instr = NEON_SUBHN | NEON_Q;
pub const NEON_UABAL: Instr = NEON_SABAL | NEON3SameUBit;
pub const NEON_UABAL2: Instr = NEON_UABAL | NEON_Q;
pub const NEON_UABDL: Instr = NEON_SABDL | NEON3SameUBit;
pub const NEON_UABDL2: Instr = NEON_UABDL | NEON_Q;
pub const NEON_UADDL: Instr = NEON_SADDL | NEON3SameUBit;
pub const NEON_UADDL2: Instr = NEON_UADDL | NEON_Q;
pub const NEON_UADDW: Instr = NEON_SADDW | NEON3SameUBit;
pub const NEON_UADDW2: Instr = NEON_UADDW | NEON_Q;
pub const NEON_UMLAL: Instr = NEON_SMLAL | NEON3SameUBit;
pub const NEON_UMLAL2: Instr = NEON_UMLAL | NEON_Q;
pub const NEON_UMLSL: Instr = NEON_SMLSL | NEON3SameUBit;
pub const NEON_UMLSL2: Instr = NEON_UMLSL | NEON_Q;
pub const NEON_UMULL: Instr = NEON_SMULL | NEON3SameUBit;
pub const NEON_UMULL2: Instr = NEON_UMULL | NEON_Q;
pub const NEON_USUBL: Instr = NEON_SSUBL | NEON3SameUBit;
pub const NEON_USUBL2: Instr = NEON_USUBL | NEON_Q;
pub const NEON_USUBW: Instr = NEON_SSUBW | NEON3SameUBit;
pub const NEON_USUBW2: Instr = NEON_USUBW | NEON_Q;

pub const NEONPermFixed: Instr = 0x0E000800;
pub const NEONPermFMask: Instr = 0xBF208C00;
pub const NEONPermMask: Instr = 0x3F20FC00;
pub const NEON_UZP1: Instr = NEONPermFixed | 0x00001000;
pub const NEON_TRN1: Instr = NEONPermFixed | 0x00002000;
pub const NEON_ZIP1: Instr = NEONPermFixed | 0x00003000;
pub const NEON_UZP2: Instr = NEONPermFixed | 0x00005000;
pub const NEON_TRN2: Instr = NEONPermFixed | 0x00006000;
pub const NEON_ZIP2: Instr = NEONPermFixed | 0x00007000;

pub const NEON2RegMiscFixed: Instr = 0x0E200800;
pub const NEON2RegMiscFMask: Instr = 0x9F260C00;
pub const NEON2RegMiscMask: Instr = 0xBF3FFC00;
pub const NEON2RegMiscUBit: Instr = 0x20000000;
pub const NEON_REV64: Instr = NEON2RegMiscFixed | 0x00000000;
pub const NEON_REV32: Instr = NEON2RegMiscFixed | 0x20000000;
pub const NEON_REV16: Instr = NEON2RegMiscFixed | 0x00001000;
pub const NEON_SADDLP: Instr = NEON2RegMiscFixed | 0x00002000;
pub const NEON_UADDLP: Instr = NEON_SADDLP | NEON2RegMiscUBit;
pub const NEON_SUQADD: Instr = NEON2RegMiscFixed | 0x00003000;
pub const NEON_USQADD: Instr = NEON_SUQADD | NEON2RegMiscUBit;
pub const NEON_CLS: Instr = NEON2RegMiscFixed | 0x00004000;
pub const NEON_CLZ: Instr = NEON2RegMiscFixed | 0x20004000;
pub const NEON_CNT: Instr = NEON2RegMiscFixed | 0x00005000;
pub const NEON_RBIT_NOT: Instr = NEON2RegMiscFixed | 0x20005000;
pub const NEON_SADALP: Instr = NEON2RegMiscFixed | 0x00006000;
pub const NEON_UADALP: Instr = NEON_SADALP | NEON2RegMiscUBit;
pub const NEON_SQABS: Instr = NEON2RegMiscFixed | 0x00007000;
pub const NEON_SQNEG: Instr = NEON2RegMiscFixed | 0x20007000;
pub const NEON_CMGT_zero: Instr = NEON2RegMiscFixed | 0x00008000;
pub const NEON_CMGE_zero: Instr = NEON2RegMiscFixed | 0x20008000;
pub const NEON_CMEQ_zero: Instr = NEON2RegMiscFixed | 0x00009000;
pub const NEON_CMLE_zero: Instr = NEON2RegMiscFixed | 0x20009000;
pub const NEON_CMLT_zero: Instr = NEON2RegMiscFixed | 0x0000A000;
pub const NEON_ABS: Instr = NEON2RegMiscFixed | 0x0000B000;
pub const NEON_NEG: Instr = NEON2RegMiscFixed | 0x2000B000;
pub const NEON_XTN: Instr = NEON2RegMiscFixed | 0x00012000;
pub const NEON_SQXTUN: Instr = NEON2RegMiscFixed | 0x20012000;
pub const NEON_SHLL: Instr = NEON2RegMiscFixed | 0x20013000;
pub const NEON_SQXTN: Instr = NEON2RegMiscFixed | 0x00014000;
pub const NEON_UQXTN: Instr = NEON_SQXTN | NEON2RegMiscUBit;

pub const NEONAcrossLanesFixed: Instr = 0x0E300800;
pub const NEONAcrossLanesFMask: Instr = 0x9F3E0C00;
pub const NEONAcrossLanesMask: Instr = 0xBF3FFC00;
pub const NEON_ADDV: Instr = NEONAcrossLanesFixed | 0x0001B000;
pub const NEON_SADDLV: Instr = NEONAcrossLanesFixed | 0x00003000;
pub const NEON_UADDLV: Instr = NEONAcrossLanesFixed | 0x20003000;
pub const NEON_SMAXV: Instr = NEONAcrossLanesFixed | 0x0000A000;
pub const NEON_SMINV: Instr = NEONAcrossLanesFixed | 0x0001A000;
pub const NEON_UMAXV: Instr = NEONAcrossLanesFixed | 0x2000A000;
pub const NEON_UMINV: Instr = NEONAcrossLanesFixed | 0x2001A000;
pub const NEONAcrossLanesFPFixed: Instr = NEONAcrossLanesFixed | 0x0000C000;
pub const NEON_FMAXV: Instr = NEONAcrossLanesFPFixed | 0x2000F000;
pub const NEON_FMINV: Instr = NEONAcrossLanesFPFixed | 0x2080F000;
pub const NEON_FMAXNMV: Instr = NEONAcrossLanesFPFixed | 0x2000C000;
pub const NEON_FMINNMV: Instr = NEONAcrossLanesFPFixed | 0x2080C000;

pub const SystemSysRegFixed: Instr = 0xD5100000;
pub const MRS: Instr = SystemSysRegFixed | 0x00200000;
pub const MSR: Instr = SystemSysRegFixed | 0x00000000;

pub const MemBarrierFixed: Instr = 0xD503309F;
pub const MemBarrierFMask: Instr = 0xFFFFF09F;
pub const MemBarrierMask: Instr = 0xFFFFF0FF;
pub const DSB: Instr = MemBarrierFixed | 0x00000000;
pub const DMB: Instr = MemBarrierFixed | 0x00000020;
pub const ISB: Instr = MemBarrierFixed | 0x00000040;

pub const SystemPAuthFixed: Instr = 0xD503211F;
pub const PACIB1716: Instr = SystemPAuthFixed | 0x00000140;
pub const AUTIB1716: Instr = SystemPAuthFixed | 0x000001C0;
pub const PACIBSP: Instr = SystemPAuthFixed | 0x00000360;
pub const AUTIBSP: Instr = SystemPAuthFixed | 0x000003E0;

pub const NEONLoad_offset: u32 = 22;
pub const NEONLoad_width: u32 = 1;
pub const PrefetchMode_offset: u32 = 0;
pub const PrefetchMode_width: u32 = 5;
pub const ImmBarrierDomain_offset: u32 = 10;
pub const ImmBarrierType_offset: u32 = 8;
pub const ImmSystemRegister_offset: u32 = 5;

pub const BTI_c: Instr = 34;
pub const BTI_j: Instr = 36;
pub const BTI_jc: Instr = 38;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BarrierDomain {
    OuterShareable = 0,
    NonShareable = 1,
    InnerShareable = 2,
    FullSystem = 3,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BarrierType {
    BarrierOther = 0,
    BarrierReads = 1,
    BarrierWrites = 2,
    BarrierAll = 3,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BranchTargetIdentifier {
    Bti,
    BtiCall,
    BtiJump,
    BtiJumpCall,
    None,
    Pacibsp,
}

pub const SysO0_offset: u32 = 19;
pub const SysOp1_offset: u32 = 16;
pub const SysOp2_offset: u32 = 5;
pub const CRn_offset: u32 = 12;
pub const CRm_offset: u32 = 8;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SystemRegister {
    NZCV = (((0x1 << SysO0_offset)
        | (0x3 << SysOp1_offset)
        | (0x4 << CRn_offset)
        | (0x2 << CRm_offset)
        | (0x0 << SysOp2_offset))
        >> ImmSystemRegister_offset) as isize,
    FPCR = (((0x1 << SysO0_offset)
        | (0x3 << SysOp1_offset)
        | (0x4 << CRn_offset)
        | (0x4 << CRm_offset)
        | (0x0 << SysOp2_offset))
        >> ImmSystemRegister_offset) as isize,
}

pub const NEONL_offset: u32 = 21;
pub const NEONM_offset: u32 = 20;
pub const NEONH_offset: u32 = 11;

pub const NEONByIndexedElementFixed: Instr = 0x0F000000;
pub const NEONByIndexedElementFMask: Instr = 0x9F000400;
pub const NEONByIndexedElementMask: Instr = 0xBF00F400;
pub const NEON_MUL_byelement: Instr = NEONByIndexedElementFixed | 0x00008000;
pub const NEON_MLA_byelement: Instr = NEONByIndexedElementFixed | 0x20000000;
pub const NEON_MLS_byelement: Instr = NEONByIndexedElementFixed | 0x20004000;
pub const NEON_SMULL_byelement: Instr = NEONByIndexedElementFixed | 0x0000A000;
pub const NEON_SMLAL_byelement: Instr = NEONByIndexedElementFixed | 0x00002000;
pub const NEON_SMLSL_byelement: Instr = NEONByIndexedElementFixed | 0x00006000;
pub const NEON_UMULL_byelement: Instr = NEONByIndexedElementFixed | 0x2000A000;
pub const NEON_UMLAL_byelement: Instr = NEONByIndexedElementFixed | 0x20002000;
pub const NEON_UMLSL_byelement: Instr = NEONByIndexedElementFixed | 0x20006000;
pub const NEON_SQDMULL_byelement: Instr = NEONByIndexedElementFixed | 0x0000B000;
pub const NEON_SQDMLAL_byelement: Instr = NEONByIndexedElementFixed | 0x00003000;
pub const NEON_SQDMLSL_byelement: Instr = NEONByIndexedElementFixed | 0x00007000;
pub const NEON_SQDMULH_byelement: Instr = NEONByIndexedElementFixed | 0x0000C000;
pub const NEON_SQRDMULH_byelement: Instr = NEONByIndexedElementFixed | 0x0000D000;
pub const NEONByIndexedElementFPFixed: Instr =
    NEONByIndexedElementFixed | 0x00800000;
pub const NEON_FMLA_byelement: Instr = NEONByIndexedElementFPFixed | 0x00001000;
pub const NEON_FMLS_byelement: Instr = NEONByIndexedElementFPFixed | 0x00005000;
pub const NEON_FMUL_byelement: Instr = NEONByIndexedElementFPFixed | 0x00009000;
pub const NEON_FMULX_byelement: Instr = NEONByIndexedElementFPFixed | 0x20009000;

pub const NEONAcrossLanesFPFMask: Instr = NEONAcrossLanesFMask | 0x0000C000;

pub const NEONShiftImmediateFixed: Instr = 0x0F000400;
pub const NEONShiftImmediateFMask: Instr = 0x9F800400;
pub const NEONShiftImmediateMask: Instr = 0xBF80FC00;
pub const NEON_SHL: Instr = NEONShiftImmediateFixed | 0x00005000;
pub const NEON_SSHLL: Instr = NEONShiftImmediateFixed | 0x0000A000;
pub const NEON_USHLL: Instr = NEONShiftImmediateFixed | 0x2000A000;
pub const NEON_SLI: Instr = NEONShiftImmediateFixed | 0x20005000;
pub const NEON_SRI: Instr = NEONShiftImmediateFixed | 0x20004000;
pub const NEON_SHRN: Instr = NEONShiftImmediateFixed | 0x00008000;
pub const NEON_RSHRN: Instr = NEONShiftImmediateFixed | 0x00008800;
pub const NEON_UQSHRN: Instr = NEONShiftImmediateFixed | 0x20009000;
pub const NEON_UQRSHRN: Instr = NEONShiftImmediateFixed | 0x20009800;
pub const NEON_SQSHRN: Instr = NEONShiftImmediateFixed | 0x00009000;
pub const NEON_SQRSHRN: Instr = NEONShiftImmediateFixed | 0x00009800;
pub const NEON_SQSHRUN: Instr = NEONShiftImmediateFixed | 0x20008000;
pub const NEON_SQRSHRUN: Instr = NEONShiftImmediateFixed | 0x20008800;
pub const NEON_SSHR: Instr = NEONShiftImmediateFixed | 0x00000000;
pub const NEON_SRSHR: Instr = NEONShiftImmediateFixed | 0x00002000;
pub const NEON_USHR: Instr = NEONShiftImmediateFixed | 0x20000000;
pub const NEON_URSHR: Instr = NEONShiftImmediateFixed | 0x20002000;
pub const NEON_SSRA: Instr = NEONShiftImmediateFixed | 0x00001000;
pub const NEON_SRSRA: Instr = NEONShiftImmediateFixed | 0x00003000;
pub const NEON_USRA: Instr = NEONShiftImmediateFixed | 0x20001000;
pub const NEON_URSRA: Instr = NEONShiftImmediateFixed | 0x20003000;
pub const NEON_SQSHLU: Instr = NEONShiftImmediateFixed | 0x20006000;
pub const NEON_SQSHL_imm: Instr = NEONShiftImmediateFixed | 0x00007000;
pub const NEON_UQSHL_imm: Instr = NEONShiftImmediateFixed | 0x20007000;
pub const NEON_SCVTF_imm: Instr = NEONShiftImmediateFixed | 0x0000E000;
pub const NEON_UCVTF_imm: Instr = NEONShiftImmediateFixed | 0x2000E000;
pub const NEON_FCVTZS_imm: Instr = NEONShiftImmediateFixed | 0x0000F800;
pub const NEON_FCVTZU_imm: Instr = NEONShiftImmediateFixed | 0x2000F800;

pub const NEON3SameHPMask: Instr = 0x0020C000;
pub const NEON2RegMiscHPFixed: Instr = 0x00180000;
pub const NEON_FABS: Instr = NEON2RegMiscFixed | 0x0080F000;
pub const NEON_FNEG: Instr = NEON2RegMiscFixed | 0x2080F000;
pub const NEON_FSQRT: Instr = NEON2RegMiscFixed | 0x2081F000;
pub const NEON_FRINTN: Instr = NEON2RegMiscFixed | 0x00018000;
pub const NEON_FRINTA: Instr = NEON2RegMiscFixed | 0x20018000;
pub const NEON_FRINTP: Instr = NEON2RegMiscFixed | 0x00818000;
pub const NEON_FRINTM: Instr = NEON2RegMiscFixed | 0x00019000;
pub const NEON_FRINTX: Instr = NEON2RegMiscFixed | 0x20019000;
pub const NEON_FRINTZ: Instr = NEON2RegMiscFixed | 0x00819000;
pub const NEON_FRINTI: Instr = NEON2RegMiscFixed | 0x20819000;
pub const NEON_FRSQRTE: Instr = NEON2RegMiscFixed | 0x2081D000;
pub const NEON_FRECPE: Instr = NEON2RegMiscFixed | 0x0081D000;
pub const NEON_FCVTN: Instr = NEON2RegMiscFixed | 0x00016000;
pub const NEON_FCVTXN: Instr = NEON2RegMiscFixed | 0x20016000;
pub const NEON_FCVTL: Instr = NEON2RegMiscFixed | 0x00017000;
pub const NEON_FCVTNS: Instr = NEON2RegMiscFixed | 0x0001A000;
pub const NEON_FCVTNU: Instr = NEON_FCVTNS | NEON2RegMiscUBit;
pub const NEON_FCVTPS: Instr = NEON2RegMiscFixed | 0x0081A000;
pub const NEON_FCVTPU: Instr = NEON_FCVTPS | NEON2RegMiscUBit;
pub const NEON_FCVTMS: Instr = NEON2RegMiscFixed | 0x0001B000;
pub const NEON_FCVTMU: Instr = NEON_FCVTMS | NEON2RegMiscUBit;
pub const NEON_FCVTZS: Instr = NEON2RegMiscFixed | 0x0081B000;
pub const NEON_FCVTZU: Instr = NEON_FCVTZS | NEON2RegMiscUBit;
pub const NEON_FCVTAS: Instr = NEON2RegMiscFixed | 0x0001C000;
pub const NEON_FCVTAU: Instr = NEON_FCVTAS | NEON2RegMiscUBit;
pub const NEON_FCMGT_zero: Instr = NEON2RegMiscFixed | 0x0080C000;
pub const NEON_FCMGE_zero: Instr = NEON2RegMiscFixed | 0x2080C000;
pub const NEON_FCMEQ_zero: Instr = NEON2RegMiscFixed | 0x0080D000;
pub const NEON_FCMLE_zero: Instr = NEON2RegMiscFixed | 0x2080D000;
pub const NEON_FCMLT_zero: Instr = NEON2RegMiscFixed | 0x0080E000;

pub const NEON_FRSQRTE_scalar: Instr = NEON_Q | NEONScalar | NEON_FRSQRTE;
pub const NEON_FRECPE_scalar: Instr = NEON_Q | NEONScalar | NEON_FRECPE;
pub const NEON_FCMEQ_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMEQ;
pub const NEON_FCMGE_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMGE;
pub const NEON_FCMGT_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMGT;
pub const NEON_FACGE_scalar: Instr = NEON_Q | NEONScalar | NEON_FACGE;
pub const NEON_FACGT_scalar: Instr = NEON_Q | NEONScalar | NEON_FACGT;
pub const NEON_FMULX_scalar: Instr = NEON_Q | NEONScalar | NEON_FMULX;
pub const NEON_FRECPS_scalar: Instr = NEON_Q | NEONScalar | NEON_FRECPS;
pub const NEON_FRSQRTS_scalar: Instr = NEON_Q | NEONScalar | NEON_FRSQRTS;
pub const NEON_FABD_scalar: Instr = NEON_Q | NEONScalar | NEON_FABD;

pub const NEONSize_offset: u32 = 22;
pub const NEONScalar2RegMiscFixed: Instr = 0x5E200800;
pub const NEON_SQXTN_scalar: Instr = NEON_Q | NEONScalar | NEON_SQXTN;
pub const NEON_UQXTN_scalar: Instr = NEON_Q | NEONScalar | NEON_UQXTN;
pub const NEON_SQXTUN_scalar: Instr = NEON_Q | NEONScalar | NEON_SQXTUN;
pub const NEON_FRECPX_scalar: Instr = NEONScalar2RegMiscFixed | 0x0081F000;
pub const NEON_FCVTXN_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTXN;
pub const NEON_URSQRTE: Instr = NEON2RegMiscFixed | 0x2081C000;
pub const NEON_URECPE: Instr = NEON2RegMiscFixed | 0x0081C000;

pub const NEONTableFixed: Instr = 0x0E000000;
pub const NEONTableFMask: Instr = 0xBF208C00;
pub const NEONTableExt: Instr = 0x00001000;
pub const NEONTableMask: Instr = 0xBF20FC00;
pub const NEON_TBL_1v: Instr = NEONTableFixed | 0x00000000;
pub const NEON_TBL_2v: Instr = NEONTableFixed | 0x00002000;
pub const NEON_TBL_3v: Instr = NEONTableFixed | 0x00004000;
pub const NEON_TBL_4v: Instr = NEONTableFixed | 0x00006000;
pub const NEON_TBX_1v: Instr = NEON_TBL_1v | NEONTableExt;

pub const FPConditionalCompareFixed: Instr = 0x1E200400;
pub const FPConditionalCompareMask: Instr = 0xFFE00C10;
pub const FCCMP_s: Instr = FPConditionalCompareFixed | 0x00000000;
pub const FCCMP_d: Instr = FPConditionalCompareFixed | FP64 | 0x00000000;
pub const FCCMP: Instr = FCCMP_s;
pub const FPConditionalSelectFixed: Instr = 0x1E200C00;
pub const FPConditionalSelectMask: Instr = 0xFFE00C00;
pub const FCSEL_s: Instr = FPConditionalSelectFixed | 0x00000000;
pub const FCSEL_d: Instr = FPConditionalSelectFixed | FP64 | 0x00000000;
pub const FCSEL: Instr = FCSEL_s;

pub const ImmNEONExt_offset: u32 = 11;
pub const ImmNEON5_offset: u32 = 16;
pub const ImmNEON4_offset: u32 = 11;
pub const ImmNEONabc_offset: u32 = 16;
pub const ImmNEONdefgh_offset: u32 = 5;
pub const NEONModImmOp_offset: u32 = 29;
pub const NEONCmode_offset: u32 = 12;

pub const NEONCopyFixed: Instr = 0x0E000400;
pub const NEONCopyFMask: Instr = 0x9FE08400;
pub const NEONCopyMask: Instr = 0x3FE08400;
pub const NEON_INS_ELEMENT: Instr = NEONCopyFixed | 0x60000000;
pub const NEON_INS_GENERAL: Instr = NEONCopyFixed | 0x40001800;
pub const NEON_DUP_ELEMENT: Instr = NEONCopyFixed | 0x00000000;
pub const NEON_DUP_GENERAL: Instr = NEONCopyFixed | 0x00000800;
pub const NEON_SMOV: Instr = NEONCopyFixed | 0x00002800;
pub const NEON_UMOV: Instr = NEONCopyFixed | 0x00003800;
pub const NEON_DUP_ELEMENT_scalar: Instr = NEON_Q | NEONScalar | NEON_DUP_ELEMENT;

pub const NEONModifiedImmediateFixed: Instr = 0x0F000400;
pub const NEONModifiedImmediate_MOVI: Instr = NEONModifiedImmediateFixed | 0x00000000;
pub const NEONModifiedImmediate_MVNI: Instr = NEONModifiedImmediateFixed | 0x20000000;
pub const NEONModifiedImmediate_ORR: Instr = NEONModifiedImmediateFixed | 0x00001000;
pub const NEONModifiedImmediate_BIC: Instr = NEONModifiedImmediateFixed | 0x20001000;

pub const NEONExtractFixed: Instr = 0x2E000000;
pub const NEON_EXT: Instr = NEONExtractFixed | 0x00000000;

pub const NEON3ExtensionFixed: Instr = 0x0E008400;
pub const NEON_SDOT: Instr = NEON3ExtensionFixed | 0x00001000;

pub const NEONSHA3Fixed: Instr = 0xCE000000;
pub const NEON_BCAX: Instr = NEONSHA3Fixed | 0x00200000;
pub const NEON_EOR3: Instr = NEONSHA3Fixed;
pub const NEON_XAR: Instr = NEONSHA3Fixed | 0x00800000;

pub const NEONQ_offset: u32 = 30;
pub const NEONLSSize_offset: u32 = 10;
pub const NEONLSSize_mask: Instr = 0x3 << NEONLSSize_offset;
pub const NEONS_offset: u32 = 12;
pub const NEONS_mask: Instr = 0x1 << NEONS_offset;
pub const NEONQ_mask: Instr = 0x1 << NEONQ_offset;

pub const NEONLoadStoreMultiL: Instr = 0x00400000;
pub const NEONLoadStoreMulti1_1v: Instr = 0x00007000;
pub const NEONLoadStoreMulti1_2v: Instr = 0x0000A000;
pub const NEONLoadStoreMulti1_3v: Instr = 0x00006000;
pub const NEONLoadStoreMulti1_4v: Instr = 0x00002000;
pub const NEONLoadStoreMulti2: Instr = 0x00008000;
pub const NEONLoadStoreMulti3: Instr = 0x00004000;
pub const NEONLoadStoreMulti4: Instr = 0x00000000;

pub const NEONLoadStoreMultiStructFixed: Instr = 0x0C000000;
pub const NEONLoadStoreMultiStructFMask: Instr = 0xBFBF0000;
pub const NEONLoadStoreMultiStructMask: Instr = 0xBFFFF000;
pub const NEONLoadStoreMultiStructStore: Instr = NEONLoadStoreMultiStructFixed;
pub const NEONLoadStoreMultiStructLoad: Instr =
    NEONLoadStoreMultiStructFixed | NEONLoadStoreMultiL;
pub const NEON_LD1_1v: Instr = NEONLoadStoreMultiStructLoad | NEONLoadStoreMulti1_1v;
pub const NEON_LD1_2v: Instr = NEONLoadStoreMultiStructLoad | NEONLoadStoreMulti1_2v;
pub const NEON_LD1_3v: Instr = NEONLoadStoreMultiStructLoad | NEONLoadStoreMulti1_3v;
pub const NEON_LD1_4v: Instr = NEONLoadStoreMultiStructLoad | NEONLoadStoreMulti1_4v;
pub const NEON_LD2: Instr = NEONLoadStoreMultiStructLoad | NEONLoadStoreMulti2;
pub const NEON_LD3: Instr = NEONLoadStoreMultiStructLoad | NEONLoadStoreMulti3;
pub const NEON_LD4: Instr = NEONLoadStoreMultiStructLoad | NEONLoadStoreMulti4;
pub const NEON_ST1_1v: Instr = NEONLoadStoreMultiStructStore | NEONLoadStoreMulti1_1v;
pub const NEON_ST1_2v: Instr = NEONLoadStoreMultiStructStore | NEONLoadStoreMulti1_2v;
pub const NEON_ST1_3v: Instr = NEONLoadStoreMultiStructStore | NEONLoadStoreMulti1_3v;
pub const NEON_ST1_4v: Instr = NEONLoadStoreMultiStructStore | NEONLoadStoreMulti1_4v;
pub const NEON_ST2: Instr = NEONLoadStoreMultiStructStore | NEONLoadStoreMulti2;
pub const NEON_ST3: Instr = NEONLoadStoreMultiStructStore | NEONLoadStoreMulti3;
pub const NEON_ST4: Instr = NEONLoadStoreMultiStructStore | NEONLoadStoreMulti4;
pub const NEONLoadStoreMultiStructPostIndex: Instr = 0x00800000;

pub const NEONLoadStoreMultiStructPostIndexFixed: Instr = 0x0C800000;
pub const NEONLoadStoreMultiStructPostIndexFMask: Instr = 0xBFA00000;
pub const NEONLoadStoreMultiStructPostIndexMask: Instr = 0xBFE0F000;
pub const NEON_LD1_1v_post: Instr = NEON_LD1_1v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_LD1_2v_post: Instr = NEON_LD1_2v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_LD1_3v_post: Instr = NEON_LD1_3v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_LD1_4v_post: Instr = NEON_LD1_4v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_LD2_post: Instr = NEON_LD2 | NEONLoadStoreMultiStructPostIndex;
pub const NEON_LD3_post: Instr = NEON_LD3 | NEONLoadStoreMultiStructPostIndex;
pub const NEON_LD4_post: Instr = NEON_LD4 | NEONLoadStoreMultiStructPostIndex;
pub const NEON_ST1_1v_post: Instr = NEON_ST1_1v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_ST1_2v_post: Instr = NEON_ST1_2v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_ST1_3v_post: Instr = NEON_ST1_3v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_ST1_4v_post: Instr = NEON_ST1_4v | NEONLoadStoreMultiStructPostIndex;
pub const NEON_ST2_post: Instr = NEON_ST2 | NEONLoadStoreMultiStructPostIndex;
pub const NEON_ST3_post: Instr = NEON_ST3 | NEONLoadStoreMultiStructPostIndex;
pub const NEON_ST4_post: Instr = NEON_ST4 | NEONLoadStoreMultiStructPostIndex;
pub const NEONLoadStoreSingleLenMask: Instr = 0x00202000;
pub const NEON_LD1_b: Instr = NEONLoadStoreSingleStructLoad1 | NEONLoadStoreSingle_b;
pub const NEON_LD1_h: Instr = NEONLoadStoreSingleStructLoad1 | NEONLoadStoreSingle_h;
pub const NEON_LD1_s: Instr = NEONLoadStoreSingleStructLoad1 | NEONLoadStoreSingle_s;
pub const NEON_LD1_d: Instr = NEONLoadStoreSingleStructLoad1 | NEONLoadStoreSingle_d;
pub const NEON_ST1_b: Instr = NEONLoadStoreSingleStructStore1 | NEONLoadStoreSingle_b;
pub const NEON_ST1_h: Instr = NEONLoadStoreSingleStructStore1 | NEONLoadStoreSingle_h;
pub const NEON_ST1_s: Instr = NEONLoadStoreSingleStructStore1 | NEONLoadStoreSingle_s;
pub const NEON_ST1_d: Instr = NEONLoadStoreSingleStructStore1 | NEONLoadStoreSingle_d;
pub const NEON_LD2_b: Instr = NEONLoadStoreSingleStructLoad2 | NEONLoadStoreSingle_b;
pub const NEON_LD2_h: Instr = NEONLoadStoreSingleStructLoad2 | NEONLoadStoreSingle_h;
pub const NEON_LD2_s: Instr = NEONLoadStoreSingleStructLoad2 | NEONLoadStoreSingle_s;
pub const NEON_LD2_d: Instr = NEONLoadStoreSingleStructLoad2 | NEONLoadStoreSingle_d;
pub const NEON_ST2_b: Instr = NEONLoadStoreSingleStructStore2 | NEONLoadStoreSingle_b;
pub const NEON_ST2_h: Instr = NEONLoadStoreSingleStructStore2 | NEONLoadStoreSingle_h;
pub const NEON_ST2_s: Instr = NEONLoadStoreSingleStructStore2 | NEONLoadStoreSingle_s;
pub const NEON_ST2_d: Instr = NEONLoadStoreSingleStructStore2 | NEONLoadStoreSingle_d;
pub const NEON_LD3_b: Instr = NEONLoadStoreSingleStructLoad3 | NEONLoadStoreSingle_b;
pub const NEON_LD3_h: Instr = NEONLoadStoreSingleStructLoad3 | NEONLoadStoreSingle_h;
pub const NEON_LD3_s: Instr = NEONLoadStoreSingleStructLoad3 | NEONLoadStoreSingle_s;
pub const NEON_LD3_d: Instr = NEONLoadStoreSingleStructLoad3 | NEONLoadStoreSingle_d;
pub const NEON_ST3_b: Instr = NEONLoadStoreSingleStructStore3 | NEONLoadStoreSingle_b;
pub const NEON_ST3_h: Instr = NEONLoadStoreSingleStructStore3 | NEONLoadStoreSingle_h;
pub const NEON_ST3_s: Instr = NEONLoadStoreSingleStructStore3 | NEONLoadStoreSingle_s;
pub const NEON_ST3_d: Instr = NEONLoadStoreSingleStructStore3 | NEONLoadStoreSingle_d;
pub const NEON_LD4_b: Instr = NEONLoadStoreSingleStructLoad4 | NEONLoadStoreSingle_b;
pub const NEON_LD4_h: Instr = NEONLoadStoreSingleStructLoad4 | NEONLoadStoreSingle_h;
pub const NEON_LD4_s: Instr = NEONLoadStoreSingleStructLoad4 | NEONLoadStoreSingle_s;
pub const NEON_LD4_d: Instr = NEONLoadStoreSingleStructLoad4 | NEONLoadStoreSingle_d;
pub const NEON_ST4_b: Instr = NEONLoadStoreSingleStructStore4 | NEONLoadStoreSingle_b;
pub const NEON_ST4_h: Instr = NEONLoadStoreSingleStructStore4 | NEONLoadStoreSingle_h;
pub const NEON_ST4_s: Instr = NEONLoadStoreSingleStructStore4 | NEONLoadStoreSingle_s;
pub const NEON_ST4_d: Instr = NEONLoadStoreSingleStructStore4 | NEONLoadStoreSingle_d;
pub const NEONLoadStoreSingleStructPostIndexFixed: Instr = 0x0D800000;
pub const NEONLoadStoreSingleStructPostIndexFMask: Instr = 0xBF800000;
pub const NEONLoadStoreSingleStructPostIndexMask: Instr = 0xBFE0E000;
pub const NEONLoadStoreSingleStructPostIndex: Instr = 0x00800000;
pub const NEON_LD1_b_post: Instr = NEON_LD1_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD1_h_post: Instr = NEON_LD1_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD1_s_post: Instr = NEON_LD1_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD1_d_post: Instr = NEON_LD1_d | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD1R_post: Instr = NEON_LD1R | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST1_b_post: Instr = NEON_ST1_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST1_h_post: Instr = NEON_ST1_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST1_s_post: Instr = NEON_ST1_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST1_d_post: Instr = NEON_ST1_d | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD2_b_post: Instr = NEON_LD2_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD2_h_post: Instr = NEON_LD2_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD2_s_post: Instr = NEON_LD2_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD2_d_post: Instr = NEON_LD2_d | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD2R_post: Instr = NEON_LD2R | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST2_b_post: Instr = NEON_ST2_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST2_h_post: Instr = NEON_ST2_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST2_s_post: Instr = NEON_ST2_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST2_d_post: Instr = NEON_ST2_d | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD3_b_post: Instr = NEON_LD3_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD3_h_post: Instr = NEON_LD3_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD3_s_post: Instr = NEON_LD3_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD3_d_post: Instr = NEON_LD3_d | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD3R_post: Instr = NEON_LD3R | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST3_b_post: Instr = NEON_ST3_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST3_h_post: Instr = NEON_ST3_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST3_s_post: Instr = NEON_ST3_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST3_d_post: Instr = NEON_ST3_d | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD4_b_post: Instr = NEON_LD4_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD4_h_post: Instr = NEON_LD4_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD4_s_post: Instr = NEON_LD4_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD4_d_post: Instr = NEON_LD4_d | NEONLoadStoreSingleStructPostIndex;
pub const NEON_LD4R_post: Instr = NEON_LD4R | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST4_b_post: Instr = NEON_ST4_b | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST4_h_post: Instr = NEON_ST4_h | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST4_s_post: Instr = NEON_ST4_s | NEONLoadStoreSingleStructPostIndex;
pub const NEON_ST4_d_post: Instr = NEON_ST4_d | NEONLoadStoreSingleStructPostIndex;

pub const NEONLoadStoreSingle1: Instr = 0x00000000;
pub const NEONLoadStoreSingle2: Instr = 0x00200000;
pub const NEONLoadStoreSingle3: Instr = 0x00002000;
pub const NEONLoadStoreSingle4: Instr = 0x00202000;
pub const NEONLoadStoreSingleL: Instr = 0x00400000;
pub const NEONLoadStoreSingle_b: Instr = 0x00000000;
pub const NEONLoadStoreSingle_h: Instr = 0x00004000;
pub const NEONLoadStoreSingle_s: Instr = 0x00008000;
pub const NEONLoadStoreSingle_d: Instr = 0x00008400;
pub const NEONLoadStoreSingleAllLanes: Instr = 0x0000C000;

pub const NEONLoadStoreSingleStructFixed: Instr = 0x0D000000;
pub const NEONLoadStoreSingleStructFMask: Instr = 0xBF9F0000;
pub const NEONLoadStoreSingleStructMask: Instr = 0xBFFFE000;
pub const NEONLoadStoreSingleStructStore: Instr = NEONLoadStoreSingleStructFixed;
pub const NEONLoadStoreSingleStructLoad: Instr =
    NEONLoadStoreSingleStructFixed | NEONLoadStoreSingleL;
pub const NEONLoadStoreSingleStructLoad1: Instr =
    NEONLoadStoreSingle1 | NEONLoadStoreSingleStructLoad;
pub const NEONLoadStoreSingleStructLoad2: Instr =
    NEONLoadStoreSingle2 | NEONLoadStoreSingleStructLoad;
pub const NEONLoadStoreSingleStructLoad3: Instr =
    NEONLoadStoreSingle3 | NEONLoadStoreSingleStructLoad;
pub const NEONLoadStoreSingleStructLoad4: Instr =
    NEONLoadStoreSingle4 | NEONLoadStoreSingleStructLoad;
pub const NEONLoadStoreSingleStructStore1: Instr =
    NEONLoadStoreSingle1 | NEONLoadStoreSingleStructFixed;
pub const NEONLoadStoreSingleStructStore2: Instr =
    NEONLoadStoreSingle2 | NEONLoadStoreSingleStructFixed;
pub const NEONLoadStoreSingleStructStore3: Instr =
    NEONLoadStoreSingle3 | NEONLoadStoreSingleStructFixed;
pub const NEONLoadStoreSingleStructStore4: Instr =
    NEONLoadStoreSingle4 | NEONLoadStoreSingleStructFixed;
pub const NEON_LD1R: Instr =
    NEONLoadStoreSingleStructLoad1 | NEONLoadStoreSingleAllLanes;
pub const NEON_LD2R: Instr =
    NEONLoadStoreSingleStructLoad2 | NEONLoadStoreSingleAllLanes;
pub const NEON_LD3R: Instr =
    NEONLoadStoreSingleStructLoad3 | NEONLoadStoreSingleAllLanes;
pub const NEON_LD4R: Instr =
    NEONLoadStoreSingleStructLoad4 | NEONLoadStoreSingleAllLanes;



pub const CpyFixed: Instr = 0x1D000400;
pub const CpyFMask: Instr = 0xFF20FC00;
pub const CPYP: Instr = CpyFixed | 0x00000000;
pub const CPYM: Instr = CpyFixed | 0x00400000;
pub const CPYE: Instr = CpyFixed | 0x00800000;
pub const SetFixed: Instr = 0x19C00400;
pub const SetFMask: Instr = 0xFFE03C00;
pub const CpyMask: Instr = CpyFMask | 0x00E00000;
pub const SetMask: Instr = SetFMask | 0x0000C000;
pub const SETP: Instr = SetFixed | 0x00000000;
pub const SETM: Instr = SetFixed | 0x00004000;
pub const SETE: Instr = SetFixed | 0x00008000;

pub const FMOV_d1_x: Instr = FPIntegerConvertFixed | SixtyFourBits | 0x008F0000;
pub const FMOV_x_d1: Instr = FPIntegerConvertFixed | SixtyFourBits | 0x008E0000;
pub const FPFixedPointConvertFMask: Instr = 0x5F200000;
pub const NEON2RegMiscOpcode: Instr = 0x0001F000;
pub const NEON_RBIT_NOT_opcode: Instr = NEON_RBIT_NOT & NEON2RegMiscOpcode;
pub const NEON_NEG_opcode: Instr = NEON_NEG & NEON2RegMiscOpcode;
pub const NEON_XTN_opcode: Instr = NEON_XTN & NEON2RegMiscOpcode;
pub const NEON_UQXTN_opcode: Instr = NEON_UQXTN & NEON2RegMiscOpcode;
pub const NEON2RegMiscFPMask: Instr = NEON2RegMiscMask | 0x00800000;
pub const NEON_SCVTF: Instr = NEON2RegMiscFixed | 0x0001D000;
pub const NEON_UCVTF: Instr = NEON_SCVTF | NEON2RegMiscUBit;
pub const NEON_FCVTL_opcode: Instr = NEON_FCVTL & NEON2RegMiscOpcode;
pub const NEON_FCVTN_opcode: Instr = NEON_FCVTN & NEON2RegMiscOpcode;
pub const NEON3SameFPFMask: Instr = NEON3SameFMask | 0x0000C000;
pub const NEON3SameFPMask: Instr = NEON3SameMask | 0x00800000;
pub const NEON3SameHPFixed: Instr = 0x0E400400;
pub const NEON3SameHPFMask: Instr = 0x9F400400;
pub const NEON3SameLogicalFMask: Instr = NEON3SameFMask | 0x0000F800;
pub const NEON3DifferentDot: Instr = 0x0E800000;
pub const NEON3ExtensionFMask: Instr = 0x9F208400;
pub const NEON3ExtensionMask: Instr = 0xBF20FC00;
pub const NEONAcrossLanesFPMask: Instr = NEONAcrossLanesMask | 0x00800000;
pub const NEONByIndexedElementFPMask: Instr = NEONByIndexedElementMask | 0x00800000;
pub const NEONModifiedImmediateFMask: Instr = 0x9FF80400;
pub const NEONModifiedImmediateOpBit: Instr = 0x20000000;
pub const NEONExtractFMask: Instr = 0xBF208400;
pub const NEONExtractMask: Instr = 0xBFE08400;
pub const NEONCopyInsElementMask: Instr = NEONCopyMask | 0x40000000;
pub const NEONCopyInsGeneralMask: Instr = NEONCopyMask | 0x40007800;
pub const NEONCopyDupElementMask: Instr = NEONCopyMask | 0x20007800;
pub const NEONCopyDupGeneralMask: Instr = NEONCopyDupElementMask;
pub const NEONCopyUmovMask: Instr = NEONCopyMask | 0x20007800;
pub const NEONCopySmovMask: Instr = NEONCopyMask | 0x20007800;
pub const NEONScalarByIndexedElementFixed: Instr = 0x5F000000;
pub const NEONScalarByIndexedElementFMask: Instr = 0xDF000400;
pub const NEONScalarByIndexedElementMask: Instr = 0xFF00F400;
pub const NEON_SQDMLAL_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMLAL_byelement;
pub const NEON_SQDMLSL_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMLSL_byelement;
pub const NEON_SQDMULL_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMULL_byelement;
pub const NEON_SQDMULH_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMULH_byelement;
pub const NEON_SQRDMULH_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_SQRDMULH_byelement;
pub const NEONScalarByIndexedElementFPFixed: Instr = NEONScalarByIndexedElementFixed | 0x00800000;
pub const NEONScalarByIndexedElementFPMask: Instr = NEONScalarByIndexedElementMask | 0x00800000;
pub const NEON_FMLA_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_FMLA_byelement;
pub const NEON_FMLS_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_FMLS_byelement;
pub const NEON_FMUL_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_FMUL_byelement;
pub const NEON_FMULX_byelement_scalar: Instr = NEON_Q | NEONScalar | NEON_FMULX_byelement;
pub const NEONShiftImmediateUBit: Instr = 0x20000000;
pub const NEONScalarCopyFixed: Instr = 0x5E000400;
pub const NEONScalarCopyFMask: Instr = 0xDFE08400;
pub const NEONScalarCopyMask: Instr = 0xFFE0FC00;
pub const NEONScalarPairwiseFixed: Instr = 0x5E300800;
pub const NEONScalarPairwiseFMask: Instr = 0xDF3E0C00;
pub const NEONScalarPairwiseMask: Instr = 0xFFB1F800;
pub const NEON_ADDP_scalar: Instr = NEONScalarPairwiseFixed | 0x0081B000;
pub const NEON_FMAXNMP_scalar: Instr = NEONScalarPairwiseFixed | 0x2000C000;
pub const NEON_FMINNMP_scalar: Instr = NEONScalarPairwiseFixed | 0x2080C000;
pub const NEON_FADDP_scalar: Instr = NEONScalarPairwiseFixed | 0x2000D000;
pub const NEON_FMAXP_scalar: Instr = NEONScalarPairwiseFixed | 0x2000F000;
pub const NEON_FMINP_scalar: Instr = NEONScalarPairwiseFixed | 0x2080F000;
pub const NEONScalarShiftImmediateFixed: Instr = 0x5F000400;
pub const NEONScalarShiftImmediateFMask: Instr = 0xDF800400;
pub const NEONScalarShiftImmediateMask: Instr = 0xFF80FC00;
pub const NEON_SHL_scalar: Instr = NEON_Q | NEONScalar | NEON_SHL;
pub const NEON_SLI_scalar: Instr = NEON_Q | NEONScalar | NEON_SLI;
pub const NEON_SRI_scalar: Instr = NEON_Q | NEONScalar | NEON_SRI;
pub const NEON_SSHR_scalar: Instr = NEON_Q | NEONScalar | NEON_SSHR;
pub const NEON_USHR_scalar: Instr = NEON_Q | NEONScalar | NEON_USHR;
pub const NEON_SRSHR_scalar: Instr = NEON_Q | NEONScalar | NEON_SRSHR;
pub const NEON_URSHR_scalar: Instr = NEON_Q | NEONScalar | NEON_URSHR;
pub const NEON_SSRA_scalar: Instr = NEON_Q | NEONScalar | NEON_SSRA;
pub const NEON_USRA_scalar: Instr = NEON_Q | NEONScalar | NEON_USRA;
pub const NEON_SRSRA_scalar: Instr = NEON_Q | NEONScalar | NEON_SRSRA;
pub const NEON_URSRA_scalar: Instr = NEON_Q | NEONScalar | NEON_URSRA;
pub const NEON_UQSHRN_scalar: Instr = NEON_Q | NEONScalar | NEON_UQSHRN;
pub const NEON_UQRSHRN_scalar: Instr = NEON_Q | NEONScalar | NEON_UQRSHRN;
pub const NEON_SQSHRN_scalar: Instr = NEON_Q | NEONScalar | NEON_SQSHRN;
pub const NEON_SQRSHRN_scalar: Instr = NEON_Q | NEONScalar | NEON_SQRSHRN;
pub const NEON_SQSHRUN_scalar: Instr = NEON_Q | NEONScalar | NEON_SQSHRUN;
pub const NEON_SQRSHRUN_scalar: Instr = NEON_Q | NEONScalar | NEON_SQRSHRUN;
pub const NEON_SQSHLU_scalar: Instr = NEON_Q | NEONScalar | NEON_SQSHLU;
pub const NEON_SQSHL_imm_scalar: Instr = NEON_Q | NEONScalar | NEON_SQSHL_imm;
pub const NEON_UQSHL_imm_scalar: Instr = NEON_Q | NEONScalar | NEON_UQSHL_imm;
pub const NEON_SCVTF_imm_scalar: Instr = NEON_Q | NEONScalar | NEON_SCVTF_imm;
pub const NEON_UCVTF_imm_scalar: Instr = NEON_Q | NEONScalar | NEON_UCVTF_imm;
pub const NEON_FCVTZS_imm_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTZS_imm;
pub const NEON_FCVTZU_imm_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTZU_imm;
pub const NEON_TBX_2v: Instr = NEON_TBL_2v | NEONTableExt;
pub const NEON_TBX_3v: Instr = NEON_TBL_3v | NEONTableExt;
pub const NEON_TBX_4v: Instr = NEON_TBL_4v | NEONTableExt;
pub const NEONSHA3FMask: Instr = 0xce000000;
pub const NEONSHA3Mask: Instr = 0xcee00000;

pub const MinMaxImmediateFixed: Instr = 0x11C00000;
pub const MinMaxImmediateFMask: Instr = 0x7FF00000;
pub const MinMaxImmediateMask: Instr = 0xFFFC0000;
pub const SMAX_w_imm: Instr = MinMaxImmediateFixed;
pub const SMAX_x_imm: Instr = SMAX_w_imm | SixtyFourBits;
pub const UMAX_w_imm: Instr = MinMaxImmediateFixed | 0x00040000;
pub const UMAX_x_imm: Instr = UMAX_w_imm | SixtyFourBits;
pub const SMIN_w_imm: Instr = MinMaxImmediateFixed | 0x00080000;
pub const SMIN_x_imm: Instr = SMIN_w_imm | SixtyFourBits;
pub const UMIN_w_imm: Instr = MinMaxImmediateFixed | 0x000C0000;
pub const UMIN_x_imm: Instr = UMIN_w_imm | SixtyFourBits;
pub const SMAX_w: Instr = DataProcessing2SourceFixed | 0x0006000;
pub const SMAX_x: Instr = SMAX_w | SixtyFourBits;
pub const UMAX_w: Instr = DataProcessing2SourceFixed | 0x0006400;
pub const UMAX_x: Instr = UMAX_w | SixtyFourBits;
pub const SMIN_w: Instr = DataProcessing2SourceFixed | 0x0006800;
pub const SMIN_x: Instr = SMIN_w | SixtyFourBits;
pub const UMIN_w: Instr = DataProcessing2SourceFixed | 0x0006C00;
pub const UMIN_x: Instr = UMIN_w | SixtyFourBits;
pub const NEONScalar2RegMiscMask: Instr = NEON_Q | NEONScalar | NEON2RegMiscMask;
pub const NEON_CMGT_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_CMGT_zero;
pub const NEON_CMEQ_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_CMEQ_zero;
pub const NEON_CMLT_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_CMLT_zero;
pub const NEON_CMGE_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_CMGE_zero;
pub const NEON_CMLE_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_CMLE_zero;
pub const NEON_ABS_scalar: Instr = NEON_Q | NEONScalar | NEON_ABS;
pub const NEON_SQABS_scalar: Instr = NEON_Q | NEONScalar | NEON_SQABS;
pub const NEON_NEG_scalar: Instr = NEON_Q | NEONScalar | NEON_NEG;
pub const NEON_SQNEG_scalar: Instr = NEON_Q | NEONScalar | NEON_SQNEG;
pub const NEON_SUQADD_scalar: Instr = NEON_Q | NEONScalar | NEON_SUQADD;
pub const NEON_USQADD_scalar: Instr = NEON_Q | NEONScalar | NEON_USQADD;
pub const NEON_NEG_scalar_opcode: Instr = NEON_NEG_scalar & NEONScalar2RegMiscOpcode;
pub const NEONScalar2RegMiscFPMask: Instr = NEONScalar2RegMiscMask | 0x00800000;
pub const NEON_SCVTF_scalar: Instr = NEON_Q | NEONScalar | NEON_SCVTF;
pub const NEON_UCVTF_scalar: Instr = NEON_Q | NEONScalar | NEON_UCVTF;
pub const NEON_FCMGT_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMGT_zero;
pub const NEON_FCMEQ_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMEQ_zero;
pub const NEON_FCMLT_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMLT_zero;
pub const NEON_FCMGE_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMGE_zero;
pub const NEON_FCMLE_zero_scalar: Instr = NEON_Q | NEONScalar | NEON_FCMLE_zero;
pub const NEON_FCVTNS_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTNS;
pub const NEON_FCVTNU_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTNU;
pub const NEON_FCVTPS_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTPS;
pub const NEON_FCVTPU_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTPU;
pub const NEON_FCVTMS_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTMS;
pub const NEON_FCVTMU_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTMU;
pub const NEON_FCVTZS_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTZS;
pub const NEON_FCVTZU_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTZU;
pub const NEON_FCVTAS_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTAS;
pub const NEON_FCVTAU_scalar: Instr = NEON_Q | NEONScalar | NEON_FCVTAU;
pub const NEONScalar3SameMask: Instr = 0xFF20FC00;
pub const NEON_ADD_scalar: Instr = NEON_Q | NEONScalar | NEON_ADD;
pub const NEON_CMEQ_scalar: Instr = NEON_Q | NEONScalar | NEON_CMEQ;
pub const NEON_CMGE_scalar: Instr = NEON_Q | NEONScalar | NEON_CMGE;
pub const NEON_CMGT_scalar: Instr = NEON_Q | NEONScalar | NEON_CMGT;
pub const NEON_CMHI_scalar: Instr = NEON_Q | NEONScalar | NEON_CMHI;
pub const NEON_CMHS_scalar: Instr = NEON_Q | NEONScalar | NEON_CMHS;
pub const NEON_CMTST_scalar: Instr = NEON_Q | NEONScalar | NEON_CMTST;
pub const NEON_SUB_scalar: Instr = NEON_Q | NEONScalar | NEON_SUB;
pub const NEON_UQADD_scalar: Instr = NEON_Q | NEONScalar | NEON_UQADD;
pub const NEON_SQADD_scalar: Instr = NEON_Q | NEONScalar | NEON_SQADD;
pub const NEON_UQSUB_scalar: Instr = NEON_Q | NEONScalar | NEON_UQSUB;
pub const NEON_SQSUB_scalar: Instr = NEON_Q | NEONScalar | NEON_SQSUB;
pub const NEON_USHL_scalar: Instr = NEON_Q | NEONScalar | NEON_USHL;
pub const NEON_SSHL_scalar: Instr = NEON_Q | NEONScalar | NEON_SSHL;
pub const NEON_UQSHL_scalar: Instr = NEON_Q | NEONScalar | NEON_UQSHL;
pub const NEON_SQSHL_scalar: Instr = NEON_Q | NEONScalar | NEON_SQSHL;
pub const NEON_URSHL_scalar: Instr = NEON_Q | NEONScalar | NEON_URSHL;
pub const NEON_SRSHL_scalar: Instr = NEON_Q | NEONScalar | NEON_SRSHL;
pub const NEON_UQRSHL_scalar: Instr = NEON_Q | NEONScalar | NEON_UQRSHL;
pub const NEON_SQRSHL_scalar: Instr = NEON_Q | NEONScalar | NEON_SQRSHL;
pub const NEON_SQDMULH_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMULH;
pub const NEON_SQRDMULH_scalar: Instr = NEON_Q | NEONScalar | NEON_SQRDMULH;
pub const NEONScalar3SameFPFixed: Instr = NEONScalar3SameFixed | 0x0000C000;
pub const NEONScalar3SameFPFMask: Instr = NEONScalar3SameFMask | 0x0000C000;
pub const NEONScalar3SameFPMask: Instr = NEONScalar3SameMask | 0x00800000;
pub const NEONScalar3DiffMask: Instr = NEON_Q | NEONScalar | NEON3DifferentMask;
pub const NEON_SQDMLAL_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMLAL;
pub const NEON_SQDMLSL_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMLSL;
pub const NEON_SQDMULL_scalar: Instr = NEON_Q | NEONScalar | NEON_SQDMULL;

pub const LoadStoreXLoad_offset: u32 = 22;
pub const LoadStoreXNotExclusive_offset: u32 = 23;
pub const LoadStoreXSizeLog2_offset: u32 = 30;
pub const NEONScalar2RegMiscOpcode: Instr = NEON2RegMiscOpcode;
pub const NEONScalar3SameFMask: Instr = 0xDF200400;
pub const NEONScalar3SameFixed: Instr = 0x5E200400;

pub const ADD_w_imm: Instr = AddSubImmediateFixed | ADD;
pub const ADD_x_imm: Instr = AddSubImmediateFixed | ADD | SixtyFourBits;
pub const ADD_w_shift: Instr = AddSubShiftedFixed | ADD;
pub const ADD_x_shift: Instr = AddSubShiftedFixed | ADD | SixtyFourBits;
pub const ADD_w_ext: Instr = AddSubExtendedFixed | ADD;
pub const ADD_x_ext: Instr = AddSubExtendedFixed | ADD | SixtyFourBits;
pub const ADDS_w_imm: Instr = AddSubImmediateFixed | ADDS;
pub const ADDS_x_imm: Instr = AddSubImmediateFixed | ADDS | SixtyFourBits;
pub const ADDS_w_shift: Instr = AddSubShiftedFixed | ADDS;
pub const ADDS_x_shift: Instr = AddSubShiftedFixed | ADDS | SixtyFourBits;
pub const ADDS_w_ext: Instr = AddSubExtendedFixed | ADDS;
pub const ADDS_x_ext: Instr = AddSubExtendedFixed | ADDS | SixtyFourBits;
pub const SUB_w_imm: Instr = AddSubImmediateFixed | SUB;
pub const SUB_x_imm: Instr = AddSubImmediateFixed | SUB | SixtyFourBits;
pub const SUB_w_shift: Instr = AddSubShiftedFixed | SUB;
pub const SUB_x_shift: Instr = AddSubShiftedFixed | SUB | SixtyFourBits;
pub const SUB_w_ext: Instr = AddSubExtendedFixed | SUB;
pub const SUB_x_ext: Instr = AddSubExtendedFixed | SUB | SixtyFourBits;
pub const SUBS_w_imm: Instr = AddSubImmediateFixed | SUBS;
pub const SUBS_x_imm: Instr = AddSubImmediateFixed | SUBS | SixtyFourBits;
pub const SUBS_w_shift: Instr = AddSubShiftedFixed | SUBS;
pub const SUBS_x_shift: Instr = AddSubShiftedFixed | SUBS | SixtyFourBits;
pub const SUBS_w_ext: Instr = AddSubExtendedFixed | SUBS;
pub const SUBS_x_ext: Instr = AddSubExtendedFixed | SUBS | SixtyFourBits;

pub const ABS_w: Instr = ABS;
pub const ABS_x: Instr = ABS | SixtyFourBits;
pub const ADCS_x: Instr = AddSubWithCarryFixed | ADDS | SixtyFourBits;
pub const ADC_w: Instr = AddSubWithCarryFixed | ADD;
pub const ADC_x: Instr = AddSubWithCarryFixed | ADD | SixtyFourBits;
pub const ANDS_w: Instr = LogicalShiftedFixed | ANDS;
pub const ANDS_w_imm: Instr = LogicalImmediateFixed | ANDS;
pub const ANDS_x: Instr = LogicalShiftedFixed | ANDS | SixtyFourBits;
pub const ANDS_x_imm: Instr = LogicalImmediateFixed | ANDS | SixtyFourBits;
pub const AND_w: Instr = LogicalShiftedFixed | AND;
pub const AND_w_imm: Instr = LogicalImmediateFixed | AND;
pub const AND_x: Instr = LogicalShiftedFixed | AND | SixtyFourBits;
pub const AND_x_imm: Instr = LogicalImmediateFixed | AND | SixtyFourBits;
pub const AddSubExtendedMask: Instr = 0xFFE00000;
pub const AddSubImmediateMask: Instr = 0xFF000000;
pub const AddSubShiftedMask: Instr = 0xFF200000;
pub const AddSubWithCarryMask: Instr = 0xFFE0FC00;
pub const BC_cond: Instr = ConditionalBranchConsistentFixed | 0x00000000;
pub const BEXT: Instr = SVEBitPermFixed;
pub const BICS_w: Instr = LogicalShiftedFixed | BICS;
pub const BICS_x: Instr = LogicalShiftedFixed | BICS | SixtyFourBits;
pub const BIC_w: Instr = LogicalShiftedFixed | BIC;
pub const BIC_x: Instr = LogicalShiftedFixed | BIC | SixtyFourBits;
pub const CCMN_w: Instr = ConditionalCompareRegisterFixed | CCMN;
pub const CCMN_x: Instr = ConditionalCompareRegisterFixed | SixtyFourBits | CCMN;
pub const CCMP_w: Instr = ConditionalCompareRegisterFixed | CCMP;
pub const CCMP_x: Instr = ConditionalCompareRegisterFixed | SixtyFourBits | CCMP;
pub const CNT_w: Instr = CNT;
pub const CNT_x: Instr = CNT | SixtyFourBits;
pub const CTZ_w: Instr = CTZ;
pub const CTZ_x: Instr = CTZ | SixtyFourBits;
pub const ConditionalCompareRegisterMask: Instr = 0xFFE00C10;
pub const DCPS1: Instr = ExceptionFixed | 0x00A00001;
pub const DCPS2: Instr = ExceptionFixed | 0x00A00002;
pub const DCPS3: Instr = ExceptionFixed | 0x00A00003;
pub const EON_w: Instr = LogicalShiftedFixed | EON;
pub const EON_x: Instr = LogicalShiftedFixed | EON | SixtyFourBits;
pub const EOR_w: Instr = LogicalShiftedFixed | EOR;
pub const EOR_w_imm: Instr = LogicalImmediateFixed | EOR;
pub const EOR_x: Instr = LogicalShiftedFixed | EOR | SixtyFourBits;
pub const EOR_x_imm: Instr = LogicalImmediateFixed | EOR | SixtyFourBits;
pub const EXTR_w: Instr = ExtractFixed | 0x00000000;
pub const EXTR_x: Instr = ExtractFixed | 0x80000000;
pub const FCCMPE_d: Instr = FPConditionalCompareFixed | FP64 | 0x00000010;
pub const FCCMPE_s: Instr = FPConditionalCompareFixed | 0x00000010;
pub const FCVTAS_ws: Instr = FCVTAS;
pub const FCVTAS_xs: Instr = FCVTAS | SixtyFourBits;
pub const FCVTAU_wd: Instr = FCVTAU | FP64;
pub const FCVTAU_ws: Instr = FCVTAU;
pub const FCVTAU_xd: Instr = FCVTAU | SixtyFourBits | FP64;
pub const FCVTAU_xs: Instr = FCVTAU | SixtyFourBits;
pub const FCVTMS_ws: Instr = FCVTMS;
pub const FCVTMS_xs: Instr = FCVTMS | SixtyFourBits;
pub const FCVTMU_wd: Instr = FCVTMU | FP64;
pub const FCVTMU_ws: Instr = FCVTMU;
pub const FCVTMU_xd: Instr = FCVTMU | SixtyFourBits | FP64;
pub const FCVTMU_xs: Instr = FCVTMU | SixtyFourBits;
pub const FCVTNU_wd: Instr = FCVTNU | FP64;
pub const FCVTNU_ws: Instr = FCVTNU;
pub const FCVTNU_xd: Instr = FCVTNU | SixtyFourBits | FP64;
pub const FCVTNU_xs: Instr = FCVTNU | SixtyFourBits;
pub const FCVTPS_wd: Instr = FCVTPS | FP64;
pub const FCVTPS_ws: Instr = FCVTPS;
pub const FCVTPS_xd: Instr = FCVTPS | SixtyFourBits | FP64;
pub const FCVTPS_xs: Instr = FCVTPS | SixtyFourBits;
pub const FCVTPU_wd: Instr = FCVTPU | FP64;
pub const FCVTPU_ws: Instr = FCVTPU;
pub const FCVTPU_xd: Instr = FCVTPU | SixtyFourBits | FP64;
pub const FCVTPU_xs: Instr = FCVTPU | SixtyFourBits;
pub const HVC: Instr = ExceptionFixed | 0x00000002;
pub const LoadStorePairOffsetMask: Instr = 0xFFC00000;
pub const LoadStorePairPostIndexMask: Instr = 0xFFC00000;
pub const LoadStorePairPreIndexMask: Instr = 0xFFC00000;
pub const LoadStorePostIndexMask: Instr = 0xFFE00C00;
pub const LoadStorePreIndexMask: Instr = 0xFFE00C00;
pub const LoadStoreRegisterOffsetMask: Instr = 0xFFE00C00;
pub const LoadStoreUnscaledOffsetMask: Instr = 0xFFE00C00;
pub const LoadStoreUnsignedOffsetMask: Instr = 0xFFC00000;
pub const LogicalShiftedMask: Instr = 0xFF200000;
pub const ORN_w: Instr = LogicalShiftedFixed | ORN;
pub const ORN_x: Instr = LogicalShiftedFixed | ORN | SixtyFourBits;
pub const ORR_w: Instr = LogicalShiftedFixed | ORR;
pub const ORR_w_imm: Instr = LogicalImmediateFixed | ORR;
pub const ORR_x: Instr = LogicalShiftedFixed | ORR | SixtyFourBits;
pub const ORR_x_imm: Instr = LogicalImmediateFixed | ORR | SixtyFourBits;
pub const PRFM_reg: Instr = LoadStoreRegisterOffsetFixed | PRFM;
pub const PRFM_unsigned: Instr = LoadStoreUnsignedOffsetFixed | PRFM;
pub const SBCS_x: Instr = AddSubWithCarryFixed | SUBS | SixtyFourBits;
pub const SBC_w: Instr = AddSubWithCarryFixed | SUB;
pub const SBC_x: Instr = AddSubWithCarryFixed | SUB | SixtyFourBits;
pub const SMC: Instr = ExceptionFixed | 0x00000003;
pub const SVC: Instr = ExceptionFixed | 0x00000001;
pub const SVEBitPermFMask: Instr = 0xFF20F000;
pub const SVEBitPermFixed: Instr = 0x4500B000;
pub const SVEBitPermMask: Instr = 0xFF20FC00;
pub const SystemHintFMask: Instr = 0xFFFFF01F;
pub const SystemHintMask: Instr = 0xFFFFF01F;
pub const SystemPAuthFMask: Instr = 0xFFFFFD1F;
pub const SystemPAuthMask: Instr = 0xFFFFFFFF;
pub const SystemSysRegFMask: Instr = 0xFFD00000;
pub const SystemSysRegMask: Instr = 0xFFF00000;
pub const ABS: Instr = DataProcessing1SourceFixed | 0x00002000;
pub const CNT: Instr = DataProcessing1SourceFixed | 0x00001C00;
pub const CTZ: Instr = DataProcessing1SourceFixed | 0x00001800;
pub const ConditionalBranchConsistentFixed: Instr = 0x54000010;
pub const PRFM: Instr = 0xC0800000;

pub const CCMN_w_imm: Instr = ConditionalCompareImmediateFixed | CCMN;
pub const CCMN_x_imm: Instr = ConditionalCompareImmediateFixed | SixtyFourBits | CCMN;
pub const CCMP_w_imm: Instr = ConditionalCompareImmediateFixed | CCMP;
pub const CCMP_x_imm: Instr = ConditionalCompareImmediateFixed | SixtyFourBits | CCMP;
pub const ConditionalCompareImmediateMask: Instr = 0xFFE00C10;

pub const LDPSW_x_off: Instr = LoadStorePairOffsetFixed | LDPSW_x;
pub const LDPSW_x_post: Instr = LoadStorePairPostIndexFixed | LDPSW_x;
pub const LDPSW_x_pre: Instr = LoadStorePairPreIndexFixed | LDPSW_x;
pub const LDP_d_off: Instr = LoadStorePairOffsetFixed | LDP_d;
pub const LDP_d_post: Instr = LoadStorePairPostIndexFixed | LDP_d;
pub const LDP_d_pre: Instr = LoadStorePairPreIndexFixed | LDP_d;
pub const LDP_q_off: Instr = LoadStorePairOffsetFixed | LDP_q;
pub const LDP_q_post: Instr = LoadStorePairPostIndexFixed | LDP_q;
pub const LDP_q_pre: Instr = LoadStorePairPreIndexFixed | LDP_q;
pub const LDP_s_off: Instr = LoadStorePairOffsetFixed | LDP_s;
pub const LDP_s_post: Instr = LoadStorePairPostIndexFixed | LDP_s;
pub const LDP_s_pre: Instr = LoadStorePairPreIndexFixed | LDP_s;
pub const LDP_w_off: Instr = LoadStorePairOffsetFixed | LDP_w;
pub const LDP_w_post: Instr = LoadStorePairPostIndexFixed | LDP_w;
pub const LDP_w_pre: Instr = LoadStorePairPreIndexFixed | LDP_w;
pub const LDP_x_off: Instr = LoadStorePairOffsetFixed | LDP_x;
pub const LDP_x_post: Instr = LoadStorePairPostIndexFixed | LDP_x;
pub const LDP_x_pre: Instr = LoadStorePairPreIndexFixed | LDP_x;
pub const LDRB_w_post: Instr = LoadStorePostIndexFixed | 0x00400000;
pub const LDRB_w_pre: Instr = LoadStorePreIndexFixed | 0x00400000;
pub const LDRB_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x00400000;
pub const LDRB_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x00400000;
pub const LDRH_w_post: Instr = LoadStorePostIndexFixed | 0x40400000;
pub const LDRH_w_pre: Instr = LoadStorePreIndexFixed | 0x40400000;
pub const LDRH_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x40400000;
pub const LDRH_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x40400000;
pub const LDRSB_w_post: Instr = LoadStorePostIndexFixed | 0x00C00000;
pub const LDRSB_w_pre: Instr = LoadStorePreIndexFixed | 0x00C00000;
pub const LDRSB_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x00C00000;
pub const LDRSB_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x00C00000;
pub const LDRSB_x_post: Instr = LoadStorePostIndexFixed | 0x00800000;
pub const LDRSB_x_pre: Instr = LoadStorePreIndexFixed | 0x00800000;
pub const LDRSB_x_reg: Instr = LoadStoreRegisterOffsetFixed | 0x00800000;
pub const LDRSB_x_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x00800000;
pub const LDRSH_w_post: Instr = LoadStorePostIndexFixed | 0x40C00000;
pub const LDRSH_w_pre: Instr = LoadStorePreIndexFixed | 0x40C00000;
pub const LDRSH_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x40C00000;
pub const LDRSH_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x40C00000;
pub const LDRSH_x_post: Instr = LoadStorePostIndexFixed | 0x40800000;
pub const LDRSH_x_pre: Instr = LoadStorePreIndexFixed | 0x40800000;
pub const LDRSH_x_reg: Instr = LoadStoreRegisterOffsetFixed | 0x40800000;
pub const LDRSH_x_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x40800000;
pub const LDRSW_x_post: Instr = LoadStorePostIndexFixed | 0x80800000;
pub const LDRSW_x_pre: Instr = LoadStorePreIndexFixed | 0x80800000;
pub const LDRSW_x_reg: Instr = LoadStoreRegisterOffsetFixed | 0x80800000;
pub const LDRSW_x_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x80800000;
pub const LDR_b_post: Instr = LoadStorePostIndexFixed | 0x04400000;
pub const LDR_b_pre: Instr = LoadStorePreIndexFixed | 0x04400000;
pub const LDR_b_reg: Instr = LoadStoreRegisterOffsetFixed | 0x04400000;
pub const LDR_b_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x04400000;
pub const LDR_d_post: Instr = LoadStorePostIndexFixed | 0xC4400000;
pub const LDR_d_pre: Instr = LoadStorePreIndexFixed | 0xC4400000;
pub const LDR_d_reg: Instr = LoadStoreRegisterOffsetFixed | 0xC4400000;
pub const LDR_d_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0xC4400000;
pub const LDR_h_post: Instr = LoadStorePostIndexFixed | 0x44400000;
pub const LDR_h_pre: Instr = LoadStorePreIndexFixed | 0x44400000;
pub const LDR_h_reg: Instr = LoadStoreRegisterOffsetFixed | 0x44400000;
pub const LDR_h_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x44400000;
pub const LDR_q_post: Instr = LoadStorePostIndexFixed | 0x04C00000;
pub const LDR_q_pre: Instr = LoadStorePreIndexFixed | 0x04C00000;
pub const LDR_q_reg: Instr = LoadStoreRegisterOffsetFixed | 0x04C00000;
pub const LDR_q_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x04C00000;
pub const LDR_s_post: Instr = LoadStorePostIndexFixed | 0x84400000;
pub const LDR_s_pre: Instr = LoadStorePreIndexFixed | 0x84400000;
pub const LDR_s_reg: Instr = LoadStoreRegisterOffsetFixed | 0x84400000;
pub const LDR_s_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x84400000;
pub const LDR_w_post: Instr = LoadStorePostIndexFixed | 0x80400000;
pub const LDR_w_pre: Instr = LoadStorePreIndexFixed | 0x80400000;
pub const LDR_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x80400000;
pub const LDR_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x80400000;
pub const LDR_x_post: Instr = LoadStorePostIndexFixed | 0xC0400000;
pub const LDR_x_pre: Instr = LoadStorePreIndexFixed | 0xC0400000;
pub const LDR_x_reg: Instr = LoadStoreRegisterOffsetFixed | 0xC0400000;
pub const LDR_x_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0xC0400000;
pub const LDURB_w: Instr = LoadStoreUnscaledOffsetFixed | 0x00400000;
pub const LDURH_w: Instr = LoadStoreUnscaledOffsetFixed | 0x40400000;
pub const LDURSB_w: Instr = LoadStoreUnscaledOffsetFixed | 0x00C00000;
pub const LDURSB_x: Instr = LoadStoreUnscaledOffsetFixed | 0x00800000;
pub const LDURSH_w: Instr = LoadStoreUnscaledOffsetFixed | 0x40C00000;
pub const LDURSH_x: Instr = LoadStoreUnscaledOffsetFixed | 0x40800000;
pub const LDURSW_x: Instr = LoadStoreUnscaledOffsetFixed | 0x80800000;
pub const LDUR_b: Instr = LoadStoreUnscaledOffsetFixed | 0x04400000;
pub const LDUR_d: Instr = LoadStoreUnscaledOffsetFixed | 0xC4400000;
pub const LDUR_h: Instr = LoadStoreUnscaledOffsetFixed | 0x44400000;
pub const LDUR_q: Instr = LoadStoreUnscaledOffsetFixed | 0x04C00000;
pub const LDUR_s: Instr = LoadStoreUnscaledOffsetFixed | 0x84400000;
pub const LDUR_w: Instr = LoadStoreUnscaledOffsetFixed | 0x80400000;
pub const LDUR_x: Instr = LoadStoreUnscaledOffsetFixed | 0xC0400000;
pub const STP_d_off: Instr = LoadStorePairOffsetFixed | STP_d;
pub const STP_d_post: Instr = LoadStorePairPostIndexFixed | STP_d;
pub const STP_d_pre: Instr = LoadStorePairPreIndexFixed | STP_d;
pub const STP_q_off: Instr = LoadStorePairOffsetFixed | STP_q;
pub const STP_q_post: Instr = LoadStorePairPostIndexFixed | STP_q;
pub const STP_q_pre: Instr = LoadStorePairPreIndexFixed | STP_q;
pub const STP_s_off: Instr = LoadStorePairOffsetFixed | STP_s;
pub const STP_s_post: Instr = LoadStorePairPostIndexFixed | STP_s;
pub const STP_s_pre: Instr = LoadStorePairPreIndexFixed | STP_s;
pub const STP_w_off: Instr = LoadStorePairOffsetFixed | STP_w;
pub const STP_w_post: Instr = LoadStorePairPostIndexFixed | STP_w;
pub const STP_w_pre: Instr = LoadStorePairPreIndexFixed | STP_w;
pub const STP_x_off: Instr = LoadStorePairOffsetFixed | STP_x;
pub const STP_x_post: Instr = LoadStorePairPostIndexFixed | STP_x;
pub const STP_x_pre: Instr = LoadStorePairPreIndexFixed | STP_x;
pub const STRB_w_post: Instr = LoadStorePostIndexFixed | 0x00000000;
pub const STRB_w_pre: Instr = LoadStorePreIndexFixed | 0x00000000;
pub const STRB_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x00000000;
pub const STRB_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x00000000;
pub const STRH_w_post: Instr = LoadStorePostIndexFixed | 0x40000000;
pub const STRH_w_pre: Instr = LoadStorePreIndexFixed | 0x40000000;
pub const STRH_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x40000000;
pub const STRH_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x40000000;
pub const STR_b_post: Instr = LoadStorePostIndexFixed | 0x04000000;
pub const STR_b_pre: Instr = LoadStorePreIndexFixed | 0x04000000;
pub const STR_b_reg: Instr = LoadStoreRegisterOffsetFixed | 0x04000000;
pub const STR_b_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x04000000;
pub const STR_d_post: Instr = LoadStorePostIndexFixed | 0xC4000000;
pub const STR_d_pre: Instr = LoadStorePreIndexFixed | 0xC4000000;
pub const STR_d_reg: Instr = LoadStoreRegisterOffsetFixed | 0xC4000000;
pub const STR_d_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0xC4000000;
pub const STR_h_post: Instr = LoadStorePostIndexFixed | 0x44000000;
pub const STR_h_pre: Instr = LoadStorePreIndexFixed | 0x44000000;
pub const STR_h_reg: Instr = LoadStoreRegisterOffsetFixed | 0x44000000;
pub const STR_h_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x44000000;
pub const STR_q_post: Instr = LoadStorePostIndexFixed | 0x04800000;
pub const STR_q_pre: Instr = LoadStorePreIndexFixed | 0x04800000;
pub const STR_q_reg: Instr = LoadStoreRegisterOffsetFixed | 0x04800000;
pub const STR_q_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x04800000;
pub const STR_s_post: Instr = LoadStorePostIndexFixed | 0x84000000;
pub const STR_s_pre: Instr = LoadStorePreIndexFixed | 0x84000000;
pub const STR_s_reg: Instr = LoadStoreRegisterOffsetFixed | 0x84000000;
pub const STR_s_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x84000000;
pub const STR_w_post: Instr = LoadStorePostIndexFixed | 0x80000000;
pub const STR_w_pre: Instr = LoadStorePreIndexFixed | 0x80000000;
pub const STR_w_reg: Instr = LoadStoreRegisterOffsetFixed | 0x80000000;
pub const STR_w_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0x80000000;
pub const STR_x_post: Instr = LoadStorePostIndexFixed | 0xC0000000;
pub const STR_x_pre: Instr = LoadStorePreIndexFixed | 0xC0000000;
pub const STR_x_reg: Instr = LoadStoreRegisterOffsetFixed | 0xC0000000;
pub const STR_x_unsigned: Instr = LoadStoreUnsignedOffsetFixed | 0xC0000000;
pub const STURB_w: Instr = LoadStoreUnscaledOffsetFixed | 0x00000000;
pub const STURH_w: Instr = LoadStoreUnscaledOffsetFixed | 0x40000000;
pub const STUR_b: Instr = LoadStoreUnscaledOffsetFixed | 0x04000000;
pub const STUR_d: Instr = LoadStoreUnscaledOffsetFixed | 0xC4000000;
pub const STUR_h: Instr = LoadStoreUnscaledOffsetFixed | 0x44000000;
pub const STUR_q: Instr = LoadStoreUnscaledOffsetFixed | 0x04800000;
pub const STUR_s: Instr = LoadStoreUnscaledOffsetFixed | 0x84000000;
pub const STUR_w: Instr = LoadStoreUnscaledOffsetFixed | 0x80000000;
pub const STUR_x: Instr = LoadStoreUnscaledOffsetFixed | 0xC0000000;

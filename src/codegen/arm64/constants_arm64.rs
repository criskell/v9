pub type Instr = u32;

pub const K_INSTR_SIZE: u8 = 4;
pub const K_INSTR_SIZE_LOG2: u8 = 2;

pub const K_NUMBER_OF_REGISTERS: i32 = 32;
pub const K_NUMBER_OF_V_REGISTERS: i32 = 32;
pub const K_W_REG_SIZE_IN_BITS: i32 = 32;
pub const K_X_REG_SIZE_IN_BITS: i32 = 64;
pub const K_S_REG_SIZE_IN_BITS: i32 = 32;
pub const K_D_REG_SIZE_IN_BITS: i32 = 64;
pub const K_B_REG_SIZE_IN_BITS: i32 = 8;
pub const K_H_REG_SIZE_IN_BITS: i32 = 16;
pub const K_Q_REG_SIZE_IN_BITS: i32 = 128;
pub const K_Q_REG_SIZE_LOG2: i32 = 4;
pub const K_W_REG_MASK: i64 = 0x00000000ffffffff;
pub const K_ZERO_REG_CODE: i32 = 31;
pub const K_SP_REG_INTERNAL_CODE: i32 = 63;
pub const K_REG_CODE_MASK: u32 = 0x1f;

pub const RD_OFFSET: u32 = 0;
pub const RD_WIDTH: u32 = 5;
pub const RN_OFFSET: u32 = 5;
pub const RN_WIDTH: u32 = 5;
pub const RM_OFFSET: u32 = 16;
pub const RM_WIDTH: u32 = 5;
pub const RT_OFFSET: u32 = 0;
pub const RT_WIDTH: u32 = 5;

pub const FLAGS_UPDATE_OFFSET: u32 = 29;

pub const SHIFT_DP_OFFSET: u32 = 22;
pub const IMM_DP_SHIFT_OFFSET: u32 = 10;

pub const IMM_ADD_SUB_OFFSET: u32 = 10;
pub const SHIFT_ADD_SUB_OFFSET: u32 = 22;

pub const IMM_MOVE_WIDE_OFFSET: u32 = 5;
pub const SHIFT_MOVE_WIDE_OFFSET: u32 = 21;

pub const IMM_TEST_BRANCH_OFFSET: u32 = 5;
pub const IMM_TEST_BRANCH_WIDTH: u32 = 14;
pub const IMM_TEST_BRANCH_MASK: u32 = ((1 << IMM_TEST_BRANCH_WIDTH) - 1) << IMM_TEST_BRANCH_OFFSET;

pub const CONDITION_BRANCH_OFFSET: u32 = 0;
pub const IMM_COND_BRANCH_OFFSET: u32 = 5;
pub const IMM_COND_BRANCH_WIDTH: u32 = 19;
pub const IMM_COND_BRANCH_MASK: u32 = ((1 << IMM_COND_BRANCH_WIDTH) - 1) << IMM_COND_BRANCH_OFFSET;

pub const IMM_LS_OFFSET: u32 = 12;
pub const IMM_LS_UNSIGNED_OFFSET: u32 = 10;
pub const LS_OPC_OFFSET: u32 = 22;
pub const LS_OPC_MASK: u32 = 0x3 << LS_OPC_OFFSET;
pub const LS_VECTOR_OFFSET: u32 = 26;
pub const LS_VECTOR_MASK: u32 = 0x1 << LS_VECTOR_OFFSET;
pub const LS_SIZE_OFFSET: u32 = 30;

pub const IMM_UNCOND_BRANCH_OFFSET: u32 = 0;
pub const IMM_UNCOND_BRANCH_WIDTH: u32 = 26;
pub const IMM_UNCOND_BRANCH_MASK: u32 =
    (((1u64 << IMM_UNCOND_BRANCH_WIDTH) - 1) as u32) << IMM_UNCOND_BRANCH_OFFSET;
pub const IMM_CMP_BRANCH_OFFSET: u32 = 5;
pub const IMM_CMP_BRANCH_WIDTH: u32 = 19;
pub const IMM_CMP_BRANCH_MASK: u32 = ((1 << IMM_CMP_BRANCH_WIDTH) - 1) << IMM_CMP_BRANCH_OFFSET;
pub const IMM_EXCEPTION_OFFSET: u32 = 5;
pub const IMM_HINT_OFFSET: u32 = 5;

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

pub const CS: Condition = Condition::Hs;
pub const CC: Condition = Condition::Lo;

pub fn negate_condition(cond: Condition) -> Condition {
    debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
    match cond as u32 ^ 1 {
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
        _ => unreachable!(),
    }
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

pub const SIXTY_FOUR_BITS: Instr = 0x80000000;
pub const THIRTY_TWO_BITS: Instr = 0x00000000;

pub const K_SF_OFFSET: u32 = 31;
pub const ADD_SUB_SET_FLAGS_BIT: Instr = 0x20000000;
pub const ADD: Instr = 0x00000000;
pub const ADDS: Instr = ADD | ADD_SUB_SET_FLAGS_BIT;
pub const SUB: Instr = 0x40000000;
pub const SUBS: Instr = SUB | ADD_SUB_SET_FLAGS_BIT;

pub const ADD_SUB_IMMEDIATE_FIXED: Instr = 0x11000000;
pub const ADD_SUB_SHIFTED_FIXED: Instr = 0x0B000000;
pub const ADD_SUB_EXTENDED_FIXED: Instr = 0x0B200000;

pub const NOT: Instr = 0x00200000;
pub const AND: Instr = 0x00000000;
pub const BIC: Instr = AND | NOT;
pub const ORR: Instr = 0x20000000;
pub const ORN: Instr = ORR | NOT;
pub const EOR: Instr = 0x40000000;
pub const EON: Instr = EOR | NOT;
pub const ANDS: Instr = 0x60000000;
pub const BICS: Instr = ANDS | NOT;

pub const LOGICAL_SHIFTED_FIXED: Instr = 0x0A000000;

pub const MOVE_WIDE_IMMEDIATE_FIXED: Instr = 0x12800000;
pub const MOVN: Instr = 0x00000000;
pub const MOVZ: Instr = 0x40000000;
pub const MOVK: Instr = 0x60000000;

pub const UNCONDITIONAL_BRANCH_FIXED: Instr = 0x14000000;
pub const UNCONDITIONAL_BRANCH_F_MASK: Instr = 0x7C000000;
pub const B: Instr = UNCONDITIONAL_BRANCH_FIXED | 0x00000000;
pub const BL: Instr = UNCONDITIONAL_BRANCH_FIXED | 0x80000000;

pub const UNCONDITIONAL_BRANCH_TO_REGISTER_FIXED: Instr = 0xD6000000;
pub const BR: Instr = UNCONDITIONAL_BRANCH_TO_REGISTER_FIXED | 0x001F0000;
pub const BLR: Instr = UNCONDITIONAL_BRANCH_TO_REGISTER_FIXED | 0x003F0000;
pub const RET: Instr = UNCONDITIONAL_BRANCH_TO_REGISTER_FIXED | 0x005F0000;

pub const COMPARE_BRANCH_FIXED: Instr = 0x34000000;
pub const COMPARE_BRANCH_F_MASK: Instr = 0x7E000000;
pub const CBZ: Instr = COMPARE_BRANCH_FIXED | 0x00000000;
pub const CBNZ: Instr = COMPARE_BRANCH_FIXED | 0x01000000;

pub const CONDITIONAL_BRANCH_FIXED: Instr = 0x54000000;
pub const CONDITIONAL_BRANCH_F_MASK: Instr = 0xFE000000;
pub const B_COND: Instr = CONDITIONAL_BRANCH_FIXED | 0x00000000;

pub const TEST_BRANCH_FIXED: Instr = 0x36000000;
pub const TEST_BRANCH_F_MASK: Instr = 0x7E000000;

pub const PC_REL_ADDRESSING_FIXED: Instr = 0x10000000;
pub const PC_REL_ADDRESSING_F_MASK: Instr = 0x1F000000;
pub const PC_REL_ADDRESSING_MASK: Instr = 0x9F000000;

pub const SYSTEM_HINT_FIXED: Instr = 0xD503201F;
pub const HINT: Instr = SYSTEM_HINT_FIXED | 0x00000000;

pub const EXCEPTION_FIXED: Instr = 0xD4000000;
pub const HLT: Instr = EXCEPTION_FIXED | 0x00400000;
pub const BRK: Instr = EXCEPTION_FIXED | 0x00200000;

pub const LOAD_STORE_UNSCALED_OFFSET_FIXED: Instr = 0x38000000;
pub const LOAD_STORE_UNSIGNED_OFFSET_FIXED: Instr = 0x39000000;
pub const LOAD_STORE_POST_INDEX_FIXED: Instr = 0x38000400;
pub const LOAD_STORE_PRE_INDEX_FIXED: Instr = 0x38000C00;

pub const STRB_W: Instr = 0x00000000;
pub const STRH_W: Instr = 0x40000000;
pub const STR_W: Instr = 0x80000000;
pub const STR_X: Instr = 0xC0000000;
pub const LDRB_W: Instr = 0x00400000;
pub const LDRH_W: Instr = 0x40400000;
pub const LDR_W: Instr = 0x80400000;
pub const LDR_X: Instr = 0xC0400000;
pub const LDRSB_X: Instr = 0x00800000;
pub const LDRSH_X: Instr = 0x40800000;
pub const LDRSW_X: Instr = 0x80800000;
pub const LDRSB_W: Instr = 0x00C00000;
pub const LDRSH_W: Instr = 0x40C00000;
pub const STR_B: Instr = 0x04000000;
pub const STR_H: Instr = 0x44000000;
pub const STR_S: Instr = 0x84000000;
pub const STR_D: Instr = 0xC4000000;
pub const STR_Q: Instr = 0x04800000;
pub const LDR_B: Instr = 0x04400000;
pub const LDR_H: Instr = 0x44400000;
pub const LDR_S: Instr = 0x84400000;
pub const LDR_D: Instr = 0xC4400000;
pub const LDR_Q: Instr = 0x04C00000;
pub const LOAD_STORE_REGISTER_OFFSET_FIXED: Instr = 0x38200800;

pub const K_X_REG_SIZE_LOG2: i32 = 3;
pub const K_W_REG_SIZE_LOG2: i32 = 2;
pub const K_BITFIELD_N_OFFSET: u32 = 22;

pub const RA_OFFSET: u32 = 10;
pub const RT2_OFFSET: u32 = 10;
pub const BIT_N_OFFSET: u32 = 22;
pub const IMM_ROTATE_OFFSET: u32 = 16;
pub const IMM_SET_BITS_OFFSET: u32 = 10;
pub const IMM_R_OFFSET: u32 = 16;
pub const IMM_S_OFFSET: u32 = 10;
pub const CONDITION_OFFSET: u32 = 12;
pub const NZCV_OFFSET: u32 = 0;
pub const IMM_COND_CMP_OFFSET: u32 = 16;
pub const IMM_LS_PAIR_OFFSET: u32 = 15;
pub const FLAGS_OFFSET: u32 = 28;

pub const IMM_TEST_BRANCH_BIT40_OFFSET: u32 = 19;
pub const IMM_TEST_BRANCH_BIT40_MASK: u32 = 0x1f << IMM_TEST_BRANCH_BIT40_OFFSET;
pub const IMM_TEST_BRANCH_BIT5_OFFSET: u32 = 31;
pub const IMM_TEST_BRANCH_BIT5_MASK: u32 = 0x1 << IMM_TEST_BRANCH_BIT5_OFFSET;

pub const IMM_PC_REL_HI_OFFSET: u32 = 5;
pub const IMM_PC_REL_HI_WIDTH: u32 = 19;
pub const IMM_PC_REL_HI_MASK: u32 = ((1 << IMM_PC_REL_HI_WIDTH) - 1) << IMM_PC_REL_HI_OFFSET;
pub const IMM_PC_REL_LO_OFFSET: u32 = 29;
pub const IMM_PC_REL_LO_WIDTH: u32 = 2;
pub const IMM_PC_REL_LO_MASK: u32 = ((1 << IMM_PC_REL_LO_WIDTH) - 1) << IMM_PC_REL_LO_OFFSET;
pub const IMM_PC_REL_MASK: u32 = IMM_PC_REL_LO_MASK | IMM_PC_REL_HI_MASK;

pub const N_MASK: u32 = 0x1 << 31;
pub const Z_MASK: u32 = 0x1 << 30;
pub const C_MASK: u32 = 0x1 << 29;
pub const V_MASK: u32 = 0x1 << 28;

pub const NO_FLAG: u32 = 0;
pub const N_FLAG: u32 = N_MASK;
pub const Z_FLAG: u32 = Z_MASK;
pub const C_FLAG: u32 = C_MASK;
pub const V_FLAG: u32 = V_MASK;

pub const ADR: Instr = PC_REL_ADDRESSING_FIXED | 0x00000000;
pub const ADRP: Instr = PC_REL_ADDRESSING_FIXED | 0x80000000;

pub const ADD_SUB_WITH_CARRY_FIXED: Instr = 0x1A000000;
pub const ADC: Instr = ADD_SUB_WITH_CARRY_FIXED | ADD;
pub const ADCS: Instr = ADD_SUB_WITH_CARRY_FIXED | ADDS;
pub const SBC: Instr = ADD_SUB_WITH_CARRY_FIXED | SUB;
pub const SBCS: Instr = ADD_SUB_WITH_CARRY_FIXED | SUBS;

pub const LOGICAL_IMMEDIATE_FIXED: Instr = 0x12000000;

pub const BITFIELD_FIXED: Instr = 0x13000000;
pub const SBFM: Instr = BITFIELD_FIXED | 0x00000000;
pub const BFM: Instr = BITFIELD_FIXED | 0x20000000;
pub const UBFM: Instr = BITFIELD_FIXED | 0x40000000;

pub const EXTRACT_FIXED: Instr = 0x13800000;
pub const EXTR: Instr = EXTRACT_FIXED | 0x00000000;

pub const TBZ: Instr = TEST_BRANCH_FIXED | 0x00000000;
pub const TBNZ: Instr = TEST_BRANCH_FIXED | 0x01000000;

pub const LOAD_STORE_PAIR_L_BIT: Instr = 1 << 22;
pub const STP_W: Instr = 0x00000000;
pub const LDP_W: Instr = 0x00400000;
pub const LDPSW_X: Instr = 0x40400000;
pub const STP_X: Instr = 0x80000000;
pub const LDP_X: Instr = 0x80400000;
pub const STP_S: Instr = 0x04000000;
pub const LDP_S: Instr = 0x04400000;
pub const STP_D: Instr = 0x44000000;
pub const LDP_D: Instr = 0x44400000;
pub const STP_Q: Instr = 0x84000000;
pub const LDP_Q: Instr = 0x84400000;
pub const LOAD_STORE_PAIR_POST_INDEX_FIXED: Instr = 0x28800000;
pub const LOAD_STORE_PAIR_PRE_INDEX_FIXED: Instr = 0x29800000;
pub const LOAD_STORE_PAIR_OFFSET_FIXED: Instr = 0x29000000;

pub const CONDITIONAL_COMPARE_REGISTER_FIXED: Instr = 0x1A400000;
pub const CONDITIONAL_COMPARE_IMMEDIATE_FIXED: Instr = 0x1A400800;
pub const CCMN: Instr = 0x20000000;
pub const CCMP: Instr = 0x60000000;

pub const CONDITIONAL_SELECT_FIXED: Instr = 0x1A800000;
pub const CSEL: Instr = CONDITIONAL_SELECT_FIXED | 0x00000000;
pub const CSINC: Instr = CONDITIONAL_SELECT_FIXED | 0x00000400;
pub const CSINV: Instr = CONDITIONAL_SELECT_FIXED | 0x40000000;
pub const CSNEG: Instr = CONDITIONAL_SELECT_FIXED | 0x40000400;

pub const DATA_PROCESSING_1_SOURCE_FIXED: Instr = 0x5AC00000;
pub const RBIT: Instr = DATA_PROCESSING_1_SOURCE_FIXED | 0x00000000;
pub const REV16: Instr = DATA_PROCESSING_1_SOURCE_FIXED | 0x00000400;
pub const REV: Instr = DATA_PROCESSING_1_SOURCE_FIXED | 0x00000800;
pub const REV_W: Instr = REV;
pub const REV_X: Instr = DATA_PROCESSING_1_SOURCE_FIXED | SIXTY_FOUR_BITS | 0x00000C00;
pub const CLZ: Instr = DATA_PROCESSING_1_SOURCE_FIXED | 0x00001000;
pub const CLS: Instr = DATA_PROCESSING_1_SOURCE_FIXED | 0x00001400;

pub const DATA_PROCESSING_2_SOURCE_FIXED: Instr = 0x1AC00000;
pub const UDIV: Instr = DATA_PROCESSING_2_SOURCE_FIXED | 0x00000800;
pub const SDIV: Instr = DATA_PROCESSING_2_SOURCE_FIXED | 0x00000C00;
pub const LSLV: Instr = DATA_PROCESSING_2_SOURCE_FIXED | 0x00002000;
pub const LSRV: Instr = DATA_PROCESSING_2_SOURCE_FIXED | 0x00002400;
pub const ASRV: Instr = DATA_PROCESSING_2_SOURCE_FIXED | 0x00002800;
pub const RORV: Instr = DATA_PROCESSING_2_SOURCE_FIXED | 0x00002C00;

pub const DATA_PROCESSING_3_SOURCE_FIXED: Instr = 0x1B000000;
pub const MADD: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x00000000;
pub const MSUB: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x00008000;
pub const SMADDL_X: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x80200000;
pub const SMSUBL_X: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x80208000;
pub const SMULH_X: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x80400000;
pub const UMADDL_X: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x80A00000;
pub const UMSUBL_X: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x80A08000;
pub const UMULH_X: Instr = DATA_PROCESSING_3_SOURCE_FIXED | 0x80C00000;

pub const FP32: Instr = 0x00000000;
pub const FP64: Instr = 0x00400000;

pub const IMM_FP_OFFSET: u32 = 13;
pub const FP_SCALE_OFFSET: u32 = 10;

pub const FP_COMPARE_FIXED: Instr = 0x1E202000;
pub const FCMP_S: Instr = FP_COMPARE_FIXED | 0x00000000;
pub const FCMP: Instr = FCMP_S;
pub const FCMP_S_ZERO: Instr = FP_COMPARE_FIXED | 0x00000008;
pub const FCMP_ZERO: Instr = FCMP_S_ZERO;

pub const FP_IMMEDIATE_FIXED: Instr = 0x1E201000;
pub const FMOV_S_IMM: Instr = FP_IMMEDIATE_FIXED | 0x00000000;
pub const FMOV_D_IMM: Instr = FP_IMMEDIATE_FIXED | FP64 | 0x00000000;

pub const FP_DATA_PROCESSING_1_SOURCE_FIXED: Instr = 0x1E204000;
pub const FMOV: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00000000;
pub const FABS: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00008000;
pub const FNEG: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00010000;
pub const FSQRT: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00018000;
pub const FCVT_DS: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00028000;
pub const FCVT_SD: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | FP64 | 0x00020000;
pub const FCVT_HS: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00038000;
pub const FCVT_HD: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | FP64 | 0x00038000;
pub const FCVT_SH: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00C20000;
pub const FCVT_DH: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00C28000;
pub const FRINTN: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00040000;
pub const FRINTP: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00048000;
pub const FRINTM: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00050000;
pub const FRINTZ: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00058000;
pub const FRINTA: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00060000;
pub const FRINTX: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00070000;
pub const FRINTI: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00078000;

pub const FP_DATA_PROCESSING_2_SOURCE_FIXED: Instr = 0x1E200800;
pub const FMUL: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00000000;
pub const FDIV: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00001000;
pub const FADD: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00002000;
pub const FSUB: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00003000;
pub const FMAX: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00004000;
pub const FMIN: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00005000;
pub const FMAXNM: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00006000;
pub const FMINNM: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00007000;
pub const FNMUL_S: Instr = FP_DATA_PROCESSING_2_SOURCE_FIXED | 0x00008000;
pub const FNMUL_D: Instr = FNMUL_S | FP64;

pub const FP_DATA_PROCESSING_3_SOURCE_FIXED: Instr = 0x1F000000;
pub const FMADD_S: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00000000;
pub const FMSUB_S: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00008000;
pub const FNMADD_S: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00200000;
pub const FNMSUB_S: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00208000;
pub const FMADD_D: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00400000;
pub const FMSUB_D: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00408000;
pub const FNMADD_D: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00600000;
pub const FNMSUB_D: Instr = FP_DATA_PROCESSING_3_SOURCE_FIXED | 0x00608000;

pub const FP_INTEGER_CONVERT_FIXED: Instr = 0x1E200000;
pub const FCVTNS: Instr = FP_INTEGER_CONVERT_FIXED | 0x00000000;
pub const FCVTNU: Instr = FP_INTEGER_CONVERT_FIXED | 0x00010000;
pub const SCVTF: Instr = FP_INTEGER_CONVERT_FIXED | 0x00020000;
pub const UCVTF: Instr = FP_INTEGER_CONVERT_FIXED | 0x00030000;
pub const FCVTAS: Instr = FP_INTEGER_CONVERT_FIXED | 0x00040000;
pub const FCVTAU: Instr = FP_INTEGER_CONVERT_FIXED | 0x00050000;
pub const FMOV_WS: Instr = FP_INTEGER_CONVERT_FIXED | 0x00060000;
pub const FMOV_SW: Instr = FP_INTEGER_CONVERT_FIXED | 0x00070000;
pub const FMOV_XD: Instr = FMOV_WS | SIXTY_FOUR_BITS | FP64;
pub const FMOV_DX: Instr = FMOV_SW | SIXTY_FOUR_BITS | FP64;
pub const FCVTPS: Instr = FP_INTEGER_CONVERT_FIXED | 0x00080000;
pub const FCVTPU: Instr = FP_INTEGER_CONVERT_FIXED | 0x00090000;
pub const FCVTMS: Instr = FP_INTEGER_CONVERT_FIXED | 0x00100000;
pub const FCVTMU: Instr = FP_INTEGER_CONVERT_FIXED | 0x00110000;
pub const FCVTZS: Instr = FP_INTEGER_CONVERT_FIXED | 0x00180000;
pub const FCVTZU: Instr = FP_INTEGER_CONVERT_FIXED | 0x00190000;
pub const FJCVTZS: Instr = FP_INTEGER_CONVERT_FIXED | FP64 | 0x001E0000;

pub const FP_FIXED_POINT_CONVERT_FIXED: Instr = 0x1E000000;
pub const SCVTF_FIXED: Instr = FP_FIXED_POINT_CONVERT_FIXED | 0x00020000;
pub const UCVTF_FIXED: Instr = FP_FIXED_POINT_CONVERT_FIXED | 0x00030000;
pub const FCVTZS_FIXED: Instr = FP_FIXED_POINT_CONVERT_FIXED | 0x00180000;
pub const FCVTZU_FIXED: Instr = FP_FIXED_POINT_CONVERT_FIXED | 0x00190000;

pub const K_LOAD_LITERAL_SCALE_LOG2: u8 = 2;
pub const K_INT32_SIZE: i32 = 4;
pub const K_INT64_SIZE: i32 = 8;
pub const K_MAX_INT: i32 = 0x7fffffff;

pub const IMM_L_LITERAL_OFFSET: u32 = 5;
pub const IMM_L_LITERAL_WIDTH: u32 = 19;
pub const IMM_L_LITERAL_MASK: u32 = ((1 << IMM_L_LITERAL_WIDTH) - 1) << IMM_L_LITERAL_OFFSET;

pub const LOAD_LITERAL_FIXED: Instr = 0x18000000;
pub const LOAD_LITERAL_F_MASK: Instr = 0x3B000000;
pub const LOAD_LITERAL_MASK: Instr = 0xFF000000;
pub const LDR_W_LIT: Instr = LOAD_LITERAL_FIXED | 0x00000000;
pub const LDR_X_LIT: Instr = LOAD_LITERAL_FIXED | 0x40000000;
pub const LDRSW_X_LIT: Instr = LOAD_LITERAL_FIXED | 0x80000000;
pub const LDR_S_LIT: Instr = LOAD_LITERAL_FIXED | 0x04000000;
pub const LDR_D_LIT: Instr = LOAD_LITERAL_FIXED | 0x44000000;

pub const ADD_SUB_IMMEDIATE_F_MASK: Instr = 0x1F000000;
pub const ADD_SUB_EXTENDED_F_MASK: Instr = 0x1F200000;
pub const ADD_SUB_SHIFTED_F_MASK: Instr = 0x1F200000;
pub const ADD_SUB_OP_MASK: Instr = 0x60000000;
pub const LOGICAL_IMMEDIATE_F_MASK: Instr = 0x1F800000;
pub const LOGICAL_IMMEDIATE_MASK: Instr = 0xFF800000;
pub const LOGICAL_OP_MASK: Instr = 0x60200000;
pub const LOAD_STORE_MASK: Instr = 0xC4C00000;
pub const LOAD_STORE_PAIR_MASK: Instr = 0xC4400000;
pub const LOAD_STORE_ANY_F_MASK: Instr = 0x0a000000;
pub const LOAD_STORE_ANY_FIXED: Instr = 0x08000000;
pub const LOAD_STORE_PAIR_ANY_F_MASK: Instr = 0x3a000000;
pub const LOAD_STORE_PAIR_ANY_FIXED: Instr = 0x28000000;
pub const MOVE_WIDE_IMMEDIATE_MASK: Instr = 0xFF800000;
pub const UNCONDITIONAL_BRANCH_MASK: Instr = 0xFC000000;
pub const UNCONDITIONAL_BRANCH_TO_REGISTER_MASK: Instr = 0xFFFFFC1F;
pub const COMPARE_BRANCH_MASK: Instr = 0xFF000000;
pub const CONDITIONAL_BRANCH_MASK: Instr = 0xFF000010;
pub const TEST_BRANCH_MASK: Instr = 0x7F000000;
pub const CONDITIONAL_SELECT_MASK: Instr = 0xFFE00C00;
pub const CONDITIONAL_COMPARE_MASK: Instr = 0x60000000;
pub const DATA_PROCESSING_1_SOURCE_MASK: Instr = 0xFFFFFC00;
pub const DATA_PROCESSING_2_SOURCE_MASK: Instr = 0xFFE0FC00;
pub const DATA_PROCESSING_3_SOURCE_MASK: Instr = 0xFFE08000;
pub const BITFIELD_MASK: Instr = 0xFF800000;
pub const EXTRACT_MASK: Instr = 0xFFA00000;
pub const EXCEPTION_MASK: Instr = 0xFFE0001F;
pub const PC_REL_ADDRESSING_MASK_FULL: Instr = 0x9F000000;

pub const CBZ_W: Instr = COMPARE_BRANCH_FIXED | 0x00000000;
pub const CBZ_X: Instr = COMPARE_BRANCH_FIXED | 0x80000000;
pub const CBNZ_W: Instr = COMPARE_BRANCH_FIXED | 0x01000000;
pub const CBNZ_X: Instr = COMPARE_BRANCH_FIXED | 0x81000000;
pub const CSEL_W: Instr = CONDITIONAL_SELECT_FIXED | 0x00000000;
pub const CSEL_X: Instr = CONDITIONAL_SELECT_FIXED | 0x80000000;
pub const CSINC_W: Instr = CONDITIONAL_SELECT_FIXED | 0x00000400;
pub const CSINC_X: Instr = CONDITIONAL_SELECT_FIXED | 0x80000400;
pub const CSINV_W: Instr = CONDITIONAL_SELECT_FIXED | 0x40000000;
pub const CSINV_X: Instr = CONDITIONAL_SELECT_FIXED | 0xC0000000;
pub const CSNEG_W: Instr = CONDITIONAL_SELECT_FIXED | 0x40000400;
pub const CSNEG_X: Instr = CONDITIONAL_SELECT_FIXED | 0xC0000400;
pub const MOVN_W: Instr = MOVE_WIDE_IMMEDIATE_FIXED | MOVN;
pub const MOVN_X: Instr = MOVE_WIDE_IMMEDIATE_FIXED | MOVN | SIXTY_FOUR_BITS;
pub const MOVZ_W: Instr = MOVE_WIDE_IMMEDIATE_FIXED | MOVZ;
pub const MOVZ_X: Instr = MOVE_WIDE_IMMEDIATE_FIXED | MOVZ | SIXTY_FOUR_BITS;
pub const MOVK_W: Instr = MOVE_WIDE_IMMEDIATE_FIXED | MOVK;
pub const MOVK_X: Instr = MOVE_WIDE_IMMEDIATE_FIXED | MOVK | SIXTY_FOUR_BITS;
pub const SBFM_W: Instr = BITFIELD_FIXED | 0x00000000;
pub const SBFM_X: Instr = BITFIELD_FIXED | 0x80000000;
pub const BFM_W: Instr = BITFIELD_FIXED | 0x20000000;
pub const BFM_X: Instr = BITFIELD_FIXED | 0xA0000000;
pub const UBFM_W: Instr = BITFIELD_FIXED | 0x40000000;
pub const UBFM_X: Instr = BITFIELD_FIXED | 0xC0000000;
pub const RBIT_W: Instr = RBIT;
pub const RBIT_X: Instr = RBIT | SIXTY_FOUR_BITS;
pub const REV16_W: Instr = REV16;
pub const REV16_X: Instr = REV16 | SIXTY_FOUR_BITS;
pub const REV32_X: Instr = REV | SIXTY_FOUR_BITS;
pub const CLZ_W: Instr = CLZ;
pub const CLZ_X: Instr = CLZ | SIXTY_FOUR_BITS;
pub const CLS_W: Instr = CLS;
pub const CLS_X: Instr = CLS | SIXTY_FOUR_BITS;
pub const UDIV_W: Instr = UDIV;
pub const UDIV_X: Instr = UDIV | SIXTY_FOUR_BITS;
pub const SDIV_W: Instr = SDIV;
pub const SDIV_X: Instr = SDIV | SIXTY_FOUR_BITS;
pub const LSLV_W: Instr = LSLV;
pub const LSLV_X: Instr = LSLV | SIXTY_FOUR_BITS;
pub const LSRV_W: Instr = LSRV;
pub const LSRV_X: Instr = LSRV | SIXTY_FOUR_BITS;
pub const ASRV_W: Instr = ASRV;
pub const ASRV_X: Instr = ASRV | SIXTY_FOUR_BITS;
pub const RORV_W: Instr = RORV;
pub const RORV_X: Instr = RORV | SIXTY_FOUR_BITS;
pub const MADD_W: Instr = MADD;
pub const MADD_X: Instr = MADD | SIXTY_FOUR_BITS;
pub const MSUB_W: Instr = MSUB;
pub const MSUB_X: Instr = MSUB | SIXTY_FOUR_BITS;

pub const K_BYTE_MASK: i64 = 0xff;
pub const K_HALF_WORD_MASK: i64 = 0xffff;
pub const K_WORD_MASK: i64 = 0xffffffff;
pub const K_SHIFT_AMOUNT_W_REG_MASK: u32 = 0x1f;
pub const K_SHIFT_AMOUNT_X_REG_MASK: u32 = 0x3f;
pub const K_LINK_REG_CODE: u32 = 30;

pub const FP_DATA_PROCESSING_1_SOURCE_MASK: Instr = 0xFFFFFC00;
pub const FP_DATA_PROCESSING_2_SOURCE_MASK: Instr = 0xFFE0FC00;
pub const FP_DATA_PROCESSING_3_SOURCE_MASK: Instr = 0xFFE08000;
pub const FP_COMPARE_MASK: Instr = 0xFFE0FC1F;
pub const FP_IMMEDIATE_MASK: Instr = 0xFFE01C00;
pub const FP_INTEGER_CONVERT_MASK: Instr = 0xFFFFFC00;
pub const FP_FIXED_POINT_CONVERT_MASK: Instr = 0xFFFF0000;

pub const FMOV_S: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00000000;
pub const FMOV_D: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | FP64 | 0x00000000;
pub const FABS_S: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00008000;
pub const FABS_D: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | FP64 | 0x00008000;
pub const FNEG_S: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00010000;
pub const FNEG_D: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | FP64 | 0x00010000;
pub const FSQRT_S: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | 0x00018000;
pub const FSQRT_D: Instr = FP_DATA_PROCESSING_1_SOURCE_FIXED | FP64 | 0x00018000;
pub const FRINTN_S: Instr = FRINTN;
pub const FRINTN_D: Instr = FRINTN | FP64;
pub const FRINTP_S: Instr = FRINTP;
pub const FRINTP_D: Instr = FRINTP | FP64;
pub const FRINTM_S: Instr = FRINTM;
pub const FRINTM_D: Instr = FRINTM | FP64;
pub const FRINTZ_S: Instr = FRINTZ;
pub const FRINTZ_D: Instr = FRINTZ | FP64;
pub const FRINTA_S: Instr = FRINTA;
pub const FRINTA_D: Instr = FRINTA | FP64;
pub const FRINTX_S: Instr = FRINTX;
pub const FRINTX_D: Instr = FRINTX | FP64;
pub const FRINTI_S: Instr = FRINTI;
pub const FRINTI_D: Instr = FRINTI | FP64;

pub const FMUL_S: Instr = FMUL;
pub const FMUL_D: Instr = FMUL | FP64;
pub const FDIV_S: Instr = FDIV;
pub const FDIV_D: Instr = FDIV | FP64;
pub const FADD_S: Instr = FADD;
pub const FADD_D: Instr = FADD | FP64;
pub const FSUB_S: Instr = FSUB;
pub const FSUB_D: Instr = FSUB | FP64;
pub const FMAX_S: Instr = FMAX;
pub const FMAX_D: Instr = FMAX | FP64;
pub const FMIN_S: Instr = FMIN;
pub const FMIN_D: Instr = FMIN | FP64;
pub const FMAXNM_S: Instr = FMAXNM;
pub const FMAXNM_D: Instr = FMAXNM | FP64;
pub const FMINNM_S: Instr = FMINNM;
pub const FMINNM_D: Instr = FMINNM | FP64;

pub const FCMP_D: Instr = FP_COMPARE_FIXED | FP64 | 0x00000000;
pub const FCMP_D_ZERO: Instr = FP_COMPARE_FIXED | FP64 | 0x00000008;

pub const FCVTZS_WS: Instr = FCVTZS;
pub const FCVTZS_XS: Instr = FCVTZS | SIXTY_FOUR_BITS;
pub const FCVTZS_WD: Instr = FCVTZS | FP64;
pub const FCVTZS_XD: Instr = FCVTZS | SIXTY_FOUR_BITS | FP64;
pub const FCVTZU_WS: Instr = FCVTZU;
pub const FCVTZU_XS: Instr = FCVTZU | SIXTY_FOUR_BITS;
pub const FCVTZU_WD: Instr = FCVTZU | FP64;
pub const FCVTZU_XD: Instr = FCVTZU | SIXTY_FOUR_BITS | FP64;
pub const SCVTF_SW: Instr = SCVTF;
pub const SCVTF_SX: Instr = SCVTF | SIXTY_FOUR_BITS;
pub const SCVTF_DW: Instr = SCVTF | FP64;
pub const SCVTF_DX: Instr = SCVTF | SIXTY_FOUR_BITS | FP64;
pub const UCVTF_SW: Instr = UCVTF;
pub const UCVTF_SX: Instr = UCVTF | SIXTY_FOUR_BITS;
pub const UCVTF_DW: Instr = UCVTF | FP64;
pub const UCVTF_DX: Instr = UCVTF | SIXTY_FOUR_BITS | FP64;
pub const FCVTNS_WS: Instr = FCVTNS;
pub const FCVTNS_XS: Instr = FCVTNS | SIXTY_FOUR_BITS;
pub const FCVTNS_WD: Instr = FCVTNS | FP64;
pub const FCVTNS_XD: Instr = FCVTNS | SIXTY_FOUR_BITS | FP64;
pub const FCVTMS_WD: Instr = FCVTMS | FP64;
pub const FCVTMS_XD: Instr = FCVTMS | SIXTY_FOUR_BITS | FP64;
pub const FCVTAS_WD: Instr = FCVTAS | FP64;
pub const FCVTAS_XD: Instr = FCVTAS | SIXTY_FOUR_BITS | FP64;

pub const SCVTF_SW_FIXED: Instr = SCVTF_FIXED;
pub const SCVTF_SX_FIXED: Instr = SCVTF_FIXED | SIXTY_FOUR_BITS;
pub const SCVTF_DW_FIXED: Instr = SCVTF_FIXED | FP64;
pub const SCVTF_DX_FIXED: Instr = SCVTF_FIXED | SIXTY_FOUR_BITS | FP64;
pub const UCVTF_SW_FIXED: Instr = UCVTF_FIXED;
pub const UCVTF_SX_FIXED: Instr = UCVTF_FIXED | SIXTY_FOUR_BITS;
pub const UCVTF_DW_FIXED: Instr = UCVTF_FIXED | FP64;
pub const UCVTF_DX_FIXED: Instr = UCVTF_FIXED | SIXTY_FOUR_BITS | FP64;
pub const FCVTZS_WS_FIXED: Instr = FCVTZS_FIXED;
pub const FCVTZS_XS_FIXED: Instr = FCVTZS_FIXED | SIXTY_FOUR_BITS;
pub const FCVTZS_WD_FIXED: Instr = FCVTZS_FIXED | FP64;
pub const FCVTZS_XD_FIXED: Instr = FCVTZS_FIXED | SIXTY_FOUR_BITS | FP64;
pub const FCVTZU_WS_FIXED: Instr = FCVTZU_FIXED;
pub const FCVTZU_XS_FIXED: Instr = FCVTZU_FIXED | SIXTY_FOUR_BITS;
pub const FCVTZU_WD_FIXED: Instr = FCVTZU_FIXED | FP64;
pub const FCVTZU_XD_FIXED: Instr = FCVTZU_FIXED | SIXTY_FOUR_BITS | FP64;

pub const K_FP64_POSITIVE_INFINITY: u64 = 0x7FF0000000000000;
pub const K_FP64_NEGATIVE_INFINITY: u64 = 0xFFF0000000000000;

pub const FP_EQUAL_FLAG: u32 = Z_FLAG | C_FLAG;
pub const FP_LESS_THAN_FLAG: u32 = N_FLAG;
pub const FP_GREATER_THAN_FLAG: u32 = C_FLAG;
pub const FP_UNORDERED_FLAG: u32 = C_FLAG | V_FLAG;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FPRounding {
    FPTieEven = 0x0,
    FPPositiveInfinity = 0x1,
    FPNegativeInfinity = 0x2,
    FPZero = 0x3,
    FPTieAway,
}

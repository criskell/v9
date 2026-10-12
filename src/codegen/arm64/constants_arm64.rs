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

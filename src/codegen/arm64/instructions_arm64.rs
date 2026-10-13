#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::*;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Reg31Mode {
    Reg31IsStackPointer,
    Reg31IsZeroRegister,
}

#[inline]
pub fn signed_bitextract_32(msb: u32, lsb: u32, x: u32) -> i32 {
    ((x as i32) << (31 - msb)) >> (lsb + 31 - msb)
}

#[inline]
pub fn unsigned_bitextract_32(msb: u32, lsb: u32, x: u32) -> u32 {
    (x >> lsb) & ((1 << (msb - lsb + 1)) - 1)
}

#[inline]
pub fn rotate_right(value: u64, rotate: u32, width: u32) -> u64 {
    debug_assert!(width <= 64);
    let rotate = rotate & 63;
    (value & ((1u128 << width) - 1) as u64)
        .wrapping_shr(rotate)
        | ((value.wrapping_shl(width - rotate)) & ((1u128 << width) - 1) as u64)
}

#[inline]
pub fn repeat_bits_across_reg(reg_size: u32, value: u64, width: u32) -> u64 {
    debug_assert!(width == 2 || width == 4 || width == 8 || width == 16 || width == 32);
    debug_assert!(reg_size == 32 || reg_size == 64);
    let mut result = value & ((1u128 << width) - 1) as u64;
    let mut i = width;
    while i < reg_size {
        result |= result << i;
        i *= 2;
    }
    result
}

pub fn calc_ls_data_size_log2(op: Instr) -> u32 {
    let mut size_log2 = op >> LSSize_offset;
    if (op & LSVector_mask) != 0
        && size_log2 == 0
        && ((op & LSOpc_mask) >> LSOpc_offset) >= 2
    {
        size_log2 = kQRegSizeLog2 as u32;
    }
    size_log2
}

pub fn calc_ls_pair_data_size(op: Instr) -> u32 {
    match op {
        STP_q | LDP_q => kQRegSizeLog2 as u32,
        STP_x | LDP_x | STP_d | LDP_d => kXRegSizeLog2 as u32,
        _ => kWRegSizeLog2 as u32,
    }
}

pub fn imm_pc_rel_address(imm21: i32) -> Instr {
    let imm = crate::utils::utils::checked_truncate_to_int21(imm21 as i64) as Instr;
    let immhi = (imm >> ImmPCRelLo_width) << ImmPCRelHi_offset;
    let immlo = imm << ImmPCRelLo_offset;
    (immhi & ImmPCRelHi_mask) | (immlo & ImmPCRelLo_mask)
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Instruction(Instr);

impl Instruction {
    #[inline]
    pub fn new(bits: Instr) -> Instruction {
        Instruction(bits)
    }

    #[inline]
    pub fn instruction_bits(self) -> Instr {
        self.0
    }

    #[inline]
    pub fn set_instruction_bits(&mut self, new_instr: Instr) {
        self.0 = new_instr;
    }

    #[inline]
    pub fn mask(self, mask: u32) -> Instr {
        self.instruction_bits() & mask
    }

    #[inline]
    fn signed_bits(self, msb: u32, lsb: u32) -> i32 {
        signed_bitextract_32(msb, lsb, self.0)
    }

    #[inline]
    pub fn imm_cond_branch(self) -> i32 {
        self.signed_bits(23, 5)
    }

    #[inline]
    pub fn imm_uncond_branch(self) -> i32 {
        self.signed_bits(25, 0)
    }

    #[inline]
    pub fn imm_cmp_branch(self) -> i32 {
        self.signed_bits(23, 5)
    }

    #[inline]
    pub fn imm_test_branch(self) -> i32 {
        self.signed_bits(18, 5)
    }

    #[inline]
    pub fn imm_pc_rel_hi(self) -> i32 {
        self.signed_bits(23, 5)
    }

    #[inline]
    pub fn imm_pc_rel_lo(self) -> i32 {
        ((self.0 & ImmPCRelLo_mask) >> ImmPCRelLo_offset) as i32
    }

    pub fn imm_pc_rel(self) -> i32 {
        debug_assert!(self.is_pc_rel_addressing());
        let offset = ((self.imm_pc_rel_hi() as u32) << ImmPCRelLo_width)
            | (self.imm_pc_rel_lo() as u32);
        let width = ImmPCRelLo_width + ImmPCRelHi_width;
        signed_bitextract_32(width - 1, 0, offset)
    }

    #[inline]
    pub fn bits(self, msb: u32, lsb: u32) -> u32 {
        unsigned_bitextract_32(msb, lsb, self.0)
    }

    #[inline]
    pub fn bit(self, pos: u32) -> u32 {
        (self.0 >> pos) & 1
    }

    #[inline]
    pub fn rd(self) -> u32 {
        self.bits(4, 0)
    }

    #[inline]
    pub fn rn(self) -> u32 {
        self.bits(9, 5)
    }

    #[inline]
    pub fn rm(self) -> u32 {
        self.bits(20, 16)
    }

    #[inline]
    pub fn ra(self) -> u32 {
        self.bits(14, 10)
    }

    #[inline]
    pub fn rt(self) -> u32 {
        self.bits(4, 0)
    }

    #[inline]
    pub fn rt2(self) -> u32 {
        self.bits(14, 10)
    }

    #[inline]
    pub fn sixty_four_bits(self) -> u32 {
        self.bit(31)
    }

    #[inline]
    pub fn flags_update(self) -> u32 {
        self.bit(29)
    }

    #[inline]
    pub fn shift_dp(self) -> u32 {
        self.bits(23, 22)
    }

    #[inline]
    pub fn imm_dp_shift(self) -> u32 {
        self.bits(15, 10)
    }

    #[inline]
    pub fn imm_add_sub(self) -> u32 {
        self.bits(21, 10)
    }

    #[inline]
    pub fn shift_add_sub(self) -> u32 {
        self.bits(23, 22)
    }

    #[inline]
    pub fn extend_mode(self) -> u32 {
        self.bits(15, 13)
    }

    #[inline]
    pub fn imm_extend_shift(self) -> u32 {
        self.bits(12, 10)
    }

    #[inline]
    pub fn imm_move_wide(self) -> u32 {
        self.bits(20, 5)
    }

    #[inline]
    pub fn shift_move_wide(self) -> u32 {
        self.bits(22, 21)
    }

    #[inline]
    pub fn condition(self) -> u32 {
        self.bits(15, 12)
    }

    #[inline]
    pub fn condition_branch(self) -> u32 {
        self.bits(3, 0)
    }

    #[inline]
    pub fn nzcv(self) -> u32 {
        self.bits(3, 0)
    }

    #[inline]
    pub fn imm_cond_cmp(self) -> u32 {
        self.bits(20, 16)
    }

    #[inline]
    pub fn bit_n(self) -> u32 {
        self.bit(22)
    }

    #[inline]
    pub fn imm_rotate(self) -> u32 {
        self.bits(21, 16)
    }

    #[inline]
    pub fn imm_set_bits(self) -> u32 {
        self.bits(15, 10)
    }

    #[inline]
    pub fn imm_r(self) -> u32 {
        self.bits(21, 16)
    }

    #[inline]
    pub fn imm_s(self) -> u32 {
        self.bits(15, 10)
    }

    #[inline]
    pub fn imm_ls(self) -> i32 {
        self.signed_bits(20, 12)
    }

    #[inline]
    pub fn imm_ls_unsigned(self) -> u32 {
        self.bits(21, 10)
    }

    #[inline]
    pub fn imm_ls_pair(self) -> i32 {
        self.signed_bits(21, 15)
    }

    #[inline]
    pub fn imm_shift_ls(self) -> u32 {
        self.bit(12)
    }

    #[inline]
    pub fn imm_test_branch_bit40(self) -> u32 {
        self.bits(23, 19)
    }

    #[inline]
    pub fn imm_test_branch_bit5(self) -> u32 {
        self.bit(31)
    }

    #[inline]
    pub fn imm_exception(self) -> u32 {
        self.bits(20, 5)
    }

    #[inline]
    pub fn size_ls(self) -> u32 {
        calc_ls_data_size_log2(self.mask(LoadStoreMask))
    }

    #[inline]
    pub fn size_lsp(self) -> u32 {
        calc_ls_pair_data_size(self.mask(LoadStorePairMask))
    }

    pub fn imm_logical(self) -> u64 {
        let reg_size = if self.sixty_four_bits() != 0 {
            kXRegSizeInBits as u32
        } else {
            kWRegSizeInBits as u32
        };
        let n = self.bit_n();
        let imm_s = self.imm_set_bits();
        let imm_r = self.imm_rotate();

        if n == 1 {
            if imm_s == 0x3F {
                return 0;
            }
            let bits = (1u64 << (imm_s + 1)) - 1;
            return rotate_right(bits, imm_r, 64);
        }
        if (imm_s >> 1) == 0x1F {
            return 0;
        }
        let mut width = 0x20u32;
        while width >= 0x2 {
            if (imm_s & width) == 0 {
                let mask = width - 1;
                if (imm_s & mask) == mask {
                    return 0;
                }
                let bits = (1u64 << ((imm_s & mask) + 1)) - 1;
                return repeat_bits_across_reg(
                    reg_size,
                    rotate_right(bits, imm_r & mask, width),
                    width,
                );
            }
            width >>= 1;
        }
        unreachable!()
    }

    pub fn is_add_sub_immediate(self) -> bool {
        self.mask(AddSubImmediateFMask) == AddSubImmediateFixed
    }

    pub fn is_add_sub_extended(self) -> bool {
        self.mask(AddSubExtendedFMask) == AddSubExtendedFixed
    }

    pub fn is_logical_immediate(self) -> bool {
        self.mask(LogicalImmediateFMask) == LogicalImmediateFixed
    }

    pub fn rd_mode(self) -> Reg31Mode {
        if self.is_add_sub_immediate() || self.is_add_sub_extended() {
            if self.mask(AddSubSetFlagsBit) != 0 {
                return Reg31Mode::Reg31IsZeroRegister;
            }
            return Reg31Mode::Reg31IsStackPointer;
        }
        if self.is_logical_immediate() {
            if self.mask(LogicalImmediateMask & LogicalOpMask) == ANDS {
                return Reg31Mode::Reg31IsZeroRegister;
            }
            return Reg31Mode::Reg31IsStackPointer;
        }
        Reg31Mode::Reg31IsZeroRegister
    }

    pub fn rn_mode(self) -> Reg31Mode {
        if self.is_add_sub_immediate()
            || self.is_add_sub_extended()
            || self.is_logical_immediate()
            || self.is_load_store()
        {
            return Reg31Mode::Reg31IsStackPointer;
        }
        Reg31Mode::Reg31IsZeroRegister
    }

    pub fn is_load_store(self) -> bool {
        self.mask(LoadStoreAnyFMask) == LoadStoreAnyFixed
    }

    pub fn is_load(self) -> bool {
        if self.mask(LoadStoreAnyFMask) != LoadStoreAnyFixed {
            return false;
        }
        if self.mask(LoadStorePairAnyFMask) == LoadStorePairAnyFixed {
            return self.mask(LoadStorePairLBit) != 0;
        }
        matches!(
            self.mask(LoadStoreMask),
            LDRB_w | LDRH_w | LDR_w | LDR_x | LDRSB_w | LDRSB_x | LDRSH_w | LDRSH_x
                | LDRSW_x | LDR_b | LDR_h | LDR_s | LDR_d | LDR_q
        )
    }

    pub fn is_store(self) -> bool {
        if self.mask(LoadStoreAnyFMask) != LoadStoreAnyFixed {
            return false;
        }
        if self.mask(LoadStorePairAnyFMask) == LoadStorePairAnyFixed {
            return self.mask(LoadStorePairLBit) == 0;
        }
        matches!(
            self.mask(LoadStoreMask),
            STRB_w | STRH_w | STR_w | STR_x | STR_b | STR_h | STR_s | STR_d | STR_q
        )
    }

    #[inline]
    pub fn imm_fp(self) -> u32 {
        self.bits(20, 13)
    }

    pub fn imm_fp32(self) -> f32 {
        Instruction::imm8_to_fp32(self.imm_fp())
    }

    pub fn imm_fp64(self) -> f64 {
        Instruction::imm8_to_fp64(self.imm_fp())
    }

    pub fn imm8_to_fp32(imm8: u32) -> f32 {
        let bits = imm8;
        let bit7 = (bits >> 7) & 0x1;
        let bit6 = (bits >> 6) & 0x1;
        let bit5_to_0 = bits & 0x3f;
        let result = (bit7 << 31) | ((32 - bit6) << 25) | (bit5_to_0 << 19);
        f32::from_bits(result)
    }

    pub fn imm8_to_fp64(imm8: u32) -> f64 {
        let bits = imm8 as u64;
        let bit7 = (bits >> 7) & 0x1;
        let bit6 = (bits >> 6) & 0x1;
        let bit5_to_0 = bits & 0x3f;
        let result = (bit7 << 63) | ((256 - bit6) << 54) | (bit5_to_0 << 48);
        f64::from_bits(result)
    }

    #[inline]
    pub fn fp_type(self) -> u32 {
        self.bits(23, 22)
    }

    #[inline]
    pub fn fp_scale(self) -> u32 {
        self.bits(15, 10)
    }

    pub fn is_ldr_literal(self) -> bool {
        self.mask(LoadLiteralFMask) == LoadLiteralFixed
    }

    pub fn is_ldr_literal_x(self) -> bool {
        self.mask(LoadLiteralMask) == LDR_x_lit
    }

    #[inline]
    pub fn imm_l_literal(self) -> i32 {
        self.signed_bits(23, 5)
    }

    pub fn set_imm_l_literal(&mut self, distance: i64) {
        debug_assert!(self.is_ldr_literal());
        debug_assert_eq!(distance % kInstrSize as i64, 0);
        let imm = (crate::utils::utils::checked_truncate_to_int19(
            distance >> kLoadLiteralScaleLog2,
        ) as Instr)
            << ImmLLiteral_offset;
        let bits = self.mask(!ImmLLiteral_mask) | imm;
        self.set_instruction_bits(bits);
    }

    pub fn is_adr(self) -> bool {
        self.mask(PCRelAddressingMask) == ADR
    }

    pub fn is_pc_rel_addressing(self) -> bool {
        self.mask(PCRelAddressingFMask) == PCRelAddressingFixed
    }

    pub fn is_cond_branch_imm(self) -> bool {
        self.mask(ConditionalBranchFMask) == ConditionalBranchFixed
    }

    pub fn is_uncond_branch_imm(self) -> bool {
        self.mask(UnconditionalBranchFMask) == UnconditionalBranchFixed
    }

    pub fn is_compare_branch(self) -> bool {
        self.mask(CompareBranchFMask) == CompareBranchFixed
    }

    pub fn is_test_branch(self) -> bool {
        self.mask(TestBranchFMask) == TestBranchFixed
    }

    pub fn branch_type(self) -> ImmBranchType {
        if self.is_cond_branch_imm() {
            ImmBranchType::CondBranchType
        } else if self.is_uncond_branch_imm() {
            ImmBranchType::UncondBranchType
        } else if self.is_compare_branch() {
            ImmBranchType::CompareBranchType
        } else if self.is_test_branch() {
            ImmBranchType::TestBranchType
        } else {
            ImmBranchType::UnknownBranchType
        }
    }

    pub fn imm_branch(self) -> i32 {
        match self.branch_type() {
            ImmBranchType::CondBranchType => self.imm_cond_branch(),
            ImmBranchType::UncondBranchType => self.imm_uncond_branch(),
            ImmBranchType::CompareBranchType => self.imm_cmp_branch(),
            ImmBranchType::TestBranchType => self.imm_test_branch(),
            ImmBranchType::UnknownBranchType => unreachable!(),
        }
    }

    pub fn imm_pc_offset(self) -> i64 {
        let offset;
        if self.is_pc_rel_addressing() {
            offset = self.imm_pc_rel() as i64;
        } else if self.branch_type() != ImmBranchType::UnknownBranchType {
            offset = self.imm_branch() as i64 * kInstrSize as i64;
        } else {
            debug_assert!(self.is_ldr_literal());
            offset = self.imm_l_literal() as i64 * kInstrSize as i64;
        }
        offset
    }

    pub fn imm_branch_range_bitwidth(branch_type: ImmBranchType) -> u32 {
        match branch_type {
            ImmBranchType::UnknownBranchType => unreachable!(),
            ImmBranchType::CondBranchType => ImmCondBranch_width,
            ImmBranchType::UncondBranchType => ImmUncondBranch_width,
            ImmBranchType::CompareBranchType => ImmCmpBranch_width,
            ImmBranchType::TestBranchType => ImmTestBranch_width,
        }
    }

    pub fn is_valid_imm_pc_offset(branch_type: ImmBranchType, offset: i64) -> bool {
        debug_assert_eq!(offset % kInstrSize as i64, 0);
        crate::utils::utils::is_intn(
            offset / kInstrSize as i64,
            Instruction::imm_branch_range_bitwidth(branch_type),
        )
    }

    pub fn set_branch_imm_target(&mut self, branch_type: ImmBranchType, distance: i64) {
        use crate::utils::utils::{
            checked_truncate_to_int14, checked_truncate_to_int19, checked_truncate_to_int26,
        };
        debug_assert_eq!(distance % kInstrSize as i64, 0);
        let offset = distance >> kInstrSizeLog2;
        let branch_imm;
        let imm_mask;
        match branch_type {
            ImmBranchType::CondBranchType | ImmBranchType::CompareBranchType => {
                branch_imm =
                    (checked_truncate_to_int19(offset) as u32) << ImmCondBranch_offset;
                imm_mask = ImmCondBranch_mask;
            }
            ImmBranchType::UncondBranchType => {
                branch_imm =
                    (checked_truncate_to_int26(offset) as u32) << ImmUncondBranch_offset;
                imm_mask = ImmUncondBranch_mask;
            }
            ImmBranchType::TestBranchType => {
                branch_imm =
                    (checked_truncate_to_int14(offset) as u32) << ImmTestBranch_offset;
                imm_mask = ImmTestBranch_mask;
            }
            ImmBranchType::UnknownBranchType => unreachable!(),
        }
        let bits = self.mask(!imm_mask) | branch_imm;
        self.set_instruction_bits(bits);
    }

    pub fn is_valid_pc_rel_offset(offset: i64) -> bool {
        crate::utils::utils::is_int21(offset)
    }

    pub fn set_pc_rel_imm_target(&mut self, distance: i64) {
        debug_assert!(self.is_adr());
        debug_assert!(Instruction::is_valid_pc_rel_offset(distance));
        let imm = imm_pc_rel_address(distance as i32);
        let bits = self.mask(!ImmPCRel_mask) | imm;
        self.set_instruction_bits(bits);
    }

    pub fn set_imm_pc_offset_target(&mut self, distance: i64) {
        if self.is_pc_rel_addressing() {
            self.set_pc_rel_imm_target(distance);
            return;
        }
        let branch_type = self.branch_type();
        if branch_type == ImmBranchType::UnknownBranchType {
            self.set_imm_l_literal(distance);
            return;
        }
        self.set_branch_imm_target(branch_type, distance);
    }
}

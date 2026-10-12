use crate::codegen::arm64::constants_arm64::*;

#[inline]
pub fn signed_bitextract_32(msb: u32, lsb: u32, x: u32) -> i32 {
    ((x as i32) << (31 - msb)) >> (lsb + 31 - msb)
}

pub fn imm_pc_rel_address(imm21: i32) -> Instr {
    let imm = crate::utils::utils::checked_truncate_to_int21(imm21 as i64) as Instr;
    let immhi = (imm >> IMM_PC_REL_LO_WIDTH) << IMM_PC_REL_HI_OFFSET;
    let immlo = imm << IMM_PC_REL_LO_OFFSET;
    (immhi & IMM_PC_REL_HI_MASK) | (immlo & IMM_PC_REL_LO_MASK)
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
        ((self.0 & IMM_PC_REL_LO_MASK) >> IMM_PC_REL_LO_OFFSET) as i32
    }

    pub fn imm_pc_rel(self) -> i32 {
        debug_assert!(self.is_pc_rel_addressing());
        let offset = ((self.imm_pc_rel_hi() as u32) << IMM_PC_REL_LO_WIDTH)
            | (self.imm_pc_rel_lo() as u32);
        let width = IMM_PC_REL_LO_WIDTH + IMM_PC_REL_HI_WIDTH;
        signed_bitextract_32(width - 1, 0, offset)
    }

    pub fn is_adr(self) -> bool {
        self.mask(PC_REL_ADDRESSING_MASK) == ADR
    }

    pub fn is_pc_rel_addressing(self) -> bool {
        self.mask(PC_REL_ADDRESSING_F_MASK) == PC_REL_ADDRESSING_FIXED
    }

    pub fn is_cond_branch_imm(self) -> bool {
        self.mask(CONDITIONAL_BRANCH_F_MASK) == CONDITIONAL_BRANCH_FIXED
    }

    pub fn is_uncond_branch_imm(self) -> bool {
        self.mask(UNCONDITIONAL_BRANCH_F_MASK) == UNCONDITIONAL_BRANCH_FIXED
    }

    pub fn is_compare_branch(self) -> bool {
        self.mask(COMPARE_BRANCH_F_MASK) == COMPARE_BRANCH_FIXED
    }

    pub fn is_test_branch(self) -> bool {
        self.mask(TEST_BRANCH_F_MASK) == TEST_BRANCH_FIXED
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
        } else {
            debug_assert_ne!(self.branch_type(), ImmBranchType::UnknownBranchType);
            offset = self.imm_branch() as i64 * K_INSTR_SIZE as i64;
        }
        offset
    }

    pub fn imm_branch_range_bitwidth(branch_type: ImmBranchType) -> u32 {
        match branch_type {
            ImmBranchType::UnknownBranchType => unreachable!(),
            ImmBranchType::CondBranchType => IMM_COND_BRANCH_WIDTH,
            ImmBranchType::UncondBranchType => IMM_UNCOND_BRANCH_WIDTH,
            ImmBranchType::CompareBranchType => IMM_CMP_BRANCH_WIDTH,
            ImmBranchType::TestBranchType => IMM_TEST_BRANCH_WIDTH,
        }
    }

    pub fn is_valid_imm_pc_offset(branch_type: ImmBranchType, offset: i64) -> bool {
        debug_assert_eq!(offset % K_INSTR_SIZE as i64, 0);
        crate::utils::utils::is_intn(
            offset / K_INSTR_SIZE as i64,
            Instruction::imm_branch_range_bitwidth(branch_type),
        )
    }

    pub fn set_branch_imm_target(&mut self, branch_type: ImmBranchType, distance: i64) {
        use crate::utils::utils::{
            checked_truncate_to_int14, checked_truncate_to_int19, checked_truncate_to_int26,
        };
        debug_assert_eq!(distance % K_INSTR_SIZE as i64, 0);
        let offset = distance >> K_INSTR_SIZE_LOG2;
        let branch_imm;
        let imm_mask;
        match branch_type {
            ImmBranchType::CondBranchType | ImmBranchType::CompareBranchType => {
                branch_imm =
                    (checked_truncate_to_int19(offset) as u32) << IMM_COND_BRANCH_OFFSET;
                imm_mask = IMM_COND_BRANCH_MASK;
            }
            ImmBranchType::UncondBranchType => {
                branch_imm =
                    (checked_truncate_to_int26(offset) as u32) << IMM_UNCOND_BRANCH_OFFSET;
                imm_mask = IMM_UNCOND_BRANCH_MASK;
            }
            ImmBranchType::TestBranchType => {
                branch_imm =
                    (checked_truncate_to_int14(offset) as u32) << IMM_TEST_BRANCH_OFFSET;
                imm_mask = IMM_TEST_BRANCH_MASK;
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
        let bits = self.mask(!IMM_PC_REL_MASK) | imm;
        self.set_instruction_bits(bits);
    }

    pub fn set_imm_pc_offset_target(&mut self, distance: i64) {
        if self.is_pc_rel_addressing() {
            self.set_pc_rel_imm_target(distance);
            return;
        }
        let branch_type = self.branch_type();
        self.set_branch_imm_target(branch_type, distance);
    }
}

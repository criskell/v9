use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::instructions_arm64::Instruction;
use crate::codegen::arm64::register_arm64::*;
use crate::codegen::assembler::AssemblerBase;
use crate::codegen::label::{Distance, Label};
use crate::codegen::arm64::utils_arm64::{count_leading_zeros, largest_power_of_2_divisor};
use crate::utils::utils::{
    checked_truncate_to_int14, checked_truncate_to_int19, checked_truncate_to_int26,
    checked_truncate_to_int7, checked_truncate_to_int9, is_int7,
    is_int9, is_uint1, is_uint12, is_uint16, is_uint2, is_uint32, is_uint5, is_uint6,
    is_uint7,
};

#[derive(Copy, Clone)]
pub struct Operand {
    immediate: i64,
    reg: Register,
    shift: Shift,
    extend: Extend,
    shift_amount: u32,
}

impl Operand {
    pub fn with_shift(reg: Register, shift: Shift, shift_amount: u32) -> Operand {
        debug_assert!(reg.is_64_bits() || (shift_amount < K_W_REG_SIZE_IN_BITS as u32));
        debug_assert!(reg.is_32_bits() || (shift_amount < K_X_REG_SIZE_IN_BITS as u32));
        debug_assert!(!reg.is_sp() || shift_amount == 0);
        Operand {
            immediate: 0,
            reg,
            shift,
            extend: Extend::NoExtend,
            shift_amount,
        }
    }

    pub fn with_extend(reg: Register, extend: Extend, shift_amount: u32) -> Operand {
        debug_assert!(reg.is_valid());
        debug_assert!(shift_amount <= 4);
        debug_assert!(!reg.is_sp());
        debug_assert!(
            (extend != Extend::Sxtx) || (extend != Extend::Uxtx) || !reg.is_32_bits()
        );
        Operand {
            immediate: 0,
            reg,
            shift: Shift::NoShift,
            extend,
            shift_amount,
        }
    }

    #[inline]
    pub fn is_immediate(&self) -> bool {
        self.reg == NO_REG
    }

    #[inline]
    pub fn is_plain_register(&self) -> bool {
        self.reg.is_valid()
            && (((self.shift == Shift::NoShift) && (self.extend == Extend::NoExtend))
                || ((self.shift != Shift::NoShift) && (self.shift_amount == 0))
                || (((self.extend == Extend::Uxtx) || (self.extend == Extend::Sxtx))
                    && (self.shift_amount == 0)))
    }

    #[inline]
    pub fn is_shifted_register(&self) -> bool {
        self.reg.is_valid() && (self.shift != Shift::NoShift)
    }

    #[inline]
    pub fn is_extended_register(&self) -> bool {
        self.reg.is_valid() && (self.extend != Extend::NoExtend)
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        if self.is_immediate() {
            self.immediate_value() == 0
        } else {
            self.reg().is_zero()
        }
    }

    pub fn to_extended_register(&self) -> Operand {
        debug_assert!(self.is_shifted_register());
        debug_assert!((self.shift == Shift::Lsl) && (self.shift_amount <= 4));
        Operand::with_extend(
            self.reg,
            if self.reg.is_64_bits() {
                Extend::Uxtx
            } else {
                Extend::Uxtw
            },
            self.shift_amount,
        )
    }

    #[inline]
    pub fn immediate_value(&self) -> i64 {
        debug_assert!(self.is_immediate());
        self.immediate
    }

    #[inline]
    pub fn reg(&self) -> Register {
        debug_assert!(self.is_shifted_register() || self.is_extended_register());
        self.reg
    }

    #[inline]
    pub fn shift(&self) -> Shift {
        debug_assert!(self.is_shifted_register());
        self.shift
    }

    #[inline]
    pub fn extend(&self) -> Extend {
        debug_assert!(self.is_extended_register());
        self.extend
    }

    #[inline]
    pub fn shift_amount(&self) -> u32 {
        debug_assert!(self.is_shifted_register() || self.is_extended_register());
        self.shift_amount
    }
}

impl From<Register> for Operand {
    fn from(reg: Register) -> Operand {
        Operand::with_shift(reg, Shift::Lsl, 0)
    }
}

impl From<i64> for Operand {
    fn from(immediate: i64) -> Operand {
        Operand {
            immediate,
            reg: NO_REG,
            shift: Shift::NoShift,
            extend: Extend::NoExtend,
            shift_amount: 0,
        }
    }
}

impl From<i32> for Operand {
    fn from(immediate: i32) -> Operand {
        Operand::from(immediate as i64)
    }
}

#[derive(Copy, Clone)]
pub struct MemOperand {
    base: Register,
    regoffset: Register,
    offset: i64,
    addrmode: AddrMode,
    shift: Shift,
    extend: Extend,
    shift_amount: u32,
}

impl MemOperand {
    pub fn new(base: Register, offset: i64, addrmode: AddrMode) -> MemOperand {
        debug_assert!(base.is_64_bits() && !base.is_zero());
        MemOperand {
            base,
            regoffset: NO_REG,
            offset,
            addrmode,
            shift: Shift::NoShift,
            extend: Extend::NoExtend,
            shift_amount: 0,
        }
    }

    pub fn with_shift(
        base: Register,
        regoffset: Register,
        shift: Shift,
        shift_amount: u32,
    ) -> MemOperand {
        debug_assert!(base.is_64_bits() && !base.is_zero());
        debug_assert!(!regoffset.is_sp());
        debug_assert!(shift == Shift::Lsl);
        MemOperand {
            base,
            regoffset,
            offset: 0,
            addrmode: AddrMode::Offset,
            shift,
            extend: Extend::NoExtend,
            shift_amount,
        }
    }

    #[inline]
    pub fn base(&self) -> Register {
        self.base
    }

    #[inline]
    pub fn regoffset(&self) -> Register {
        self.regoffset
    }

    #[inline]
    pub fn offset(&self) -> i64 {
        self.offset
    }

    #[inline]
    pub fn addrmode(&self) -> AddrMode {
        self.addrmode
    }

    #[inline]
    pub fn shift(&self) -> Shift {
        self.shift
    }

    #[inline]
    pub fn extend(&self) -> Extend {
        self.extend
    }

    #[inline]
    pub fn shift_amount(&self) -> u32 {
        self.shift_amount
    }

    #[inline]
    pub fn is_immediate_offset(&self) -> bool {
        (self.addrmode == AddrMode::Offset) && self.regoffset == NO_REG
    }

    #[inline]
    pub fn is_register_offset(&self) -> bool {
        (self.addrmode == AddrMode::Offset) && self.regoffset != NO_REG
    }

    #[inline]
    pub fn is_pre_index(&self) -> bool {
        self.addrmode == AddrMode::PreIndex
    }

    #[inline]
    pub fn is_post_index(&self) -> bool {
        self.addrmode == AddrMode::PostIndex
    }
}

pub struct Assembler {
    base: AssemblerBase,
    constpool: crate::codegen::arm64::constant_pool_arm64::ConstantPool,
}

impl Default for Assembler {
    fn default() -> Assembler {
        Assembler::new()
    }
}

impl Assembler {
    pub const K_START_OF_LABEL_LINK_CHAIN: i32 = 0;

    pub fn new() -> Assembler {
        Assembler {
            base: AssemblerBase::new(),
            constpool: crate::codegen::arm64::constant_pool_arm64::ConstantPool::new(),
        }
    }

    #[inline]
    pub fn pc_offset(&self) -> i32 {
        self.base.pc_offset()
    }

    #[inline]
    pub fn buffer_start(&self) -> &[u8] {
        self.base.buffer_start()
    }

    pub fn instruction_at(&self, offset: i32) -> Instruction {
        let offset = offset as usize;
        let bytes = &self.base.buffer_start()[offset..offset + K_INSTR_SIZE as usize];
        Instruction::new(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn set_instruction_at(&mut self, offset: i32, instr: Instruction) {
        self.base
            .write_bytes_at(offset, &instr.instruction_bits().to_le_bytes());
    }

    fn emit(&mut self, instruction: Instr) {
        self.base.emit_bytes(&instruction.to_le_bytes());
        self.check_buffer();
    }

    pub fn bind(&mut self, label: &mut Label) {
        debug_assert!(!label.is_near_linked());
        debug_assert!(!label.is_bound());

        while label.is_linked() {
            let linkoffset = label.pos();
            let mut link = self.instruction_at(linkoffset);
            let prevlinkoffset = linkoffset + link.imm_pc_offset() as i32;

            debug_assert!(linkoffset >= 0);
            debug_assert!(linkoffset < self.pc_offset());
            debug_assert!(
                (linkoffset > prevlinkoffset)
                    || (linkoffset - prevlinkoffset == Assembler::K_START_OF_LABEL_LINK_CHAIN)
            );
            debug_assert!(prevlinkoffset >= 0);

            link.set_imm_pc_offset_target((self.pc_offset() - linkoffset) as i64);
            self.set_instruction_at(linkoffset, link);

            if linkoffset - prevlinkoffset == Assembler::K_START_OF_LABEL_LINK_CHAIN {
                label.unuse();
            } else {
                label.link_to(prevlinkoffset, Distance::Far);
            }
        }
        label.bind_to(self.pc_offset());

        debug_assert!(label.is_bound());
        debug_assert!(!label.is_linked());
    }

    fn link_and_get_byte_offset_to(&mut self, label: &mut Label) -> i32 {
        let offset;
        if label.is_bound() {
            offset = label.pos() - self.pc_offset();
            debug_assert!(offset <= 0);
        } else {
            if label.is_linked() {
                offset = label.pos() - self.pc_offset();
                debug_assert_ne!(offset, Assembler::K_START_OF_LABEL_LINK_CHAIN);
            } else {
                offset = Assembler::K_START_OF_LABEL_LINK_CHAIN;
            }
            label.link_to(self.pc_offset(), Distance::Far);
        }
        offset
    }

    fn link_and_get_branch_instruction_offset_to(&mut self, label: &mut Label) -> i32 {
        debug_assert_eq!(Assembler::K_START_OF_LABEL_LINK_CHAIN, 0);
        let offset = self.link_and_get_byte_offset_to(label);
        debug_assert_eq!(offset % K_INSTR_SIZE as i32, 0);
        offset >> K_INSTR_SIZE_LOG2
    }

    fn rd(rd: impl Into<CPURegister>) -> Instr {
        let rd = rd.into();
        debug_assert_ne!(rd.code(), K_SP_REG_INTERNAL_CODE);
        (rd.code() as Instr) << RD_OFFSET
    }

    fn rn(rn: impl Into<CPURegister>) -> Instr {
        let rn = rn.into();
        debug_assert_ne!(rn.code(), K_SP_REG_INTERNAL_CODE);
        (rn.code() as Instr) << RN_OFFSET
    }

    fn rm(rm: impl Into<CPURegister>) -> Instr {
        let rm = rm.into();
        debug_assert_ne!(rm.code(), K_SP_REG_INTERNAL_CODE);
        (rm.code() as Instr) << RM_OFFSET
    }

    fn rt(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        debug_assert_ne!(rt.code(), K_SP_REG_INTERNAL_CODE);
        (rt.code() as Instr) << RT_OFFSET
    }

    fn rd_sp(rd: Register) -> Instr {
        debug_assert!(!rd.is_zero());
        ((rd.code() as Instr) & K_REG_CODE_MASK) << RD_OFFSET
    }

    fn rn_sp(rn: Register) -> Instr {
        debug_assert!(!rn.is_zero());
        ((rn.code() as Instr) & K_REG_CODE_MASK) << RN_OFFSET
    }

    fn flags(s: FlagsUpdate) -> Instr {
        match s {
            FlagsUpdate::SetFlags => 1 << FLAGS_UPDATE_OFFSET,
            FlagsUpdate::LeaveFlags => 0 << FLAGS_UPDATE_OFFSET,
        }
    }

    fn imm_uncond_branch(imm26: i32) -> Instr {
        (checked_truncate_to_int26(imm26 as i64) as Instr) << IMM_UNCOND_BRANCH_OFFSET
    }

    fn imm_cond_branch(imm19: i32) -> Instr {
        (checked_truncate_to_int19(imm19 as i64) as Instr) << IMM_COND_BRANCH_OFFSET
    }

    fn imm_cmp_branch(imm19: i32) -> Instr {
        (checked_truncate_to_int19(imm19 as i64) as Instr) << IMM_CMP_BRANCH_OFFSET
    }

    fn sf(rd: Register) -> Instr {
        if rd.is_64_bits() {
            SIXTY_FOUR_BITS
        } else {
            THIRTY_TWO_BITS
        }
    }

    fn imm_add_sub(imm: i32) -> Instr {
        debug_assert!(Assembler::is_imm_add_sub(imm as i64));
        let mut imm = imm;
        if is_uint12(imm as i64) {
            imm <<= IMM_ADD_SUB_OFFSET;
        } else {
            imm = ((imm >> 12) << IMM_ADD_SUB_OFFSET) | (1 << SHIFT_ADD_SUB_OFFSET);
        }
        imm as Instr
    }

    fn shift_dp(shift: Shift) -> Instr {
        debug_assert!(
            shift == Shift::Lsl
                || shift == Shift::Lsr
                || shift == Shift::Asr
                || shift == Shift::Ror
        );
        (shift as Instr) << SHIFT_DP_OFFSET
    }

    fn imm_dp_shift(amount: u32) -> Instr {
        debug_assert!(is_uint6(amount as i64));
        amount << IMM_DP_SHIFT_OFFSET
    }

    fn imm_ls_unsigned(imm12: i32) -> Instr {
        debug_assert!(is_uint12(imm12 as i64));
        (imm12 as Instr) << IMM_LS_UNSIGNED_OFFSET
    }

    fn imm_ls(imm9: i32) -> Instr {
        (checked_truncate_to_int9(imm9 as i64) as Instr) << IMM_LS_OFFSET
    }

    fn imm_exception(imm16: i32) -> Instr {
        debug_assert!(is_uint16(imm16 as i64));
        (imm16 as Instr) << IMM_EXCEPTION_OFFSET
    }

    fn imm_hint(imm7: i32) -> Instr {
        debug_assert!(is_uint7(imm7 as i64));
        (imm7 as Instr) << IMM_HINT_OFFSET
    }

    fn imm_move_wide(imm: i32) -> Instr {
        debug_assert!(is_uint16(imm as i64));
        (imm as Instr) << IMM_MOVE_WIDE_OFFSET
    }

    fn shift_move_wide(shift: i32) -> Instr {
        debug_assert!(is_uint2(shift as i64));
        (shift as Instr) << SHIFT_MOVE_WIDE_OFFSET
    }

    pub fn is_imm_add_sub(immediate: i64) -> bool {
        is_uint12(immediate)
            || (is_uint12(immediate >> 12) && ((immediate & 0xFFF) == 0))
    }

    pub fn is_imm_ls_unscaled(offset: i64) -> bool {
        is_int9(offset)
    }

    pub fn is_imm_ls_scaled(offset: i64, size_log2: u32) -> bool {
        let offset_is_size_multiple = ((offset >> size_log2) << size_log2) == offset;
        offset_is_size_multiple && is_uint12(offset >> size_log2)
    }

    pub fn appropriate_zero_reg_for(reg: impl Into<CPURegister>) -> Register {
        let reg = reg.into();
        if reg.is_64_bits() {
            XZR
        } else {
            WZR
        }
    }

    fn load_op_for(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        debug_assert!(rt.is_valid());
        if rt.is_register() {
            return if rt.is_64_bits() { LDR_X } else { LDR_W };
        }
        debug_assert!(rt.is_v_register());
        match rt.size_in_bits() as i32 {
            K_B_REG_SIZE_IN_BITS => LDR_B,
            K_H_REG_SIZE_IN_BITS => LDR_H,
            K_S_REG_SIZE_IN_BITS => LDR_S,
            K_D_REG_SIZE_IN_BITS => LDR_D,
            _ => {
                debug_assert!(rt.is_q());
                LDR_Q
            }
        }
    }

    fn store_op_for(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        debug_assert!(rt.is_valid());
        if rt.is_register() {
            return if rt.is_64_bits() { STR_X } else { STR_W };
        }
        debug_assert!(rt.is_v_register());
        match rt.size_in_bits() as i32 {
            K_B_REG_SIZE_IN_BITS => STR_B,
            K_H_REG_SIZE_IN_BITS => STR_H,
            K_S_REG_SIZE_IN_BITS => STR_S,
            K_D_REG_SIZE_IN_BITS => STR_D,
            _ => {
                debug_assert!(rt.is_q());
                STR_Q
            }
        }
    }

    fn calc_ls_data_size_log2(op: Instr) -> u32 {
        let mut size_log2 = op >> LS_SIZE_OFFSET;
        if (op & LS_VECTOR_MASK) != 0 && size_log2 == 0 && ((op & LS_OPC_MASK) >> LS_OPC_OFFSET) >= 2
        {
            size_log2 = K_Q_REG_SIZE_LOG2 as u32;
        }
        size_log2
    }

    pub fn b(&mut self, imm26: i32) {
        let instruction = B | Assembler::imm_uncond_branch(imm26);
        self.emit(instruction);
    }

    pub fn b_label(&mut self, label: &mut Label) {
        let offset = self.link_and_get_branch_instruction_offset_to(label);
        self.b(offset);
    }

    pub fn b_cond(&mut self, imm19: i32, cond: Condition) {
        let instruction = B_COND | Assembler::imm_cond_branch(imm19) | (cond as Instr);
        self.emit(instruction);
    }

    pub fn b_cond_label(&mut self, label: &mut Label, cond: Condition) {
        let offset = self.link_and_get_branch_instruction_offset_to(label);
        self.b_cond(offset, cond);
    }

    pub fn bl(&mut self, imm26: i32) {
        let instruction = BL | Assembler::imm_uncond_branch(imm26);
        self.emit(instruction);
    }

    pub fn bl_label(&mut self, label: &mut Label) {
        let offset = self.link_and_get_branch_instruction_offset_to(label);
        self.bl(offset);
    }

    pub fn cbz(&mut self, rt: Register, imm19: i32) {
        let instruction =
            Assembler::sf(rt) | CBZ | Assembler::imm_cmp_branch(imm19) | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn cbz_label(&mut self, rt: Register, label: &mut Label) {
        let offset = self.link_and_get_branch_instruction_offset_to(label);
        self.cbz(rt, offset);
    }

    pub fn cbnz(&mut self, rt: Register, imm19: i32) {
        let instruction =
            Assembler::sf(rt) | CBNZ | Assembler::imm_cmp_branch(imm19) | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn cbnz_label(&mut self, rt: Register, label: &mut Label) {
        let offset = self.link_and_get_branch_instruction_offset_to(label);
        self.cbnz(rt, offset);
    }

    pub fn br(&mut self, xn: Register) {
        debug_assert!(xn.is_64_bits());
        let instruction = BR | Assembler::rn(xn);
        self.emit(instruction);
    }

    pub fn blr(&mut self, xn: Register) {
        debug_assert!(xn.is_64_bits());
        debug_assert_ne!(xn, XZR);
        let instruction = BLR | Assembler::rn(xn);
        self.emit(instruction);
    }

    pub fn ret(&mut self, xn: Register) {
        debug_assert!(xn.is_64_bits());
        let instruction = RET | Assembler::rn(xn);
        self.emit(instruction);
    }

    pub fn adds<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub(rd, rn, operand.into(), FlagsUpdate::SetFlags, ADD);
    }

    pub fn cmn<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.adds(zr, rn, operand);
    }

    pub fn subs<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub(rd, rn, operand.into(), FlagsUpdate::SetFlags, SUB);
    }

    pub fn cmp<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.subs(zr, rn, operand);
    }

    pub fn neg<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.sub(rd, zr, operand);
    }

    pub fn mov(&mut self, rd: Register, rm: Register) {
        if rd.is_sp() || rm.is_sp() {
            self.add(rd, rm, 0);
        } else {
            let zr = Assembler::appropriate_zero_reg_for(rd);
            self.orr(rd, zr, rm);
        }
    }

    pub fn movz(&mut self, rd: Register, imm: u64, shift: i32) {
        self.move_wide(rd, imm, shift, MOVZ);
    }

    pub fn movn(&mut self, rd: Register, imm: u64, shift: i32) {
        self.move_wide(rd, imm, shift, MOVN);
    }

    pub fn movk(&mut self, rd: Register, imm: u64, shift: i32) {
        self.move_wide(rd, imm, shift, MOVK);
    }

    pub fn ldr(&mut self, rt: impl Into<CPURegister> + Copy, src: MemOperand) {
        self.load_store(rt, src, Assembler::load_op_for(rt));
    }

    pub fn str(&mut self, rt: impl Into<CPURegister> + Copy, src: MemOperand) {
        self.load_store(rt, src, Assembler::store_op_for(rt));
    }

    pub fn ldrb(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, LDRB_W);
    }

    pub fn strb(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, STRB_W);
    }

    pub fn ldrh(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, LDRH_W);
    }

    pub fn strh(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, STRH_W);
    }

    pub fn ldrsb(&mut self, rt: Register, src: MemOperand) {
        let op = if rt.is_64_bits() { LDRSB_X } else { LDRSB_W };
        self.load_store(rt, src, op);
    }

    pub fn ldrsh(&mut self, rt: Register, src: MemOperand) {
        let op = if rt.is_64_bits() { LDRSH_X } else { LDRSH_W };
        self.load_store(rt, src, op);
    }

    pub fn ldrsw(&mut self, rt: Register, src: MemOperand) {
        debug_assert!(rt.is_64_bits());
        self.load_store(rt, src, LDRSW_X);
    }

    pub fn brk(&mut self, code: i32) {
        debug_assert!(is_uint16(code as i64));
        let instruction = BRK | Assembler::imm_exception(code);
        self.emit(instruction);
    }

    pub fn hlt(&mut self, code: i32) {
        debug_assert!(is_uint16(code as i64));
        let instruction = HLT | Assembler::imm_exception(code);
        self.emit(instruction);
    }

    pub fn hint(&mut self, code: SystemHint) {
        let instruction = HINT | Assembler::imm_hint(code as i32) | Assembler::rt(XZR);
        self.emit(instruction);
    }

    pub fn nop(&mut self) {
        self.hint(SystemHint::Nop);
    }

    fn ra(ra: impl Into<CPURegister>) -> Instr {
        let ra = ra.into();
        debug_assert_ne!(ra.code(), K_SP_REG_INTERNAL_CODE);
        (ra.code() as Instr) << RA_OFFSET
    }

    fn rt2(rt2: impl Into<CPURegister>) -> Instr {
        let rt2 = rt2.into();
        debug_assert_ne!(rt2.code(), K_SP_REG_INTERNAL_CODE);
        (rt2.code() as Instr) << RT2_OFFSET
    }

    fn cond(cond: Condition) -> Instr {
        (cond as Instr) << CONDITION_OFFSET
    }

    fn nzcv(nzcv: u32) -> Instr {
        ((nzcv >> FLAGS_OFFSET) & 0xf) << NZCV_OFFSET
    }

    fn imm_cond_cmp(imm: u32) -> Instr {
        debug_assert!(is_uint5(imm as i64));
        imm << IMM_COND_CMP_OFFSET
    }

    fn bit_n(bitn: u32, reg_size: u32) -> Instr {
        debug_assert!(
            (reg_size == K_W_REG_SIZE_IN_BITS as u32)
                || (reg_size == K_X_REG_SIZE_IN_BITS as u32)
        );
        debug_assert!((reg_size == K_X_REG_SIZE_IN_BITS as u32) || (bitn == 0));
        bitn << BIT_N_OFFSET
    }

    fn imm_rotate(immr: u32, reg_size: u32) -> Instr {
        debug_assert!(
            ((reg_size == K_X_REG_SIZE_IN_BITS as u32) && is_uint6(immr as i64))
                || ((reg_size == K_W_REG_SIZE_IN_BITS as u32) && is_uint5(immr as i64))
        );
        immr << IMM_ROTATE_OFFSET
    }

    fn imm_set_bits(imms: u32, reg_size: u32) -> Instr {
        debug_assert!(
            (reg_size == K_W_REG_SIZE_IN_BITS as u32)
                || (reg_size == K_X_REG_SIZE_IN_BITS as u32)
        );
        debug_assert!(is_uint6(imms as i64));
        debug_assert!(
            (reg_size == K_X_REG_SIZE_IN_BITS as u32) || is_uint6(imms as i64 + 3)
        );
        imms << IMM_SET_BITS_OFFSET
    }

    fn imm_r(immr: u32, reg_size: u32) -> Instr {
        debug_assert!(
            ((reg_size == K_X_REG_SIZE_IN_BITS as u32) && is_uint6(immr as i64))
                || ((reg_size == K_W_REG_SIZE_IN_BITS as u32) && is_uint5(immr as i64))
        );
        debug_assert!(is_uint6(immr as i64));
        immr << IMM_R_OFFSET
    }

    fn imm_s(imms: u32, reg_size: u32) -> Instr {
        debug_assert!(
            ((reg_size == K_X_REG_SIZE_IN_BITS as u32) && is_uint6(imms as i64))
                || ((reg_size == K_W_REG_SIZE_IN_BITS as u32) && is_uint5(imms as i64))
        );
        imms << IMM_S_OFFSET
    }

    fn imm_test_branch(imm14: i32) -> Instr {
        (checked_truncate_to_int14(imm14 as i64) as Instr) << IMM_TEST_BRANCH_OFFSET
    }

    fn imm_test_branch_bit(bit_pos: u32) -> Instr {
        debug_assert!(is_uint6(bit_pos as i64));
        let b5 = bit_pos << (IMM_TEST_BRANCH_BIT5_OFFSET - 5);
        let b40 = bit_pos << IMM_TEST_BRANCH_BIT40_OFFSET;
        let b5 = b5 & IMM_TEST_BRANCH_BIT5_MASK;
        let b40 = b40 & IMM_TEST_BRANCH_BIT40_MASK;
        b5 | b40
    }

    fn imm_ls_pair(imm7: i32, size: u32) -> Instr {
        debug_assert_eq!(imm7, (imm7 >> size) << size);
        let scaled_imm7 = imm7 >> size;
        (checked_truncate_to_int7(scaled_imm7 as i64) as Instr) << IMM_LS_PAIR_OFFSET
    }

    pub fn is_imm_ls_pair(offset: i64, size: u32) -> bool {
        let offset_is_size_multiple = ((offset >> size) << size) == offset;
        offset_is_size_multiple && is_int7(offset >> size)
    }

    pub fn is_imm_conditional_compare(immediate: i64) -> bool {
        is_uint5(immediate)
    }

    fn calc_ls_pair_data_size(op: Instr) -> u32 {
        match op {
            STP_Q | LDP_Q => K_Q_REG_SIZE_LOG2 as u32,
            STP_X | LDP_X | STP_D | LDP_D => K_X_REG_SIZE_LOG2 as u32,
            _ => K_W_REG_SIZE_LOG2 as u32,
        }
    }

    fn load_pair_op_for(rt: impl Into<CPURegister>, rt2: impl Into<CPURegister>) -> Instr {
        debug_assert_eq!(STP_W | LOAD_STORE_PAIR_L_BIT, LDP_W);
        Assembler::store_pair_op_for(rt, rt2) | LOAD_STORE_PAIR_L_BIT
    }

    fn store_pair_op_for(rt: impl Into<CPURegister>, _rt2: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        if rt.is_register() {
            return if rt.is_64_bits() { STP_X } else { STP_W };
        }
        debug_assert!(rt.is_v_register());
        match rt.size_in_bits() as i32 {
            K_S_REG_SIZE_IN_BITS => STP_S,
            K_D_REG_SIZE_IN_BITS => STP_D,
            _ => {
                debug_assert!(rt.is_q());
                STP_Q
            }
        }
    }

    pub fn is_imm_logical(
        value: u64,
        width: u32,
        n: &mut u32,
        imm_s: &mut u32,
        imm_r: &mut u32,
    ) -> bool {
        debug_assert!(
            (width == K_W_REG_SIZE_IN_BITS as u32) || (width == K_X_REG_SIZE_IN_BITS as u32)
        );

        let mut negate = false;
        let mut value = value;

        if value & 1 != 0 {
            negate = true;
            value = !value;
        }

        if width == K_W_REG_SIZE_IN_BITS as u32 {
            value <<= K_W_REG_SIZE_IN_BITS;
            value |= value >> K_W_REG_SIZE_IN_BITS;
        }

        let a = largest_power_of_2_divisor(value);
        let value_plus_a = value.wrapping_add(a);
        let b = largest_power_of_2_divisor(value_plus_a);
        let value_plus_a_minus_b = value_plus_a.wrapping_sub(b);
        let c = largest_power_of_2_divisor(value_plus_a_minus_b);

        let d;
        let clz_a;
        let out_n;
        let mask;

        if c != 0 {
            clz_a = count_leading_zeros(a, K_X_REG_SIZE_IN_BITS);
            let clz_c = count_leading_zeros(c, K_X_REG_SIZE_IN_BITS);
            d = clz_a - clz_c;
            mask = (1u64 << d) - 1;
            out_n = 0;
        } else {
            if a == 0 {
                return false;
            }
            clz_a = count_leading_zeros(a, K_X_REG_SIZE_IN_BITS);
            d = 64;
            mask = !0u64;
            out_n = 1;
        }

        if !crate::base::bits::is_power_of_two(d as u32) {
            return false;
        }

        if (b.wrapping_sub(a) & !mask) != 0 {
            return false;
        }

        const MULTIPLIERS: [u64; 6] = [
            0x0000000000000001,
            0x0000000100000001,
            0x0001000100010001,
            0x0101010101010101,
            0x1111111111111111,
            0x5555555555555555,
        ];
        let multiplier_idx = count_leading_zeros(d as u64, K_X_REG_SIZE_IN_BITS) - 57;
        debug_assert!(
            (multiplier_idx >= 0) && ((multiplier_idx as usize) < MULTIPLIERS.len())
        );
        let multiplier = MULTIPLIERS[multiplier_idx as usize];
        let candidate = b.wrapping_sub(a).wrapping_mul(multiplier);

        if value != candidate {
            return false;
        }

        let clz_b = if b == 0 {
            -1
        } else {
            count_leading_zeros(b, K_X_REG_SIZE_IN_BITS)
        };
        let mut s = clz_a - clz_b;

        let r;
        if negate {
            s = d - s;
            r = (clz_b + 1) & (d - 1);
        } else {
            r = (clz_a + 1) & (d - 1);
        }

        *n = out_n;
        *imm_s = (((-d * 2) | (s - 1)) & 0x3F) as u32;
        *imm_r = r as u32;

        true
    }

    fn logical_immediate(
        &mut self,
        rd: Register,
        rn: Register,
        n: u32,
        imm_s: u32,
        imm_r: u32,
        op: Instr,
    ) {
        let reg_size = rd.size_in_bits();
        let dest_reg = if op == ANDS {
            Assembler::rd(rd)
        } else {
            Assembler::rd_sp(rd)
        };
        let instruction = Assembler::sf(rd)
            | LOGICAL_IMMEDIATE_FIXED
            | op
            | Assembler::bit_n(n, reg_size)
            | Assembler::imm_set_bits(imm_s, reg_size)
            | Assembler::imm_rotate(imm_r, reg_size)
            | dest_reg
            | Assembler::rn(rn);
        self.emit(instruction);
    }

    pub fn ands<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), ANDS);
    }

    pub fn bics<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), BICS);
    }

    pub fn eon<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), EON);
    }

    pub fn tst<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.ands(zr, rn, operand);
    }

    pub fn mvn<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.orn(rd, zr, operand);
    }

    pub fn negs<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.subs(rd, zr, operand);
    }

    pub fn adc<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, ADC);
    }

    pub fn adcs<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::SetFlags, ADC);
    }

    pub fn sbc<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, SBC);
    }

    pub fn sbcs<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::SetFlags, SBC);
    }

    pub fn ngc<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.sbc(rd, zr, operand);
    }

    pub fn ngcs<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.sbcs(rd, zr, operand);
    }

    fn add_sub_with_carry(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), operand.reg().size_in_bits());
        debug_assert!(operand.is_shifted_register() && (operand.shift_amount() == 0));
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::flags(s)
            | Assembler::rm(operand.reg())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn bfm(&mut self, rd: Register, rn: Register, immr: u32, imms: u32) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        let n = Assembler::sf(rd) >> (K_SF_OFFSET - K_BITFIELD_N_OFFSET);
        let instruction = Assembler::sf(rd)
            | BFM
            | n
            | Assembler::imm_r(immr, rd.size_in_bits())
            | Assembler::imm_s(imms, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn sbfm(&mut self, rd: Register, rn: Register, immr: u32, imms: u32) {
        debug_assert!(rd.is_64_bits() || rn.is_32_bits());
        let n = Assembler::sf(rd) >> (K_SF_OFFSET - K_BITFIELD_N_OFFSET);
        let instruction = Assembler::sf(rd)
            | SBFM
            | n
            | Assembler::imm_r(immr, rd.size_in_bits())
            | Assembler::imm_s(imms, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn ubfm(&mut self, rd: Register, rn: Register, immr: u32, imms: u32) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        let n = Assembler::sf(rd) >> (K_SF_OFFSET - K_BITFIELD_N_OFFSET);
        let instruction = Assembler::sf(rd)
            | UBFM
            | n
            | Assembler::imm_r(immr, rd.size_in_bits())
            | Assembler::imm_s(imms, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn extr(&mut self, rd: Register, rn: Register, rm: Register, lsb: u32) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let n = Assembler::sf(rd) >> (K_SF_OFFSET - K_BITFIELD_N_OFFSET);
        let instruction = Assembler::sf(rd)
            | EXTR
            | n
            | Assembler::rm(rm)
            | Assembler::imm_s(lsb, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn lsl(&mut self, rd: Register, rn: Register, shift: u32) {
        let reg_size = rd.size_in_bits();
        debug_assert!(shift < reg_size);
        self.ubfm(rd, rn, (reg_size - shift) % reg_size, reg_size - shift - 1);
    }

    pub fn lsr(&mut self, rd: Register, rn: Register, shift: u32) {
        debug_assert!(shift < rd.size_in_bits());
        self.ubfm(rd, rn, shift, rd.size_in_bits() - 1);
    }

    pub fn asr(&mut self, rd: Register, rn: Register, shift: u32) {
        debug_assert!(shift < rd.size_in_bits());
        self.sbfm(rd, rn, shift, rd.size_in_bits() - 1);
    }

    pub fn ror(&mut self, rd: Register, rs: Register, shift: u32) {
        self.extr(rd, rs, rs, shift);
    }

    pub fn sbfx(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.sbfm(rd, rn, lsb, lsb + width - 1);
    }

    pub fn ubfx(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.ubfm(rd, rn, lsb, lsb + width - 1);
    }

    pub fn sbfiz(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.sbfm(
            rd,
            rn,
            (rd.size_in_bits() - lsb) & (rd.size_in_bits() - 1),
            width - 1,
        );
    }

    pub fn ubfiz(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.ubfm(
            rd,
            rn,
            (rd.size_in_bits() - lsb) & (rd.size_in_bits() - 1),
            width - 1,
        );
    }

    pub fn bfi(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.bfm(
            rd,
            rn,
            (rd.size_in_bits() - lsb) & (rd.size_in_bits() - 1),
            width - 1,
        );
    }

    pub fn bfxil(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.bfm(rd, rn, lsb, lsb + width - 1);
    }

    pub fn sxtb(&mut self, rd: Register, rn: Register) {
        self.sbfm(rd, rn, 0, 7);
    }

    pub fn sxth(&mut self, rd: Register, rn: Register) {
        self.sbfm(rd, rn, 0, 15);
    }

    pub fn sxtw(&mut self, rd: Register, rn: Register) {
        self.sbfm(rd, rn, 0, 31);
    }

    pub fn uxtb(&mut self, rd: Register, rn: Register) {
        self.ubfm(rd, rn, 0, 7);
    }

    pub fn uxth(&mut self, rd: Register, rn: Register) {
        self.ubfm(rd, rn, 0, 15);
    }

    pub fn uxtw(&mut self, rd: Register, rn: Register) {
        self.ubfm(rd, rn, 0, 31);
    }

    fn conditional_select(
        &mut self,
        rd: Register,
        rn: Register,
        rm: Register,
        cond: Condition,
        op: Instr,
    ) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::rm(rm)
            | Assembler::cond(cond)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn csel(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSEL);
    }

    pub fn csinc(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSINC);
    }

    pub fn csinv(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSINV);
    }

    pub fn csneg(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSNEG);
    }

    pub fn cset(&mut self, rd: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.csinc(rd, zr, zr, negate_condition(cond));
    }

    pub fn csetm(&mut self, rd: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.csinv(rd, zr, zr, negate_condition(cond));
    }

    pub fn cinc(&mut self, rd: Register, rn: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.csinc(rd, rn, rn, negate_condition(cond));
    }

    pub fn cinv(&mut self, rd: Register, rn: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.csinv(rd, rn, rn, negate_condition(cond));
    }

    pub fn cneg(&mut self, rd: Register, rn: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.csneg(rd, rn, rn, negate_condition(cond));
    }

    fn conditional_compare(
        &mut self,
        rn: Register,
        operand: Operand,
        nzcv: u32,
        cond: Condition,
        op: Instr,
    ) {
        let ccmpop;
        if operand.is_immediate() {
            let immediate = operand.immediate_value();
            debug_assert!(Assembler::is_imm_conditional_compare(immediate));
            ccmpop = CONDITIONAL_COMPARE_IMMEDIATE_FIXED
                | op
                | Assembler::imm_cond_cmp(immediate as u32);
        } else {
            debug_assert!(operand.is_shifted_register() && (operand.shift_amount() == 0));
            ccmpop = CONDITIONAL_COMPARE_REGISTER_FIXED | op | Assembler::rm(operand.reg());
        }
        let instruction = Assembler::sf(rn)
            | ccmpop
            | Assembler::cond(cond)
            | Assembler::rn(rn)
            | Assembler::nzcv(nzcv);
        self.emit(instruction);
    }

    pub fn ccmn<T: Into<Operand>>(
        &mut self,
        rn: Register,
        operand: T,
        nzcv: u32,
        cond: Condition,
    ) {
        self.conditional_compare(rn, operand.into(), nzcv, cond, CCMN);
    }

    pub fn ccmp<T: Into<Operand>>(
        &mut self,
        rn: Register,
        operand: T,
        nzcv: u32,
        cond: Condition,
    ) {
        self.conditional_compare(rn, operand.into(), nzcv, cond, CCMP);
    }

    fn data_processing_1_source(&mut self, rd: Register, rn: Register, op: Instr) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        let instruction = Assembler::sf(rn) | op | Assembler::rn(rn) | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn rbit(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, RBIT);
    }

    pub fn rev16(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, REV16);
    }

    pub fn rev32(&mut self, rd: Register, rn: Register) {
        debug_assert!(rd.is_64_bits());
        self.data_processing_1_source(rd, rn, REV);
    }

    pub fn rev(&mut self, rd: Register, rn: Register) {
        let op = if rd.is_64_bits() { REV_X } else { REV_W };
        self.data_processing_1_source(rd, rn, op);
    }

    pub fn clz(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, CLZ);
    }

    pub fn cls(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, CLS);
    }

    pub fn lslv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | LSLV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn lsrv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | LSRV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn asrv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | ASRV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn rorv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | RORV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn sdiv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | SDIV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn udiv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | UDIV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    fn data_processing_3_source(
        &mut self,
        rd: Register,
        rn: Register,
        rm: Register,
        ra: Register,
        op: Instr,
    ) {
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::rm(rm)
            | Assembler::ra(ra)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn madd(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        self.data_processing_3_source(rd, rn, rm, ra, MADD);
    }

    pub fn msub(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        self.data_processing_3_source(rd, rn, rm, ra, MSUB);
    }

    pub fn mneg(&mut self, rd: Register, rn: Register, rm: Register) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.data_processing_3_source(rd, rn, rm, zr, MSUB);
    }

    pub fn smaddl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, SMADDL_X);
    }

    pub fn smsubl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, SMSUBL_X);
    }

    pub fn umaddl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, UMADDL_X);
    }

    pub fn umsubl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, UMSUBL_X);
    }

    pub fn smulh(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rm.is_64_bits());
        self.data_processing_3_source(rd, rn, rm, XZR, SMULH_X);
    }

    pub fn umulh(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rm.is_64_bits());
        self.data_processing_3_source(rd, rn, rm, XZR, UMULH_X);
    }

    pub fn tbz(&mut self, rt: Register, bit_pos: u32, imm14: i32) {
        debug_assert!(rt.is_64_bits() || (rt.is_32_bits() && (bit_pos < K_W_REG_SIZE_IN_BITS as u32)));
        let instruction = TBZ
            | Assembler::imm_test_branch_bit(bit_pos)
            | Assembler::imm_test_branch(imm14)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn tbz_label(&mut self, rt: Register, bit_pos: u32, label: &mut Label) {
        let offset = self.link_and_get_branch_instruction_offset_to(label);
        self.tbz(rt, bit_pos, offset);
    }

    pub fn tbnz(&mut self, rt: Register, bit_pos: u32, imm14: i32) {
        debug_assert!(rt.is_64_bits() || (rt.is_32_bits() && (bit_pos < K_W_REG_SIZE_IN_BITS as u32)));
        let instruction = TBNZ
            | Assembler::imm_test_branch_bit(bit_pos)
            | Assembler::imm_test_branch(imm14)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn tbnz_label(&mut self, rt: Register, bit_pos: u32, label: &mut Label) {
        let offset = self.link_and_get_branch_instruction_offset_to(label);
        self.tbnz(rt, bit_pos, offset);
    }

    pub fn adr(&mut self, rd: Register, imm21: i32) {
        debug_assert!(rd.is_64_bits());
        let instruction = ADR | crate::codegen::arm64::instructions_arm64::imm_pc_rel_address(imm21) | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn adr_label(&mut self, rd: Register, label: &mut Label) {
        let offset = self.link_and_get_byte_offset_to(label);
        self.adr(rd, offset);
    }

    pub fn ldp(&mut self, rt: impl Into<CPURegister> + Copy, rt2: impl Into<CPURegister> + Copy, src: MemOperand) {
        let op = Assembler::load_pair_op_for(rt, rt2);
        self.load_store_pair(rt, rt2, src, op);
    }

    pub fn stp(&mut self, rt: impl Into<CPURegister> + Copy, rt2: impl Into<CPURegister> + Copy, dst: MemOperand) {
        let op = Assembler::store_pair_op_for(rt, rt2);
        self.load_store_pair(rt, rt2, dst, op);
    }

    pub fn ldpsw(&mut self, rt: Register, rt2: Register, src: MemOperand) {
        debug_assert!(rt.is_64_bits());
        self.load_store_pair(rt, rt2, src, LDPSW_X);
    }

    fn load_store_pair(
        &mut self,
        rt: impl Into<CPURegister>,
        rt2: impl Into<CPURegister>,
        addr: MemOperand,
        op: Instr,
    ) {
        let rt = rt.into();
        let rt2 = rt2.into();
        debug_assert!(((op & LOAD_STORE_PAIR_L_BIT) == 0) || rt != rt2);
        debug_assert_eq!(rt.size_in_bits(), rt2.size_in_bits());
        debug_assert!(Assembler::is_imm_ls_pair(
            addr.offset(),
            Assembler::calc_ls_pair_data_size(op)
        ));
        let offset = addr.offset() as i32;

        let memop = op
            | Assembler::rt(rt)
            | Assembler::rt2(rt2)
            | Assembler::rn_sp(addr.base())
            | Assembler::imm_ls_pair(offset, Assembler::calc_ls_pair_data_size(op));

        let addrmodeop;
        if addr.is_immediate_offset() {
            addrmodeop = LOAD_STORE_PAIR_OFFSET_FIXED;
        } else {
            debug_assert_ne!(rt, addr.base().into());
            debug_assert_ne!(rt2, addr.base().into());
            debug_assert_ne!(addr.offset(), 0);
            if addr.is_pre_index() {
                addrmodeop = LOAD_STORE_PAIR_PRE_INDEX_FIXED;
            } else {
                debug_assert!(addr.is_post_index());
                addrmodeop = LOAD_STORE_PAIR_POST_INDEX_FIXED;
            }
        }
        self.emit(addrmodeop | memop);
    }

    fn fp_type(fd: VRegister) -> Instr {
        if fd.is_64_bits() {
            FP64
        } else {
            FP32
        }
    }

    fn fp_scale(scale: u32) -> Instr {
        debug_assert!(is_uint6(scale as i64));
        scale << FP_SCALE_OFFSET
    }

    fn imm_fp(imm: f64) -> Instr {
        Assembler::fp_to_imm8(imm) << IMM_FP_OFFSET
    }

    pub fn fp_to_imm8(imm: f64) -> u32 {
        let bits = imm.to_bits();
        debug_assert!(Assembler::is_imm_fp64(bits));
        let bit7 = ((bits >> 63) & 0x1) << 7;
        let bit6 = ((bits >> 61) & 0x1) << 6;
        let bit5_to_0 = (bits >> 48) & 0x3F;
        (bit7 | bit6 | bit5_to_0) as u32
    }

    pub fn is_imm_fp64(bits: u64) -> bool {
        if (bits & 0xFFFFFFFFFFFF) != 0 {
            return false;
        }

        let b_pattern = ((bits >> 48) & 0x3FC0) as u32;
        if b_pattern != 0 && b_pattern != 0x3FC0 {
            return false;
        }

        if ((bits ^ (bits << 1)) & 0x4000000000000000) == 0 {
            return false;
        }

        true
    }

    fn fp_data_processing_1_source(&mut self, vd: VRegister, vn: VRegister, op: Instr) {
        let instruction =
            Assembler::fp_type(vn) | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn fp_data_processing_2_source(
        &mut self,
        fd: VRegister,
        fn_: VRegister,
        fm: VRegister,
        op: Instr,
    ) {
        debug_assert_eq!(fd.size_in_bits(), fn_.size_in_bits());
        debug_assert_eq!(fd.size_in_bits(), fm.size_in_bits());
        let instruction = Assembler::fp_type(fd)
            | op
            | Assembler::rm(fm)
            | Assembler::rn(fn_)
            | Assembler::rd(fd);
        self.emit(instruction);
    }

    fn fp_data_processing_3_source(
        &mut self,
        fd: VRegister,
        fn_: VRegister,
        fm: VRegister,
        fa: VRegister,
        op: Instr,
    ) {
        let instruction = Assembler::fp_type(fd)
            | op
            | Assembler::rm(fm)
            | Assembler::rn(fn_)
            | Assembler::ra(fa)
            | Assembler::rd(fd);
        self.emit(instruction);
    }

    pub fn fmov(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert_eq!(vd.size_in_bits(), vn.size_in_bits());
        let instruction =
            Assembler::fp_type(vd) | FMOV | Assembler::rd(vd) | Assembler::rn(vn);
        self.emit(instruction);
    }

    pub fn fmov_from_register(&mut self, vd: VRegister, rn: Register) {
        debug_assert_eq!(vd.size_in_bits(), rn.size_in_bits());
        let op = if vd.is_32_bits() { FMOV_SW } else { FMOV_DX };
        let instruction = op | Assembler::rd(vd) | Assembler::rn(rn);
        self.emit(instruction);
    }

    pub fn fmov_to_register(&mut self, rd: Register, fn_: VRegister) {
        debug_assert_eq!(rd.size_in_bits(), fn_.size_in_bits());
        let op = if rd.is_32_bits() { FMOV_WS } else { FMOV_XD };
        let instruction = op | Assembler::rd(rd) | Assembler::rn(fn_);
        self.emit(instruction);
    }

    pub fn fmov_immediate(&mut self, vd: VRegister, imm: f64) {
        debug_assert!(vd.is_scalar());
        let op = if vd.is_1d() { FMOV_D_IMM } else { FMOV_S_IMM };
        debug_assert!(vd.is_1d() || vd.is_1s());
        let instruction = op | Assembler::rd(vd) | Assembler::imm_fp(imm);
        self.emit(instruction);
    }

    pub fn fcmp(&mut self, fn_: VRegister, fm: VRegister) {
        debug_assert_eq!(fn_.size_in_bits(), fm.size_in_bits());
        let instruction =
            Assembler::fp_type(fn_) | FCMP | Assembler::rm(fm) | Assembler::rn(fn_);
        self.emit(instruction);
    }

    pub fn fcmp_zero(&mut self, fn_: VRegister, value: f64) {
        debug_assert_eq!(value, 0.0);
        let instruction = Assembler::fp_type(fn_) | FCMP_ZERO | Assembler::rn(fn_);
        self.emit(instruction);
    }

    pub fn fcvt(&mut self, vd: VRegister, vn: VRegister) {
        let op;
        if vd.is_1d() {
            debug_assert!(vn.is_1s() || vn.is_1h());
            op = if vn.is_1s() { FCVT_DS } else { FCVT_DH };
        } else if vd.is_1s() {
            debug_assert!(vn.is_1d() || vn.is_1h());
            op = if vn.is_1d() { FCVT_SD } else { FCVT_SH };
        } else {
            debug_assert!(vd.is_1h());
            debug_assert!(vn.is_1d() || vn.is_1s());
            op = if vn.is_1d() { FCVT_HD } else { FCVT_HS };
        }
        self.fp_data_processing_1_source(vd, vn, op);
    }

    pub fn scvtf(&mut self, vd: VRegister, rn: Register, fbits: u32) {
        let instruction = if fbits == 0 {
            Assembler::sf(rn)
                | Assembler::fp_type(vd)
                | SCVTF
                | Assembler::rn(rn)
                | Assembler::rd(vd)
        } else {
            Assembler::sf(rn)
                | Assembler::fp_type(vd)
                | SCVTF_FIXED
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(rn)
                | Assembler::rd(vd)
        };
        self.emit(instruction);
    }

    pub fn ucvtf(&mut self, fd: VRegister, rn: Register, fbits: u32) {
        let instruction = if fbits == 0 {
            Assembler::sf(rn)
                | Assembler::fp_type(fd)
                | UCVTF
                | Assembler::rn(rn)
                | Assembler::rd(fd)
        } else {
            Assembler::sf(rn)
                | Assembler::fp_type(fd)
                | UCVTF_FIXED
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(rn)
                | Assembler::rd(fd)
        };
        self.emit(instruction);
    }

    pub fn fcvtzs(&mut self, rd: Register, vn: VRegister, fbits: u32) {
        debug_assert!(vn.is_1s() || vn.is_1d());
        debug_assert!(fbits <= rd.size_in_bits());
        let instruction = if fbits == 0 {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZS
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        } else {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZS_FIXED
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        };
        self.emit(instruction);
    }

    pub fn fcvtzu(&mut self, rd: Register, vn: VRegister, fbits: u32) {
        debug_assert!(vn.is_1s() || vn.is_1d());
        debug_assert!(fbits <= rd.size_in_bits());
        let instruction = if fbits == 0 {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZU
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        } else {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZU_FIXED
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        };
        self.emit(instruction);
    }

    pub fn fjcvtzs(&mut self, rd: Register, vn: VRegister) {
        debug_assert!(rd.is_w());
        debug_assert!(vn.is_1d());
        let instruction = FJCVTZS | Assembler::rn(vn) | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn fnmul(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
        let op = if vd.is_1s() { FNMUL_S } else { FNMUL_D };
        let instruction = Assembler::fp_type(vd)
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn fmadd(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FMADD_S } else { FMADD_D };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn fmsub(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FMSUB_S } else { FMSUB_D };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn fnmadd(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FNMADD_S } else { FNMADD_D };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn fnmsub(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FNMSUB_S } else { FNMSUB_D };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn emit_instruction(&mut self, instruction: Instr) {
        self.emit(instruction);
    }

    pub fn rt_of(rt: impl Into<CPURegister>) -> Instr {
        Assembler::rt(rt)
    }

    pub fn imm_l_literal(imm19: i32) -> Instr {
        (checked_truncate_to_int19(imm19 as i64) as Instr) << IMM_L_LITERAL_OFFSET
    }

    pub fn is_imm_l_literal(offset: i64) -> bool {
        crate::utils::utils::is_int19(offset >> K_LOAD_LITERAL_SCALE_LOG2)
    }

    fn load_literal_op_for(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        if rt.is_register() {
            return if rt.is_64_bits() { LDR_X_LIT } else { LDR_W_LIT };
        }
        debug_assert!(rt.is_v_register());
        if rt.is_64_bits() {
            LDR_D_LIT
        } else {
            LDR_S_LIT
        }
    }

    pub fn ldr_pcrel(&mut self, rt: impl Into<CPURegister> + Copy, imm19: i32) {
        debug_assert!(!rt.into().is_zero());
        let instruction = Assembler::load_literal_op_for(rt)
            | Assembler::imm_l_literal(imm19)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldr_literal(&mut self, rt: impl Into<CPURegister> + Copy, imm: u64) {
        self.block_pools();
        if rt.into().is_32_bits() {
            self.constpool.record_entry_32(imm as u32, self.pc_offset());
        } else {
            self.constpool.record_entry_64(imm, self.pc_offset());
        }
        self.ldr_pcrel(rt, 0);
        self.release_pools();
    }

    pub fn emit_pool_guard(&mut self) {
        let instruction = BLR | Assembler::rn(XZR);
        self.emit(instruction);
    }

    pub fn align(&mut self, m: i32) {
        debug_assert_eq!(self.pc_offset() % K_INSTR_SIZE as i32, 0);
        debug_assert!(m >= K_INSTR_SIZE as i32 && crate::base::bits::is_power_of_two(m as u32));
        while (self.pc_offset() & (m - 1)) != 0 {
            self.nop();
        }
    }

    pub fn dc32(&mut self, data: u32) {
        self.base.emit_bytes(&data.to_le_bytes());
    }

    pub fn dc64(&mut self, data: u64) {
        self.base.emit_bytes(&data.to_le_bytes());
    }

    pub fn dd(&mut self, data: u32) {
        self.block_pools();
        self.dc32(data);
        self.release_pools();
    }

    pub fn dq(&mut self, data: u64) {
        self.block_pools();
        self.dc64(data);
        self.release_pools();
    }

    pub fn block_pools(&mut self) {
        self.constpool.start_block();
    }

    pub fn release_pools(&mut self) {
        self.constpool.end_block();
    }

    pub fn check_buffer(&mut self) {
        if self.constpool.is_blocked() {
            return;
        }
        let mut constpool = std::mem::take(&mut self.constpool);
        constpool.maybe_check(self);
        self.constpool = constpool;
    }

    pub fn emit_const_pool_with_jump_if_needed(&mut self, margin: usize) {
        use crate::codegen::arm64::constant_pool_arm64::{Emission, Jump};
        if self.constpool.is_empty() {
            return;
        }
        let mut constpool = std::mem::take(&mut self.constpool);
        constpool.check(self, Emission::IfNeeded, Jump::Required, margin);
        self.constpool = constpool;
    }

    pub fn force_constant_pool_emission(&mut self) {
        use crate::codegen::arm64::constant_pool_arm64::{Emission, Jump};
        let mut constpool = std::mem::take(&mut self.constpool);
        constpool.check(self, Emission::Forced, Jump::Required, 0);
        self.constpool = constpool;
    }

    pub fn constant_pool_is_empty(&self) -> bool {
        self.constpool.is_empty()
    }

    pub fn add_sub_for_macro(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        self.add_sub(rd, rn, operand, s, op);
    }

    pub fn logical_for_macro(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        op: Instr,
    ) {
        self.logical(rd, rn, operand, op);
    }

    pub fn logical_immediate_for_macro(
        &mut self,
        rd: Register,
        rn: Register,
        n: u32,
        imm_s: u32,
        imm_r: u32,
        op: Instr,
    ) {
        self.logical_immediate(rd, rn, n, imm_s, imm_r, op);
    }

    pub fn load_store_for_macro(&mut self, rt: impl Into<CPURegister>, addr: MemOperand, op: Instr) {
        self.load_store(rt, addr, op);
    }

    pub fn load_op_for_macro(rt: CPURegister) -> Instr {
        Assembler::load_op_for(rt)
    }

    pub fn store_op_for_macro(rt: CPURegister) -> Instr {
        Assembler::store_op_for(rt)
    }

    pub fn calc_ls_data_size_log2_for_macro(op: Instr) -> u32 {
        Assembler::calc_ls_data_size_log2(op)
    }

    fn rs(rs: impl Into<CPURegister>) -> Instr {
        let rs = rs.into();
        debug_assert_ne!(rs.code(), K_SP_REG_INTERNAL_CODE);
        (rs.code() as Instr) << RS_OFFSET
    }

    pub fn are_even(reg1: Register, reg2: Register) -> bool {
        reg1.is_even() && reg2.is_even()
    }

    pub fn are_consecutive(reg1: Register, reg2: Register) -> bool {
        (reg1.code() + 1) % K_NUMBER_OF_REGISTERS == reg2.code()
    }

    pub fn ldar(&mut self, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        let op = if rt.is_32_bits() { LDAR_W } else { LDAR_X };
        let instruction = op
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldaxr(&mut self, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        let op = if rt.is_32_bits() { LDAXR_W } else { LDAXR_X };
        let instruction = op
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlr(&mut self, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        let op = if rt.is_32_bits() { STLR_W } else { STLR_X };
        let instruction = op
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlxr(&mut self, rs: Register, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        debug_assert!(rs != rt && rs != rn);
        let op = if rt.is_32_bits() { STLXR_W } else { STLXR_X };
        let instruction = op
            | Assembler::rs(rs)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldarb(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAR_B
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldaxrb(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAXR_B
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlrb(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = STLR_B
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlxrb(&mut self, rs: Register, rt: Register, rn: Register) {
        debug_assert!(rs.is_32_bits());
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rs != rt && rs != rn);
        let instruction = STLXR_B
            | Assembler::rs(rs)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldarh(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAR_H
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldaxrh(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAXR_H
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlrh(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = STLR_H
            | Assembler::rs(X31)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlxrh(&mut self, rs: Register, rt: Register, rn: Register) {
        debug_assert!(rs.is_32_bits());
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rs != rt && rs != rn);
        let instruction = STLXR_H
            | Assembler::rs(rs)
            | Assembler::rt2(X31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    fn v_format(vd: VRegister) -> Instr {
        if vd.is_64_bits() {
            return match vd.lane_count() {
                1 => NEON_1D,
                2 => NEON_2S,
                4 => NEON_4H,
                8 => NEON_8B,
                _ => unreachable!(),
            };
        }
        debug_assert!(vd.is_128_bits());
        match vd.lane_count() {
            2 => NEON_2D,
            4 => NEON_4S,
            8 => NEON_8H,
            16 => NEON_16B,
            _ => unreachable!(),
        }
    }

    fn s_format(vd: VRegister) -> Instr {
        debug_assert!(vd.is_scalar());
        match vd.size_in_bytes() {
            1 => NEON_B,
            2 => NEON_H,
            4 => NEON_S,
            8 => NEON_D,
            _ => unreachable!(),
        }
    }

    fn neon_3_different_l(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vn, &[vm]));
        let format;
        let mut op = vop;
        if vd.is_scalar() {
            op |= NEON_Q | NEON_SCALAR;
            format = Assembler::s_format(vn);
        } else {
            format = Assembler::v_format(vn);
        }
        let instruction = format
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_3_different_w(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        let instruction = Assembler::v_format(vm)
            | vop
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_3_different_hn(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vm, &[vn]));
        let instruction = Assembler::v_format(vd)
            | vop
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_perm(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, op: Instr) {
        debug_assert!(are_same_format(vd, &[vn, vm]));
        debug_assert!(!vd.is_1d());
        let instruction = Assembler::v_format(vd)
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn imm_barrier_domain(imm2: u32) -> Instr {
        debug_assert!(is_uint2(imm2 as i64));
        imm2 << IMM_BARRIER_DOMAIN_OFFSET
    }

    fn imm_barrier_type(imm2: u32) -> Instr {
        debug_assert!(is_uint2(imm2 as i64));
        imm2 << IMM_BARRIER_TYPE_OFFSET
    }

    fn imm_system_register(imm15: u32) -> Instr {
        debug_assert!(crate::utils::utils::is_uintn(imm15 as i64, 15));
        imm15 << IMM_SYSTEM_REGISTER_OFFSET
    }

    pub fn dmb(&mut self, domain: BarrierDomain, barrier_type: BarrierType) {
        let instruction = DMB
            | Assembler::imm_barrier_domain(domain as u32)
            | Assembler::imm_barrier_type(barrier_type as u32);
        self.emit(instruction);
    }

    pub fn dsb(&mut self, domain: BarrierDomain, barrier_type: BarrierType) {
        let instruction = DSB
            | Assembler::imm_barrier_domain(domain as u32)
            | Assembler::imm_barrier_type(barrier_type as u32);
        self.emit(instruction);
    }

    pub fn isb(&mut self) {
        let instruction = ISB
            | Assembler::imm_barrier_domain(BarrierDomain::FullSystem as u32)
            | Assembler::imm_barrier_type(BarrierType::BarrierAll as u32);
        self.emit(instruction);
    }

    pub fn csdb(&mut self) {
        self.hint(SystemHint::Csdb);
    }

    pub fn mrs(&mut self, rt: Register, sysreg: SystemRegister) {
        debug_assert!(rt.is_64_bits());
        let instruction =
            MRS | Assembler::imm_system_register(sysreg as u32) | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn msr(&mut self, sysreg: SystemRegister, rt: Register) {
        debug_assert!(rt.is_64_bits());
        let instruction =
            MSR | Assembler::rt(rt) | Assembler::imm_system_register(sysreg as u32);
        self.emit(instruction);
    }

    pub fn pacib1716(&mut self) {
        self.emit(PACIB1716);
    }

    pub fn autib1716(&mut self) {
        self.emit(AUTIB1716);
    }

    pub fn pacibsp(&mut self) {
        self.emit(PACIBSP);
    }

    pub fn autibsp(&mut self) {
        self.emit(AUTIBSP);
    }

    pub fn bti(&mut self, id: BranchTargetIdentifier) {
        let op = match id {
            BranchTargetIdentifier::Bti => SystemHint::Bti,
            BranchTargetIdentifier::BtiCall => SystemHint::BtiC,
            BranchTargetIdentifier::BtiJump => SystemHint::BtiJ,
            BranchTargetIdentifier::BtiJumpCall => SystemHint::BtiJc,
            BranchTargetIdentifier::Pacibsp | BranchTargetIdentifier::None => unreachable!(),
        };
        self.hint(op);
    }

    fn neon_2_reg_misc(&mut self, vd: VRegister, vn: VRegister, vop: Instr) {
        debug_assert!(are_same_format(vd, &[vn]));
        let format;
        let mut op = vop;
        if vd.is_scalar() {
            op |= NEON_Q | NEON_SCALAR;
            format = Assembler::s_format(vd);
        } else {
            format = Assembler::v_format(vd);
        }
        let instruction = format | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_across_lanes(&mut self, vd: VRegister, vn: VRegister, op: Instr) {
        debug_assert!(
            (vn.is_8b() && vd.is_1b())
                || (vn.is_16b() && vd.is_1b())
                || (vn.is_4h() && vd.is_1h())
                || (vn.is_8h() && vd.is_1h())
                || (vn.is_4s() && vd.is_1s())
        );
        let instruction = if (op & NEON_ACROSS_LANES_FP_F_MASK) == NEON_ACROSS_LANES_FP_FIXED {
            Assembler::fp_format(vn) | op | Assembler::rn(vn) | Assembler::rd(vd)
        } else {
            Assembler::v_format(vn) | op | Assembler::rn(vn) | Assembler::rd(vd)
        };
        self.emit(instruction);
    }

    fn fp_format(vd: VRegister) -> Instr {
        if vd.lane_count() == 1 {
            debug_assert!(vd.is_32_bits() || vd.is_64_bits());
            return if vd.is_64_bits() { FP64 } else { FP32 };
        }
        if vd.lane_count() == 2 {
            debug_assert!(vd.is_64_bits() || vd.is_128_bits());
            return if vd.is_128_bits() { NEON_FP_2D } else { NEON_FP_2S };
        }
        if vd.lane_count() == 4 {
            debug_assert!(vd.is_64_bits() || vd.is_128_bits());
            return if vd.is_128_bits() { NEON_FP_4S } else { NEON_FP_4H };
        }
        debug_assert!((vd.lane_count() == 8) && vd.is_128_bits());
        NEON_FP_8H
    }

    fn imm_neon_hlm(index: i32, num_bits: i32) -> Instr {
        let h;
        let l;
        let m;
        if num_bits == 3 {
            debug_assert!(crate::utils::utils::is_uintn(index as i64, 3));
            h = (index >> 2) & 1;
            l = (index >> 1) & 1;
            m = index & 1;
        } else if num_bits == 2 {
            debug_assert!(is_uint2(index as i64));
            h = (index >> 1) & 1;
            l = index & 1;
            m = 0;
        } else {
            debug_assert!(is_uint1(index as i64) && (num_bits == 1));
            h = index & 1;
            l = 0;
            m = 0;
        }
        ((h as Instr) << NEON_H_OFFSET)
            | ((l as Instr) << NEON_L_OFFSET)
            | ((m as Instr) << NEON_M_OFFSET)
    }

    fn neon_by_element(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vm_index: i32,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(
            (vd.is_4h() && vm.is_1h())
                || (vd.is_8h() && vm.is_1h())
                || (vd.is_1h() && vm.is_1h())
                || (vd.is_2s() && vm.is_1s())
                || (vd.is_4s() && vm.is_1s())
                || (vd.is_1s() && vm.is_1s())
        );
        debug_assert!(
            (vm.is_1h() && (vm.code() < 16) && (vm_index < 8))
                || (vm.is_1s() && (vm_index < 4))
        );

        let format;
        let mut op = vop;
        let index_num_bits = if vm.is_1h() { 3 } else { 2 };
        if vd.is_scalar() {
            op |= NEON_SCALAR | NEON_Q;
            format = Assembler::s_format(vn);
        } else {
            format = Assembler::v_format(vn);
        }
        let instruction = format
            | op
            | Assembler::imm_neon_hlm(vm_index, index_num_bits)
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_by_element_l(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vm_index: i32,
        vop: Instr,
    ) {
        debug_assert!(
            (vd.is_4s() && vn.is_4h() && vm.is_1h())
                || (vd.is_4s() && vn.is_8h() && vm.is_1h())
                || (vd.is_1s() && vn.is_1h() && vm.is_1h())
                || (vd.is_2d() && vn.is_2s() && vm.is_1s())
                || (vd.is_2d() && vn.is_4s() && vm.is_1s())
                || (vd.is_1d() && vn.is_1s() && vm.is_1s())
        );
        debug_assert!(
            (vm.is_1h() && (vm.code() < 16) && (vm_index < 8))
                || (vm.is_1s() && (vm_index < 4))
        );

        let format;
        let mut op = vop;
        let index_num_bits = if vm.is_1h() { 3 } else { 2 };
        if vd.is_scalar() {
            op |= NEON_SCALAR | NEON_Q;
            format = Assembler::s_format(vn);
        } else {
            format = Assembler::v_format(vn);
        }
        let instruction = format
            | op
            | Assembler::imm_neon_hlm(vm_index, index_num_bits)
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_by_element(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vm_index: i32,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(
            (vd.is_2s() && vm.is_1s())
                || (vd.is_4s() && vm.is_1s())
                || (vd.is_1s() && vm.is_1s())
                || (vd.is_2d() && vm.is_1d())
                || (vd.is_1d() && vm.is_1d())
        );
        debug_assert!((vm.is_1s() && (vm_index < 4)) || (vm.is_1d() && (vm_index < 2)));

        let mut op = vop;
        let index_num_bits = if vm.is_1s() { 2 } else { 1 };
        if vd.is_scalar() {
            op |= NEON_Q | NEON_SCALAR;
        }
        let instruction = Assembler::fp_format(vd)
            | op
            | Assembler::imm_neon_hlm(vm_index, index_num_bits)
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_shift_immediate(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        op: Instr,
        immh_immb: i32,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        let q;
        let scalar;
        if vn.is_scalar() {
            q = NEON_Q;
            scalar = NEON_SCALAR;
        } else {
            q = if vd.is_d() { 0 } else { NEON_Q };
            scalar = 0;
        }
        let instruction = q
            | op
            | scalar
            | (immh_immb as Instr)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_shift_left_immediate(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let lane_size_in_bits = vn.lane_size_in_bits() as i32;
        debug_assert!((shift >= 0) && (shift < lane_size_in_bits));
        self.neon_shift_immediate(vd, vn, op, (lane_size_in_bits + shift) << 16);
    }

    fn neon_shift_right_immediate(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let lane_size_in_bits = vn.lane_size_in_bits() as i32;
        debug_assert!((shift >= 1) && (shift <= lane_size_in_bits));
        self.neon_shift_immediate(vd, vn, op, ((2 * lane_size_in_bits) - shift) << 16);
    }

    fn neon_shift_immediate_l(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let lane_size_in_bits = vn.lane_size_in_bits() as i32;
        debug_assert!((shift >= 0) && (shift < lane_size_in_bits));
        let immh_immb = (lane_size_in_bits + shift) << 16;

        debug_assert!(
            (vn.is_8b() && vd.is_8h())
                || (vn.is_4h() && vd.is_4s())
                || (vn.is_2s() && vd.is_2d())
                || (vn.is_16b() && vd.is_8h())
                || (vn.is_8h() && vd.is_4s())
                || (vn.is_4s() && vd.is_2d())
        );
        let q = if vn.is_d() { 0 } else { NEON_Q };
        let instruction = q
            | op
            | (immh_immb as Instr)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_shift_immediate_n(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let q;
        let scalar;
        let lane_size_in_bits = vd.lane_size_in_bits() as i32;
        debug_assert!((shift >= 1) && (shift <= lane_size_in_bits));
        let immh_immb = (2 * lane_size_in_bits - shift) << 16;

        if vn.is_scalar() {
            debug_assert!(
                (vd.is_1b() && vn.is_1h())
                    || (vd.is_1h() && vn.is_1s())
                    || (vd.is_1s() && vn.is_1d())
            );
            q = NEON_Q;
            scalar = NEON_SCALAR;
        } else {
            debug_assert!(
                (vd.is_8b() && vn.is_8h())
                    || (vd.is_4h() && vn.is_4s())
                    || (vd.is_2s() && vn.is_2d())
                    || (vd.is_16b() && vn.is_8h())
                    || (vd.is_8h() && vn.is_4s())
                    || (vd.is_4s() && vn.is_2d())
            );
            scalar = 0;
            q = if vd.is_d() { 0 } else { NEON_Q };
        }
        let instruction = q
            | op
            | scalar
            | (immh_immb as Instr)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_3_same(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, op: Instr) {
        debug_assert!(are_same_format(vd, &[vn, vm]));
        let mut op = op;
        if vd.is_4h() || vd.is_8h() {
            op |= NEON_SZ;
            op ^= NEON_3_SAME_HP_MASK;
        }
        let instruction = Assembler::fp_format(vd)
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_2_reg_misc_zero(&mut self, vd: VRegister, vn: VRegister, vop: Instr) {
        debug_assert!(are_same_format(vd, &[vn]));
        let mut op = vop;
        if vd.is_scalar() {
            debug_assert!(vd.is_1s() || vd.is_1d());
            op |= NEON_Q | NEON_SCALAR;
        } else if vd.is_4h() || vd.is_8h() {
            op |= NEON_SZ | NEON_2_REG_MISC_HP_FIXED;
        } else {
            debug_assert!(vd.is_2s() || vd.is_2d() || vd.is_4s());
        }
        let instruction =
            Assembler::fp_format(vd) | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_2_reg_misc(&mut self, vd: VRegister, vn: VRegister, op: Instr) {
        debug_assert!(are_same_format(vd, &[vn]));
        let instruction =
            Assembler::fp_format(vd) | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_3_same(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, vop: Instr) {
        debug_assert!(are_same_format(vd, &[vn, vm]));
        debug_assert!(vd.is_vector() || !vd.is_q());

        let format;
        let mut op = vop;
        if vd.is_scalar() {
            op |= NEON_Q | NEON_SCALAR;
            format = Assembler::s_format(vd);
        } else {
            format = Assembler::v_format(vd);
        }

        let instruction = format
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn add_sub(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        if operand.is_immediate() {
            let immediate = operand.immediate_value();
            debug_assert!(Assembler::is_imm_add_sub(immediate));
            let dest_reg = if s == FlagsUpdate::SetFlags {
                Assembler::rd(rd)
            } else {
                Assembler::rd_sp(rd)
            };
            let instruction = Assembler::sf(rd)
                | ADD_SUB_IMMEDIATE_FIXED
                | op
                | Assembler::flags(s)
                | Assembler::imm_add_sub(immediate as i32)
                | dest_reg
                | Assembler::rn_sp(rn);
            self.emit(instruction);
        } else if operand.is_shifted_register() {
            debug_assert_eq!(operand.reg().size_in_bits(), rd.size_in_bits());
            debug_assert_ne!(operand.shift(), Shift::Ror);

            if rn.is_sp() || rd.is_sp() {
                debug_assert!(!(rd.is_sp() && (s == FlagsUpdate::SetFlags)));
                self.data_proc_extended_register(
                    rd,
                    rn,
                    operand.to_extended_register(),
                    s,
                    ADD_SUB_EXTENDED_FIXED | op,
                );
            } else {
                self.data_proc_shifted_register(rd, rn, operand, s, ADD_SUB_SHIFTED_FIXED | op);
            }
        } else {
            debug_assert!(operand.is_extended_register());
            self.data_proc_extended_register(rd, rn, operand, s, ADD_SUB_EXTENDED_FIXED | op);
        }
    }

    fn logical(&mut self, rd: Register, rn: Register, operand: Operand, op: Instr) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        if operand.is_immediate() {
            let mut immediate = operand.immediate_value();
            let reg_size = rd.size_in_bits();
            let mut op = op;

            debug_assert_ne!(immediate, 0);
            debug_assert_ne!(immediate, -1);
            debug_assert!(rd.is_64_bits() || is_uint32(immediate));

            if (op & NOT) == NOT {
                op &= !NOT;
                immediate = if rd.is_64_bits() {
                    !immediate
                } else {
                    !immediate & K_W_REG_MASK
                };
            }

            let mut n = 0;
            let mut imm_s = 0;
            let mut imm_r = 0;
            if Assembler::is_imm_logical(
                immediate as u64,
                reg_size,
                &mut n,
                &mut imm_s,
                &mut imm_r,
            ) {
                self.logical_immediate(rd, rn, n, imm_s, imm_r, op);
            } else {
                unreachable!()
            }
        } else {
            debug_assert!(operand.is_shifted_register());
            debug_assert_eq!(operand.reg().size_in_bits(), rd.size_in_bits());
            let dp_op = op | LOGICAL_SHIFTED_FIXED;
            self.data_proc_shifted_register(rd, rn, operand, FlagsUpdate::LeaveFlags, dp_op);
        }
    }

    fn data_proc_shifted_register(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        debug_assert!(operand.is_shifted_register());
        debug_assert!(rn.is_64_bits() || (rn.is_32_bits() && is_uint5(operand.shift_amount() as i64)));
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::flags(s)
            | Assembler::shift_dp(operand.shift())
            | Assembler::imm_dp_shift(operand.shift_amount())
            | Assembler::rm(operand.reg())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    fn data_proc_extended_register(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        let dest_reg = if s == FlagsUpdate::SetFlags {
            Assembler::rd(rd)
        } else {
            Assembler::rd_sp(rd)
        };
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::flags(s)
            | Assembler::extend_mode(operand.extend())
            | Assembler::imm_extend_shift(operand.shift_amount())
            | Assembler::rm(operand.reg())
            | Assembler::rn_sp(rn)
            | dest_reg;
        self.emit(instruction);
    }

    fn extend_mode(extend: Extend) -> Instr {
        (extend as Instr) << EXTEND_MODE_OFFSET
    }

    fn imm_extend_shift(left_shift: u32) -> Instr {
        debug_assert!(left_shift <= 4);
        left_shift << IMM_EXTEND_SHIFT_OFFSET
    }

    fn move_wide(&mut self, rd: Register, imm: u64, shift: i32, mov_op: Instr) {
        let mut imm = imm;
        let mut shift = shift;
        if rd.is_32_bits() {
            debug_assert!(
                ((imm >> K_W_REG_SIZE_IN_BITS) == 0)
                    || ((imm >> (K_W_REG_SIZE_IN_BITS - 1)) == 0x1FFFFFFFF)
            );
            imm &= K_W_REG_MASK as u64;
        }

        if shift >= 0 {
            debug_assert!((shift == 0) || (shift == 16) || (shift == 32) || (shift == 48));
            debug_assert!(rd.is_64_bits() || (shift == 0) || (shift == 16));
            shift /= 16;
        } else {
            shift = 0;
            if (imm & !0xFFFFu64) == 0 {
            } else if (imm & !(0xFFFFu64 << 16)) == 0 {
                imm >>= 16;
                shift = 1;
            } else if (imm & !(0xFFFFu64 << 32)) == 0 {
                debug_assert!(rd.is_64_bits());
                imm >>= 32;
                shift = 2;
            } else if (imm & !(0xFFFFu64 << 48)) == 0 {
                debug_assert!(rd.is_64_bits());
                imm >>= 48;
                shift = 3;
            }
        }

        debug_assert!(is_uint16(imm as i64));

        let instruction = Assembler::sf(rd)
            | MOVE_WIDE_IMMEDIATE_FIXED
            | mov_op
            | Assembler::rd(rd)
            | Assembler::imm_move_wide(imm as i32)
            | Assembler::shift_move_wide(shift);
        self.emit(instruction);
    }

    fn load_store(&mut self, rt: impl Into<CPURegister>, addr: MemOperand, op: Instr) {
        let rt = rt.into();
        let memop = op | Assembler::rt(rt) | Assembler::rn_sp(addr.base());

        if addr.is_immediate_offset() {
            let size_log2 = Assembler::calc_ls_data_size_log2(op);
            let offset = addr.offset() as i32;
            if Assembler::is_imm_ls_scaled(addr.offset(), size_log2) {
                self.load_store_scaled_imm_offset(memop, offset, size_log2);
            } else {
                debug_assert!(Assembler::is_imm_ls_unscaled(addr.offset()));
                self.load_store_unscaled_imm_offset(memop, offset);
            }
        } else if addr.is_register_offset() {
            let mut ext = addr.extend();
            let shift = addr.shift();
            let shift_amount = addr.shift_amount();

            if shift == Shift::Lsl {
                ext = Extend::Uxtx;
            }

            debug_assert!(
                shift_amount == 0 || shift_amount == Assembler::calc_ls_data_size_log2(op)
            );
            let instruction = LOAD_STORE_REGISTER_OFFSET_FIXED
                | memop
                | Assembler::rm(addr.regoffset())
                | Assembler::extend_mode(ext)
                | Assembler::imm_shift_ls(if shift_amount > 0 { 1 } else { 0 });
            self.emit(instruction);
        } else {
            debug_assert!(Assembler::is_imm_ls_unscaled(addr.offset()));
            debug_assert_ne!(rt, addr.base().into());
            let offset = addr.offset() as i32;
            if addr.is_pre_index() {
                let instruction =
                    LOAD_STORE_PRE_INDEX_FIXED | memop | Assembler::imm_ls(offset);
                self.emit(instruction);
            } else {
                debug_assert!(addr.is_post_index());
                let instruction =
                    LOAD_STORE_POST_INDEX_FIXED | memop | Assembler::imm_ls(offset);
                self.emit(instruction);
            }
        }
    }

    fn imm_shift_ls(shift_amount: u32) -> Instr {
        debug_assert!(is_uint1(shift_amount as i64));
        shift_amount << IMM_SHIFT_LS_OFFSET
    }

    fn load_store_scaled_imm_offset(&mut self, memop: Instr, offset: i32, size: u32) {
        let instruction = LOAD_STORE_UNSIGNED_OFFSET_FIXED
            | memop
            | Assembler::imm_ls_unsigned(offset >> size);
        self.emit(instruction);
    }

    fn load_store_unscaled_imm_offset(&mut self, memop: Instr, offset: i32) {
        let instruction =
            LOAD_STORE_UNSCALED_OFFSET_FIXED | memop | Assembler::imm_ls(offset);
        self.emit(instruction);
    }
}


macro_rules! define_compare_and_swap_w_x {
    ($(($name:ident, $op_x:ident, $op_w:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, src: MemOperand) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    let op = if rt.is_64_bits() { $op_x } else { $op_w };
                    let instruction = op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | RT2_MASK
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_compare_and_swap_w_x!(
    (cas, CAS_X, CAS_W),
    (casa, CASA_X, CASA_W),
    (casl, CASL_X, CASL_W),
    (casal, CASAL_X, CASAL_W),
);

macro_rules! define_compare_and_swap_w {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, src: MemOperand) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    let instruction = $op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | RT2_MASK
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_compare_and_swap_w!(
    (casb, CASB),
    (casab, CASAB),
    (caslb, CASLB),
    (casalb, CASALB),
    (cash, CASH),
    (casah, CASAH),
    (caslh, CASLH),
    (casalh, CASALH),
);

macro_rules! define_compare_and_swap_pair {
    ($(($name:ident, $op_x:ident, $op_w:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    rs: Register,
                    rs1: Register,
                    rt: Register,
                    rt1: Register,
                    src: MemOperand,
                ) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    debug_assert!(Assembler::are_even(rs, rt));
                    debug_assert!(Assembler::are_consecutive(rs, rs1));
                    debug_assert!(Assembler::are_consecutive(rt, rt1));
                    let op = if rt.is_64_bits() { $op_x } else { $op_w };
                    let instruction = op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | RT2_MASK
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_compare_and_swap_pair!(
    (casp, CASP_X, CASP_W),
    (caspa, CASPA_X, CASPA_W),
    (caspl, CASPL_X, CASPL_W),
    (caspal, CASPAL_X, CASPAL_W),
);

macro_rules! define_atomic_memory_load {
    ($(($name:ident, $op_x:ident, $op_w:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, src: MemOperand) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    let op = if rt.is_64_bits() { $op_x } else { $op_w };
                    let instruction = op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_atomic_memory_load!(
    (ldadd, LDADD_X, LDADD_W),
    (ldaddl, LDADDL_X, LDADDL_W),
    (ldaddb, LDADDB, LDADDB),
    (ldaddlb, LDADDLB, LDADDLB),
    (ldaddh, LDADDH, LDADDH),
    (ldaddlh, LDADDLH, LDADDLH),
    (ldadda, LDADDA_X, LDADDA_W),
    (ldaddal, LDADDAL_X, LDADDAL_W),
    (ldaddab, LDADDAB, LDADDAB),
    (ldaddalb, LDADDALB, LDADDALB),
    (ldaddah, LDADDAH, LDADDAH),
    (ldaddalh, LDADDALH, LDADDALH),
    (ldclr, LDCLR_X, LDCLR_W),
    (ldclrl, LDCLRL_X, LDCLRL_W),
    (ldclrb, LDCLRB, LDCLRB),
    (ldclrlb, LDCLRLB, LDCLRLB),
    (ldclrh, LDCLRH, LDCLRH),
    (ldclrlh, LDCLRLH, LDCLRLH),
    (ldclra, LDCLRA_X, LDCLRA_W),
    (ldclral, LDCLRAL_X, LDCLRAL_W),
    (ldclrab, LDCLRAB, LDCLRAB),
    (ldclralb, LDCLRALB, LDCLRALB),
    (ldclrah, LDCLRAH, LDCLRAH),
    (ldclralh, LDCLRALH, LDCLRALH),
    (ldeor, LDEOR_X, LDEOR_W),
    (ldeorl, LDEORL_X, LDEORL_W),
    (ldeorb, LDEORB, LDEORB),
    (ldeorlb, LDEORLB, LDEORLB),
    (ldeorh, LDEORH, LDEORH),
    (ldeorlh, LDEORLH, LDEORLH),
    (ldeora, LDEORA_X, LDEORA_W),
    (ldeoral, LDEORAL_X, LDEORAL_W),
    (ldeorab, LDEORAB, LDEORAB),
    (ldeoralb, LDEORALB, LDEORALB),
    (ldeorah, LDEORAH, LDEORAH),
    (ldeoralh, LDEORALH, LDEORALH),
    (ldset, LDSET_X, LDSET_W),
    (ldsetl, LDSETL_X, LDSETL_W),
    (ldsetb, LDSETB, LDSETB),
    (ldsetlb, LDSETLB, LDSETLB),
    (ldseth, LDSETH, LDSETH),
    (ldsetlh, LDSETLH, LDSETLH),
    (ldseta, LDSETA_X, LDSETA_W),
    (ldsetal, LDSETAL_X, LDSETAL_W),
    (ldsetab, LDSETAB, LDSETAB),
    (ldsetalb, LDSETALB, LDSETALB),
    (ldsetah, LDSETAH, LDSETAH),
    (ldsetalh, LDSETALH, LDSETALH),
    (ldsmax, LDSMAX_X, LDSMAX_W),
    (ldsmaxl, LDSMAXL_X, LDSMAXL_W),
    (ldsmaxb, LDSMAXB, LDSMAXB),
    (ldsmaxlb, LDSMAXLB, LDSMAXLB),
    (ldsmaxh, LDSMAXH, LDSMAXH),
    (ldsmaxlh, LDSMAXLH, LDSMAXLH),
    (ldsmaxa, LDSMAXA_X, LDSMAXA_W),
    (ldsmaxal, LDSMAXAL_X, LDSMAXAL_W),
    (ldsmaxab, LDSMAXAB, LDSMAXAB),
    (ldsmaxalb, LDSMAXALB, LDSMAXALB),
    (ldsmaxah, LDSMAXAH, LDSMAXAH),
    (ldsmaxalh, LDSMAXALH, LDSMAXALH),
    (ldsmin, LDSMIN_X, LDSMIN_W),
    (ldsminl, LDSMINL_X, LDSMINL_W),
    (ldsminb, LDSMINB, LDSMINB),
    (ldsminlb, LDSMINLB, LDSMINLB),
    (ldsminh, LDSMINH, LDSMINH),
    (ldsminlh, LDSMINLH, LDSMINLH),
    (ldsmina, LDSMINA_X, LDSMINA_W),
    (ldsminal, LDSMINAL_X, LDSMINAL_W),
    (ldsminab, LDSMINAB, LDSMINAB),
    (ldsminalb, LDSMINALB, LDSMINALB),
    (ldsminah, LDSMINAH, LDSMINAH),
    (ldsminalh, LDSMINALH, LDSMINALH),
    (ldumax, LDUMAX_X, LDUMAX_W),
    (ldumaxl, LDUMAXL_X, LDUMAXL_W),
    (ldumaxb, LDUMAXB, LDUMAXB),
    (ldumaxlb, LDUMAXLB, LDUMAXLB),
    (ldumaxh, LDUMAXH, LDUMAXH),
    (ldumaxlh, LDUMAXLH, LDUMAXLH),
    (ldumaxa, LDUMAXA_X, LDUMAXA_W),
    (ldumaxal, LDUMAXAL_X, LDUMAXAL_W),
    (ldumaxab, LDUMAXAB, LDUMAXAB),
    (ldumaxalb, LDUMAXALB, LDUMAXALB),
    (ldumaxah, LDUMAXAH, LDUMAXAH),
    (ldumaxalh, LDUMAXALH, LDUMAXALH),
    (ldumin, LDUMIN_X, LDUMIN_W),
    (lduminl, LDUMINL_X, LDUMINL_W),
    (lduminb, LDUMINB, LDUMINB),
    (lduminlb, LDUMINLB, LDUMINLB),
    (lduminh, LDUMINH, LDUMINH),
    (lduminlh, LDUMINLH, LDUMINLH),
    (ldumina, LDUMINA_X, LDUMINA_W),
    (lduminal, LDUMINAL_X, LDUMINAL_W),
    (lduminab, LDUMINAB, LDUMINAB),
    (lduminalb, LDUMINALB, LDUMINALB),
    (lduminah, LDUMINAH, LDUMINAH),
    (lduminalh, LDUMINALH, LDUMINALH),
);

macro_rules! define_atomic_memory_store {
    ($(($name:ident, $load:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, src: MemOperand) {
                    let zr = Assembler::appropriate_zero_reg_for(rs);
                    self.$load(rs, zr, src);
                }
            )*
        }
    };
}

define_atomic_memory_store!(
    (stadd, ldadd),
    (staddl, ldaddl),
    (staddb, ldaddb),
    (staddlb, ldaddlb),
    (staddh, ldaddh),
    (staddlh, ldaddlh),
    (stclr, ldclr),
    (stclrl, ldclrl),
    (stclrb, ldclrb),
    (stclrlb, ldclrlb),
    (stclrh, ldclrh),
    (stclrlh, ldclrlh),
    (steor, ldeor),
    (steorl, ldeorl),
    (steorb, ldeorb),
    (steorlb, ldeorlb),
    (steorh, ldeorh),
    (steorlh, ldeorlh),
    (stset, ldset),
    (stsetl, ldsetl),
    (stsetb, ldsetb),
    (stsetlb, ldsetlb),
    (stseth, ldseth),
    (stsetlh, ldsetlh),
    (stsmax, ldsmax),
    (stsmaxl, ldsmaxl),
    (stsmaxb, ldsmaxb),
    (stsmaxlb, ldsmaxlb),
    (stsmaxh, ldsmaxh),
    (stsmaxlh, ldsmaxlh),
    (stsmin, ldsmin),
    (stsminl, ldsminl),
    (stsminb, ldsminb),
    (stsminlb, ldsminlb),
    (stsminh, ldsminh),
    (stsminlh, ldsminlh),
    (stumax, ldumax),
    (stumaxl, ldumaxl),
    (stumaxb, ldumaxb),
    (stumaxlb, ldumaxlb),
    (stumaxh, ldumaxh),
    (stumaxlh, ldumaxlh),
    (stumin, ldumin),
    (stuminl, lduminl),
    (stuminb, lduminb),
    (stuminlb, lduminlb),
    (stuminh, lduminh),
    (stuminlh, lduminlh),
);

define_atomic_memory_load!(
    (swp, SWP_X, SWP_W),
    (swpl, SWPL_X, SWPL_W),
    (swpb, SWPB, SWPB),
    (swplb, SWPLB, SWPLB),
    (swph, SWPH, SWPH),
    (swplh, SWPLH, SWPLH),
    (swpa, SWPA_X, SWPA_W),
    (swpal, SWPAL_X, SWPAL_W),
    (swpab, SWPAB, SWPAB),
    (swpalb, SWPALB, SWPALB),
    (swpah, SWPAH, SWPAH),
    (swpalh, SWPALH, SWPALH),
);

macro_rules! define_neon_3_same {
    ($(($name:ident, $op:ident, $vd:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, vn: VRegister, vm: VRegister) {
                    debug_assert!($as);
                    self.neon_3_same($vd, vn, vm, $op);
                }
            )*
        }
    };
}

define_neon_3_same!(
    (addp, NEON_ADDP, vd, vd.is_vector() || vd.is_1d()),
    (cmeq, NEON_CMEQ, vd, vd.is_vector() || vd.is_1d()),
    (cmge, NEON_CMGE, vd, vd.is_vector() || vd.is_1d()),
    (cmgt, NEON_CMGT, vd, vd.is_vector() || vd.is_1d()),
    (cmhi, NEON_CMHI, vd, vd.is_vector() || vd.is_1d()),
    (cmhs, NEON_CMHS, vd, vd.is_vector() || vd.is_1d()),
    (cmtst, NEON_CMTST, vd, vd.is_vector() || vd.is_1d()),
    (sshl, NEON_SSHL, vd, vd.is_vector() || vd.is_1d()),
    (ushl, NEON_USHL, vd, vd.is_vector() || vd.is_1d()),
    (srshl, NEON_SRSHL, vd, vd.is_vector() || vd.is_1d()),
    (urshl, NEON_URSHL, vd, vd.is_vector() || vd.is_1d()),
    (shadd, NEON_SHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uhadd, NEON_UHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (srhadd, NEON_SRHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (urhadd, NEON_URHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (shsub, NEON_SHSUB, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uhsub, NEON_UHSUB, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (smax, NEON_SMAX, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (smaxp, NEON_SMAXP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (smin, NEON_SMIN, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (sminp, NEON_SMINP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (umax, NEON_UMAX, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (umaxp, NEON_UMAXP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (umin, NEON_UMIN, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uminp, NEON_UMINP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (saba, NEON_SABA, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (sabd, NEON_SABD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uaba, NEON_UABA, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uabd, NEON_UABD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (bit, NEON_BIT, vd, vd.is_8b() || vd.is_16b()),
    (bif, NEON_BIF, vd, vd.is_8b() || vd.is_16b()),
    (bsl, NEON_BSL, vd, vd.is_8b() || vd.is_16b()),
    (pmul, NEON_PMUL, vd, vd.is_8b() || vd.is_16b()),
    (uqadd, NEON_UQADD, vd, true),
    (sqadd, NEON_SQADD, vd, true),
    (uqsub, NEON_UQSUB, vd, true),
    (sqsub, NEON_SQSUB, vd, true),
    (sqrshl, NEON_SQRSHL, vd, true),
    (uqrshl, NEON_UQRSHL, vd, true),
);

pub trait Arithmetic: Copy {
    type Rhs;

    fn add(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn sub(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn mul(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn and_(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn orr(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn orn(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn eor(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn bic(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn smull(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn umull(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
}

impl Arithmetic for Register {
    type Rhs = Operand;

    fn add(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.add_sub(rd, rn, operand, FlagsUpdate::LeaveFlags, ADD);
    }

    fn sub(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.add_sub(rd, rn, operand, FlagsUpdate::LeaveFlags, SUB);
    }

    fn mul(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        let rm = operand.reg();
        let zr = Assembler::appropriate_zero_reg_for(rn);
        asm.data_processing_3_source(rd, rn, rm, zr, MADD);
    }

    fn and_(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, AND);
    }

    fn orr(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, ORR);
    }

    fn orn(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, ORN);
    }

    fn eor(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, EOR);
    }

    fn bic(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, BIC);
    }

    fn smull(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        let rm = operand.reg();
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        asm.data_processing_3_source(rd, rn, rm, XZR, SMADDL_X);
    }

    fn umull(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        let rm = operand.reg();
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        asm.data_processing_3_source(rd, rn, rm, XZR, UMADDL_X);
    }
}

impl Arithmetic for VRegister {
    type Rhs = VOperand;

    fn add(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_vector() || vd.is_1d());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_ADD);
    }

    fn sub(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_vector() || vd.is_1d());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_SUB);
    }

    fn mul(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        let vm = operand.reg();
        if operand.has_lane() {
            debug_assert!(vn.is_vector());
            asm.neon_by_element(vd, vn, vm, operand.index(), NEON_MUL_BYELEMENT);
            return;
        }
        debug_assert!(vd.is_vector() && !vd.is_lane_size_d());
        asm.neon_3_same(vd, vn, vm, NEON_MUL);
    }

    fn and_(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_AND);
    }

    fn orr(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_ORR);
    }

    fn orn(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_ORN);
    }

    fn eor(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_EOR);
    }

    fn bic(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_BIC);
    }

    fn smull(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        let vm = operand.reg();
        debug_assert!(vn.is_vector() && vn.is_d());
        if operand.has_lane() {
            asm.neon_by_element_l(vd, vn, vm, operand.index(), NEON_SMULL_BYELEMENT);
            return;
        }
        asm.neon_3_different_l(vd, vn, vm, NEON_SMULL);
    }

    fn umull(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        let vm = operand.reg();
        debug_assert!(vn.is_vector() && vn.is_d());
        if operand.has_lane() {
            asm.neon_by_element_l(vd, vn, vm, operand.index(), NEON_UMULL_BYELEMENT);
            return;
        }
        asm.neon_3_different_l(vd, vn, vm, NEON_UMULL);
    }
}

impl Assembler {
    pub fn add<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::add(self, rd, rn, operand.into());
    }

    pub fn sub<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::sub(self, rd, rn, operand.into());
    }

    pub fn mul<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::mul(self, rd, rn, operand.into());
    }

    pub fn and_<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::and_(self, rd, rn, operand.into());
    }

    pub fn orr<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::orr(self, rd, rn, operand.into());
    }

    pub fn orn<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::orn(self, rd, rn, operand.into());
    }

    pub fn eor<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::eor(self, rd, rn, operand.into());
    }

    pub fn bic<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::bic(self, rd, rn, operand.into());
    }

    pub fn smull<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::smull(self, rd, rn, operand.into());
    }

    pub fn umull<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::umull(self, rd, rn, operand.into());
    }
}

macro_rules! define_neon_3_different_l {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $vm:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    $vm: VRegister,
                ) {
                    debug_assert!($as);
                    self.neon_3_different_l($vd, $vn, $vm, $op);
                }
            )*
        }
    };
}

define_neon_3_different_l!(
    (saddl, NEON_SADDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (saddl2, NEON_SADDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (sabal, NEON_SABAL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (sabal2, NEON_SABAL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (uabal, NEON_UABAL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (uabal2, NEON_UABAL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (sabdl, NEON_SABDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (sabdl2, NEON_SABDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (uabdl, NEON_UABDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (uabdl2, NEON_UABDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (ssubl, NEON_SSUBL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (ssubl2, NEON_SSUBL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (uaddl, NEON_UADDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (uaddl2, NEON_UADDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (usubl, NEON_USUBL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (usubl2, NEON_USUBL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
);

macro_rules! define_neon_3_different_hn {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $vm:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    $vm: VRegister,
                ) {
                    debug_assert!($as);
                    self.neon_3_different_hn($vd, $vn, $vm, $op);
                }
            )*
        }
    };
}

define_neon_3_different_hn!(
    (addhn, NEON_ADDHN, vd, vn, vm, vd.is_d()),
    (addhn2, NEON_ADDHN2, vd, vn, vm, vd.is_q()),
    (raddhn, NEON_RADDHN, vd, vn, vm, vd.is_d()),
    (raddhn2, NEON_RADDHN2, vd, vn, vm, vd.is_q()),
    (subhn, NEON_SUBHN, vd, vn, vm, vd.is_d()),
    (subhn2, NEON_SUBHN2, vd, vn, vm, vd.is_q()),
    (rsubhn, NEON_RSUBHN, vd, vn, vm, vd.is_d()),
    (rsubhn2, NEON_RSUBHN2, vd, vn, vm, vd.is_q()),
);

macro_rules! define_neon_3_different_w {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $vm:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    $vm: VRegister,
                ) {
                    debug_assert!($as);
                    self.neon_3_different_w($vd, $vn, $vm, $op);
                }
            )*
        }
    };
}

define_neon_3_different_w!(
    (uaddw, NEON_UADDW, vd, vn, vm, vm.is_d()),
    (uaddw2, NEON_UADDW2, vd, vn, vm, vm.is_q()),
    (saddw, NEON_SADDW, vd, vn, vm, vm.is_d()),
    (saddw2, NEON_SADDW2, vd, vn, vm, vm.is_q()),
    (usubw, NEON_USUBW, vd, vn, vm, vm.is_d()),
    (usubw2, NEON_USUBW2, vd, vn, vm, vm.is_q()),
    (ssubw, NEON_SSUBW, vd, vn, vm, vm.is_d()),
    (ssubw2, NEON_SSUBW2, vd, vn, vm, vm.is_q()),
);

macro_rules! define_neon_perm {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
                    self.neon_perm(vd, vn, vm, $op);
                }
            )*
        }
    };
}

define_neon_perm!(
    (trn1, NEON_TRN1),
    (trn2, NEON_TRN2),
    (uzp1, NEON_UZP1),
    (uzp2, NEON_UZP2),
    (zip1, NEON_ZIP1),
    (zip2, NEON_ZIP2),
);

macro_rules! define_neon_across_lanes {
    ($(($name:ident, $op:ident, $vd:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, vn: VRegister) {
                    debug_assert!($as);
                    self.neon_across_lanes($vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_across_lanes!(
    (fmaxv, NEON_FMAXV, vd, vd.is_1s()),
    (fminv, NEON_FMINV, vd, vd.is_1s()),
    (fmaxnmv, NEON_FMAXNMV, vd, vd.is_1s()),
    (fminnmv, NEON_FMINNMV, vd, vd.is_1s()),
    (addv, NEON_ADDV, vd, true),
    (smaxv, NEON_SMAXV, vd, true),
    (sminv, NEON_SMINV, vd, true),
    (umaxv, NEON_UMAXV, vd, true),
    (uminv, NEON_UMINV, vd, true),
);

macro_rules! define_neon_2_reg_misc {
    ($(($name:ident, $op:ident, $vd:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, vn: VRegister) {
                    debug_assert!($as);
                    self.neon_2_reg_misc($vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_2_reg_misc!(
    (suqadd, NEON_SUQADD, vd, true),
    (usqadd, NEON_USQADD, vd, true),
    (abs, NEON_ABS, vd, vd.is_vector() || vd.is_1d()),
    (sqabs, NEON_SQABS, vd, true),
    (sqneg, NEON_SQNEG, vd, true),
    (rev64, NEON_REV64, vd, !vd.is_1d()),
    (saddlp, NEON_SADDLP, vd, true),
    (uaddlp, NEON_UADDLP, vd, true),
    (sadalp, NEON_SADALP, vd, true),
    (uadalp, NEON_UADALP, vd, true),
    (cnt, NEON_CNT, vd, vd.is_8b() || vd.is_16b()),
);

macro_rules! define_neon_dual_3_same {
    ($(($name:ident, $op:ident, $op_byelement:ident, $vd:ident, $vn:ident, $vm:ident,
        $as:expr, $as_byelement:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name<T: Into<VOperand>>(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    operand: T,
                ) {
                    let operand = operand.into();
                    let $vm = operand.reg();
                    if operand.has_lane() {
                        debug_assert!($as_byelement);
                        self.neon_by_element($vd, $vn, $vm, operand.index(), $op_byelement);
                    } else {
                        debug_assert!($as);
                        self.neon_3_same($vd, $vn, $vm, $op);
                    }
                }
            )*
        }
    };
}

define_neon_dual_3_same!(
    (mla, NEON_MLA, NEON_MLA_BYELEMENT, vd, vn, vm,
     vd.is_vector() && !vd.is_lane_size_d(), vn.is_vector()),
    (mls, NEON_MLS, NEON_MLS_BYELEMENT, vd, vn, vm,
     vd.is_vector() && !vd.is_lane_size_d(), vn.is_vector()),
    (sqdmulh, NEON_SQDMULH, NEON_SQDMULH_BYELEMENT, vd, vn, vm,
     vd.is_lane_size_h() || vd.is_lane_size_s(), true),
    (sqrdmulh, NEON_SQRDMULH, NEON_SQRDMULH_BYELEMENT, vd, vn, vm,
     vd.is_lane_size_h() || vd.is_lane_size_s(), true),
);

macro_rules! define_neon_dual_3_different_l {
    ($(($name:ident, $op:ident, $op_byelement:ident, $vd:ident, $vn:ident, $vm:ident,
        $as:expr, $as_byelement:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name<T: Into<VOperand>>(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    operand: T,
                ) {
                    let operand = operand.into();
                    let $vm = operand.reg();
                    if operand.has_lane() {
                        debug_assert!($as_byelement);
                        self.neon_by_element_l($vd, $vn, $vm, operand.index(), $op_byelement);
                    } else {
                        debug_assert!($as);
                        self.neon_3_different_l($vd, $vn, $vm, $op);
                    }
                }
            )*
        }
    };
}

define_neon_dual_3_different_l!(
    (smlal, NEON_SMLAL, NEON_SMLAL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (smlal2, NEON_SMLAL2, NEON_SMLAL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (umlal, NEON_UMLAL, NEON_UMLAL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (umlal2, NEON_UMLAL2, NEON_UMLAL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (smlsl, NEON_SMLSL, NEON_SMLSL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (smlsl2, NEON_SMLSL2, NEON_SMLSL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (umlsl, NEON_UMLSL, NEON_UMLSL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (umlsl2, NEON_UMLSL2, NEON_UMLSL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (smull2, NEON_SMULL2, NEON_SMULL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (umull2, NEON_UMULL2, NEON_UMULL_BYELEMENT, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (sqdmull, NEON_SQDMULL, NEON_SQDMULL_BYELEMENT, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_4h() || vn.is_2s(), vn.is_scalar() || vn.is_d()),
    (sqdmull2, NEON_SQDMULL2, NEON_SQDMULL_BYELEMENT, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_8h() || vn.is_4s(), vn.is_vector() && vn.is_q()),
    (sqdmlal, NEON_SQDMLAL, NEON_SQDMLAL_BYELEMENT, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_4h() || vn.is_2s(), vn.is_scalar() || vn.is_d()),
    (sqdmlal2, NEON_SQDMLAL2, NEON_SQDMLAL_BYELEMENT, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_8h() || vn.is_4s(), vn.is_vector() && vn.is_q()),
    (sqdmlsl, NEON_SQDMLSL, NEON_SQDMLSL_BYELEMENT, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_4h() || vn.is_2s(), vn.is_scalar() || vn.is_d()),
    (sqdmlsl2, NEON_SQDMLSL2, NEON_SQDMLSL_BYELEMENT, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_8h() || vn.is_4s(), vn.is_vector() && vn.is_q()),
);

macro_rules! define_neon_dual_fp {
    ($(($name:ident, $op:ident, $vec_op:ident, $op_byelement:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name<T: Into<VOperand>>(
                    &mut self,
                    vd: VRegister,
                    vn: VRegister,
                    operand: T,
                ) {
                    let operand = operand.into();
                    let vm = operand.reg();
                    if operand.has_lane() {
                        self.neon_fp_by_element(vd, vn, vm, operand.index(), $op_byelement);
                        return;
                    }
                    let op;
                    if vd.is_scalar() {
                        debug_assert!(vd.is_1s() || vd.is_1d());
                        op = $op;
                    } else {
                        debug_assert!(vd.is_vector());
                        op = $vec_op;
                    }
                    self.neon_fp_3_same(vd, vn, vm, op);
                }
            )*
        }
    };
}

define_neon_dual_fp!(
    (fmul, FMUL, NEON_FMUL, NEON_FMUL_BYELEMENT),
);

macro_rules! define_neon_fp_by_element {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    vd: VRegister,
                    vn: VRegister,
                    vm: VRegister,
                    vm_index: i32,
                ) {
                    self.neon_fp_by_element(vd, vn, vm, vm_index, $op);
                }
            )*
        }
    };
}

define_neon_fp_by_element!(
    (fmla, NEON_FMLA_BYELEMENT),
    (fmls, NEON_FMLS_BYELEMENT),
);

macro_rules! define_neon_shift_left {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_left_immediate($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_left!(
    (shl, NEON_SHL, vd, vn, vd.is_vector() || vd.is_1d()),
    (sli, NEON_SLI, vd, vn, vd.is_vector() || vd.is_1d()),
    (sqshlu, NEON_SQSHLU, vd, vn, true),
);

macro_rules! define_neon_shift_right {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_right_immediate($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_right!(
    (sri, NEON_SRI, vd, vn, vd.is_vector() || vd.is_1d()),
    (sshr, NEON_SSHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (ushr, NEON_USHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (srshr, NEON_SRSHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (urshr, NEON_URSHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (ssra, NEON_SSRA, vd, vn, vd.is_vector() || vd.is_1d()),
    (usra, NEON_USRA, vd, vn, vd.is_vector() || vd.is_1d()),
    (srsra, NEON_SRSRA, vd, vn, vd.is_vector() || vd.is_1d()),
    (ursra, NEON_URSRA, vd, vn, vd.is_vector() || vd.is_1d()),
);

macro_rules! define_neon_shift_l {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_immediate_l($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_l!(
    (sshll, NEON_SSHLL, vd, vn, vn.is_d()),
    (sshll2, NEON_SSHLL, vd, vn, vn.is_q()),
    (ushll, NEON_USHLL, vd, vn, vn.is_d()),
    (ushll2, NEON_USHLL, vd, vn, vn.is_q()),
);

macro_rules! define_neon_shift_n {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_immediate_n($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_n!(
    (shrn, NEON_SHRN, vd, vn, vn.is_vector() && vd.is_d()),
    (shrn2, NEON_SHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (rshrn, NEON_RSHRN, vd, vn, vn.is_vector() && vd.is_d()),
    (rshrn2, NEON_RSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqshrn, NEON_SQSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqshrn2, NEON_SQSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqrshrn, NEON_SQRSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqrshrn2, NEON_SQRSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqshrun, NEON_SQSHRUN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqshrun2, NEON_SQSHRUN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqrshrun, NEON_SQRSHRUN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqrshrun2, NEON_SQRSHRUN, vd, vn, vn.is_vector() && vd.is_q()),
    (uqshrn, NEON_UQSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (uqshrn2, NEON_UQSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (uqrshrn, NEON_UQRSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (uqrshrn2, NEON_UQRSHRN, vd, vn, vn.is_vector() && vd.is_q()),
);

pub trait NeonShift: Copy {
    fn sqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: Self);
    fn uqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: Self);
}

impl NeonShift for VRegister {
    fn sqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, vm: VRegister) {
        asm.neon_3_same(vd, vn, vm, NEON_SQSHL);
    }

    fn uqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, vm: VRegister) {
        asm.neon_3_same(vd, vn, vm, NEON_UQSHL);
    }
}

impl NeonShift for i32 {
    fn sqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, shift: i32) {
        asm.neon_shift_left_immediate(vd, vn, shift, NEON_SQSHL_IMM);
    }

    fn uqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, shift: i32) {
        asm.neon_shift_left_immediate(vd, vn, shift, NEON_UQSHL_IMM);
    }
}

impl Assembler {
    pub fn sqshl<T: NeonShift>(&mut self, vd: VRegister, vn: VRegister, operand: T) {
        T::sqshl(self, vd, vn, operand);
    }

    pub fn uqshl<T: NeonShift>(&mut self, vd: VRegister, vn: VRegister, operand: T) {
        T::uqshl(self, vd, vn, operand);
    }
}

macro_rules! define_neon_fp_3_same {
    ($(($name:ident, $vec_op:ident, $sca_op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
                    let op;
                    if vd.is_scalar() {
                        debug_assert!(vd.is_1s() || vd.is_1d());
                        op = $sca_op;
                    } else {
                        debug_assert!(vd.is_vector());
                        debug_assert!(
                            vd.is_2s() || vd.is_2d() || vd.is_4s() || vd.is_4h() || vd.is_8h()
                        );
                        op = $vec_op;
                    }
                    self.neon_fp_3_same(vd, vn, vm, op);
                }
            )*
        }
    };
}

define_neon_fp_3_same!(
    (fadd, NEON_FADD, FADD),
    (fsub, NEON_FSUB, FSUB),
    (fdiv, NEON_FDIV, FDIV),
    (fmax, NEON_FMAX, FMAX),
    (fmaxnm, NEON_FMAXNM, FMAXNM),
    (fmin, NEON_FMIN, FMIN),
    (fminnm, NEON_FMINNM, FMINNM),
    (frecps, NEON_FRECPS, NEON_FRECPS_SCALAR),
    (frsqrts, NEON_FRSQRTS, NEON_FRSQRTS_SCALAR),
    (fabd, NEON_FABD, NEON_FABD_SCALAR),
    (facge, NEON_FACGE, NEON_FACGE_SCALAR),
    (facgt, NEON_FACGT, NEON_FACGT_SCALAR),
    (fcmeq, NEON_FCMEQ, NEON_FCMEQ_SCALAR),
    (fcmge, NEON_FCMGE, NEON_FCMGE_SCALAR),
    (fcmgt, NEON_FCMGT, NEON_FCMGT_SCALAR),
);

macro_rules! define_neon_fp_3_same_vector_only {
    ($(($name:ident, $vec_op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
                    debug_assert!(vd.is_vector());
                    debug_assert!(
                        vd.is_2s() || vd.is_2d() || vd.is_4s() || vd.is_4h() || vd.is_8h()
                    );
                    self.neon_fp_3_same(vd, vn, vm, $vec_op);
                }
            )*
        }
    };
}

define_neon_fp_3_same_vector_only!(
    (faddp, NEON_FADDP),
    (fmaxp, NEON_FMAXP),
    (fminp, NEON_FMINP),
    (fmaxnmp, NEON_FMAXNMP),
    (fminnmp, NEON_FMINNMP),
);

macro_rules! define_neon_fp_2_reg_misc {
    ($(($name:ident, $vec_op:ident, $sca_op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister) {
                    if vd.is_scalar() {
                        debug_assert!(vd.is_1s() || vd.is_1d());
                        self.neon_fp_2_reg_misc(vd, vn, $sca_op);
                    } else {
                        self.neon_fp_2_reg_misc_zero(vd, vn, $vec_op);
                    }
                }
            )*
        }
    };
}

define_neon_fp_2_reg_misc!(
    (fabs, NEON_FABS_V, FABS),
    (fneg, NEON_FNEG_V, FNEG),
    (fsqrt, NEON_FSQRT_V, FSQRT),
    (frintn, NEON_FRINTN, FRINTN),
    (frinta, NEON_FRINTA, FRINTA),
    (frintp, NEON_FRINTP, FRINTP),
    (frintm, NEON_FRINTM, FRINTM),
    (frintx, NEON_FRINTX, FRINTX),
    (frintz, NEON_FRINTZ, FRINTZ),
    (frinti, NEON_FRINTI, FRINTI),
    (frsqrte, NEON_FRSQRTE, NEON_FRSQRTE_SCALAR),
    (frecpe, NEON_FRECPE, NEON_FRECPE_SCALAR),
);

macro_rules! define_neon_fp_convert {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister) {
                    self.neon_fp_2_reg_misc_zero(vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_fp_convert!(
    (fcvtns, NEON_FCVTNS_V),
    (fcvtnu, NEON_FCVTNU_V),
    (fcvtps, NEON_FCVTPS_V),
    (fcvtpu, NEON_FCVTPU_V),
    (fcvtms, NEON_FCVTMS_V),
    (fcvtmu, NEON_FCVTMU_V),
    (fcvtas, NEON_FCVTAS_V),
    (fcvtau, NEON_FCVTAU_V),
);

impl Assembler {
    pub fn fmulx<T: Into<VOperand>>(&mut self, vd: VRegister, vn: VRegister, operand: T) {
        let operand = operand.into();
        let vm = operand.reg();
        if operand.has_lane() {
            self.neon_fp_by_element(vd, vn, vm, operand.index(), NEON_FMULX_BYELEMENT);
            return;
        }
        let op;
        if vd.is_scalar() {
            debug_assert!(vd.is_1s() || vd.is_1d());
            op = NEON_FMULX_SCALAR;
        } else {
            debug_assert!(vd.is_vector());
            op = NEON_FMULX;
        }
        self.neon_fp_3_same(vd, vn, vm, op);
    }
}

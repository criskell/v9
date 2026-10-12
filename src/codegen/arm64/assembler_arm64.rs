use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::instructions_arm64::Instruction;
use crate::codegen::arm64::register_arm64::*;
use crate::codegen::assembler::AssemblerBase;
use crate::codegen::label::{Distance, Label};
use crate::utils::utils::{
    checked_truncate_to_int19, checked_truncate_to_int26, checked_truncate_to_int9, is_int9,
    is_uint12, is_uint16, is_uint2, is_uint5, is_uint6, is_uint7,
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

    fn set_instruction_at(&mut self, offset: i32, instr: Instruction) {
        self.base
            .write_bytes_at(offset, &instr.instruction_bits().to_le_bytes());
    }

    fn emit(&mut self, instruction: Instr) {
        self.base.emit_bytes(&instruction.to_le_bytes());
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

    fn rd(rd: CPURegister) -> Instr {
        debug_assert_ne!(rd.code(), K_SP_REG_INTERNAL_CODE);
        (rd.code() as Instr) << RD_OFFSET
    }

    fn rn(rn: CPURegister) -> Instr {
        debug_assert_ne!(rn.code(), K_SP_REG_INTERNAL_CODE);
        (rn.code() as Instr) << RN_OFFSET
    }

    fn rm(rm: CPURegister) -> Instr {
        debug_assert_ne!(rm.code(), K_SP_REG_INTERNAL_CODE);
        (rm.code() as Instr) << RM_OFFSET
    }

    fn rt(rt: CPURegister) -> Instr {
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

    fn cond(cond: Condition) -> Instr {
        (cond as Instr) << CONDITION_BRANCH_OFFSET
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
        if is_uint12(imm as i64) {
            (imm as Instr) << IMM_ADD_SUB_OFFSET
        } else {
            (((imm >> 12) as Instr) << IMM_ADD_SUB_OFFSET) | (1 << SHIFT_ADD_SUB_OFFSET)
        }
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

    fn appropriate_zero_reg_for(reg: CPURegister) -> Register {
        if reg.is_64_bits() {
            XZR
        } else {
            WZR
        }
    }

    fn load_op_for(rt: CPURegister) -> Instr {
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

    fn store_op_for(rt: CPURegister) -> Instr {
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
        let instruction = B_COND | Assembler::imm_cond_branch(imm19) | Assembler::cond(cond);
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

    pub fn add<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, ADD);
    }

    pub fn adds<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub(rd, rn, operand.into(), FlagsUpdate::SetFlags, ADD);
    }

    pub fn cmn<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.adds(zr, rn, operand);
    }

    pub fn sub<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, SUB);
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

    pub fn and_<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), AND);
    }

    pub fn orr<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), ORR);
    }

    pub fn eor<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), EOR);
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

    pub fn ldr(&mut self, rt: CPURegister, src: MemOperand) {
        self.load_store(rt, src, Assembler::load_op_for(rt));
    }

    pub fn str(&mut self, rt: CPURegister, src: MemOperand) {
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
        debug_assert!(operand.is_shifted_register());
        debug_assert_eq!(operand.reg().size_in_bits(), rd.size_in_bits());
        let dp_op = op | LOGICAL_SHIFTED_FIXED;
        self.data_proc_shifted_register(rd, rn, operand, FlagsUpdate::LeaveFlags, dp_op);
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
        (extend as Instr) << 13
    }

    fn imm_extend_shift(left_shift: u32) -> Instr {
        debug_assert!(left_shift <= 4);
        left_shift << 10
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

    fn load_store(&mut self, rt: CPURegister, addr: MemOperand, op: Instr) {
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
            debug_assert_ne!(rt, addr.base());
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
        debug_assert!(shift_amount <= 1);
        shift_amount << 12
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

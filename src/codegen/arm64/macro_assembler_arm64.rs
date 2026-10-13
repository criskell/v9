#![allow(non_upper_case_globals)]

use crate::codegen::arm64::assembler_arm64::{Assembler, LabelId, MemOperand, Operand};
use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::instructions_arm64::Instruction;
use crate::codegen::arm64::register_arm64::*;
use crate::codegen::arm64::reglist_arm64::CPURegList;
use crate::codegen::arm64::utils_arm64::count_leading_zeros;
use crate::utils::utils::{is_int32, is_uint32};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum BranchType {
    IntegerEq = Condition::Eq as isize,
    IntegerNe = Condition::Ne as isize,
    IntegerHs = Condition::Hs as isize,
    IntegerLo = Condition::Lo as isize,
    IntegerMi = Condition::Mi as isize,
    IntegerPl = Condition::Pl as isize,
    IntegerVs = Condition::Vs as isize,
    IntegerVc = Condition::Vc as isize,
    IntegerHi = Condition::Hi as isize,
    IntegerLs = Condition::Ls as isize,
    IntegerGe = Condition::Ge as isize,
    IntegerLt = Condition::Lt as isize,
    IntegerGt = Condition::Gt as isize,
    IntegerLe = Condition::Le as isize,
    IntegerAl = Condition::Al as isize,
    IntegerNv = Condition::Nv as isize,

    Always,
    Never,
    RegZero,
    RegNotZero,
    RegBitClear,
    RegBitSet,
}

pub const kBranchTypeFirstCondition: BranchType = BranchType::IntegerEq;
pub const kBranchTypeLastCondition: BranchType = BranchType::IntegerNv;
pub const kBranchTypeFirstUsingReg: BranchType = BranchType::RegZero;
pub const kBranchTypeFirstUsingBit: BranchType = BranchType::RegBitClear;

pub fn invert_branch_type(branch_type: BranchType) -> BranchType {
    if kBranchTypeFirstCondition <= branch_type && branch_type <= kBranchTypeLastCondition {
        BranchType::from_isize(negate_condition(branch_type.to_condition()) as isize)
    } else {
        BranchType::from_isize(branch_type as isize ^ 1)
    }
}

impl BranchType {
    pub fn to_condition(self) -> Condition {
        debug_assert!(kBranchTypeFirstCondition <= self && self <= kBranchTypeLastCondition);
        condition_from_bits(self as u32)
    }

    fn from_isize(value: isize) -> BranchType {
        match value {
            x if x == BranchType::IntegerEq as isize => BranchType::IntegerEq,
            x if x == BranchType::IntegerNe as isize => BranchType::IntegerNe,
            x if x == BranchType::IntegerHs as isize => BranchType::IntegerHs,
            x if x == BranchType::IntegerLo as isize => BranchType::IntegerLo,
            x if x == BranchType::IntegerMi as isize => BranchType::IntegerMi,
            x if x == BranchType::IntegerPl as isize => BranchType::IntegerPl,
            x if x == BranchType::IntegerVs as isize => BranchType::IntegerVs,
            x if x == BranchType::IntegerVc as isize => BranchType::IntegerVc,
            x if x == BranchType::IntegerHi as isize => BranchType::IntegerHi,
            x if x == BranchType::IntegerLs as isize => BranchType::IntegerLs,
            x if x == BranchType::IntegerGe as isize => BranchType::IntegerGe,
            x if x == BranchType::IntegerLt as isize => BranchType::IntegerLt,
            x if x == BranchType::IntegerGt as isize => BranchType::IntegerGt,
            x if x == BranchType::IntegerLe as isize => BranchType::IntegerLe,
            x if x == BranchType::IntegerAl as isize => BranchType::IntegerAl,
            x if x == BranchType::IntegerNv as isize => BranchType::IntegerNv,
            x if x == BranchType::Always as isize => BranchType::Always,
            x if x == BranchType::Never as isize => BranchType::Never,
            x if x == BranchType::RegZero as isize => BranchType::RegZero,
            x if x == BranchType::RegNotZero as isize => BranchType::RegNotZero,
            x if x == BranchType::RegBitClear as isize => BranchType::RegBitClear,
            x if x == BranchType::RegBitSet as isize => BranchType::RegBitSet,
            _ => unreachable!(),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PreShiftImmMode {
    NoShift,
    LimitShiftForSP,
    AnyShift,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiscardMoveMode {
    DontDiscardForSameWReg,
    DiscardForSameWReg,
}

pub struct UseScratchRegisterScope {
    old_available: u64,
    old_availablefp: u64,
}

impl UseScratchRegisterScope {
    pub fn new(masm: &MacroAssembler) -> UseScratchRegisterScope {
        UseScratchRegisterScope {
            old_available: masm.tmp_list().bits(),
            old_availablefp: masm.fptmp_list().bits(),
        }
    }

    pub fn close(self, masm: &mut MacroAssembler) {
        masm.tmp_list_mut().set_bits(self.old_available);
        masm.fptmp_list_mut().set_bits(self.old_availablefp);
    }

    pub fn acquire_x(&mut self, masm: &mut MacroAssembler) -> Register {
        let code = UseScratchRegisterScope::acquire_next_available(masm.tmp_list_mut()).code();
        Register::x_reg_from_code(code as u32)
    }

    pub fn acquire_w(&mut self, masm: &mut MacroAssembler) -> Register {
        let code = UseScratchRegisterScope::acquire_next_available(masm.tmp_list_mut()).code();
        Register::w_reg_from_code(code as u32)
    }

    pub fn acquire_d(&mut self, masm: &mut MacroAssembler) -> VRegister {
        let code =
            UseScratchRegisterScope::acquire_next_available(masm.fptmp_list_mut()).code();
        VRegister::d_reg_from_code(code as u32)
    }

    pub fn acquire_s(&mut self, masm: &mut MacroAssembler) -> VRegister {
        let code =
            UseScratchRegisterScope::acquire_next_available(masm.fptmp_list_mut()).code();
        VRegister::s_reg_from_code(code as u32)
    }

    pub fn acquire_same_size_as(
        &mut self,
        masm: &mut MacroAssembler,
        reg: Register,
    ) -> Register {
        let code = UseScratchRegisterScope::acquire_next_available(masm.tmp_list_mut()).code();
        Register::create(code, reg.size_in_bits())
    }

    pub fn can_acquire(masm: &MacroAssembler) -> bool {
        !masm.tmp_list().is_empty()
    }

    fn acquire_next_available(available: &mut CPURegList) -> CPURegister {
        assert!(!available.is_empty());
        let result = available.pop_lowest_index();
        debug_assert!(!result.aliases(xzr.into()) && !result.aliases(sp.into()));
        result
    }
}

pub struct MacroAssembler {
    asm: Assembler,
    tmp_list: CPURegList,
    fptmp_list: CPURegList,
    allow_macro_instructions: bool,
}

impl Default for MacroAssembler {
    fn default() -> MacroAssembler {
        MacroAssembler::new()
    }
}

impl MacroAssembler {
    pub fn new() -> MacroAssembler {
        MacroAssembler {
            asm: Assembler::new(),
            tmp_list: MacroAssembler::default_tmp_list(),
            fptmp_list: MacroAssembler::default_fptmp_list(),
            allow_macro_instructions: true,
        }
    }

    pub fn default_tmp_list() -> CPURegList {
        CPURegList::new(&[ip0.into(), ip1.into()])
    }

    pub fn default_fptmp_list() -> CPURegList {
        CPURegList::new(&[fp_scratch1.into(), fp_scratch2.into()])
    }

    pub fn tmp_list(&self) -> &CPURegList {
        &self.tmp_list
    }

    pub fn tmp_list_mut(&mut self) -> &mut CPURegList {
        &mut self.tmp_list
    }

    pub fn fptmp_list(&self) -> &CPURegList {
        &self.fptmp_list
    }

    pub fn fptmp_list_mut(&mut self) -> &mut CPURegList {
        &mut self.fptmp_list
    }

    pub fn allow_macro_instructions(&self) -> bool {
        self.allow_macro_instructions
    }

    pub fn set_allow_macro_instructions(&mut self, value: bool) {
        self.allow_macro_instructions = value;
    }

    pub fn asm(&mut self) -> &mut Assembler {
        &mut self.asm
    }

    pub fn pc_offset(&self) -> i32 {
        self.asm.pc_offset()
    }

    pub fn buffer_start(&self) -> &[u8] {
        self.asm.buffer_start()
    }

    fn need_extra_instructions_or_register_branch(
        &mut self,
        label: LabelId,
        branch_type: ImmBranchType,
    ) -> bool {
        debug_assert!(
            (branch_type == ImmBranchType::CondBranchType)
                || (branch_type == ImmBranchType::CompareBranchType)
                || (branch_type == ImmBranchType::TestBranchType)
        );

        let mut need_longer_range = false;
        if self.asm.label(label).is_bound() || self.asm.label(label).is_linked() {
            let distance = (self.asm.label(label).pos() - self.pc_offset()) as i64;
            need_longer_range = !Instruction::is_valid_imm_pc_offset(branch_type, distance);
        }
        if !need_longer_range && !self.asm.label(label).is_bound() {
            self.asm
                .register_pending_unresolved_branch(branch_type, label);
        }
        need_longer_range
    }

    pub fn b(&mut self, label: LabelId) {
        self.asm.b(label);
        self.asm
            .check_veneer_pool(false, false, Assembler::kVeneerDistanceMargin);
    }

    pub fn b_cond(&mut self, label: LabelId, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        let need_extra_instructions =
            self.need_extra_instructions_or_register_branch(label, ImmBranchType::CondBranchType);
        if need_extra_instructions {
            let done = self.asm.new_label();
            self.asm.b_cond(done, negate_condition(cond));
            self.b(label);
            self.asm.bind(done);
        } else {
            self.asm.b_cond(label, cond);
        }
    }

    pub fn b_branch_type(
        &mut self,
        label: LabelId,
        branch_type: BranchType,
        reg: Register,
        bit: i32,
    ) {
        debug_assert!(
            (reg == NoReg || branch_type >= kBranchTypeFirstUsingReg)
                && (bit == -1 || branch_type >= kBranchTypeFirstUsingBit)
        );
        if kBranchTypeFirstCondition <= branch_type && branch_type <= kBranchTypeLastCondition {
            self.b_cond(label, branch_type.to_condition());
        } else {
            match branch_type {
                BranchType::Always => self.b(label),
                BranchType::Never => (),
                BranchType::RegZero => self.cbz(reg, label),
                BranchType::RegNotZero => self.cbnz(reg, label),
                BranchType::RegBitClear => self.tbz(reg, bit as u32, label),
                BranchType::RegBitSet => self.tbnz(reg, bit as u32, label),
                _ => unreachable!(),
            }
        }
    }

    pub fn cbnz(&mut self, rt: Register, label: LabelId) {
        let need_extra_instructions = self
            .need_extra_instructions_or_register_branch(label, ImmBranchType::CompareBranchType);
        if need_extra_instructions {
            let done = self.asm.new_label();
            self.asm.cbz(rt, done);
            self.b(label);
            self.asm.bind(done);
        } else {
            self.asm.cbnz(rt, label);
        }
    }

    pub fn cbz(&mut self, rt: Register, label: LabelId) {
        let need_extra_instructions = self
            .need_extra_instructions_or_register_branch(label, ImmBranchType::CompareBranchType);
        if need_extra_instructions {
            let done = self.asm.new_label();
            self.asm.cbnz(rt, done);
            self.b(label);
            self.asm.bind(done);
        } else {
            self.asm.cbz(rt, label);
        }
    }

    pub fn tbnz(&mut self, rt: Register, bit_pos: u32, label: LabelId) {
        let need_extra_instructions =
            self.need_extra_instructions_or_register_branch(label, ImmBranchType::TestBranchType);
        if need_extra_instructions {
            let done = self.asm.new_label();
            self.asm.tbz(rt, bit_pos, done);
            self.b(label);
            self.asm.bind(done);
        } else {
            self.asm.tbnz(rt, bit_pos, label);
        }
    }

    pub fn tbz(&mut self, rt: Register, bit_pos: u32, label: LabelId) {
        let need_extra_instructions =
            self.need_extra_instructions_or_register_branch(label, ImmBranchType::TestBranchType);
        if need_extra_instructions {
            let done = self.asm.new_label();
            self.asm.tbnz(rt, bit_pos, done);
            self.b(label);
            self.asm.bind(done);
        } else {
            self.asm.tbz(rt, bit_pos, label);
        }
    }

    pub fn mov_immediate(&mut self, rd: Register, imm: u64) {
        debug_assert!(is_uint32(imm as i64) || is_int32(imm as i64) || rd.is_64_bits());
        debug_assert!(!rd.is_zero());

        if self.try_one_instr_move_immediate(rd, imm as i64) {
            return;
        }
        let reg_size = rd.size_in_bits();

        let mut ignored_halfword = 0u64;
        let mut invert_move = false;
        if MacroAssembler::count_set_half_words(imm, reg_size)
            > MacroAssembler::count_set_half_words(!imm, reg_size)
        {
            ignored_halfword = 0xFFFF;
            invert_move = true;
        }

        let mut temps = UseScratchRegisterScope::new(self);
        let temp = if rd.is_sp() {
            temps.acquire_same_size_as(self, rd)
        } else {
            rd
        };

        debug_assert_eq!(reg_size % 16, 0);
        let mut first_mov_done = false;
        for i in 0..(reg_size / 16) {
            let imm16 = (imm >> (16 * i)) & 0xFFFF;
            if imm16 != ignored_halfword {
                if !first_mov_done {
                    if invert_move {
                        self.asm.movn(temp, (!imm16) & 0xFFFF, (16 * i) as i32);
                    } else {
                        self.asm.movz(temp, imm16, (16 * i) as i32);
                    }
                    first_mov_done = true;
                } else {
                    self.asm.movk(temp, imm16, (16 * i) as i32);
                }
            }
        }
        debug_assert!(first_mov_done);

        if rd.is_sp() {
            self.asm.mov(rd, temp);
        }
        temps.close(self);
    }

    pub fn mov<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        self.mov_with_discard_mode(rd, operand.into(), DiscardMoveMode::DontDiscardForSameWReg);
    }

    pub fn mov_with_discard_mode(
        &mut self,
        rd: Register,
        operand: Operand,
        discard_mode: DiscardMoveMode,
    ) {
        debug_assert!(!rd.is_zero());

        let mut temps = UseScratchRegisterScope::new(self);
        let dst = if rd.is_sp() {
            temps.acquire_same_size_as(self, rd)
        } else {
            rd
        };

        if operand.is_immediate() {
            self.mov_immediate(dst, operand.immediate_value() as u64);
        } else if operand.is_shifted_register() && (operand.shift_amount() != 0) {
            self.emit_shift(dst, operand.reg(), operand.shift(), operand.shift_amount());
        } else if operand.is_extended_register() {
            self.emit_extend_shift(
                dst,
                operand.reg(),
                operand.extend(),
                operand.shift_amount(),
            );
        } else {
            if rd != operand.reg()
                || (rd.is_32_bits()
                    && (discard_mode == DiscardMoveMode::DontDiscardForSameWReg))
            {
                self.asm.mov(rd, operand.reg());
            }
            temps.close(self);
            return;
        }

        if rd.is_sp() {
            self.asm.mov(rd, dst);
        }
        temps.close(self);
    }

    pub fn mvn<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let operand = operand.into();
        if operand.is_immediate() {
            self.mov_immediate(rd, !(operand.immediate_value() as u64));
        } else {
            let zr = MacroAssembler::appropriate_zero_reg_for(rd);
            self.logical_macro(rd, zr, operand, ORN);
        }
    }

    fn count_set_half_words(imm: u64, reg_size: u32) -> u32 {
        debug_assert_eq!(reg_size % 16, 0);
        let mut count = 0;
        for i in 0..(reg_size / 16) {
            if ((imm >> (16 * i)) & 0xFFFF) != 0 {
                count += 1;
            }
        }
        count
    }

    fn appropriate_zero_reg_for(reg: impl Into<CPURegister>) -> Register {
        let reg = reg.into();
        if reg.is_64_bits() {
            xzr
        } else {
            wzr
        }
    }

    pub fn try_one_instr_move_immediate(&mut self, dst: Register, imm: i64) -> bool {
        let reg_size = dst.size_in_bits();
        if MacroAssembler::is_imm_movz(imm as u64, reg_size) && !dst.is_sp() {
            self.asm.movz(dst, imm as u64, -1);
            return true;
        }
        if MacroAssembler::is_imm_movn(imm as u64, reg_size) && !dst.is_sp() {
            let value = if dst.is_64_bits() {
                !(imm as u64)
            } else {
                (!(imm as u64)) & (kWRegMask as u64)
            };
            self.asm.movn(dst, value, -1);
            return true;
        }
        let mut n = 0;
        let mut imm_s = 0;
        let mut imm_r = 0;
        if Assembler::is_imm_logical(imm as u64, reg_size, &mut n, &mut imm_s, &mut imm_r) {
            let zr = MacroAssembler::appropriate_zero_reg_for(dst);
            self.asm
                .logical_immediate_for_macro(dst, zr, n, imm_s, imm_r, ORR);
            return true;
        }
        false
    }

    pub fn is_imm_movz(imm: u64, reg_size: u32) -> bool {
        debug_assert!(
            (reg_size == kXRegSizeInBits as u32)
                || (reg_size == kWRegSizeInBits as u32)
        );
        MacroAssembler::count_set_half_words(imm, reg_size) <= 1
    }

    pub fn is_imm_movn(imm: u64, reg_size: u32) -> bool {
        MacroAssembler::is_imm_movz(!imm, reg_size)
    }

    pub fn move_immediate_for_shifted_op(
        &mut self,
        dst: Register,
        imm: i64,
        mode: PreShiftImmMode,
    ) -> Operand {
        let reg_size = dst.size_in_bits();
        if self.try_one_instr_move_immediate(dst, imm) {
            return Operand::from(dst);
        }

        let mut shift_low = if reg_size == 64 {
            (imm as u64).trailing_zeros()
        } else {
            (imm as u32).trailing_zeros()
        };
        if mode == PreShiftImmMode::LimitShiftForSP {
            shift_low = std::cmp::min(shift_low, 4);
        }
        let imm_low = imm >> shift_low;

        let shift_high = count_leading_zeros(imm as u64, reg_size as i32) as u32;
        let imm_high = ((imm as u64) << shift_high) | ((1u64 << shift_high) - 1);

        if (mode != PreShiftImmMode::NoShift)
            && self.try_one_instr_move_immediate(dst, imm_low)
        {
            return Operand::with_shift(dst, Shift::Lsl, shift_low);
        }
        if (mode == PreShiftImmMode::AnyShift)
            && self.try_one_instr_move_immediate(dst, imm_high as i64)
        {
            return Operand::with_shift(dst, Shift::Lsr, shift_high);
        }
        self.mov_immediate(dst, imm as u64);
        Operand::from(dst)
    }

    fn emit_shift(&mut self, rd: Register, rn: Register, shift: Shift, shift_amount: u32) {
        match shift {
            Shift::Lsl => self.asm.lsl(rd, rn, shift_amount),
            Shift::Lsr => self.asm.lsr(rd, rn, shift_amount),
            Shift::Asr => self.asm.asr(rd, rn, shift_amount),
            Shift::Ror => self.asm.ror(rd, rn, shift_amount),
            _ => unreachable!(),
        }
    }

    fn emit_extend_shift(
        &mut self,
        rd: Register,
        rn: Register,
        extend: Extend,
        left_shift: u32,
    ) {
        debug_assert!(rd.size_in_bits() >= rn.size_in_bits());
        let reg_size = rd.size_in_bits();
        let rn_ = Register::create(rn.code(), rd.size_in_bits());
        let high_bit = (8u32 << ((extend as u32) & 0x3)) - 1;
        let non_shift_bits = (reg_size - left_shift) & (reg_size - 1);

        if (non_shift_bits > high_bit) || (non_shift_bits == 0) {
            match extend {
                Extend::Uxtb | Extend::Uxth | Extend::Uxtw => {
                    self.asm.ubfm(rd, rn_, non_shift_bits, high_bit)
                }
                Extend::Sxtb | Extend::Sxth | Extend::Sxtw => {
                    self.asm.sbfm(rd, rn_, non_shift_bits, high_bit)
                }
                Extend::Uxtx | Extend::Sxtx => {
                    debug_assert_eq!(rn.size_in_bits(), kXRegSizeInBits as u32);
                    self.asm.lsl(rd, rn_, left_shift)
                }
                Extend::NoExtend => unreachable!(),
            }
        } else {
            match extend {
                Extend::Uxtb | Extend::Uxth | Extend::Uxtw => {
                    self.asm.ubfiz(rd, rn_, left_shift, high_bit + 1)
                }
                Extend::Sxtb | Extend::Sxth | Extend::Sxtw => {
                    self.asm.sbfiz(rd, rn_, left_shift, high_bit + 1)
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn add<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_macro(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, ADD);
    }

    pub fn adds<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_macro(rd, rn, operand.into(), FlagsUpdate::SetFlags, ADD);
    }

    pub fn sub<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_macro(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, SUB);
    }

    pub fn subs<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_macro(rd, rn, operand.into(), FlagsUpdate::SetFlags, SUB);
    }

    pub fn cmp<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = MacroAssembler::appropriate_zero_reg_for(rn);
        self.subs(zr, rn, operand);
    }

    pub fn cmn<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = MacroAssembler::appropriate_zero_reg_for(rn);
        self.adds(zr, rn, operand);
    }

    pub fn neg<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let operand = operand.into();
        if operand.is_immediate() {
            self.mov_immediate(rd, (-operand.immediate_value()) as u64);
        } else {
            let zr = MacroAssembler::appropriate_zero_reg_for(rd);
            self.sub(rd, zr, operand);
        }
    }

    pub fn negs<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = MacroAssembler::appropriate_zero_reg_for(rd);
        self.subs(rd, zr, operand);
    }

    pub fn and_<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), AND);
    }

    pub fn ands<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), ANDS);
    }

    pub fn tst<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = MacroAssembler::appropriate_zero_reg_for(rn);
        self.logical_macro(zr, rn, operand.into(), ANDS);
    }

    pub fn bic<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), BIC);
    }

    pub fn bics<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), BICS);
    }

    pub fn orr<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), ORR);
    }

    pub fn orn<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), ORN);
    }

    pub fn eor<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), EOR);
    }

    pub fn eon<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical_macro(rd, rn, operand.into(), EON);
    }

    fn add_sub_macro(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        if operand.is_zero()
            && rd == rn
            && rd.is_64_bits()
            && rn.is_64_bits()
            && (s == FlagsUpdate::LeaveFlags)
        {
            return;
        }

        if (operand.is_immediate() && !Assembler::is_imm_add_sub(operand.immediate_value()))
            || (rn.is_zero() && !operand.is_shifted_register())
            || (operand.is_shifted_register() && (operand.shift() == Shift::Ror))
        {
            let mut temps = UseScratchRegisterScope::new(self);
            let temp = temps.acquire_same_size_as(self, rn);
            if operand.is_immediate() {
                let mode = if rd == sp {
                    if s == FlagsUpdate::SetFlags {
                        PreShiftImmMode::NoShift
                    } else {
                        PreShiftImmMode::LimitShiftForSP
                    }
                } else if rn == sp {
                    PreShiftImmMode::LimitShiftForSP
                } else {
                    PreShiftImmMode::AnyShift
                };
                let imm_operand =
                    self.move_immediate_for_shifted_op(temp, operand.immediate_value(), mode);
                self.asm.add_sub_for_macro(rd, rn, imm_operand, s, op);
            } else {
                self.mov(temp, operand);
                self.asm
                    .add_sub_for_macro(rd, rn, Operand::from(temp), s, op);
            }
            temps.close(self);
        } else {
            self.asm.add_sub_for_macro(rd, rn, operand, s, op);
        }
    }

    fn logical_macro(&mut self, rd: Register, rn: Register, operand: Operand, op: Instr) {
        let mut temps = UseScratchRegisterScope::new(self);

        if operand.is_immediate() {
            let mut immediate = operand.immediate_value();
            let reg_size = rd.size_in_bits();
            let mut op = op;

            if (op & NOT) == NOT {
                op &= !NOT;
                immediate = !immediate;
            }

            if rd.is_32_bits() {
                immediate &= kWRegMask;
            }

            debug_assert!(rd.is_64_bits() || is_uint32(immediate));

            if immediate == 0 {
                match op {
                    AND => {
                        self.mov_immediate(rd, 0);
                        temps.close(self);
                        return;
                    }
                    ORR | EOR => {
                        self.mov(rd, rn);
                        temps.close(self);
                        return;
                    }
                    ANDS | BICS => {}
                    _ => unreachable!(),
                }
            } else if (rd.is_64_bits() && (immediate == -1))
                || (rd.is_32_bits() && (immediate == 0xFFFFFFFF))
            {
                match op {
                    AND => {
                        self.mov(rd, rn);
                        temps.close(self);
                        return;
                    }
                    ORR => {
                        self.mov_immediate(rd, immediate as u64);
                        temps.close(self);
                        return;
                    }
                    EOR => {
                        self.mvn(rd, rn);
                        temps.close(self);
                        return;
                    }
                    ANDS | BICS => {}
                    _ => unreachable!(),
                }
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
                self.asm
                    .logical_immediate_for_macro(rd, rn, n, imm_s, imm_r, op);
            } else {
                let temp = temps.acquire_same_size_as(self, rn);
                let mode = if rn == sp {
                    PreShiftImmMode::NoShift
                } else {
                    PreShiftImmMode::AnyShift
                };
                let imm_operand = self.move_immediate_for_shifted_op(temp, immediate, mode);
                if rd.is_sp() {
                    self.asm.logical_for_macro(temp, rn, imm_operand, op);
                    self.asm.mov(sp, temp);
                } else {
                    self.asm.logical_for_macro(rd, rn, imm_operand, op);
                }
            }
        } else if operand.is_extended_register() {
            debug_assert!(operand.reg().size_in_bits() <= rd.size_in_bits());
            debug_assert!(operand.shift_amount() <= 4);
            let temp = temps.acquire_same_size_as(self, rn);
            self.emit_extend_shift(
                temp,
                operand.reg(),
                operand.extend(),
                operand.shift_amount(),
            );
            self.asm.logical_for_macro(rd, rn, Operand::from(temp), op);
        } else {
            debug_assert!(operand.is_shifted_register());
            self.asm.logical_for_macro(rd, rn, operand, op);
        }
        temps.close(self);
    }

    pub fn push<T: Into<CPURegister> + Copy>(&mut self, regs: &[T]) {
        debug_assert!(!regs.is_empty() && regs.len() <= 4);
        let regs: Vec<CPURegister> = regs.iter().map(|reg| (*reg).into()).collect();
        let size = regs[0].size_in_bytes() as i64;
        self.push_helper(regs.len(), size, &regs);
    }

    pub fn pop<T: Into<CPURegister> + Copy>(&mut self, regs: &[T]) {
        debug_assert!(!regs.is_empty() && regs.len() <= 4);
        let regs: Vec<CPURegister> = regs.iter().map(|reg| (*reg).into()).collect();
        let size = regs[0].size_in_bytes() as i64;
        self.pop_helper(regs.len(), size, &regs);
    }

    fn push_helper(&mut self, count: usize, size: i64, src: &[CPURegister]) {
        match count {
            1 => self
                .asm
                .str(src[0], MemOperand::new(sp, -size, AddrMode::PreIndex)),
            2 => self.asm.stp(
                src[1],
                src[0],
                MemOperand::new(sp, -2 * size, AddrMode::PreIndex),
            ),
            3 => {
                self.asm.stp(
                    src[2],
                    src[1],
                    MemOperand::new(sp, -3 * size, AddrMode::PreIndex),
                );
                self.asm
                    .str(src[0], MemOperand::new(sp, 2 * size, AddrMode::Offset));
            }
            4 => {
                self.asm.stp(
                    src[3],
                    src[2],
                    MemOperand::new(sp, -4 * size, AddrMode::PreIndex),
                );
                self.asm.stp(
                    src[1],
                    src[0],
                    MemOperand::new(sp, 2 * size, AddrMode::Offset),
                );
            }
            _ => unreachable!(),
        }
    }

    fn pop_helper(&mut self, count: usize, size: i64, dst: &[CPURegister]) {
        match count {
            1 => self
                .asm
                .ldr(dst[0], MemOperand::new(sp, size, AddrMode::PostIndex)),
            2 => self.asm.ldp(
                dst[0],
                dst[1],
                MemOperand::new(sp, 2 * size, AddrMode::PostIndex),
            ),
            3 => {
                self.asm
                    .ldr(dst[2], MemOperand::new(sp, 2 * size, AddrMode::Offset));
                self.asm.ldp(
                    dst[0],
                    dst[1],
                    MemOperand::new(sp, 3 * size, AddrMode::PostIndex),
                );
            }
            4 => {
                self.asm.ldp(
                    dst[2],
                    dst[3],
                    MemOperand::new(sp, 2 * size, AddrMode::Offset),
                );
                self.asm.ldp(
                    dst[0],
                    dst[1],
                    MemOperand::new(sp, 4 * size, AddrMode::PostIndex),
                );
            }
            _ => unreachable!(),
        }
    }

    pub fn claim(&mut self, count: i64, unit_size: u64) {
        debug_assert!(count >= 0);
        let size = count as u64 * unit_size;
        if size == 0 {
            return;
        }
        debug_assert_eq!(size % 16, 0);
        self.sub(sp, sp, size as i64);
    }

    pub fn drop(&mut self, count: i64, unit_size: u64) {
        debug_assert!(count >= 0);
        let size = count as u64 * unit_size;
        if size == 0 {
            return;
        }
        debug_assert_eq!(size % 16, 0);
        self.add(sp, sp, size as i64);
    }

    pub fn ldr(&mut self, rt: impl Into<CPURegister> + Copy, addr: MemOperand) {
        let rt = rt.into();
        let op = Assembler::load_op_for_macro(rt);
        self.load_store_macro(rt, addr, op);
    }

    pub fn str(&mut self, rt: impl Into<CPURegister> + Copy, addr: MemOperand) {
        let rt = rt.into();
        let op = Assembler::store_op_for_macro(rt);
        self.load_store_macro(rt, addr, op);
    }

    fn load_store_macro(&mut self, rt: CPURegister, addr: MemOperand, op: Instr) {
        if addr.is_immediate_offset() {
            let offset = addr.offset();
            let size_log2 = Assembler::calc_ls_data_size_log2_for_macro(op);
            if Assembler::is_imm_ls_scaled(offset, size_log2)
                || Assembler::is_imm_ls_unscaled(offset)
            {
                self.asm.load_store_for_macro(rt, addr, op);
                return;
            }
        } else if addr.is_register_offset() {
            self.asm.load_store_for_macro(rt, addr, op);
            return;
        }
        self.load_store_macro_complex(rt, addr, op);
    }

    fn load_store_macro_complex(&mut self, rt: CPURegister, addr: MemOperand, op: Instr) {
        let offset = addr.offset();
        let is_imm_unscaled = Assembler::is_imm_ls_unscaled(offset);
        if addr.is_register_offset()
            || (is_imm_unscaled && (addr.is_post_index() || addr.is_pre_index()))
        {
            self.asm.load_store_for_macro(rt, addr, op);
        } else if addr.is_immediate_offset() {
            let mut temps = UseScratchRegisterScope::new(self);
            let temp = temps.acquire_same_size_as(self, addr.base());
            self.mov_immediate(temp, offset as u64);
            self.asm.load_store_for_macro(
                rt,
                MemOperand::with_shift(addr.base(), temp, Shift::Lsl, 0),
                op,
            );
            temps.close(self);
        } else if addr.is_post_index() {
            self.asm.load_store_for_macro(
                rt,
                MemOperand::new(addr.base(), 0, AddrMode::Offset),
                op,
            );
            self.add(addr.base(), addr.base(), offset);
        } else {
            debug_assert!(addr.is_pre_index());
            self.add(addr.base(), addr.base(), offset);
            self.asm.load_store_for_macro(
                rt,
                MemOperand::new(addr.base(), 0, AddrMode::Offset),
                op,
            );
        }
    }
}

macro_rules! define_two_register_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, rn: Register) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$op(rd, rn);
                }
            )*
        }
    };
}

define_two_register_macro! {
    cls, cls;
    clz, clz;
    ctz, ctz;
    abs, abs_register;
    cnt, cnt_register;
    rbit, rbit;
    rev, rev;
    rev16, rev16;
    rev32, rev32;
    sxtb, sxtb;
    sxth, sxth;
    sxtw, sxtw;
    uxtb, uxtb;
    uxth, uxth;
    uxtw, uxtw;
}

macro_rules! define_conditional_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, rn: Register, cond: Condition) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
                    self.asm.$op(rd, rn, cond);
                }
            )*
        }
    };
}

define_conditional_macro! {
    cinc, cinc;
    cinv, cinv;
    cneg, cneg;
}

macro_rules! define_conditional_select_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(
                    &mut self,
                    rd: Register,
                    rn: Register,
                    rm: Register,
                    cond: Condition,
                ) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
                    self.asm.$op(rd, rn, rm, cond);
                }
            )*
        }
    };
}

define_conditional_select_macro! {
    csel, csel;
    csinc, csinc;
    csinv, csinv;
    csneg, csneg;
}

macro_rules! define_condition_only_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, cond: Condition) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
                    self.asm.$op(rd, cond);
                }
            )*
        }
    };
}

define_condition_only_macro! {
    cset, cset;
    csetm, csetm;
}

macro_rules! define_shift_immediate_macro {
    ($($name:ident, $op:ident, $variable:ident, $variable_op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, rn: Register, shift: u32) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$op(rd, rn, shift);
                }

                pub fn $variable(&mut self, rd: Register, rn: Register, rm: Register) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$variable_op(rd, rn, rm);
                }
            )*
        }
    };
}

define_shift_immediate_macro! {
    asr, asr, asr_register, asrv;
    lsl, lsl, lsl_register, lslv;
    lsr, lsr, lsr_register, lsrv;
    ror, ror, ror_register, rorv;
}

macro_rules! define_three_register_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, rn: Register, rm: Register) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$op(rd, rn, rm);
                }
            )*
        }
    };
}

define_three_register_macro! {
    mul, mul;
    mneg, mneg;
    sdiv, sdiv;
    udiv, udiv;
    smulh, smulh;
    umulh, umulh;
    smull, smull;
    umull, umull;
}

macro_rules! define_four_register_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$op(rd, rn, rm, ra);
                }
            )*
        }
    };
}

define_four_register_macro! {
    madd, madd;
    msub, msub;
    smaddl, smaddl;
    smsubl, smsubl;
    umaddl, umaddl;
    umsubl, umsubl;
}

macro_rules! define_bitfield_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$op(rd, rn, lsb, width);
                }
            )*
        }
    };
}

define_bitfield_macro! {
    bfi, bfi;
    bfxil, bfxil;
    sbfx, sbfx;
    ubfx, ubfx;
    sbfiz, sbfiz;
    ubfiz, ubfiz;
}

macro_rules! define_fp_two_register_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, fd: VRegister, fn_: VRegister) {
                    debug_assert!(self.allow_macro_instructions());
                    self.asm.$op(fd, fn_);
                }
            )*
        }
    };
}

define_fp_two_register_macro! {
    fabs_macro, fabs;
    fneg, fneg;
    fsqrt, fsqrt;
    fcvt, fcvt;
}

macro_rules! define_fp_three_register_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister) {
                    debug_assert!(self.allow_macro_instructions());
                    self.asm.$op(fd, fn_, fm);
                }
            )*
        }
    };
}

define_fp_three_register_macro! {
    fadd, fadd;
    fsub, fsub;
    fmul, fmul;
    fdiv, fdiv;
    fmax, fmax;
    fmin, fmin;
    fmaxnm, fmaxnm;
    fminnm, fminnm;
}

macro_rules! define_fp_four_register_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(
                    &mut self,
                    fd: VRegister,
                    fn_: VRegister,
                    fm: VRegister,
                    fa: VRegister,
                ) {
                    debug_assert!(self.allow_macro_instructions());
                    self.asm.$op(fd, fn_, fm, fa);
                }
            )*
        }
    };
}

define_fp_four_register_macro! {
    fmadd, fmadd;
    fmsub, fmsub;
    fnmadd, fnmadd;
    fnmsub, fnmsub;
}

macro_rules! define_fp_convert_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, fn_: VRegister) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$op(rd, fn_);
                }
            )*
        }
    };
}

define_fp_convert_macro! {
    fcvtas, fcvtas_register;
    fcvtau, fcvtau_register;
    fcvtms, fcvtms_register;
    fcvtmu, fcvtmu_register;
    fcvtns, fcvtns_register;
    fcvtnu, fcvtnu_register;
}

macro_rules! define_fp_convert_fbits_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rd: Register, fn_: VRegister, fbits: u32) {
                    debug_assert!(self.allow_macro_instructions());
                    debug_assert!(!rd.is_zero());
                    self.asm.$op(rd, fn_, fbits);
                }
            )*
        }
    };
}

define_fp_convert_fbits_macro! {
    fcvtzs, fcvtzs;
    fcvtzu, fcvtzu;
}

impl MacroAssembler {
    fn load_store_pair_macro(
        &mut self,
        rt: CPURegister,
        rt2: CPURegister,
        addr: MemOperand,
        op: Instr,
    ) {
        if addr.is_register_offset() {
            let mut temps = UseScratchRegisterScope::new(self);
            let base = addr.base();
            let temp = temps.acquire_same_size_as(self, base);
            self.add(temp, base, addr.regoffset());
            self.asm
                .load_store_pair(rt, rt2, MemOperand::new(temp, 0, AddrMode::Offset), op);
            temps.close(self);
            return;
        }

        let offset = addr.offset();
        let size = Assembler::calc_ls_pair_data_size(op);

        if Assembler::is_imm_ls_pair(offset, size) {
            self.asm.load_store_pair(rt, rt2, addr, op);
        } else {
            let base = addr.base();
            if addr.is_immediate_offset() {
                let mut temps = UseScratchRegisterScope::new(self);
                let temp = temps.acquire_same_size_as(self, base);
                self.add(temp, base, offset);
                self.asm
                    .load_store_pair(rt, rt2, MemOperand::new(temp, 0, AddrMode::Offset), op);
                temps.close(self);
            } else if addr.is_post_index() {
                self.asm
                    .load_store_pair(rt, rt2, MemOperand::new(base, 0, AddrMode::Offset), op);
                self.add(base, base, offset);
            } else {
                debug_assert!(addr.is_pre_index());
                self.add(base, base, offset);
                self.asm
                    .load_store_pair(rt, rt2, MemOperand::new(base, 0, AddrMode::Offset), op);
            }
        }
    }

    fn conditional_compare_macro(
        &mut self,
        rn: Register,
        operand: Operand,
        nzcv: u32,
        cond: Condition,
        op: Instr,
    ) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        if (operand.is_shifted_register() && (operand.shift_amount() == 0))
            || (operand.is_immediate()
                && Assembler::is_imm_conditional_compare(operand.immediate_value()))
        {
            self.asm.conditional_compare(rn, operand, nzcv, cond, op);
        } else {
            let mut temps = UseScratchRegisterScope::new(self);
            let temp = temps.acquire_same_size_as(self, rn);
            self.mov(temp, operand);
            self.asm.conditional_compare(rn, temp.into(), nzcv, cond, op);
            temps.close(self);
        }
    }

    fn add_sub_with_carry_macro(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        let mut temps = UseScratchRegisterScope::new(self);

        if operand.is_immediate()
            || (operand.is_shifted_register() && (operand.shift() == Shift::Ror))
        {
            let temp = temps.acquire_same_size_as(self, rn);
            self.mov(temp, operand);
            self.asm.add_sub_with_carry(rd, rn, temp.into(), s, op);
        } else if operand.is_shifted_register() && (operand.shift_amount() != 0) {
            debug_assert_eq!(operand.reg().size_in_bits(), rd.size_in_bits());
            debug_assert!(operand.shift() != Shift::Ror);
            debug_assert!(crate::utils::utils::is_uintn(
                operand.shift_amount() as i64,
                if rd.size_in_bits() == kXRegSizeInBits as u32 {
                    kXRegSizeInBitsLog2 as u32
                } else {
                    kWRegSizeInBitsLog2 as u32
                }
            ));
            let temp = temps.acquire_same_size_as(self, rn);
            self.emit_shift(temp, operand.reg(), operand.shift(), operand.shift_amount());
            self.asm.add_sub_with_carry(rd, rn, temp.into(), s, op);
        } else if operand.is_extended_register() {
            debug_assert!(operand.shift_amount() <= 4);
            debug_assert!(
                operand.reg().is_64_bits()
                    || ((operand.extend() != Extend::Uxtx)
                        && (operand.extend() != Extend::Sxtx))
            );
            let temp = temps.acquire_same_size_as(self, rn);
            self.emit_extend_shift(temp, operand.reg(), operand.extend(), operand.shift_amount());
            self.asm.add_sub_with_carry(rd, rn, temp.into(), s, op);
        } else {
            self.asm.add_sub_with_carry(rd, rn, operand, s, op);
        }
        temps.close(self);
    }

    pub fn ccmp(&mut self, rn: Register, operand: impl Into<Operand>, nzcv: u32, cond: Condition) {
        debug_assert!(self.allow_macro_instructions());
        let operand = operand.into();
        if operand.is_immediate() && (operand.immediate_value() < 0) {
            let negated = Operand::from(-operand.immediate_value());
            self.conditional_compare_macro(rn, negated, nzcv, cond, CCMN);
        } else {
            self.conditional_compare_macro(rn, operand, nzcv, cond, CCMP);
        }
    }

    pub fn ccmn(&mut self, rn: Register, operand: impl Into<Operand>, nzcv: u32, cond: Condition) {
        debug_assert!(self.allow_macro_instructions());
        let operand = operand.into();
        if operand.is_immediate() && (operand.immediate_value() < 0) {
            let negated = Operand::from(-operand.immediate_value());
            self.conditional_compare_macro(rn, negated, nzcv, cond, CCMP);
        } else {
            self.conditional_compare_macro(rn, operand, nzcv, cond, CCMN);
        }
    }

    pub fn adc(&mut self, rd: Register, rn: Register, operand: impl Into<Operand>) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        self.add_sub_with_carry_macro(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, ADC);
    }

    pub fn adcs(&mut self, rd: Register, rn: Register, operand: impl Into<Operand>) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        self.add_sub_with_carry_macro(rd, rn, operand.into(), FlagsUpdate::SetFlags, ADC);
    }

    pub fn sbc(&mut self, rd: Register, rn: Register, operand: impl Into<Operand>) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        self.add_sub_with_carry_macro(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, SBC);
    }

    pub fn sbcs(&mut self, rd: Register, rn: Register, operand: impl Into<Operand>) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        self.add_sub_with_carry_macro(rd, rn, operand.into(), FlagsUpdate::SetFlags, SBC);
    }

    pub fn ngc(&mut self, rd: Register, operand: impl Into<Operand>) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        let zr = MacroAssembler::appropriate_zero_reg_for(rd);
        self.sbc(rd, zr, operand);
    }

    pub fn ngcs(&mut self, rd: Register, operand: impl Into<Operand>) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        let zr = MacroAssembler::appropriate_zero_reg_for(rd);
        self.sbcs(rd, zr, operand);
    }

    pub fn czero_x(&mut self, rd: Register, cond: Condition) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_sp() && rd.is_64_bits());
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.asm.csel(rd, xzr, rd, cond);
    }

    pub fn cmov_x(&mut self, rd: Register, rn: Register, cond: Condition) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_sp());
        debug_assert!(rd.is_64_bits() && rn.is_64_bits());
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        if rd != rn {
            self.asm.csel(rd, rn, rd, cond);
        }
    }

    pub fn csdb(&mut self) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.csdb();
    }

    pub fn cpy(&mut self, rd: Register, rs: Register, rn: Register) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(rd.is_64_bits());
        debug_assert!(rs.is_64_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(!rd.is_zero());
        debug_assert!(!rs.is_zero());
        debug_assert!(!rn.is_zero());

        self.asm.cpyp(rd, rs, rn);
        self.asm.cpym(rd, rs, rn);
        self.asm.cpye(rd, rs, rn);
    }

    pub fn set(&mut self, rd: Register, rn: Register, rs: Register) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rs.is_64_bits());
        debug_assert!(!rd.is_zero());
        debug_assert!(!rn.is_zero());

        self.asm.setp(rd, rn, rs);
        self.asm.setm(rd, rn, rs);
        self.asm.sete(rd, rn, rs);
    }

    pub fn dmb(&mut self, domain: BarrierDomain, barrier_type: BarrierType) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.dmb(domain, barrier_type);
    }

    pub fn dsb(&mut self, domain: BarrierDomain, barrier_type: BarrierType) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.dsb(domain, barrier_type);
    }

    pub fn isb(&mut self) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.isb();
    }

    pub fn debug(&mut self, message: &str, code: u32, params: Instr) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.debug(message, code, params);
    }

    pub fn extr(&mut self, rd: Register, rn: Register, rm: Register, lsb: u32) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        self.asm.extr(rd, rn, rm, lsb);
    }

    pub fn fccmp(&mut self, fn_: VRegister, fm: VRegister, nzcv: u32, cond: Condition) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.asm.fccmp(fn_, fm, nzcv, cond);
    }

    pub fn fcmp(&mut self, fn_: VRegister, fm: VRegister) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.fcmp(fn_, fm);
    }

    pub fn fcsel(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, cond: Condition) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.asm.fcsel(fd, fn_, fm, cond);
    }

    pub fn fmov_register(&mut self, fd: VRegister, fn_: VRegister) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.fmov(fd, fn_);
    }

    pub fn fmov_from_register(&mut self, fd: VRegister, rn: Register) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.fmov_from_register(fd, rn);
    }

    pub fn fmov_to_register(&mut self, rd: Register, fn_: VRegister) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        self.asm.fmov_to_register(rd, fn_);
    }

    pub fn scvtf(&mut self, fd: VRegister, rn: Register, fbits: u32) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.scvtf(fd, rn, fbits);
    }

    pub fn ucvtf(&mut self, fd: VRegister, rn: Register, fbits: u32) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.ucvtf(fd, rn, fbits);
    }

    pub fn hint(&mut self, code: SystemHint) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.hint(code);
    }

    pub fn hlt(&mut self, code: i32) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.hlt(code);
    }

    pub fn brk(&mut self, code: i32) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.brk(code);
    }

    pub fn br(&mut self, xn: Register) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!xn.is_zero());
        self.asm.br(xn);
    }

    pub fn ret(&mut self, xn: Register) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!xn.is_zero());
        self.asm.ret(xn);
    }

    pub fn movk(&mut self, rd: Register, imm: u64, shift: i32) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rd.is_zero());
        self.asm.movk(rd, imm, shift);
    }

    pub fn mrs(&mut self, rt: Register, sysreg: SystemRegister) {
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!rt.is_zero());
        self.asm.mrs(rt, sysreg);
    }

    pub fn msr(&mut self, sysreg: SystemRegister, rt: Register) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.msr(sysreg, rt);
    }

    pub fn bind(&mut self, label: LabelId) {
        debug_assert!(self.allow_macro_instructions());
        self.asm.bind(label);
    }

    pub fn call(&mut self, label: LabelId) {
        self.asm.block_pools();
        debug_assert!(self.allow_macro_instructions());
        self.asm.bl(label);
        self.asm.release_pools();
    }

    pub fn call_register(&mut self, xn: Register) {
        self.asm.block_pools();
        debug_assert!(self.allow_macro_instructions());
        debug_assert!(!xn.is_zero());
        self.asm.blr(xn);
        self.asm.release_pools();
    }
}

macro_rules! define_load_store_macro {
    ($($name:ident, $op:expr;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rt: Register, addr: MemOperand) {
                    debug_assert!(self.allow_macro_instructions());
                    let op = $op(rt);
                    self.load_store_macro(rt.into(), addr, op);
                }
            )*
        }
    };
}

define_load_store_macro! {
    ldrb, |_rt: Register| LDRB_w;
    strb, |_rt: Register| STRB_w;
    ldrsb, |rt: Register| if rt.is_64_bits() { LDRSB_x } else { LDRSB_w };
    ldrh, |_rt: Register| LDRH_w;
    strh, |_rt: Register| STRH_w;
    ldrsh, |rt: Register| if rt.is_64_bits() { LDRSH_x } else { LDRSH_w };
    ldrsw, |_rt: Register| LDRSW_x;
}

macro_rules! define_load_store_pair_macro {
    ($($name:ident, $op:expr;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(
                    &mut self,
                    rt: impl Into<CPURegister> + Copy,
                    rt2: impl Into<CPURegister> + Copy,
                    addr: MemOperand,
                ) {
                    debug_assert!(self.allow_macro_instructions());
                    let op = $op(rt.into(), rt2.into());
                    self.load_store_pair_macro(rt.into(), rt2.into(), addr, op);
                }
            )*
        }
    };
}

define_load_store_pair_macro! {
    ldp, |rt: CPURegister, rt2: CPURegister| Assembler::load_pair_op_for_macro(rt, rt2);
    stp, |rt: CPURegister, rt2: CPURegister| Assembler::store_pair_op_for_macro(rt, rt2);
    ldpsw, |_rt: CPURegister, _rt2: CPURegister| LDPSW_x;
}

macro_rules! define_acquire_release_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rt: Register, rn: Register) {
                    debug_assert!(self.allow_macro_instructions());
                    self.asm.$op(rt, rn);
                }
            )*
        }
    };
}

define_acquire_release_macro! {
    ldarb, ldarb;
    ldarh, ldarh;
    ldar, ldar;
    ldaxrb, ldaxrb;
    ldaxrh, ldaxrh;
    ldaxr, ldaxr;
    stlrb, stlrb;
    stlrh, stlrh;
    stlr, stlr;
}

macro_rules! define_store_exclusive_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, rn: Register) {
                    debug_assert!(self.allow_macro_instructions());
                    self.asm.$op(rs, rt, rn);
                }
            )*
        }
    };
}

define_store_exclusive_macro! {
    stlxrb, stlxrb;
    stlxrh, stlxrh;
    stlxr, stlxr;
}

macro_rules! define_compare_and_swap_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, src: MemOperand) {
                    debug_assert!(self.allow_macro_instructions());
                    self.asm.$op(rs, rt, src);
                }
            )*
        }
    };
}

define_compare_and_swap_macro! {
    cas, cas;
    casa, casa;
    casl, casl;
    casal, casal;
    casb, casb;
    casab, casab;
    caslb, caslb;
    casalb, casalb;
    cash, cash;
    casah, casah;
    caslh, caslh;
    casalh, casalh;
}

macro_rules! define_compare_and_swap_pair_macro {
    ($($name:ident, $op:ident;)*) => {
        impl MacroAssembler {
            $(
                pub fn $name(
                    &mut self,
                    rs: Register,
                    rs2: Register,
                    rt: Register,
                    rt2: Register,
                    src: MemOperand,
                ) {
                    debug_assert!(self.allow_macro_instructions());
                    self.asm.$op(rs, rs2, rt, rt2, src);
                }
            )*
        }
    };
}

define_compare_and_swap_pair_macro! {
    casp, casp;
    caspa, caspa;
    caspl, caspl;
    caspal, caspal;
}

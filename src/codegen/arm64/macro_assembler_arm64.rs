use crate::codegen::arm64::assembler_arm64::{Assembler, MemOperand, Operand};
use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::register_arm64::*;
use crate::codegen::arm64::reglist_arm64::CPURegList;
use crate::codegen::arm64::utils_arm64::count_leading_zeros;
use crate::utils::utils::{is_int32, is_uint32};

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
        debug_assert!(!result.aliases(XZR.into()) && !result.aliases(SP.into()));
        result
    }
}

pub struct MacroAssembler {
    asm: Assembler,
    tmp_list: CPURegList,
    fptmp_list: CPURegList,
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
        }
    }

    pub fn default_tmp_list() -> CPURegList {
        CPURegList::new(&[IP0.into(), IP1.into()])
    }

    pub fn default_fptmp_list() -> CPURegList {
        CPURegList::new(&[FP_SCRATCH1.into(), FP_SCRATCH2.into()])
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

    pub fn asm(&mut self) -> &mut Assembler {
        &mut self.asm
    }

    pub fn pc_offset(&self) -> i32 {
        self.asm.pc_offset()
    }

    pub fn buffer_start(&self) -> &[u8] {
        self.asm.buffer_start()
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
            XZR
        } else {
            WZR
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
                (!(imm as u64)) & (K_W_REG_MASK as u64)
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
            (reg_size == K_X_REG_SIZE_IN_BITS as u32)
                || (reg_size == K_W_REG_SIZE_IN_BITS as u32)
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
                    debug_assert_eq!(rn.size_in_bits(), K_X_REG_SIZE_IN_BITS as u32);
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
                let mode = if rd == SP {
                    if s == FlagsUpdate::SetFlags {
                        PreShiftImmMode::NoShift
                    } else {
                        PreShiftImmMode::LimitShiftForSP
                    }
                } else if rn == SP {
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
                immediate &= K_W_REG_MASK;
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
                let mode = if rn == SP {
                    PreShiftImmMode::NoShift
                } else {
                    PreShiftImmMode::AnyShift
                };
                let imm_operand = self.move_immediate_for_shifted_op(temp, immediate, mode);
                if rd.is_sp() {
                    self.asm.logical_for_macro(temp, rn, imm_operand, op);
                    self.asm.mov(SP, temp);
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
                .str(src[0], MemOperand::new(SP, -size, AddrMode::PreIndex)),
            2 => self.asm.stp(
                src[1],
                src[0],
                MemOperand::new(SP, -2 * size, AddrMode::PreIndex),
            ),
            3 => {
                self.asm.stp(
                    src[2],
                    src[1],
                    MemOperand::new(SP, -3 * size, AddrMode::PreIndex),
                );
                self.asm
                    .str(src[0], MemOperand::new(SP, 2 * size, AddrMode::Offset));
            }
            4 => {
                self.asm.stp(
                    src[3],
                    src[2],
                    MemOperand::new(SP, -4 * size, AddrMode::PreIndex),
                );
                self.asm.stp(
                    src[1],
                    src[0],
                    MemOperand::new(SP, 2 * size, AddrMode::Offset),
                );
            }
            _ => unreachable!(),
        }
    }

    fn pop_helper(&mut self, count: usize, size: i64, dst: &[CPURegister]) {
        match count {
            1 => self
                .asm
                .ldr(dst[0], MemOperand::new(SP, size, AddrMode::PostIndex)),
            2 => self.asm.ldp(
                dst[0],
                dst[1],
                MemOperand::new(SP, 2 * size, AddrMode::PostIndex),
            ),
            3 => {
                self.asm
                    .ldr(dst[2], MemOperand::new(SP, 2 * size, AddrMode::Offset));
                self.asm.ldp(
                    dst[0],
                    dst[1],
                    MemOperand::new(SP, 3 * size, AddrMode::PostIndex),
                );
            }
            4 => {
                self.asm.ldp(
                    dst[2],
                    dst[3],
                    MemOperand::new(SP, 2 * size, AddrMode::Offset),
                );
                self.asm.ldp(
                    dst[0],
                    dst[1],
                    MemOperand::new(SP, 4 * size, AddrMode::PostIndex),
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
        self.sub(SP, SP, size as i64);
    }

    pub fn drop(&mut self, count: i64, unit_size: u64) {
        debug_assert!(count >= 0);
        let size = count as u64 * unit_size;
        if size == 0 {
            return;
        }
        debug_assert_eq!(size % 16, 0);
        self.add(SP, SP, size as i64);
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

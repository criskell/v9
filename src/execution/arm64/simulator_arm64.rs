use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::decoder_arm64::{decode, DecoderVisitor};
use crate::codegen::arm64::instructions_arm64::{Instruction, Reg31Mode};
use crate::codegen::arm64::utils_arm64::{count_leading_sign_bits, count_leading_zeros};

pub const K_SP_REG_CODE: u32 = (K_SP_REG_INTERNAL_CODE as u32) & K_REG_CODE_MASK;

#[derive(Default, Copy, Clone)]
pub struct SimSystemRegister {
    value: u32,
}

impl SimSystemRegister {
    pub fn n(&self) -> bool {
        (self.value & N_MASK) != 0
    }

    pub fn z(&self) -> bool {
        (self.value & Z_MASK) != 0
    }

    pub fn c(&self) -> bool {
        (self.value & C_MASK) != 0
    }

    pub fn v(&self) -> bool {
        (self.value & V_MASK) != 0
    }

    pub fn set_n(&mut self, bit: bool) {
        self.set_bit(N_MASK, bit);
    }

    pub fn set_z(&mut self, bit: bool) {
        self.set_bit(Z_MASK, bit);
    }

    pub fn set_c(&mut self, bit: bool) {
        self.set_bit(C_MASK, bit);
    }

    pub fn set_v(&mut self, bit: bool) {
        self.set_bit(V_MASK, bit);
    }

    fn set_bit(&mut self, mask: u32, bit: bool) {
        if bit {
            self.value |= mask;
        } else {
            self.value &= !mask;
        }
    }

    pub fn set_flags(&mut self, new_value: u32) {
        self.value = new_value << FLAGS_OFFSET;
    }

    pub fn flags(&self) -> u32 {
        (self.value >> FLAGS_OFFSET) & 0xf
    }
}

pub struct Simulator {
    registers: [u64; 32],
    vregisters: [u64; 32],
    pc: i32,
    pc_modified: bool,
    nzcv: SimSystemRegister,
    memory: Vec<u8>,
    code_size: i32,
    halted: bool,
    unsupported: Option<Instr>,
}

impl Simulator {
    pub const K_STACK_SIZE: usize = 16 * 1024;

    pub fn new(code: &[u8]) -> Simulator {
        let code_size = code.len();
        let mut memory = vec![0u8; code_size + Simulator::K_STACK_SIZE];
        memory[..code_size].copy_from_slice(code);
        let stack_top = memory.len() as u64 & !15;
        let mut registers = [0u64; 32];
        registers[31] = stack_top;
        Simulator {
            registers,
            vregisters: [0; 32],
            pc: 0,
            pc_modified: false,
            nzcv: SimSystemRegister::default(),
            memory,
            code_size: code_size as i32,
            halted: false,
            unsupported: None,
        }
    }

    pub fn nzcv(&mut self) -> &mut SimSystemRegister {
        &mut self.nzcv
    }

    pub fn pc(&self) -> i32 {
        self.pc
    }

    pub fn is_halted(&self) -> bool {
        self.halted
    }

    pub fn unsupported_instruction(&self) -> Option<Instr> {
        self.unsupported
    }

    pub fn xreg(&self, code: u32) -> u64 {
        self.reg_with_mode(code, Reg31Mode::Reg31IsZeroRegister)
    }

    pub fn wreg(&self, code: u32) -> u32 {
        self.xreg(code) as u32
    }

    pub fn is_zero_register(code: u32, r31mode: Reg31Mode) -> bool {
        (code == 31) && (r31mode == Reg31Mode::Reg31IsZeroRegister)
    }

    pub fn reg_with_mode(&self, code: u32, r31mode: Reg31Mode) -> u64 {
        debug_assert!(code < K_NUMBER_OF_REGISTERS as u32);
        if Simulator::is_zero_register(code, r31mode) {
            return 0;
        }
        self.registers[code as usize]
    }

    pub fn set_xreg(&mut self, code: u32, value: u64) {
        self.set_reg_with_mode(code, value, Reg31Mode::Reg31IsZeroRegister);
    }

    pub fn set_wreg(&mut self, code: u32, value: u32) {
        self.set_xreg(code, value as u64);
    }

    pub fn set_reg_with_mode(&mut self, code: u32, value: u64, r31mode: Reg31Mode) {
        debug_assert!(code < K_NUMBER_OF_REGISTERS as u32);
        if Simulator::is_zero_register(code, r31mode) {
            return;
        }
        self.registers[code as usize] = value;
    }

    pub fn sp(&self) -> u64 {
        self.reg_with_mode(31, Reg31Mode::Reg31IsStackPointer)
    }

    pub fn set_sp(&mut self, value: u64) {
        self.set_reg_with_mode(31, value, Reg31Mode::Reg31IsStackPointer);
    }

    pub fn dreg_bits(&self, code: u32) -> u64 {
        self.vregisters[code as usize]
    }

    pub fn set_dreg_bits(&mut self, code: u32, value: u64) {
        self.vregisters[code as usize] = value;
    }

    pub fn set_lr(&mut self, value: u64) {
        self.set_xreg(K_LINK_REG_CODE, value);
    }

    fn set_pc(&mut self, new_pc: i32) {
        self.pc = new_pc;
        self.pc_modified = true;
    }

    fn increment_pc(&mut self) {
        if !self.pc_modified {
            self.pc += K_INSTR_SIZE as i32;
        }
    }

    pub fn run_from(&mut self, entry: i32) {
        self.pc = entry;
        self.halted = false;
        while !self.halted && self.pc >= 0 && self.pc < self.code_size {
            self.execute_instruction();
        }
    }

    fn execute_instruction(&mut self) {
        debug_assert_eq!(self.pc % K_INSTR_SIZE as i32, 0);
        let instr = self.instruction_at(self.pc);
        self.pc_modified = false;
        decode(self, instr);
        self.increment_pc();
    }

    fn instruction_at(&self, offset: i32) -> Instruction {
        let offset = offset as usize;
        let bytes = &self.memory[offset..offset + 4];
        Instruction::new(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn memory_read(&self, address: u64, size: usize) -> u64 {
        let address = address as usize;
        let mut value = 0u64;
        for i in 0..size {
            value |= (self.memory[address + i] as u64) << (8 * i);
        }
        value
    }

    fn memory_write(&mut self, address: u64, value: u64, size: usize) {
        let address = address as usize;
        for i in 0..size {
            self.memory[address + i] = (value >> (8 * i)) as u8;
        }
    }

    fn condition_passed(&self, cond: Condition) -> bool {
        let flags = &self.nzcv;
        match cond {
            Condition::Eq => flags.z(),
            Condition::Ne => !flags.z(),
            Condition::Hs => flags.c(),
            Condition::Lo => !flags.c(),
            Condition::Mi => flags.n(),
            Condition::Pl => !flags.n(),
            Condition::Vs => flags.v(),
            Condition::Vc => !flags.v(),
            Condition::Hi => flags.c() && !flags.z(),
            Condition::Ls => !(flags.c() && !flags.z()),
            Condition::Ge => flags.n() == flags.v(),
            Condition::Lt => flags.n() != flags.v(),
            Condition::Gt => !flags.z() && (flags.n() == flags.v()),
            Condition::Le => !(!flags.z() && (flags.n() == flags.v())),
            Condition::Nv | Condition::Al => true,
        }
    }

    fn condition_failed(&self, cond: Condition) -> bool {
        !self.condition_passed(cond)
    }

    fn condition_from_bits(bits: u32) -> Condition {
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

    fn add_with_carry(
        &mut self,
        reg_size: u32,
        set_flags: bool,
        left: u64,
        right: u64,
        carry_in: u64,
    ) -> u64 {
        debug_assert!((carry_in == 0) || (carry_in == 1));
        let mask = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let result = left
            .wrapping_add(right)
            .wrapping_add(carry_in)
            & mask;

        if set_flags {
            let sign_mask = 1u64 << (reg_size - 1);
            self.nzcv.set_n((result & sign_mask) != 0);
            self.nzcv.set_z(result == 0);

            let max_uint_2op = mask - carry_in;
            self.nzcv
                .set_c((left > max_uint_2op) || ((max_uint_2op - left) < right));

            let left_sign = left & sign_mask;
            let right_sign = right & sign_mask;
            let result_sign = result & sign_mask;
            self.nzcv
                .set_v((left_sign == right_sign) && (left_sign != result_sign));
        }
        result
    }

    fn shift_operand(reg_size: u32, value: u64, shift_type: Shift, amount: u32) -> u64 {
        if amount == 0 {
            return value;
        }
        debug_assert!(amount < reg_size);
        let mask = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let value = value & mask;
        match shift_type {
            Shift::Lsl => (value << amount) & mask,
            Shift::Lsr => value >> amount,
            Shift::Asr => {
                let sign_extended = if reg_size == 32 {
                    (value as u32 as i32) as i64
                } else {
                    value as i64
                };
                ((sign_extended >> amount) as u64) & mask
            }
            Shift::Ror => {
                let low = value >> amount;
                let high = (value << (reg_size - amount)) & mask;
                low | high
            }
            _ => unreachable!(),
        }
    }

    fn extend_value(reg_size: u32, value: u64, extend_type: Extend, left_shift: u32) -> u64 {
        let mask = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let value = match extend_type {
            Extend::Uxtb => value & K_BYTE_MASK as u64,
            Extend::Uxth => value & K_HALF_WORD_MASK as u64,
            Extend::Uxtw => value & K_WORD_MASK as u64,
            Extend::Sxtb => ((value as u8) as i8 as i64) as u64,
            Extend::Sxth => ((value as u16) as i16 as i64) as u64,
            Extend::Sxtw => ((value as u32) as i32 as i64) as u64,
            Extend::Uxtx | Extend::Sxtx => value,
            Extend::NoExtend => unreachable!(),
        };
        (value << left_shift) & mask
    }

    fn reg_size_of(instr: Instruction) -> u32 {
        if instr.sixty_four_bits() != 0 {
            64
        } else {
            32
        }
    }

    fn add_sub_helper(&mut self, instr: Instruction, op2: u64) {
        let reg_size = Simulator::reg_size_of(instr);
        let set_flags = instr.flags_update() != 0;
        let mask = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let op1 = self.reg_with_mode(instr.rn(), instr.rn_mode()) & mask;
        let op2 = op2 & mask;
        let new_val = match instr.mask(ADD_SUB_OP_MASK) {
            ADD | ADDS => self.add_with_carry(reg_size, set_flags, op1, op2, 0),
            SUB | SUBS => self.add_with_carry(reg_size, set_flags, op1, (!op2) & mask, 1),
            _ => unreachable!(),
        };
        self.set_reg_with_mode(instr.rd(), new_val, instr.rd_mode());
    }

    fn logical_helper(&mut self, instr: Instruction, op2: u64) {
        let reg_size = Simulator::reg_size_of(instr);
        let mask = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let op1 = self.xreg(instr.rn()) & mask;
        let op2 = op2 & mask;
        let mut update_flags = false;
        let result = match instr.mask(LOGICAL_OP_MASK & !NOT) {
            ANDS => {
                update_flags = true;
                op1 & op2
            }
            AND => op1 & op2,
            ORR => op1 | op2,
            EOR => op1 ^ op2,
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        };

        if update_flags {
            let sign_mask = 1u64 << (reg_size - 1);
            self.nzcv.set_n((result & sign_mask) != 0);
            self.nzcv.set_z(result == 0);
            self.nzcv.set_c(false);
            self.nzcv.set_v(false);
        }

        self.set_reg_with_mode(instr.rd(), result, instr.rd_mode());
    }

    fn conditional_compare_helper(&mut self, instr: Instruction, op2: u64) {
        let reg_size = Simulator::reg_size_of(instr);
        let mask = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let op1 = self.xreg(instr.rn()) & mask;
        let op2 = op2 & mask;
        let cond = Simulator::condition_from_bits(instr.condition());
        if self.condition_passed(cond) {
            if instr.mask(CONDITIONAL_COMPARE_MASK) == CCMP {
                self.add_with_carry(reg_size, true, op1, (!op2) & mask, 1);
            } else {
                self.add_with_carry(reg_size, true, op1, op2, 0);
            }
        } else {
            let flags = instr.nzcv();
            self.nzcv.set_flags(flags);
        }
    }

    fn load_store_address(&mut self, addr_reg: u32, offset: i64, addrmode: AddrMode) -> u64 {
        let mut address = self.reg_with_mode(addr_reg, Reg31Mode::Reg31IsStackPointer);
        if (addr_reg == K_SP_REG_CODE) && (address % 16) != 0 {
            panic!("ALIGNMENT EXCEPTION");
        }
        if (addrmode == AddrMode::Offset) || (addrmode == AddrMode::PreIndex) {
            address = address.wrapping_add(offset as u64);
        }
        address
    }

    fn load_store_write_back(&mut self, addr_reg: u32, offset: i64, addrmode: AddrMode) {
        if (addrmode == AddrMode::PreIndex) || (addrmode == AddrMode::PostIndex) {
            debug_assert_ne!(offset, 0);
            let address = self.reg_with_mode(addr_reg, Reg31Mode::Reg31IsStackPointer);
            self.set_reg_with_mode(
                addr_reg,
                address.wrapping_add(offset as u64),
                Reg31Mode::Reg31IsStackPointer,
            );
        }
    }

    fn load_store_helper(&mut self, instr: Instruction, offset: i64, addrmode: AddrMode) {
        let srcdst = instr.rt();
        let addr_reg = instr.rn();
        let address = self.load_store_address(addr_reg, offset, addrmode);

        if instr.is_store() {
            self.load_store_write_back(addr_reg, offset, addrmode);
        }

        match instr.mask(LOAD_STORE_MASK) {
            LDRB_W => {
                let value = self.memory_read(address, 1);
                self.set_wreg(srcdst, value as u32);
            }
            LDRH_W => {
                let value = self.memory_read(address, 2);
                self.set_wreg(srcdst, value as u32);
            }
            LDR_W => {
                let value = self.memory_read(address, 4);
                self.set_wreg(srcdst, value as u32);
            }
            LDR_X => {
                let value = self.memory_read(address, 8);
                self.set_xreg(srcdst, value);
            }
            LDRSB_W => {
                let value = self.memory_read(address, 1) as u8 as i8 as i32;
                self.set_wreg(srcdst, value as u32);
            }
            LDRSH_W => {
                let value = self.memory_read(address, 2) as u16 as i16 as i32;
                self.set_wreg(srcdst, value as u32);
            }
            LDRSB_X => {
                let value = self.memory_read(address, 1) as u8 as i8 as i64;
                self.set_xreg(srcdst, value as u64);
            }
            LDRSH_X => {
                let value = self.memory_read(address, 2) as u16 as i16 as i64;
                self.set_xreg(srcdst, value as u64);
            }
            LDRSW_X => {
                let value = self.memory_read(address, 4) as u32 as i32 as i64;
                self.set_xreg(srcdst, value as u64);
            }
            LDR_S => {
                let value = self.memory_read(address, 4);
                self.set_dreg_bits(srcdst, value);
            }
            LDR_D => {
                let value = self.memory_read(address, 8);
                self.set_dreg_bits(srcdst, value);
            }
            STRB_W => {
                let value = self.wreg(srcdst) as u64;
                self.memory_write(address, value, 1);
            }
            STRH_W => {
                let value = self.wreg(srcdst) as u64;
                self.memory_write(address, value, 2);
            }
            STR_W => {
                let value = self.wreg(srcdst) as u64;
                self.memory_write(address, value, 4);
            }
            STR_X => {
                let value = self.xreg(srcdst);
                self.memory_write(address, value, 8);
            }
            STR_S => {
                let value = self.dreg_bits(srcdst);
                self.memory_write(address, value, 4);
            }
            STR_D => {
                let value = self.dreg_bits(srcdst);
                self.memory_write(address, value, 8);
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        if instr.is_load() {
            self.load_store_write_back(addr_reg, offset, addrmode);
        }
    }

    fn load_store_pair_helper(&mut self, instr: Instruction, addrmode: AddrMode) {
        let rt = instr.rt();
        let rt2 = instr.rt2();
        let addr_reg = instr.rn();
        let element_size = 1usize << instr.size_lsp();
        let offset = instr.imm_ls_pair() as i64 * element_size as i64;
        let address = self.load_store_address(addr_reg, offset, addrmode);
        let address2 = address.wrapping_add(element_size as u64);

        if instr.is_store() {
            self.load_store_write_back(addr_reg, offset, addrmode);
        }

        match instr.mask(LOAD_STORE_PAIR_MASK) {
            LDP_W => {
                let a = self.memory_read(address, 4);
                let b = self.memory_read(address2, 4);
                self.set_wreg(rt, a as u32);
                self.set_wreg(rt2, b as u32);
            }
            LDP_X => {
                let a = self.memory_read(address, 8);
                let b = self.memory_read(address2, 8);
                self.set_xreg(rt, a);
                self.set_xreg(rt2, b);
            }
            LDPSW_X => {
                let a = self.memory_read(address, 4) as u32 as i32 as i64;
                let b = self.memory_read(address2, 4) as u32 as i32 as i64;
                self.set_xreg(rt, a as u64);
                self.set_xreg(rt2, b as u64);
            }
            LDP_S => {
                let a = self.memory_read(address, 4);
                let b = self.memory_read(address2, 4);
                self.set_dreg_bits(rt, a);
                self.set_dreg_bits(rt2, b);
            }
            LDP_D => {
                let a = self.memory_read(address, 8);
                let b = self.memory_read(address2, 8);
                self.set_dreg_bits(rt, a);
                self.set_dreg_bits(rt2, b);
            }
            STP_W => {
                let a = self.wreg(rt) as u64;
                let b = self.wreg(rt2) as u64;
                self.memory_write(address, a, 4);
                self.memory_write(address2, b, 4);
            }
            STP_X => {
                let a = self.xreg(rt);
                let b = self.xreg(rt2);
                self.memory_write(address, a, 8);
                self.memory_write(address2, b, 8);
            }
            STP_S => {
                let a = self.dreg_bits(rt);
                let b = self.dreg_bits(rt2);
                self.memory_write(address, a, 4);
                self.memory_write(address2, b, 4);
            }
            STP_D => {
                let a = self.dreg_bits(rt);
                let b = self.dreg_bits(rt2);
                self.memory_write(address, a, 8);
                self.memory_write(address2, b, 8);
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        if instr.is_load() {
            self.load_store_write_back(addr_reg, offset, addrmode);
        }
    }

    fn bitfield_helper(&mut self, instr: Instruction) {
        let reg_size = Simulator::reg_size_of(instr) as i64;
        let mask_all = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let r = instr.imm_r() as i64;
        let s = instr.imm_s() as i64;
        let mut diff = s - r;
        let mask;
        if diff >= 0 {
            mask = if diff < reg_size - 1 {
                (1u64 << (diff + 1)) - 1
            } else {
                mask_all
            };
        } else {
            let mut umask = (1u64 << (s + 1)) - 1;
            umask = (umask >> r) | (umask << (reg_size - r));
            mask = umask & mask_all;
            diff += reg_size;
        }

        let mut inzero = false;
        let mut extend = false;
        match instr.mask(BITFIELD_MASK) {
            BFM_X | BFM_W => {}
            SBFM_X | SBFM_W => {
                inzero = true;
                extend = true;
            }
            UBFM_X | UBFM_W => {
                inzero = true;
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        let dst = if inzero { 0 } else { self.xreg(instr.rd()) & mask_all };
        let src = self.xreg(instr.rn()) & mask_all;
        let mut result = if r == 0 {
            src
        } else {
            (src >> r) | ((src << (reg_size - r)) & mask_all)
        };
        let topbits_preshift = (1u64 << (reg_size - diff - 1)) - 1;
        let signbits = if diff >= reg_size - 1 {
            0
        } else {
            let top = if extend && ((src >> s) & 1) != 0 {
                topbits_preshift
            } else {
                0
            };
            (top << (diff + 1)) & mask_all
        };

        result = signbits | (result & mask) | (dst & !mask);
        self.set_xreg(instr.rd(), result & mask_all);
    }

    fn extract_helper(&mut self, instr: Instruction) {
        let reg_size = Simulator::reg_size_of(instr);
        let mask_all = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let lsb = instr.imm_s();
        let op2 = self.xreg(instr.rm()) & mask_all;
        let mut result = op2;
        if lsb != 0 {
            let op1 = self.xreg(instr.rn()) & mask_all;
            result = (op2 >> lsb) | ((op1 << (reg_size - lsb)) & mask_all);
        }
        self.set_xreg(instr.rd(), result & mask_all);
    }

    fn data_processing_2_source(&mut self, instr: Instruction) {
        let reg_size = Simulator::reg_size_of(instr);
        let mask_all = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let shift_op;
        let mut result = 0u64;
        match instr.mask(DATA_PROCESSING_2_SOURCE_MASK) {
            SDIV_W | SDIV_X => {
                let rn = self.signed_reg(instr.rn(), reg_size);
                let rm = self.signed_reg(instr.rm(), reg_size);
                let min = if reg_size == 32 {
                    i32::MIN as i64
                } else {
                    i64::MIN
                };
                result = if (rn == min) && (rm == -1) {
                    min as u64
                } else if rm == 0 {
                    0
                } else {
                    (rn.wrapping_div(rm)) as u64
                } & mask_all;
                shift_op = Shift::NoShift;
            }
            UDIV_W | UDIV_X => {
                let rn = self.xreg(instr.rn()) & mask_all;
                let rm = self.xreg(instr.rm()) & mask_all;
                result = if rm == 0 { 0 } else { rn / rm };
                shift_op = Shift::NoShift;
            }
            LSLV_W | LSLV_X => shift_op = Shift::Lsl,
            LSRV_W | LSRV_X => shift_op = Shift::Lsr,
            ASRV_W | ASRV_X => shift_op = Shift::Asr,
            RORV_W | RORV_X => shift_op = Shift::Ror,
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        if shift_op != Shift::NoShift {
            let mut shift = self.wreg(instr.rm());
            if reg_size == 32 {
                shift &= K_SHIFT_AMOUNT_W_REG_MASK;
            } else {
                shift &= K_SHIFT_AMOUNT_X_REG_MASK;
            }
            let value = self.xreg(instr.rn()) & mask_all;
            result = Simulator::shift_operand(reg_size, value, shift_op, shift);
        }

        self.set_xreg(instr.rd(), result & mask_all);
    }

    pub fn dreg(&self, code: u32) -> f64 {
        f64::from_bits(self.vregisters[code as usize])
    }

    pub fn sreg(&self, code: u32) -> f32 {
        f32::from_bits(self.vregisters[code as usize] as u32)
    }

    pub fn set_dreg(&mut self, code: u32, value: f64) {
        self.vregisters[code as usize] = value.to_bits();
    }

    pub fn set_sreg(&mut self, code: u32, value: f32) {
        self.vregisters[code as usize] = value.to_bits() as u64;
    }

    fn fp_compare(&mut self, val0: f64, val1: f64) {
        if val0.is_nan() || val1.is_nan() {
            self.nzcv.set_flags(FP_UNORDERED_FLAG >> FLAGS_OFFSET);
        } else if val0 < val1 {
            self.nzcv.set_flags(FP_LESS_THAN_FLAG >> FLAGS_OFFSET);
        } else if val0 > val1 {
            self.nzcv.set_flags(FP_GREATER_THAN_FLAG >> FLAGS_OFFSET);
        } else {
            debug_assert_eq!(val0, val1);
            self.nzcv.set_flags(FP_EQUAL_FLAG >> FLAGS_OFFSET);
        }
    }

    fn fp_round_int(value: f64, round_mode: FPRounding) -> f64 {
        if (value == 0.0)
            || (value.to_bits() == K_FP64_POSITIVE_INFINITY)
            || (value.to_bits() == K_FP64_NEGATIVE_INFINITY)
        {
            return value;
        }
        if value.is_nan() {
            return value;
        }

        let mut int_result = value.floor();
        let error = value - int_result;
        match round_mode {
            FPRounding::FPTieAway => {
                if (-0.5 < value) && (value < 0.0) {
                    int_result = -0.0;
                } else if (error > 0.5) || ((error == 0.5) && (int_result >= 0.0)) {
                    int_result += 1.0;
                }
            }
            FPRounding::FPTieEven => {
                if (-0.5 <= value) && (value < 0.0) {
                    int_result = -0.0;
                } else if (error > 0.5) || ((error == 0.5) && (int_result % 2.0 != 0.0)) {
                    int_result += 1.0;
                }
            }
            FPRounding::FPZero => {
                if value < 0.0 {
                    int_result = value.ceil();
                }
            }
            FPRounding::FPNegativeInfinity => {}
            FPRounding::FPPositiveInfinity => {
                if (-1.0 < value) && (value < 0.0) {
                    int_result = -0.0;
                } else if error > 0.0 {
                    int_result += 1.0;
                }
            }
        }
        int_result
    }

    fn fp_to_int64(value: f64, rmode: FPRounding) -> i64 {
        let value = Simulator::fp_round_int(value, rmode);
        if value.is_nan() {
            return 0;
        }
        if value >= i64::MAX as f64 {
            return i64::MAX;
        }
        if value <= i64::MIN as f64 {
            return i64::MIN;
        }
        value as i64
    }

    fn fp_to_int32(value: f64, rmode: FPRounding) -> i32 {
        let value = Simulator::fp_round_int(value, rmode);
        if value.is_nan() {
            return 0;
        }
        if value >= i32::MAX as f64 {
            return i32::MAX;
        }
        if value <= i32::MIN as f64 {
            return i32::MIN;
        }
        value as i32
    }

    fn fp_to_uint64(value: f64, rmode: FPRounding) -> u64 {
        let value = Simulator::fp_round_int(value, rmode);
        if value.is_nan() || value <= 0.0 {
            return 0;
        }
        if value >= u64::MAX as f64 {
            return u64::MAX;
        }
        value as u64
    }

    fn fp_to_uint32(value: f64, rmode: FPRounding) -> u32 {
        let value = Simulator::fp_round_int(value, rmode);
        if value.is_nan() || value <= 0.0 {
            return 0;
        }
        if value >= u32::MAX as f64 {
            return u32::MAX;
        }
        value as u32
    }

    fn fp_max(a: f64, b: f64) -> f64 {
        if a.is_nan() || b.is_nan() {
            return f64::NAN;
        }
        if (a == 0.0) && (b == 0.0) && (a.is_sign_negative() != b.is_sign_negative()) {
            return 0.0;
        }
        if a > b {
            a
        } else {
            b
        }
    }

    fn fp_min(a: f64, b: f64) -> f64 {
        if a.is_nan() || b.is_nan() {
            return f64::NAN;
        }
        if (a == 0.0) && (b == 0.0) && (a.is_sign_negative() != b.is_sign_negative()) {
            return -0.0;
        }
        if a < b {
            a
        } else {
            b
        }
    }

    fn is_fp64(instr: Instruction) -> bool {
        instr.mask(FP64) == FP64
    }

    fn signed_reg(&self, code: u32, reg_size: u32) -> i64 {
        if reg_size == 32 {
            self.wreg(code) as i32 as i64
        } else {
            self.xreg(code) as i64
        }
    }
}

impl DecoderVisitor for Simulator {
    fn visit_pc_rel_addressing(&mut self, instr: Instruction) {
        match instr.mask(PC_REL_ADDRESSING_MASK_FULL) {
            ADR => {
                let target = self.pc as i64 + instr.imm_pc_offset();
                self.set_xreg(instr.rd(), target as u64);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_add_sub_immediate(&mut self, instr: Instruction) {
        let shift = if instr.shift_add_sub() == 1 { 12 } else { 0 };
        let op2 = (instr.imm_add_sub() as u64) << shift;
        self.add_sub_helper(instr, op2);
    }

    fn visit_add_sub_shifted(&mut self, instr: Instruction) {
        let reg_size = Simulator::reg_size_of(instr);
        let shift_type = match instr.shift_dp() {
            0 => Shift::Lsl,
            1 => Shift::Lsr,
            2 => Shift::Asr,
            _ => Shift::Ror,
        };
        let shift_amount = instr.imm_dp_shift();
        let value = self.xreg(instr.rm());
        let op2 = Simulator::shift_operand(reg_size, value, shift_type, shift_amount);
        self.add_sub_helper(instr, op2);
    }

    fn visit_add_sub_extended(&mut self, instr: Instruction) {
        let reg_size = Simulator::reg_size_of(instr);
        let ext = match instr.extend_mode() {
            0 => Extend::Uxtb,
            1 => Extend::Uxth,
            2 => Extend::Uxtw,
            3 => Extend::Uxtx,
            4 => Extend::Sxtb,
            5 => Extend::Sxth,
            6 => Extend::Sxtw,
            _ => Extend::Sxtx,
        };
        let left_shift = instr.imm_extend_shift();
        let value = self.xreg(instr.rm());
        let op2 = Simulator::extend_value(reg_size, value, ext, left_shift);
        self.add_sub_helper(instr, op2);
    }

    fn visit_add_sub_with_carry(&mut self, instr: Instruction) {
        let reg_size = Simulator::reg_size_of(instr);
        let mask = if reg_size == 32 { u32::MAX as u64 } else { u64::MAX };
        let set_flags = instr.flags_update() != 0;
        let op1 = self.xreg(instr.rn()) & mask;
        let op2 = self.xreg(instr.rm()) & mask;
        let carry = if self.nzcv.c() { 1 } else { 0 };
        let new_val = match instr.mask(ADD_SUB_OP_MASK) {
            ADD | ADDS => self.add_with_carry(reg_size, set_flags, op1, op2, carry),
            SUB | SUBS => {
                self.add_with_carry(reg_size, set_flags, op1, (!op2) & mask, carry)
            }
            _ => unreachable!(),
        };
        self.set_xreg(instr.rd(), new_val);
    }

    fn visit_logical_immediate(&mut self, instr: Instruction) {
        let op2 = instr.imm_logical();
        self.logical_helper(instr, op2);
    }

    fn visit_logical_shifted(&mut self, instr: Instruction) {
        let reg_size = Simulator::reg_size_of(instr);
        let shift_type = match instr.shift_dp() {
            0 => Shift::Lsl,
            1 => Shift::Lsr,
            2 => Shift::Asr,
            _ => Shift::Ror,
        };
        let shift_amount = instr.imm_dp_shift();
        let value = self.xreg(instr.rm());
        let mut op2 = Simulator::shift_operand(reg_size, value, shift_type, shift_amount);
        if instr.mask(NOT) == NOT {
            op2 = !op2;
        }
        self.logical_helper(instr, op2);
    }

    fn visit_move_wide_immediate(&mut self, instr: Instruction) {
        let is_64_bits = instr.sixty_four_bits() == 1;
        let shift = instr.shift_move_wide() * 16;
        let shifted_imm16 = (instr.imm_move_wide() as u64) << shift;

        let new_xn_val = match instr.mask(MOVE_WIDE_IMMEDIATE_MASK) {
            MOVN_W | MOVN_X => {
                let mut value = !shifted_imm16;
                if !is_64_bits {
                    value &= K_W_REG_MASK as u64;
                }
                value
            }
            MOVK_W | MOVK_X => {
                let reg_code = instr.rd();
                let prev = if is_64_bits {
                    self.xreg(reg_code)
                } else {
                    self.wreg(reg_code) as u64
                };
                (prev & !(0xFFFFu64 << shift)) | shifted_imm16
            }
            MOVZ_W | MOVZ_X => shifted_imm16,
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        };
        self.set_xreg(instr.rd(), new_xn_val);
    }

    fn visit_bitfield(&mut self, instr: Instruction) {
        self.bitfield_helper(instr);
    }

    fn visit_extract(&mut self, instr: Instruction) {
        self.extract_helper(instr);
    }

    fn visit_unconditional_branch(&mut self, instr: Instruction) {
        match instr.mask(UNCONDITIONAL_BRANCH_MASK) {
            BL => {
                let following = (self.pc + K_INSTR_SIZE as i32) as u64;
                self.set_lr(following);
                let target = self.pc + instr.imm_pc_offset() as i32;
                self.set_pc(target);
            }
            B => {
                let target = self.pc + instr.imm_pc_offset() as i32;
                self.set_pc(target);
            }
            _ => unreachable!(),
        }
    }

    fn visit_conditional_branch(&mut self, instr: Instruction) {
        let cond = Simulator::condition_from_bits(instr.condition_branch());
        if self.condition_passed(cond) {
            let target = self.pc + instr.imm_pc_offset() as i32;
            self.set_pc(target);
        }
    }

    fn visit_unconditional_branch_to_register(&mut self, instr: Instruction) {
        let target = self.xreg(instr.rn()) as i32;
        match instr.mask(UNCONDITIONAL_BRANCH_TO_REGISTER_MASK) {
            BLR => {
                if instr.rn() == 31 {
                    self.halted = true;
                    self.unsupported = Some(instr.instruction_bits());
                    return;
                }
                let following = (self.pc + K_INSTR_SIZE as i32) as u64;
                self.set_lr(following);
                self.set_pc(target);
            }
            BR | RET => self.set_pc(target),
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_compare_branch(&mut self, instr: Instruction) {
        let rt = instr.rt();
        let take_branch = match instr.mask(COMPARE_BRANCH_MASK) {
            CBZ_W => self.wreg(rt) == 0,
            CBZ_X => self.xreg(rt) == 0,
            CBNZ_W => self.wreg(rt) != 0,
            CBNZ_X => self.xreg(rt) != 0,
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        };
        if take_branch {
            let target = self.pc + instr.imm_pc_offset() as i32;
            self.set_pc(target);
        }
    }

    fn visit_test_branch(&mut self, instr: Instruction) {
        let bit_pos = (instr.imm_test_branch_bit5() << 5) | instr.imm_test_branch_bit40();
        let mut take_branch = (self.xreg(instr.rt()) & (1u64 << bit_pos)) == 0;
        match instr.mask(TEST_BRANCH_MASK) {
            TBZ => {}
            TBNZ => take_branch = !take_branch,
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        if take_branch {
            let target = self.pc + instr.imm_pc_offset() as i32;
            self.set_pc(target);
        }
    }

    fn visit_conditional_compare_register(&mut self, instr: Instruction) {
        let op2 = self.xreg(instr.rm());
        self.conditional_compare_helper(instr, op2);
    }

    fn visit_conditional_compare_immediate(&mut self, instr: Instruction) {
        let op2 = instr.imm_cond_cmp() as u64;
        self.conditional_compare_helper(instr, op2);
    }

    fn visit_conditional_select(&mut self, instr: Instruction) {
        let mut new_val = self.xreg(instr.rn());
        let cond = Simulator::condition_from_bits(instr.condition());
        if self.condition_failed(cond) {
            new_val = self.xreg(instr.rm());
            match instr.mask(CONDITIONAL_SELECT_MASK) {
                CSEL_W | CSEL_X => {}
                CSINC_W | CSINC_X => new_val = new_val.wrapping_add(1),
                CSINV_W | CSINV_X => new_val = !new_val,
                CSNEG_W | CSNEG_X => new_val = (!new_val).wrapping_add(1),
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
        }
        if instr.sixty_four_bits() != 0 {
            self.set_xreg(instr.rd(), new_val);
        } else {
            self.set_wreg(instr.rd(), new_val as u32);
        }
    }

    fn visit_data_processing_1_source(&mut self, instr: Instruction) {
        let dst = instr.rd();
        let src = instr.rn();
        match instr.mask(DATA_PROCESSING_1_SOURCE_MASK) {
            RBIT_W => {
                let value = self.wreg(src).reverse_bits();
                self.set_wreg(dst, value);
            }
            RBIT_X => {
                let value = self.xreg(src).reverse_bits();
                self.set_xreg(dst, value);
            }
            REV16_W => {
                let value = self.wreg(src).to_le_bytes();
                let swapped = [value[1], value[0], value[3], value[2]];
                self.set_wreg(dst, u32::from_le_bytes(swapped));
            }
            REV16_X => {
                let value = self.xreg(src).to_le_bytes();
                let swapped = [
                    value[1], value[0], value[3], value[2], value[5], value[4], value[7],
                    value[6],
                ];
                self.set_xreg(dst, u64::from_le_bytes(swapped));
            }
            REV_W => {
                let value = self.wreg(src).swap_bytes();
                self.set_wreg(dst, value);
            }
            REV32_X => {
                let value = self.xreg(src).to_le_bytes();
                let swapped = [
                    value[3], value[2], value[1], value[0], value[7], value[6], value[5],
                    value[4],
                ];
                self.set_xreg(dst, u64::from_le_bytes(swapped));
            }
            REV_X => {
                let value = self.xreg(src).swap_bytes();
                self.set_xreg(dst, value);
            }
            CLZ_W => {
                let value =
                    count_leading_zeros(self.wreg(src) as u64, K_W_REG_SIZE_IN_BITS);
                self.set_wreg(dst, value as u32);
            }
            CLZ_X => {
                let value = count_leading_zeros(self.xreg(src), K_X_REG_SIZE_IN_BITS);
                self.set_xreg(dst, value as u64);
            }
            CLS_W => {
                let value = count_leading_sign_bits(
                    self.wreg(src) as i32 as i64,
                    K_W_REG_SIZE_IN_BITS,
                );
                self.set_wreg(dst, value as u32);
            }
            CLS_X => {
                let value =
                    count_leading_sign_bits(self.xreg(src) as i64, K_X_REG_SIZE_IN_BITS);
                self.set_xreg(dst, value as u64);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_data_processing_2_source(&mut self, instr: Instruction) {
        self.data_processing_2_source(instr);
    }

    fn visit_data_processing_3_source(&mut self, instr: Instruction) {
        let rn_u32 = self.wreg(instr.rn()) as u64;
        let rm_u32 = self.wreg(instr.rm()) as u64;
        let rn_s32 = self.wreg(instr.rn()) as i32 as i64;
        let rm_s32 = self.wreg(instr.rm()) as i32 as i64;
        let result: i64 = match instr.mask(DATA_PROCESSING_3_SOURCE_MASK) {
            MADD_W | MADD_X => (self.xreg(instr.ra()) as i64)
                .wrapping_add((self.xreg(instr.rn()) as i64).wrapping_mul(self.xreg(instr.rm()) as i64)),
            MSUB_W | MSUB_X => (self.xreg(instr.ra()) as i64)
                .wrapping_sub((self.xreg(instr.rn()) as i64).wrapping_mul(self.xreg(instr.rm()) as i64)),
            SMADDL_X => (self.xreg(instr.ra()) as i64).wrapping_add(rn_s32.wrapping_mul(rm_s32)),
            SMSUBL_X => (self.xreg(instr.ra()) as i64).wrapping_sub(rn_s32.wrapping_mul(rm_s32)),
            UMADDL_X => self.xreg(instr.ra()).wrapping_add(rn_u32.wrapping_mul(rm_u32)) as i64,
            UMSUBL_X => self.xreg(instr.ra()).wrapping_sub(rn_u32.wrapping_mul(rm_u32)) as i64,
            SMULH_X => {
                let a = self.xreg(instr.rn()) as i64 as i128;
                let b = self.xreg(instr.rm()) as i64 as i128;
                ((a * b) >> 64) as i64
            }
            UMULH_X => {
                let a = self.xreg(instr.rn()) as u128;
                let b = self.xreg(instr.rm()) as u128;
                ((a * b) >> 64) as u64 as i64
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        };

        if instr.sixty_four_bits() != 0 {
            self.set_xreg(instr.rd(), result as u64);
        } else {
            self.set_wreg(instr.rd(), result as i32 as u32);
        }
    }

    fn visit_load_store_unsigned_offset(&mut self, instr: Instruction) {
        let offset = (instr.imm_ls_unsigned() << instr.size_ls()) as i64;
        self.load_store_helper(instr, offset, AddrMode::Offset);
    }

    fn visit_load_store_unscaled_offset(&mut self, instr: Instruction) {
        let offset = instr.imm_ls() as i64;
        self.load_store_helper(instr, offset, AddrMode::Offset);
    }

    fn visit_load_store_pre_index(&mut self, instr: Instruction) {
        let offset = instr.imm_ls() as i64;
        self.load_store_helper(instr, offset, AddrMode::PreIndex);
    }

    fn visit_load_store_post_index(&mut self, instr: Instruction) {
        let offset = instr.imm_ls() as i64;
        self.load_store_helper(instr, offset, AddrMode::PostIndex);
    }

    fn visit_load_store_register_offset(&mut self, instr: Instruction) {
        let ext = match instr.extend_mode() {
            2 => Extend::Uxtw,
            3 => Extend::Uxtx,
            6 => Extend::Sxtw,
            7 => Extend::Sxtx,
            _ => {
                self.visit_unallocated(instr);
                return;
            }
        };
        let shift_amount = instr.imm_shift_ls() * instr.size_ls();
        let value = self.xreg(instr.rm());
        let offset = Simulator::extend_value(64, value, ext, shift_amount) as i64;
        self.load_store_helper(instr, offset, AddrMode::Offset);
    }

    fn visit_load_store_pair_offset(&mut self, instr: Instruction) {
        self.load_store_pair_helper(instr, AddrMode::Offset);
    }

    fn visit_load_store_pair_pre_index(&mut self, instr: Instruction) {
        self.load_store_pair_helper(instr, AddrMode::PreIndex);
    }

    fn visit_load_store_pair_post_index(&mut self, instr: Instruction) {
        self.load_store_pair_helper(instr, AddrMode::PostIndex);
    }

    fn visit_load_literal(&mut self, instr: Instruction) {
        let address = (self.pc as i64 + instr.imm_pc_offset()) as u64;
        let rt = instr.rt();
        match instr.mask(LOAD_LITERAL_MASK) {
            LDR_W_LIT => {
                let value = self.memory_read(address, 4);
                self.set_wreg(rt, value as u32);
            }
            LDR_X_LIT => {
                let value = self.memory_read(address, 8);
                self.set_xreg(rt, value);
            }
            LDRSW_X_LIT => {
                let value = self.memory_read(address, 4) as u32 as i32 as i64;
                self.set_xreg(rt, value as u64);
            }
            LDR_S_LIT => {
                let value = self.memory_read(address, 4);
                self.set_dreg_bits(rt, value);
            }
            LDR_D_LIT => {
                let value = self.memory_read(address, 8);
                self.set_dreg_bits(rt, value);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_exception(&mut self, instr: Instruction) {
        match instr.mask(EXCEPTION_MASK) {
            _ => {
                self.halted = true;
            }
        }
    }

    fn visit_system(&mut self, _instr: Instruction) {}

    fn visit_fp_immediate(&mut self, instr: Instruction) {
        let dest = instr.rd();
        match instr.mask(FP_IMMEDIATE_MASK) {
            FMOV_S_IMM => {
                let value = instr.imm_fp32();
                self.set_sreg(dest, value);
            }
            FMOV_D_IMM => {
                let value = instr.imm_fp64();
                self.set_dreg(dest, value);
            }
            _ => self.visit_unallocated(instr),
        }
    }

    fn visit_fp_compare(&mut self, instr: Instruction) {
        match instr.mask(FP_COMPARE_MASK) {
            FCMP_S => {
                let a = self.sreg(instr.rn()) as f64;
                let b = self.sreg(instr.rm()) as f64;
                self.fp_compare(a, b);
            }
            FCMP_D => {
                let a = self.dreg(instr.rn());
                let b = self.dreg(instr.rm());
                self.fp_compare(a, b);
            }
            FCMP_S_ZERO => {
                let a = self.sreg(instr.rn()) as f64;
                self.fp_compare(a, 0.0);
            }
            FCMP_D_ZERO => {
                let a = self.dreg(instr.rn());
                self.fp_compare(a, 0.0);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_fp_data_processing_1_source(&mut self, instr: Instruction) {
        let fd = instr.rd();
        let fn_ = instr.rn();
        let is_double = Simulator::is_fp64(instr);
        let mut fpcr_rounding = FPRounding::FPTieEven;

        match instr.mask(FP_DATA_PROCESSING_1_SOURCE_MASK) {
            FMOV_S => {
                let value = self.sreg(fn_);
                self.set_sreg(fd, value);
                return;
            }
            FMOV_D => {
                let value = self.dreg(fn_);
                self.set_dreg(fd, value);
                return;
            }
            FABS_S => {
                let value = self.sreg(fn_).abs();
                self.set_sreg(fd, value);
                return;
            }
            FABS_D => {
                let value = self.dreg(fn_).abs();
                self.set_dreg(fd, value);
                return;
            }
            FNEG_S => {
                let value = -self.sreg(fn_);
                self.set_sreg(fd, value);
                return;
            }
            FNEG_D => {
                let value = -self.dreg(fn_);
                self.set_dreg(fd, value);
                return;
            }
            FSQRT_S => {
                let value = self.sreg(fn_).sqrt();
                self.set_sreg(fd, value);
                return;
            }
            FSQRT_D => {
                let value = self.dreg(fn_).sqrt();
                self.set_dreg(fd, value);
                return;
            }
            FCVT_DS => {
                let value = self.sreg(fn_) as f64;
                self.set_dreg(fd, value);
                return;
            }
            FCVT_SD => {
                let value = self.dreg(fn_) as f32;
                self.set_sreg(fd, value);
                return;
            }
            FRINTI_S | FRINTI_D => {}
            FRINTX_S | FRINTX_D => {}
            FRINTA_S | FRINTA_D => fpcr_rounding = FPRounding::FPTieAway,
            FRINTM_S | FRINTM_D => fpcr_rounding = FPRounding::FPNegativeInfinity,
            FRINTN_S | FRINTN_D => fpcr_rounding = FPRounding::FPTieEven,
            FRINTP_S | FRINTP_D => fpcr_rounding = FPRounding::FPPositiveInfinity,
            FRINTZ_S | FRINTZ_D => fpcr_rounding = FPRounding::FPZero,
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        if is_double {
            let value = Simulator::fp_round_int(self.dreg(fn_), fpcr_rounding);
            self.set_dreg(fd, value);
        } else {
            let value = Simulator::fp_round_int(self.sreg(fn_) as f64, fpcr_rounding);
            self.set_sreg(fd, value as f32);
        }
    }

    fn visit_fp_data_processing_2_source(&mut self, instr: Instruction) {
        let fd = instr.rd();
        let fn_ = instr.rn();
        let fm = instr.rm();
        let is_double = Simulator::is_fp64(instr);
        let a = if is_double {
            self.dreg(fn_)
        } else {
            self.sreg(fn_) as f64
        };
        let b = if is_double {
            self.dreg(fm)
        } else {
            self.sreg(fm) as f64
        };

        let result = match instr.mask(FP_DATA_PROCESSING_2_SOURCE_MASK) {
            FADD_S | FADD_D => a + b,
            FSUB_S | FSUB_D => a - b,
            FMUL_S | FMUL_D => a * b,
            FNMUL_S | FNMUL_D => -(a * b),
            FDIV_S | FDIV_D => a / b,
            FMAX_S | FMAX_D => Simulator::fp_max(a, b),
            FMIN_S | FMIN_D => Simulator::fp_min(a, b),
            FMAXNM_S | FMAXNM_D => {
                if a.is_nan() {
                    b
                } else if b.is_nan() {
                    a
                } else {
                    Simulator::fp_max(a, b)
                }
            }
            FMINNM_S | FMINNM_D => {
                if a.is_nan() {
                    b
                } else if b.is_nan() {
                    a
                } else {
                    Simulator::fp_min(a, b)
                }
            }
            _ => {
                self.visit_unallocated(instr);
                return;
            }
        };

        if is_double {
            self.set_dreg(fd, result);
        } else {
            self.set_sreg(fd, result as f32);
        }
    }

    fn visit_fp_data_processing_3_source(&mut self, instr: Instruction) {
        let fd = instr.rd();
        let fn_ = instr.rn();
        let fm = instr.rm();
        let fa = instr.ra();

        match instr.mask(FP_DATA_PROCESSING_3_SOURCE_MASK) {
            FMADD_S => {
                let result = self.sreg(fn_).mul_add(self.sreg(fm), self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FMSUB_S => {
                let result = (-self.sreg(fn_)).mul_add(self.sreg(fm), self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FMADD_D => {
                let result = self.dreg(fn_).mul_add(self.dreg(fm), self.dreg(fa));
                self.set_dreg(fd, result);
            }
            FMSUB_D => {
                let result = (-self.dreg(fn_)).mul_add(self.dreg(fm), self.dreg(fa));
                self.set_dreg(fd, result);
            }
            FNMADD_S => {
                let result = (-self.sreg(fn_)).mul_add(self.sreg(fm), -self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FNMSUB_S => {
                let result = self.sreg(fn_).mul_add(self.sreg(fm), -self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FNMADD_D => {
                let result = (-self.dreg(fn_)).mul_add(self.dreg(fm), -self.dreg(fa));
                self.set_dreg(fd, result);
            }
            FNMSUB_D => {
                let result = self.dreg(fn_).mul_add(self.dreg(fm), -self.dreg(fa));
                self.set_dreg(fd, result);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_fp_integer_convert(&mut self, instr: Instruction) {
        let dst = instr.rd();
        let src = instr.rn();

        match instr.mask(FP_INTEGER_CONVERT_MASK) {
            FCVTZS_WS => {
                let value = Simulator::fp_to_int32(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_wreg(dst, value as u32);
            }
            FCVTZS_XS => {
                let value = Simulator::fp_to_int64(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_xreg(dst, value as u64);
            }
            FCVTZS_WD => {
                let value = Simulator::fp_to_int32(self.dreg(src), FPRounding::FPZero);
                self.set_wreg(dst, value as u32);
            }
            FCVTZS_XD => {
                let value = Simulator::fp_to_int64(self.dreg(src), FPRounding::FPZero);
                self.set_xreg(dst, value as u64);
            }
            FCVTZU_WS => {
                let value = Simulator::fp_to_uint32(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_wreg(dst, value);
            }
            FCVTZU_XS => {
                let value = Simulator::fp_to_uint64(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_xreg(dst, value);
            }
            FCVTZU_WD => {
                let value = Simulator::fp_to_uint32(self.dreg(src), FPRounding::FPZero);
                self.set_wreg(dst, value);
            }
            FCVTZU_XD => {
                let value = Simulator::fp_to_uint64(self.dreg(src), FPRounding::FPZero);
                self.set_xreg(dst, value);
            }
            FCVTNS_WD => {
                let value = Simulator::fp_to_int32(self.dreg(src), FPRounding::FPTieEven);
                self.set_wreg(dst, value as u32);
            }
            FCVTNS_XD => {
                let value = Simulator::fp_to_int64(self.dreg(src), FPRounding::FPTieEven);
                self.set_xreg(dst, value as u64);
            }
            FCVTMS_WD => {
                let value =
                    Simulator::fp_to_int32(self.dreg(src), FPRounding::FPNegativeInfinity);
                self.set_wreg(dst, value as u32);
            }
            FCVTMS_XD => {
                let value =
                    Simulator::fp_to_int64(self.dreg(src), FPRounding::FPNegativeInfinity);
                self.set_xreg(dst, value as u64);
            }
            FCVTAS_WD => {
                let value = Simulator::fp_to_int32(self.dreg(src), FPRounding::FPTieAway);
                self.set_wreg(dst, value as u32);
            }
            FCVTAS_XD => {
                let value = Simulator::fp_to_int64(self.dreg(src), FPRounding::FPTieAway);
                self.set_xreg(dst, value as u64);
            }
            SCVTF_SW => {
                let value = self.wreg(src) as i32 as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_SX => {
                let value = self.xreg(src) as i64 as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_DW => {
                let value = self.wreg(src) as i32 as f64;
                self.set_dreg(dst, value);
            }
            SCVTF_DX => {
                let value = self.xreg(src) as i64 as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_SW => {
                let value = self.wreg(src) as f32;
                self.set_sreg(dst, value);
            }
            UCVTF_SX => {
                let value = self.xreg(src) as f32;
                self.set_sreg(dst, value);
            }
            UCVTF_DW => {
                let value = self.wreg(src) as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_DX => {
                let value = self.xreg(src) as f64;
                self.set_dreg(dst, value);
            }
            FMOV_WS => {
                let value = self.sreg(src).to_bits();
                self.set_wreg(dst, value);
            }
            FMOV_XD => {
                let value = self.dreg(src).to_bits();
                self.set_xreg(dst, value);
            }
            FMOV_SW => {
                let value = f32::from_bits(self.wreg(src));
                self.set_sreg(dst, value);
            }
            FMOV_DX => {
                let value = f64::from_bits(self.xreg(src));
                self.set_dreg(dst, value);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_fp_fixed_point_convert(&mut self, instr: Instruction) {
        let dst = instr.rd();
        let src = instr.rn();
        let fbits = 64 - instr.fp_scale();

        match instr.mask(FP_FIXED_POINT_CONVERT_MASK) {
            SCVTF_SW_FIXED => {
                let value = self.wreg(src) as i32 as f32 / (1u64 << fbits) as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_SX_FIXED => {
                let value = self.xreg(src) as i64 as f32 / (1u64 << fbits) as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_DW_FIXED => {
                let value = self.wreg(src) as i32 as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            SCVTF_DX_FIXED => {
                let value = self.xreg(src) as i64 as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_DX_FIXED => {
                let value = self.xreg(src) as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_DW_FIXED => {
                let value = self.wreg(src) as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            FCVTZS_XD_FIXED => {
                let value = Simulator::fp_to_int64(
                    self.dreg(src) * (1u64 << fbits) as f64,
                    FPRounding::FPZero,
                );
                self.set_xreg(dst, value as u64);
            }
            FCVTZS_WD_FIXED => {
                let value = Simulator::fp_to_int32(
                    self.dreg(src) * (1u64 << fbits) as f64,
                    FPRounding::FPZero,
                );
                self.set_wreg(dst, value as u32);
            }
            FCVTZU_XD_FIXED => {
                let value = Simulator::fp_to_uint64(
                    self.dreg(src) * (1u64 << fbits) as f64,
                    FPRounding::FPZero,
                );
                self.set_xreg(dst, value);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_fp_conditional_compare(&mut self, instr: Instruction) {
        self.visit_unimplemented(instr);
    }

    fn visit_fp_conditional_select(&mut self, instr: Instruction) {
        self.visit_unimplemented(instr);
    }

    fn visit_unallocated(&mut self, instr: Instruction) {
        self.halted = true;
        self.unsupported = Some(instr.instruction_bits());
    }

    fn visit_unimplemented(&mut self, instr: Instruction) {
        self.halted = true;
        self.unsupported = Some(instr.instruction_bits());
    }
}

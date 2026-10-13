#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::decoder_arm64::{decode, DecoderVisitor};
use crate::codegen::arm64::register_arm64::*;
use crate::codegen::arm64::instructions_arm64::{
    neon_format_map, Instruction, NEONFormat, NEONFormatDecoder, NEONFormatMap, Reg31Mode,
};
use crate::codegen::arm64::utils_arm64::{
    count_leading_sign_bits, count_leading_zeros, highest_set_bit_position,
    lowest_set_bit_position,
};

pub const kSPRegCode: u32 = (kSPRegInternalCode as u32) & kRegCodeMask;

#[derive(Default, Copy, Clone)]
pub struct SimSystemRegister {
    value: u32,
}

impl SimSystemRegister {
    pub fn n(&self) -> bool {
        (self.value & N_mask) != 0
    }

    pub fn z(&self) -> bool {
        (self.value & Z_mask) != 0
    }

    pub fn c(&self) -> bool {
        (self.value & C_mask) != 0
    }

    pub fn v(&self) -> bool {
        (self.value & V_mask) != 0
    }

    pub fn set_n(&mut self, bit: bool) {
        self.set_bit(N_mask, bit);
    }

    pub fn set_z(&mut self, bit: bool) {
        self.set_bit(Z_mask, bit);
    }

    pub fn set_c(&mut self, bit: bool) {
        self.set_bit(C_mask, bit);
    }

    pub fn set_v(&mut self, bit: bool) {
        self.set_bit(V_mask, bit);
    }

    fn set_bit(&mut self, mask: u32, bit: bool) {
        if bit {
            self.value |= mask;
        } else {
            self.value &= !mask;
        }
    }

    pub fn set_flags(&mut self, new_value: u32) {
        self.value = new_value << Flags_offset;
    }

    pub fn flags(&self) -> u32 {
        (self.value >> Flags_offset) & 0xf
    }
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct qreg_t {
    pub val: [u8; kQRegSize as usize],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct SimVRegister {
    pub value: [u8; kQRegSize as usize],
}

impl Default for SimVRegister {
    fn default() -> SimVRegister {
        SimVRegister {
            value: [0; kQRegSize as usize],
        }
    }
}

impl SimVRegister {
    pub fn set(&mut self, size_in_bytes: usize, new_value: u64) {
        debug_assert!(size_in_bytes <= kQRegSize as usize);
        self.value = [0; kQRegSize as usize];
        self.value[..size_in_bytes].copy_from_slice(&new_value.to_le_bytes()[..size_in_bytes]);
    }

    pub fn insert(&mut self, lane: i32, size_in_bytes: usize, new_value: u64) {
        debug_assert!(lane >= 0);
        let offset = lane as usize * size_in_bytes;
        debug_assert!(offset + size_in_bytes <= kQRegSize as usize);
        self.value[offset..offset + size_in_bytes]
            .copy_from_slice(&new_value.to_le_bytes()[..size_in_bytes]);
    }

    pub fn get(&self, lane: i32, size_in_bytes: usize) -> u64 {
        debug_assert!(lane >= 0);
        let offset = lane as usize * size_in_bytes;
        debug_assert!(offset + size_in_bytes <= kQRegSize as usize);
        let mut bytes = [0u8; 8];
        bytes[..size_in_bytes].copy_from_slice(&self.value[offset..offset + size_in_bytes]);
        u64::from_le_bytes(bytes)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Saturation {
    NotSaturated = 0,
    SignedSatPositive = 1 << 0,
    SignedSatNegative = 1 << 1,
    UnsignedSatPositive = 1 << 2,
    UnsignedSatNegative = 1 << 3,
}

pub const kSignedSatMask: u8 =
    Saturation::SignedSatPositive as u8 | Saturation::SignedSatNegative as u8;
pub const kUnsignedSatMask: u8 =
    Saturation::UnsignedSatPositive as u8 | Saturation::UnsignedSatNegative as u8;

#[derive(Copy, Clone, Default)]
pub struct LogicVRegister {
    register: SimVRegister,
    saturated: [u8; kQRegSize as usize],
    round: [bool; kQRegSize as usize],
}

impl LogicVRegister {
    pub fn new(other: SimVRegister) -> LogicVRegister {
        LogicVRegister {
            register: other,
            saturated: [Saturation::NotSaturated as u8; kQRegSize as usize],
            round: [false; kQRegSize as usize],
        }
    }

    pub fn register(&self) -> SimVRegister {
        self.register
    }

    pub fn int(&self, vform: VectorFormat, index: i32) -> i64 {
        let lane_size = lane_size_in_bits_from_format(vform);
        let element = self
            .register
            .get(index, lane_size_in_bytes_from_format(vform) as usize);
        ((element << (64 - lane_size)) as i64) >> (64 - lane_size)
    }

    pub fn uint(&self, vform: VectorFormat, index: i32) -> u64 {
        self.register
            .get(index, lane_size_in_bytes_from_format(vform) as usize)
    }

    pub fn uint_left_justified(&self, vform: VectorFormat, index: i32) -> u64 {
        self.uint(vform, index) << (64 - lane_size_in_bits_from_format(vform))
    }

    pub fn int_left_justified(&self, vform: VectorFormat, index: i32) -> i64 {
        self.uint_left_justified(vform, index) as i64
    }

    pub fn set_int(&mut self, vform: VectorFormat, index: i32, value: i64) {
        self.register.insert(
            index,
            lane_size_in_bytes_from_format(vform) as usize,
            value as u64,
        );
    }

    pub fn set_uint128(&mut self, vform: VectorFormat, index: i32, value: (u64, u64)) {
        if lane_size_in_bits_from_format(vform) <= 64 {
            self.set_uint(vform, index, value.1);
            return;
        }
        debug_assert!((vform == VectorFormat::Format1Q) && (index == 0));
        self.set_uint(VectorFormat::Format2D, 0, value.1);
        self.set_uint(VectorFormat::Format2D, 1, value.0);
    }

    pub fn set_uint(&mut self, vform: VectorFormat, index: i32, value: u64) {
        self.register
            .insert(index, lane_size_in_bytes_from_format(vform) as usize, value);
    }

    pub fn set_int_array(&mut self, vform: VectorFormat, src: &[i64]) {
        self.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            self.set_int(vform, i, src[i as usize]);
        }
    }

    pub fn set_uint_array(&mut self, vform: VectorFormat, src: &[u64]) {
        self.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            self.set_uint(vform, i, src[i as usize]);
        }
    }

    pub fn float32(&self, index: i32) -> f32 {
        f32::from_bits(self.register.get(index, 4) as u32)
    }

    pub fn float64(&self, index: i32) -> f64 {
        f64::from_bits(self.register.get(index, 8))
    }

    pub fn set_float32(&mut self, index: i32, value: f32) {
        self.register.insert(index, 4, value.to_bits() as u64);
    }

    pub fn set_float64(&mut self, index: i32, value: f64) {
        self.register.insert(index, 8, value.to_bits());
    }

    pub fn clear_for_write(&mut self, vform: VectorFormat) {
        let size = register_size_in_bytes_from_format(vform);
        for i in size..kQRegSize as u32 {
            self.set_uint(VectorFormat::Format16B, i as i32, 0);
        }
    }

    pub fn get_signed_saturation(&self, index: i32) -> u8 {
        self.saturated[index as usize] & kSignedSatMask
    }

    pub fn get_unsigned_saturation(&self, index: i32) -> u8 {
        self.saturated[index as usize] & kUnsignedSatMask
    }

    pub fn clear_sat(&mut self, index: i32) {
        self.saturated[index as usize] = Saturation::NotSaturated as u8;
    }

    pub fn set_signed_sat(&mut self, index: i32, positive: bool) {
        let sat = if positive {
            Saturation::SignedSatPositive
        } else {
            Saturation::SignedSatNegative
        };
        self.set_sat_flag(index, sat);
    }

    pub fn set_unsigned_sat(&mut self, index: i32, positive: bool) {
        let sat = if positive {
            Saturation::UnsignedSatPositive
        } else {
            Saturation::UnsignedSatNegative
        };
        self.set_sat_flag(index, sat);
    }

    pub fn set_sat_flag(&mut self, index: i32, sat: Saturation) {
        self.saturated[index as usize] |= sat as u8;
    }

    pub fn signed_saturate(&mut self, vform: VectorFormat) {
        for i in 0..lane_count_from_format(vform) {
            let sat = self.get_signed_saturation(i);
            if sat == Saturation::SignedSatPositive as u8 {
                self.set_int(vform, i, max_int_from_format(vform));
            } else if sat == Saturation::SignedSatNegative as u8 {
                self.set_int(vform, i, min_int_from_format(vform));
            }
        }
    }

    pub fn unsigned_saturate(&mut self, vform: VectorFormat) {
        for i in 0..lane_count_from_format(vform) {
            let sat = self.get_unsigned_saturation(i);
            if sat == Saturation::UnsignedSatPositive as u8 {
                self.set_uint(vform, i, max_uint_from_format(vform));
            } else if sat == Saturation::UnsignedSatNegative as u8 {
                self.set_uint(vform, i, 0);
            }
        }
    }

    pub fn get_rounding(&self, index: i32) -> bool {
        self.round[index as usize]
    }

    pub fn set_rounding(&mut self, index: i32, round: bool) {
        self.round[index as usize] = round;
    }

    pub fn round(&mut self, vform: VectorFormat) {
        for i in 0..lane_count_from_format(vform) {
            let value = self.uint(vform, i) + if self.get_rounding(i) { 1 } else { 0 };
            self.set_uint(vform, i, value);
        }
    }

    pub fn uhalve(&mut self, vform: VectorFormat) {
        for i in 0..lane_count_from_format(vform) {
            let mut val = self.uint(vform, i);
            self.set_rounding(i, (val & 1) == 1);
            val >>= 1;
            if self.get_unsigned_saturation(i) != Saturation::NotSaturated as u8 {
                val |= (max_uint_from_format(vform) >> 1) + 1;
            }
            self.set_int(vform, i, val as i64);
        }
    }

    pub fn halve(&mut self, vform: VectorFormat) {
        for i in 0..lane_count_from_format(vform) {
            let mut val = self.int(vform, i);
            self.set_rounding(i, (val & 1) == 1);
            val >>= 1;
            if self.get_signed_saturation(i) != Saturation::NotSaturated as u8 {
                val ^= ((max_uint_from_format(vform) >> 1) + 1) as i64;
            }
            self.set_int(vform, i, val);
        }
    }
}

pub struct Simulator {
    registers: [u64; 32],
    vregisters: [SimVRegister; 32],
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
            vregisters: [SimVRegister::default(); 32],
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
        debug_assert!(code < kNumberOfRegisters as u32);
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
        debug_assert!(code < kNumberOfRegisters as u32);
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
        self.vregisters[code as usize].get(0, kDRegSize as usize)
    }

    pub fn set_dreg_bits(&mut self, code: u32, value: u64) {
        self.vregisters[code as usize].set(kDRegSize as usize, value);
    }

    pub fn qreg(&self, code: u32) -> qreg_t {
        qreg_t {
            val: self.vregisters[code as usize].value,
        }
    }

    pub fn vreg(&self, code: u32) -> SimVRegister {
        self.vregisters[code as usize]
    }

    pub fn set_vreg(&mut self, code: u32, value: SimVRegister) {
        self.vregisters[code as usize] = value;
    }

    fn vreg_logic(&self, code: u32) -> LogicVRegister {
        LogicVRegister::new(self.vregisters[code as usize])
    }

    fn set_vreg_logic(&mut self, code: u32, value: LogicVRegister) {
        self.vregisters[code as usize] = value.register();
    }

    pub fn set_lr(&mut self, value: u64) {
        self.set_xreg(kLinkRegCode, value);
    }

    fn set_pc(&mut self, new_pc: i32) {
        self.pc = new_pc;
        self.pc_modified = true;
    }

    fn increment_pc(&mut self) {
        if !self.pc_modified {
            self.pc += kInstrSize as i32;
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
        debug_assert_eq!(self.pc % kInstrSize as i32, 0);
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

    pub(crate) fn memory_read(&self, address: u64, size: usize) -> u64 {
        let address = address as usize;
        let mut value = 0u64;
        for i in 0..size {
            value |= (self.memory[address + i] as u64) << (8 * i);
        }
        value
    }

    pub(crate) fn memory_write(&mut self, address: u64, value: u64, size: usize) {
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
            Extend::Uxtb => value & kByteMask as u64,
            Extend::Uxth => value & kHalfWordMask as u64,
            Extend::Uxtw => value & kWordMask as u64,
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
        let new_val = match instr.mask(AddSubOpMask) {
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
        let result = match instr.mask(LogicalOpMask & !NOT) {
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
        let cond = condition_from_bits(instr.condition());
        if self.condition_passed(cond) {
            if instr.mask(ConditionalCompareMask) == CCMP {
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
        if (addr_reg == kSPRegCode) && (address % 16) != 0 {
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

        match instr.mask(LoadStoreMask) {
            LDRB_w => {
                let value = self.memory_read(address, 1);
                self.set_wreg(srcdst, value as u32);
            }
            LDRH_w => {
                let value = self.memory_read(address, 2);
                self.set_wreg(srcdst, value as u32);
            }
            LDR_w => {
                let value = self.memory_read(address, 4);
                self.set_wreg(srcdst, value as u32);
            }
            LDR_x => {
                let value = self.memory_read(address, 8);
                self.set_xreg(srcdst, value);
            }
            LDRSB_w => {
                let value = self.memory_read(address, 1) as u8 as i8 as i32;
                self.set_wreg(srcdst, value as u32);
            }
            LDRSH_w => {
                let value = self.memory_read(address, 2) as u16 as i16 as i32;
                self.set_wreg(srcdst, value as u32);
            }
            LDRSB_x => {
                let value = self.memory_read(address, 1) as u8 as i8 as i64;
                self.set_xreg(srcdst, value as u64);
            }
            LDRSH_x => {
                let value = self.memory_read(address, 2) as u16 as i16 as i64;
                self.set_xreg(srcdst, value as u64);
            }
            LDRSW_x => {
                let value = self.memory_read(address, 4) as u32 as i32 as i64;
                self.set_xreg(srcdst, value as u64);
            }
            LDR_s => {
                let value = self.memory_read(address, 4);
                self.set_dreg_bits(srcdst, value);
            }
            LDR_d => {
                let value = self.memory_read(address, 8);
                self.set_dreg_bits(srcdst, value);
            }
            STRB_w => {
                let value = self.wreg(srcdst) as u64;
                self.memory_write(address, value, 1);
            }
            STRH_w => {
                let value = self.wreg(srcdst) as u64;
                self.memory_write(address, value, 2);
            }
            STR_w => {
                let value = self.wreg(srcdst) as u64;
                self.memory_write(address, value, 4);
            }
            STR_x => {
                let value = self.xreg(srcdst);
                self.memory_write(address, value, 8);
            }
            STR_s => {
                let value = self.dreg_bits(srcdst);
                self.memory_write(address, value, 4);
            }
            STR_d => {
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

        match instr.mask(LoadStorePairMask) {
            LDP_w => {
                let a = self.memory_read(address, 4);
                let b = self.memory_read(address2, 4);
                self.set_wreg(rt, a as u32);
                self.set_wreg(rt2, b as u32);
            }
            LDP_x => {
                let a = self.memory_read(address, 8);
                let b = self.memory_read(address2, 8);
                self.set_xreg(rt, a);
                self.set_xreg(rt2, b);
            }
            LDPSW_x => {
                let a = self.memory_read(address, 4) as u32 as i32 as i64;
                let b = self.memory_read(address2, 4) as u32 as i32 as i64;
                self.set_xreg(rt, a as u64);
                self.set_xreg(rt2, b as u64);
            }
            LDP_s => {
                let a = self.memory_read(address, 4);
                let b = self.memory_read(address2, 4);
                self.set_dreg_bits(rt, a);
                self.set_dreg_bits(rt2, b);
            }
            LDP_d => {
                let a = self.memory_read(address, 8);
                let b = self.memory_read(address2, 8);
                self.set_dreg_bits(rt, a);
                self.set_dreg_bits(rt2, b);
            }
            STP_w => {
                let a = self.wreg(rt) as u64;
                let b = self.wreg(rt2) as u64;
                self.memory_write(address, a, 4);
                self.memory_write(address2, b, 4);
            }
            STP_x => {
                let a = self.xreg(rt);
                let b = self.xreg(rt2);
                self.memory_write(address, a, 8);
                self.memory_write(address2, b, 8);
            }
            STP_s => {
                let a = self.dreg_bits(rt);
                let b = self.dreg_bits(rt2);
                self.memory_write(address, a, 4);
                self.memory_write(address2, b, 4);
            }
            STP_d => {
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
        match instr.mask(BitfieldMask) {
            BFM_x | BFM_w => {}
            SBFM_x | SBFM_w => {
                inzero = true;
                extend = true;
            }
            UBFM_x | UBFM_w => {
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
        match instr.mask(DataProcessing2SourceMask) {
            SDIV_w | SDIV_x => {
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
            UDIV_w | UDIV_x => {
                let rn = self.xreg(instr.rn()) & mask_all;
                let rm = self.xreg(instr.rm()) & mask_all;
                result = if rm == 0 { 0 } else { rn / rm };
                shift_op = Shift::NoShift;
            }
            LSLV_w | LSLV_x => shift_op = Shift::Lsl,
            LSRV_w | LSRV_x => shift_op = Shift::Lsr,
            ASRV_w | ASRV_x => shift_op = Shift::Asr,
            RORV_w | RORV_x => shift_op = Shift::Ror,
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        if shift_op != Shift::NoShift {
            let mut shift = self.wreg(instr.rm());
            if reg_size == 32 {
                shift &= kShiftAmountWRegMask;
            } else {
                shift &= kShiftAmountXRegMask;
            }
            let value = self.xreg(instr.rn()) & mask_all;
            result = Simulator::shift_operand(reg_size, value, shift_op, shift);
        }

        self.set_xreg(instr.rd(), result & mask_all);
    }

    pub fn dreg(&self, code: u32) -> f64 {
        f64::from_bits(self.vregisters[code as usize].get(0, kDRegSize as usize))
    }

    pub fn sreg(&self, code: u32) -> f32 {
        f32::from_bits(self.vregisters[code as usize].get(0, kSRegSize as usize) as u32)
    }

    pub fn set_dreg(&mut self, code: u32, value: f64) {
        self.vregisters[code as usize].set(kDRegSize as usize, value.to_bits());
    }

    pub fn set_sreg(&mut self, code: u32, value: f32) {
        self.vregisters[code as usize].set(kSRegSize as usize, value.to_bits() as u64);
    }

    fn fp_compare(&mut self, val0: f64, val1: f64) {
        if val0.is_nan() || val1.is_nan() {
            self.nzcv.set_flags(FPUnorderedFlag >> Flags_offset);
        } else if val0 < val1 {
            self.nzcv.set_flags(FPLessThanFlag >> Flags_offset);
        } else if val0 > val1 {
            self.nzcv.set_flags(FPGreaterThanFlag >> Flags_offset);
        } else {
            debug_assert_eq!(val0, val1);
            self.nzcv.set_flags(FPEqualFlag >> Flags_offset);
        }
    }

    pub fn fp_round_int(value: f64, round_mode: FPRounding) -> f64 {
        if (value == 0.0)
            || (value.to_bits() == kFP64PositiveInfinity)
            || (value.to_bits() == kFP64NegativeInfinity)
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

    pub fn fp_to_int64(value: f64, rmode: FPRounding) -> i64 {
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

    pub fn fp_to_int32(value: f64, rmode: FPRounding) -> i32 {
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

    pub fn fp_to_uint64(value: f64, rmode: FPRounding) -> u64 {
        let value = Simulator::fp_round_int(value, rmode);
        if value.is_nan() || value <= 0.0 {
            return 0;
        }
        if value >= u64::MAX as f64 {
            return u64::MAX;
        }
        value as u64
    }

    pub fn fp_to_uint32(value: f64, rmode: FPRounding) -> u32 {
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
        match instr.mask(PCRelAddressingMask) {
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
        let new_val = match instr.mask(AddSubOpMask) {
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

        let new_xn_val = match instr.mask(MoveWideImmediateMask) {
            MOVN_w | MOVN_x => {
                let mut value = !shifted_imm16;
                if !is_64_bits {
                    value &= kWRegMask as u64;
                }
                value
            }
            MOVK_w | MOVK_x => {
                let reg_code = instr.rd();
                let prev = if is_64_bits {
                    self.xreg(reg_code)
                } else {
                    self.wreg(reg_code) as u64
                };
                (prev & !(0xFFFFu64 << shift)) | shifted_imm16
            }
            MOVZ_w | MOVZ_x => shifted_imm16,
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
        match instr.mask(UnconditionalBranchMask) {
            BL => {
                let following = (self.pc + kInstrSize as i32) as u64;
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
        let cond = condition_from_bits(instr.condition_branch());
        if self.condition_passed(cond) {
            let target = self.pc + instr.imm_pc_offset() as i32;
            self.set_pc(target);
        }
    }

    fn visit_unconditional_branch_to_register(&mut self, instr: Instruction) {
        let target = self.xreg(instr.rn()) as i32;
        match instr.mask(UnconditionalBranchToRegisterMask) {
            BLR => {
                if instr.rn() == 31 {
                    self.halted = true;
                    self.unsupported = Some(instr.instruction_bits());
                    return;
                }
                let following = (self.pc + kInstrSize as i32) as u64;
                self.set_lr(following);
                self.set_pc(target);
            }
            BR | RET => self.set_pc(target),
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_compare_branch(&mut self, instr: Instruction) {
        let rt = instr.rt();
        let take_branch = match instr.mask(CompareBranchMask) {
            CBZ_w => self.wreg(rt) == 0,
            CBZ_x => self.xreg(rt) == 0,
            CBNZ_w => self.wreg(rt) != 0,
            CBNZ_x => self.xreg(rt) != 0,
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
        match instr.mask(TestBranchMask) {
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
        let cond = condition_from_bits(instr.condition());
        if self.condition_failed(cond) {
            new_val = self.xreg(instr.rm());
            match instr.mask(ConditionalSelectMask) {
                CSEL_w | CSEL_x => {}
                CSINC_w | CSINC_x => new_val = new_val.wrapping_add(1),
                CSINV_w | CSINV_x => new_val = !new_val,
                CSNEG_w | CSNEG_x => new_val = (!new_val).wrapping_add(1),
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
        match instr.mask(DataProcessing1SourceMask) {
            RBIT_w => {
                let value = self.wreg(src).reverse_bits();
                self.set_wreg(dst, value);
            }
            RBIT_x => {
                let value = self.xreg(src).reverse_bits();
                self.set_xreg(dst, value);
            }
            REV16_w => {
                let value = self.wreg(src).to_le_bytes();
                let swapped = [value[1], value[0], value[3], value[2]];
                self.set_wreg(dst, u32::from_le_bytes(swapped));
            }
            REV16_x => {
                let value = self.xreg(src).to_le_bytes();
                let swapped = [
                    value[1], value[0], value[3], value[2], value[5], value[4], value[7],
                    value[6],
                ];
                self.set_xreg(dst, u64::from_le_bytes(swapped));
            }
            REV_w => {
                let value = self.wreg(src).swap_bytes();
                self.set_wreg(dst, value);
            }
            REV32_x => {
                let value = self.xreg(src).to_le_bytes();
                let swapped = [
                    value[3], value[2], value[1], value[0], value[7], value[6], value[5],
                    value[4],
                ];
                self.set_xreg(dst, u64::from_le_bytes(swapped));
            }
            REV_x => {
                let value = self.xreg(src).swap_bytes();
                self.set_xreg(dst, value);
            }
            CLZ_w => {
                let value =
                    count_leading_zeros(self.wreg(src) as u64, kWRegSizeInBits);
                self.set_wreg(dst, value as u32);
            }
            CLZ_x => {
                let value = count_leading_zeros(self.xreg(src), kXRegSizeInBits);
                self.set_xreg(dst, value as u64);
            }
            CLS_w => {
                let value = count_leading_sign_bits(
                    self.wreg(src) as i32 as i64,
                    kWRegSizeInBits,
                );
                self.set_wreg(dst, value as u32);
            }
            CLS_x => {
                let value =
                    count_leading_sign_bits(self.xreg(src) as i64, kXRegSizeInBits);
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
        let result: i64 = match instr.mask(DataProcessing3SourceMask) {
            MADD_w | MADD_x => (self.xreg(instr.ra()) as i64)
                .wrapping_add((self.xreg(instr.rn()) as i64).wrapping_mul(self.xreg(instr.rm()) as i64)),
            MSUB_w | MSUB_x => (self.xreg(instr.ra()) as i64)
                .wrapping_sub((self.xreg(instr.rn()) as i64).wrapping_mul(self.xreg(instr.rm()) as i64)),
            SMADDL_x => (self.xreg(instr.ra()) as i64).wrapping_add(rn_s32.wrapping_mul(rm_s32)),
            SMSUBL_x => (self.xreg(instr.ra()) as i64).wrapping_sub(rn_s32.wrapping_mul(rm_s32)),
            UMADDL_x => self.xreg(instr.ra()).wrapping_add(rn_u32.wrapping_mul(rm_u32)) as i64,
            UMSUBL_x => self.xreg(instr.ra()).wrapping_sub(rn_u32.wrapping_mul(rm_u32)) as i64,
            SMULH_x => {
                let a = self.xreg(instr.rn()) as i64 as i128;
                let b = self.xreg(instr.rm()) as i64 as i128;
                ((a * b) >> 64) as i64
            }
            UMULH_x => {
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
        match instr.mask(LoadLiteralMask) {
            LDR_w_lit => {
                let value = self.memory_read(address, 4);
                self.set_wreg(rt, value as u32);
            }
            LDR_x_lit => {
                let value = self.memory_read(address, 8);
                self.set_xreg(rt, value);
            }
            LDRSW_x_lit => {
                let value = self.memory_read(address, 4) as u32 as i32 as i64;
                self.set_xreg(rt, value as u64);
            }
            LDR_s_lit => {
                let value = self.memory_read(address, 4);
                self.set_dreg_bits(rt, value);
            }
            LDR_d_lit => {
                let value = self.memory_read(address, 8);
                self.set_dreg_bits(rt, value);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_exception(&mut self, instr: Instruction) {
        match instr.mask(ExceptionMask) {
            _ => {
                self.halted = true;
            }
        }
    }

    fn visit_system(&mut self, _instr: Instruction) {}

    fn visit_fp_immediate(&mut self, instr: Instruction) {
        let dest = instr.rd();
        match instr.mask(FPImmediateMask) {
            FMOV_s_imm => {
                let value = instr.imm_fp32();
                self.set_sreg(dest, value);
            }
            FMOV_d_imm => {
                let value = instr.imm_fp64();
                self.set_dreg(dest, value);
            }
            _ => self.visit_unallocated(instr),
        }
    }

    fn visit_fp_compare(&mut self, instr: Instruction) {
        match instr.mask(FPCompareMask) {
            FCMP_s => {
                let a = self.sreg(instr.rn()) as f64;
                let b = self.sreg(instr.rm()) as f64;
                self.fp_compare(a, b);
            }
            FCMP_d => {
                let a = self.dreg(instr.rn());
                let b = self.dreg(instr.rm());
                self.fp_compare(a, b);
            }
            FCMP_s_zero => {
                let a = self.sreg(instr.rn()) as f64;
                self.fp_compare(a, 0.0);
            }
            FCMP_d_zero => {
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

        match instr.mask(FPDataProcessing1SourceMask) {
            FMOV_s => {
                let value = self.sreg(fn_);
                self.set_sreg(fd, value);
                return;
            }
            FMOV_d => {
                let value = self.dreg(fn_);
                self.set_dreg(fd, value);
                return;
            }
            FABS_s => {
                let value = self.sreg(fn_).abs();
                self.set_sreg(fd, value);
                return;
            }
            FABS_d => {
                let value = self.dreg(fn_).abs();
                self.set_dreg(fd, value);
                return;
            }
            FNEG_s => {
                let value = -self.sreg(fn_);
                self.set_sreg(fd, value);
                return;
            }
            FNEG_d => {
                let value = -self.dreg(fn_);
                self.set_dreg(fd, value);
                return;
            }
            FSQRT_s => {
                let value = self.sreg(fn_).sqrt();
                self.set_sreg(fd, value);
                return;
            }
            FSQRT_d => {
                let value = self.dreg(fn_).sqrt();
                self.set_dreg(fd, value);
                return;
            }
            FCVT_ds => {
                let value = self.sreg(fn_) as f64;
                self.set_dreg(fd, value);
                return;
            }
            FCVT_sd => {
                let value = self.dreg(fn_) as f32;
                self.set_sreg(fd, value);
                return;
            }
            FRINTI_s | FRINTI_d => {}
            FRINTX_s | FRINTX_d => {}
            FRINTA_s | FRINTA_d => fpcr_rounding = FPRounding::FPTieAway,
            FRINTM_s | FRINTM_d => fpcr_rounding = FPRounding::FPNegativeInfinity,
            FRINTN_s | FRINTN_d => fpcr_rounding = FPRounding::FPTieEven,
            FRINTP_s | FRINTP_d => fpcr_rounding = FPRounding::FPPositiveInfinity,
            FRINTZ_s | FRINTZ_d => fpcr_rounding = FPRounding::FPZero,
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

        let result = match instr.mask(FPDataProcessing2SourceMask) {
            FADD_s | FADD_d => a + b,
            FSUB_s | FSUB_d => a - b,
            FMUL_s | FMUL_d => a * b,
            FNMUL_s | FNMUL_d => -(a * b),
            FDIV_s | FDIV_d => a / b,
            FMAX_s | FMAX_d => Simulator::fp_max(a, b),
            FMIN_s | FMIN_d => Simulator::fp_min(a, b),
            FMAXNM_s | FMAXNM_d => {
                if a.is_nan() {
                    b
                } else if b.is_nan() {
                    a
                } else {
                    Simulator::fp_max(a, b)
                }
            }
            FMINNM_s | FMINNM_d => {
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

        match instr.mask(FPDataProcessing3SourceMask) {
            FMADD_s => {
                let result = self.sreg(fn_).mul_add(self.sreg(fm), self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FMSUB_s => {
                let result = (-self.sreg(fn_)).mul_add(self.sreg(fm), self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FMADD_d => {
                let result = self.dreg(fn_).mul_add(self.dreg(fm), self.dreg(fa));
                self.set_dreg(fd, result);
            }
            FMSUB_d => {
                let result = (-self.dreg(fn_)).mul_add(self.dreg(fm), self.dreg(fa));
                self.set_dreg(fd, result);
            }
            FNMADD_s => {
                let result = (-self.sreg(fn_)).mul_add(self.sreg(fm), -self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FNMSUB_s => {
                let result = self.sreg(fn_).mul_add(self.sreg(fm), -self.sreg(fa));
                self.set_sreg(fd, result);
            }
            FNMADD_d => {
                let result = (-self.dreg(fn_)).mul_add(self.dreg(fm), -self.dreg(fa));
                self.set_dreg(fd, result);
            }
            FNMSUB_d => {
                let result = self.dreg(fn_).mul_add(self.dreg(fm), -self.dreg(fa));
                self.set_dreg(fd, result);
            }
            _ => self.visit_unimplemented(instr),
        }
    }

    fn visit_fp_integer_convert(&mut self, instr: Instruction) {
        let dst = instr.rd();
        let src = instr.rn();

        match instr.mask(FPIntegerConvertMask) {
            FCVTZS_ws => {
                let value = Simulator::fp_to_int32(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_wreg(dst, value as u32);
            }
            FCVTZS_xs => {
                let value = Simulator::fp_to_int64(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_xreg(dst, value as u64);
            }
            FCVTZS_wd => {
                let value = Simulator::fp_to_int32(self.dreg(src), FPRounding::FPZero);
                self.set_wreg(dst, value as u32);
            }
            FCVTZS_xd => {
                let value = Simulator::fp_to_int64(self.dreg(src), FPRounding::FPZero);
                self.set_xreg(dst, value as u64);
            }
            FCVTZU_ws => {
                let value = Simulator::fp_to_uint32(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_wreg(dst, value);
            }
            FCVTZU_xs => {
                let value = Simulator::fp_to_uint64(self.sreg(src) as f64, FPRounding::FPZero);
                self.set_xreg(dst, value);
            }
            FCVTZU_wd => {
                let value = Simulator::fp_to_uint32(self.dreg(src), FPRounding::FPZero);
                self.set_wreg(dst, value);
            }
            FCVTZU_xd => {
                let value = Simulator::fp_to_uint64(self.dreg(src), FPRounding::FPZero);
                self.set_xreg(dst, value);
            }
            FCVTNS_wd => {
                let value = Simulator::fp_to_int32(self.dreg(src), FPRounding::FPTieEven);
                self.set_wreg(dst, value as u32);
            }
            FCVTNS_xd => {
                let value = Simulator::fp_to_int64(self.dreg(src), FPRounding::FPTieEven);
                self.set_xreg(dst, value as u64);
            }
            FCVTMS_wd => {
                let value =
                    Simulator::fp_to_int32(self.dreg(src), FPRounding::FPNegativeInfinity);
                self.set_wreg(dst, value as u32);
            }
            FCVTMS_xd => {
                let value =
                    Simulator::fp_to_int64(self.dreg(src), FPRounding::FPNegativeInfinity);
                self.set_xreg(dst, value as u64);
            }
            FCVTAS_wd => {
                let value = Simulator::fp_to_int32(self.dreg(src), FPRounding::FPTieAway);
                self.set_wreg(dst, value as u32);
            }
            FCVTAS_xd => {
                let value = Simulator::fp_to_int64(self.dreg(src), FPRounding::FPTieAway);
                self.set_xreg(dst, value as u64);
            }
            SCVTF_sw => {
                let value = self.wreg(src) as i32 as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_sx => {
                let value = self.xreg(src) as i64 as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_dw => {
                let value = self.wreg(src) as i32 as f64;
                self.set_dreg(dst, value);
            }
            SCVTF_dx => {
                let value = self.xreg(src) as i64 as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_sw => {
                let value = self.wreg(src) as f32;
                self.set_sreg(dst, value);
            }
            UCVTF_sx => {
                let value = self.xreg(src) as f32;
                self.set_sreg(dst, value);
            }
            UCVTF_dw => {
                let value = self.wreg(src) as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_dx => {
                let value = self.xreg(src) as f64;
                self.set_dreg(dst, value);
            }
            FMOV_ws => {
                let value = self.sreg(src).to_bits();
                self.set_wreg(dst, value);
            }
            FMOV_xd => {
                let value = self.dreg(src).to_bits();
                self.set_xreg(dst, value);
            }
            FMOV_sw => {
                let value = f32::from_bits(self.wreg(src));
                self.set_sreg(dst, value);
            }
            FMOV_dx => {
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

        match instr.mask(FPFixedPointConvertMask) {
            SCVTF_sw_fixed => {
                let value = self.wreg(src) as i32 as f32 / (1u64 << fbits) as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_sx_fixed => {
                let value = self.xreg(src) as i64 as f32 / (1u64 << fbits) as f32;
                self.set_sreg(dst, value);
            }
            SCVTF_dw_fixed => {
                let value = self.wreg(src) as i32 as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            SCVTF_dx_fixed => {
                let value = self.xreg(src) as i64 as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_dx_fixed => {
                let value = self.xreg(src) as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            UCVTF_dw_fixed => {
                let value = self.wreg(src) as f64 / (1u64 << fbits) as f64;
                self.set_dreg(dst, value);
            }
            FCVTZS_xd_fixed => {
                let value = Simulator::fp_to_int64(
                    self.dreg(src) * (1u64 << fbits) as f64,
                    FPRounding::FPZero,
                );
                self.set_xreg(dst, value as u64);
            }
            FCVTZS_wd_fixed => {
                let value = Simulator::fp_to_int32(
                    self.dreg(src) * (1u64 << fbits) as f64,
                    FPRounding::FPZero,
                );
                self.set_wreg(dst, value as u32);
            }
            FCVTZU_xd_fixed => {
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

    fn visit_neon3_same(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);
        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rm = self.vreg_logic(instr.rm());

        if instr.mask(NEON3SameLogicalFMask) == NEON3SameLogicalFixed {
            let vf = nfd.get_vector_format_from_map(NEONFormatDecoder::logical_format_map());
            match instr.mask(NEON3SameLogicalMask) {
                NEON_AND => self.and_(vf, &mut rd, &rn, &rm),
                NEON_ORR => self.orr(vf, &mut rd, &rn, &rm),
                NEON_ORN => self.orn(vf, &mut rd, &rn, &rm),
                NEON_EOR => self.eor(vf, &mut rd, &rn, &rm),
                NEON_BIC => self.bic(vf, &mut rd, &rn, &rm),
                NEON_BIF => self.bif(vf, &mut rd, &rn, &rm),
                NEON_BIT => self.bit(vf, &mut rd, &rn, &rm),
                NEON_BSL => self.bsl(vf, &mut rd, &rn, &rm),
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
        } else if instr.mask(NEON3SameFPFMask) == NEON3SameFPFixed {
            let vf = nfd.get_vector_format_from_map(NEONFormatDecoder::fp_format_map());
            if !self.visit_neon3_same_fp(instr.mask(NEON3SameFPMask), vf, &mut rd, &rn, &rm) {
                self.visit_unimplemented(instr);
                return;
            }
        } else {
            let vf = nfd.get_vector_format(0);
            match instr.mask(NEON3SameMask) {
                NEON_ADD => self.add(vf, &mut rd, &rn, &rm),
                NEON_ADDP => self.addp(vf, &mut rd, &rn, &rm),
                NEON_CMEQ => self.cmp(vf, &mut rd, &rn, &rm, Condition::Eq),
                NEON_CMGE => self.cmp(vf, &mut rd, &rn, &rm, Condition::Ge),
                NEON_CMGT => self.cmp(vf, &mut rd, &rn, &rm, Condition::Gt),
                NEON_CMHI => self.cmp(vf, &mut rd, &rn, &rm, Condition::Hi),
                NEON_CMHS => self.cmp(vf, &mut rd, &rn, &rm, Condition::Hs),
                NEON_CMTST => self.cmptst(vf, &mut rd, &rn, &rm),
                NEON_MLS => self.mls(vf, &mut rd, &rn, &rm),
                NEON_MLA => self.mla(vf, &mut rd, &rn, &rm),
                NEON_MUL => self.mul(vf, &mut rd, &rn, &rm),
                NEON_PMUL => self.pmul(vf, &mut rd, &rn, &rm),
                NEON_SMAX => self.smax(vf, &mut rd, &rn, &rm),
                NEON_SMAXP => self.smaxp(vf, &mut rd, &rn, &rm),
                NEON_SMIN => self.smin(vf, &mut rd, &rn, &rm),
                NEON_SMINP => self.sminp(vf, &mut rd, &rn, &rm),
                NEON_SUB => self.sub(vf, &mut rd, &rn, &rm),
                NEON_UMAX => self.umax(vf, &mut rd, &rn, &rm),
                NEON_UMAXP => self.umaxp(vf, &mut rd, &rn, &rm),
                NEON_UMIN => self.umin(vf, &mut rd, &rn, &rm),
                NEON_UMINP => self.uminp(vf, &mut rd, &rn, &rm),
                NEON_SSHL => self.sshl(vf, &mut rd, &rn, &rm),
                NEON_USHL => self.ushl(vf, &mut rd, &rn, &rm),
                NEON_SABD => self.abs_diff(vf, &mut rd, &rn, &rm, true),
                NEON_UABD => self.abs_diff(vf, &mut rd, &rn, &rm, false),
                NEON_SABA => self.saba(vf, &mut rd, &rn, &rm),
                NEON_UABA => self.uaba(vf, &mut rd, &rn, &rm),
                NEON_UQADD => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQADD => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.signed_saturate(vf);
                }
                NEON_UQSUB => {
                    self.sub(vf, &mut rd, &rn, &rm);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQSUB => {
                    self.sub(vf, &mut rd, &rn, &rm);
                    rd.signed_saturate(vf);
                }
                NEON_SQDMULH => self.sqdmulh(vf, &mut rd, &rn, &rm),
                NEON_SQRDMULH => self.sqrdmulh(vf, &mut rd, &rn, &rm),
                NEON_UQSHL => {
                    self.ushl(vf, &mut rd, &rn, &rm);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQSHL => {
                    self.sshl(vf, &mut rd, &rn, &rm);
                    rd.signed_saturate(vf);
                }
                NEON_URSHL => {
                    self.ushl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                }
                NEON_SRSHL => {
                    self.sshl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                }
                NEON_UQRSHL => {
                    self.ushl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQRSHL => {
                    self.sshl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                    rd.signed_saturate(vf);
                }
                NEON_UHADD => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.uhalve(vf);
                }
                NEON_URHADD => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.uhalve(vf);
                    rd.round(vf);
                }
                NEON_SHADD => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.halve(vf);
                }
                NEON_SRHADD => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.halve(vf);
                    rd.round(vf);
                }
                NEON_UHSUB => {
                    self.sub(vf, &mut rd, &rn, &rm);
                    rd.uhalve(vf);
                }
                NEON_SHSUB => {
                    self.sub(vf, &mut rd, &rn, &rm);
                    rd.halve(vf);
                }
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon3_same_hp(&mut self, instr: Instruction) {
        self.visit_unimplemented(instr);
    }

    fn visit_neon3_different(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);
        let vf = nfd.get_vector_format(0);
        let mut vf_l = nfd.get_vector_format_from_map(NEONFormatDecoder::long_integer_format_map());

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rm = self.vreg_logic(instr.rm());
        let size = instr.neon_size();

        match instr.mask(NEON3DifferentMask) {
            NEON_PMULL => {
                if (size == 1) || (size == 2) {
                    self.visit_unallocated(instr);
                    return;
                }
                if size == 3 {
                    vf_l = VectorFormat::Format1Q;
                }
                self.pmull(vf_l, &mut rd, &rn, &rm);
            }
            NEON_PMULL2 => {
                if (size == 1) || (size == 2) {
                    self.visit_unallocated(instr);
                    return;
                }
                if size == 3 {
                    vf_l = VectorFormat::Format1Q;
                }
                self.pmull2(vf_l, &mut rd, &rn, &rm);
            }
            NEON_UADDL => self.uaddl(vf_l, &mut rd, &rn, &rm),
            NEON_UADDL2 => self.uaddl2(vf_l, &mut rd, &rn, &rm),
            NEON_SADDL => self.saddl(vf_l, &mut rd, &rn, &rm),
            NEON_SADDL2 => self.saddl2(vf_l, &mut rd, &rn, &rm),
            NEON_USUBL => self.usubl(vf_l, &mut rd, &rn, &rm),
            NEON_USUBL2 => self.usubl2(vf_l, &mut rd, &rn, &rm),
            NEON_SSUBL => self.ssubl(vf_l, &mut rd, &rn, &rm),
            NEON_SSUBL2 => self.ssubl2(vf_l, &mut rd, &rn, &rm),
            NEON_SABAL => self.sabal(vf_l, &mut rd, &rn, &rm),
            NEON_SABAL2 => self.sabal2(vf_l, &mut rd, &rn, &rm),
            NEON_UABAL => self.uabal(vf_l, &mut rd, &rn, &rm),
            NEON_UABAL2 => self.uabal2(vf_l, &mut rd, &rn, &rm),
            NEON_SABDL => self.sabdl(vf_l, &mut rd, &rn, &rm),
            NEON_SABDL2 => self.sabdl2(vf_l, &mut rd, &rn, &rm),
            NEON_UABDL => self.uabdl(vf_l, &mut rd, &rn, &rm),
            NEON_UABDL2 => self.uabdl2(vf_l, &mut rd, &rn, &rm),
            NEON_SMLAL => self.smlal(vf_l, &mut rd, &rn, &rm),
            NEON_SMLAL2 => self.smlal2(vf_l, &mut rd, &rn, &rm),
            NEON_UMLAL => self.umlal(vf_l, &mut rd, &rn, &rm),
            NEON_UMLAL2 => self.umlal2(vf_l, &mut rd, &rn, &rm),
            NEON_SMLSL => self.smlsl(vf_l, &mut rd, &rn, &rm),
            NEON_SMLSL2 => self.smlsl2(vf_l, &mut rd, &rn, &rm),
            NEON_UMLSL => self.umlsl(vf_l, &mut rd, &rn, &rm),
            NEON_UMLSL2 => self.umlsl2(vf_l, &mut rd, &rn, &rm),
            NEON_SMULL => self.smull(vf_l, &mut rd, &rn, &rm),
            NEON_SMULL2 => self.smull2(vf_l, &mut rd, &rn, &rm),
            NEON_UMULL => self.umull(vf_l, &mut rd, &rn, &rm),
            NEON_UMULL2 => self.umull2(vf_l, &mut rd, &rn, &rm),
            NEON_SQDMLAL => self.sqdmlal(vf_l, &mut rd, &rn, &rm),
            NEON_SQDMLAL2 => self.sqdmlal2(vf_l, &mut rd, &rn, &rm),
            NEON_SQDMLSL => self.sqdmlsl(vf_l, &mut rd, &rn, &rm),
            NEON_SQDMLSL2 => self.sqdmlsl2(vf_l, &mut rd, &rn, &rm),
            NEON_SQDMULL => self.sqdmull(vf_l, &mut rd, &rn, &rm),
            NEON_SQDMULL2 => self.sqdmull2(vf_l, &mut rd, &rn, &rm),
            NEON_UADDW => self.uaddw(vf_l, &mut rd, &rn, &rm),
            NEON_UADDW2 => self.uaddw2(vf_l, &mut rd, &rn, &rm),
            NEON_SADDW => self.saddw(vf_l, &mut rd, &rn, &rm),
            NEON_SADDW2 => self.saddw2(vf_l, &mut rd, &rn, &rm),
            NEON_USUBW => self.usubw(vf_l, &mut rd, &rn, &rm),
            NEON_USUBW2 => self.usubw2(vf_l, &mut rd, &rn, &rm),
            NEON_SSUBW => self.ssubw(vf_l, &mut rd, &rn, &rm),
            NEON_SSUBW2 => self.ssubw2(vf_l, &mut rd, &rn, &rm),
            NEON_ADDHN => self.addhn(vf, &mut rd, &rn, &rm),
            NEON_ADDHN2 => self.addhn2(vf, &mut rd, &rn, &rm),
            NEON_RADDHN => self.raddhn(vf, &mut rd, &rn, &rm),
            NEON_RADDHN2 => self.raddhn2(vf, &mut rd, &rn, &rm),
            NEON_SUBHN => self.subhn(vf, &mut rd, &rn, &rm),
            NEON_SUBHN2 => self.subhn2(vf, &mut rd, &rn, &rm),
            NEON_RSUBHN => self.rsubhn(vf, &mut rd, &rn, &rm),
            NEON_RSUBHN2 => self.rsubhn2(vf, &mut rd, &rn, &rm),
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon3_extension(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);
        let mut rd = self.vreg_logic(instr.rd());
        let rm = self.vreg_logic(instr.rm());
        let rn = self.vreg_logic(instr.rn());
        let vf = nfd.get_vector_format(0);

        match instr.mask(NEON3ExtensionMask) {
            NEON_SDOT => {
                if vf == VectorFormat::Format4S || vf == VectorFormat::Format2S {
                    self.sdot(vf, &mut rd, &rn, &rm);
                } else {
                    self.visit_unallocated(instr);
                    return;
                }
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon2_reg_misc(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);
        let mut vf = nfd.get_vector_format(0);

        let vf_lp = nfd.get_vector_format_from_map(map_lp());
        let vf_fcvtl = nfd.get_vector_format_from_map(map_fcvtl());
        let vf_fcvtn = nfd.get_vector_format_from_map(map_fcvtn());

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());

        if instr.mask(NEON2RegMiscOpcode) <= NEON_NEG_opcode {
            match instr.mask(NEON2RegMiscMask) {
                NEON_REV64 => self.rev64(vf, &mut rd, &rn),
                NEON_REV32 => self.rev32(vf, &mut rd, &rn),
                NEON_REV16 => self.rev16(vf, &mut rd, &rn),
                NEON_SUQADD => self.suqadd(vf, &mut rd, &rn),
                NEON_USQADD => self.usqadd(vf, &mut rd, &rn),
                NEON_CLS => self.cls(vf, &mut rd, &rn),
                NEON_CLZ => self.clz(vf, &mut rd, &rn),
                NEON_CNT => self.cnt(vf, &mut rd, &rn),
                NEON_SQABS => {
                    self.abs(vf, &mut rd, &rn);
                    rd.signed_saturate(vf);
                }
                NEON_SQNEG => {
                    self.neg(vf, &mut rd, &rn);
                    rd.signed_saturate(vf);
                }
                NEON_CMGT_zero => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Gt),
                NEON_CMGE_zero => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Ge),
                NEON_CMEQ_zero => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Eq),
                NEON_CMLE_zero => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Le),
                NEON_CMLT_zero => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Lt),
                NEON_ABS => self.abs(vf, &mut rd, &rn),
                NEON_NEG => self.neg(vf, &mut rd, &rn),
                NEON_SADDLP => self.saddlp(vf_lp, &mut rd, &rn),
                NEON_UADDLP => self.uaddlp(vf_lp, &mut rd, &rn),
                NEON_SADALP => self.sadalp(vf_lp, &mut rd, &rn),
                NEON_UADALP => self.uadalp(vf_lp, &mut rd, &rn),
                NEON_RBIT_NOT => {
                    vf = nfd.get_vector_format_from_map(NEONFormatDecoder::logical_format_map());
                    match instr.fp_type() {
                        0 => self.not_(vf, &mut rd, &rn),
                        1 => self.rbit(vf, &mut rd, &rn),
                        _ => {
                            self.visit_unimplemented(instr);
                            return;
                        }
                    }
                }
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
            self.set_vreg_logic(instr.rd(), rd);
            return;
        }

        let fpf = nfd.get_vector_format_from_map(
            if instr.mask(NEON2RegMiscHPFixed) == NEON2RegMiscHPFixed {
                NEONFormatDecoder::fphp_format_map()
            } else {
                NEONFormatDecoder::fp_format_map()
            },
        );
        let mut fpcr_rounding = FPRounding::FPTieEven;
        let mut inexact_exception = false;

        match instr.mask(NEON2RegMiscFPMask ^ NEON2RegMiscHPFixed) {
            NEON_FABS => {
                self.fabs_(fpf, &mut rd, &rn);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FNEG => {
                self.fneg(fpf, &mut rd, &rn);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FSQRT => {
                self.fsqrt(fpf, &mut rd, &rn);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTL => {
                if instr.mask(NEON_Q) != 0 {
                    self.fcvtl2(vf_fcvtl, &mut rd, &rn);
                } else {
                    self.fcvtl(vf_fcvtl, &mut rd, &rn);
                }
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTN => {
                if instr.mask(NEON_Q) != 0 {
                    self.fcvtn2(vf_fcvtn, &mut rd, &rn);
                } else {
                    self.fcvtn(vf_fcvtn, &mut rd, &rn);
                }
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTXN => {
                if instr.mask(NEON_Q) != 0 {
                    self.fcvtxn2(vf_fcvtn, &mut rd, &rn);
                } else {
                    self.fcvtxn(vf_fcvtn, &mut rd, &rn);
                }
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }

            NEON_FRINTI => {}
            NEON_FRINTX => inexact_exception = true,
            NEON_FRINTA => fpcr_rounding = FPRounding::FPTieAway,
            NEON_FRINTM => fpcr_rounding = FPRounding::FPNegativeInfinity,
            NEON_FRINTN => fpcr_rounding = FPRounding::FPTieEven,
            NEON_FRINTP => fpcr_rounding = FPRounding::FPPositiveInfinity,
            NEON_FRINTZ => fpcr_rounding = FPRounding::FPZero,

            NEON_FCVTNS => {
                self.fcvts(fpf, &mut rd, &rn, FPRounding::FPTieEven, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTNU => {
                self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPTieEven, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTPS => {
                self.fcvts(fpf, &mut rd, &rn, FPRounding::FPPositiveInfinity, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTPU => {
                self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPPositiveInfinity, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTMS => {
                self.fcvts(fpf, &mut rd, &rn, FPRounding::FPNegativeInfinity, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTMU => {
                self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPNegativeInfinity, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTZS => {
                self.fcvts(fpf, &mut rd, &rn, FPRounding::FPZero, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTZU => {
                self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPZero, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTAS => {
                self.fcvts(fpf, &mut rd, &rn, FPRounding::FPTieAway, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCVTAU => {
                self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPTieAway, 0);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_SCVTF => {
                self.scvtf(fpf, &mut rd, &rn, 0, fpcr_rounding);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_UCVTF => {
                self.ucvtf(fpf, &mut rd, &rn, 0, fpcr_rounding);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_URSQRTE => {
                self.ursqrte(fpf, &mut rd, &rn);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_URECPE => {
                self.urecpe(fpf, &mut rd, &rn);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FRSQRTE => {
                self.frsqrte(fpf, &mut rd, &rn);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FRECPE => {
                self.frecpe(fpf, &mut rd, &rn, fpcr_rounding);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCMGT_zero => {
                self.fcmp_zero(fpf, &mut rd, &rn, Condition::Gt);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCMGE_zero => {
                self.fcmp_zero(fpf, &mut rd, &rn, Condition::Ge);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCMEQ_zero => {
                self.fcmp_zero(fpf, &mut rd, &rn, Condition::Eq);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCMLE_zero => {
                self.fcmp_zero(fpf, &mut rd, &rn, Condition::Le);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            NEON_FCMLT_zero => {
                self.fcmp_zero(fpf, &mut rd, &rn, Condition::Lt);
                self.set_vreg_logic(instr.rd(), rd);
                return;
            }
            _ => {
                if (NEON_XTN_opcode <= instr.mask(NEON2RegMiscOpcode))
                    && (instr.mask(NEON2RegMiscOpcode) <= NEON_UQXTN_opcode)
                {
                    match instr.mask(NEON2RegMiscMask) {
                        NEON_XTN => self.xtn(vf, &mut rd, &rn),
                        NEON_SQXTN => self.sqxtn(vf, &mut rd, &rn),
                        NEON_UQXTN => self.uqxtn(vf, &mut rd, &rn),
                        NEON_SQXTUN => self.sqxtun(vf, &mut rd, &rn),
                        NEON_SHLL => {
                            vf = nfd.get_vector_format_from_map(
                                NEONFormatDecoder::long_integer_format_map(),
                            );
                            if instr.mask(NEON_Q) != 0 {
                                self.shll2(vf, &mut rd, &rn);
                            } else {
                                self.shll(vf, &mut rd, &rn);
                            }
                        }
                        _ => {
                            self.visit_unimplemented(instr);
                            return;
                        }
                    }
                    self.set_vreg_logic(instr.rd(), rd);
                    return;
                }
                self.visit_unimplemented(instr);
                return;
            }
        }

        self.frint(fpf, &mut rd, &rn, fpcr_rounding, inexact_exception);
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_across_lanes(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());

        if instr.mask(NEONAcrossLanesFPFMask) == NEONAcrossLanesFPFixed {
            let vf = nfd.get_vector_format_from_map(NEONFormatDecoder::fp_format_map());

            match instr.mask(NEONAcrossLanesFPMask) {
                NEON_FMAXV => self.fmaxv(vf, &mut rd, &rn),
                NEON_FMINV => self.fminv(vf, &mut rd, &rn),
                NEON_FMAXNMV => self.fmaxnmv(vf, &mut rd, &rn),
                NEON_FMINNMV => self.fminnmv(vf, &mut rd, &rn),
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
        } else {
            let vf = nfd.get_vector_format(0);

            match instr.mask(NEONAcrossLanesMask) {
                NEON_ADDV => self.addv(vf, &mut rd, &rn),
                NEON_SMAXV => self.smaxv(vf, &mut rd, &rn),
                NEON_SMINV => self.sminv(vf, &mut rd, &rn),
                NEON_UMAXV => self.umaxv(vf, &mut rd, &rn),
                NEON_UMINV => self.uminv(vf, &mut rd, &rn),
                NEON_SADDLV => self.saddlv(vf, &mut rd, &rn),
                NEON_UADDLV => self.uaddlv(vf, &mut rd, &rn),
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_copy(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::triangular_format_map()]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let imm5 = instr.imm_neon5();
        let lsb = lowest_set_bit_position(imm5 as u64);
        let reg_index = (imm5 >> lsb) as i32;

        if instr.mask(NEONCopyInsElementMask) == NEON_INS_ELEMENT {
            let imm4 = instr.imm_neon4();
            debug_assert!(lsb >= 1);
            let rn_index = (imm4 >> (lsb - 1)) as i32;
            self.ins_element(vf, &mut rd, reg_index, &rn, rn_index);
        } else if instr.mask(NEONCopyInsGeneralMask) == NEON_INS_GENERAL {
            let value = self.xreg(instr.rn());
            self.ins_immediate(vf, &mut rd, reg_index, value);
        } else if instr.mask(NEONCopyUmovMask) == NEON_UMOV {
            let mut value = rn.uint(vf, reg_index);
            value &= max_uint_from_format(vf);
            self.set_xreg(instr.rd(), value);
            return;
        } else if instr.mask(NEONCopyUmovMask) == NEON_SMOV {
            let value = rn.int(vf, reg_index);
            if instr.neon_q() != 0 {
                self.set_xreg(instr.rd(), value as u64);
            } else {
                debug_assert!(crate::utils::utils::is_int32(value));
                self.set_wreg(instr.rd(), value as i32 as u32);
            }
            return;
        } else if instr.mask(NEONCopyDupElementMask) == NEON_DUP_ELEMENT {
            self.dup_element(vf, &mut rd, &rn, reg_index);
        } else if instr.mask(NEONCopyDupGeneralMask) == NEON_DUP_GENERAL {
            let value = self.xreg(instr.rn());
            self.dup_immediate(vf, &mut rd, value);
        } else {
            self.visit_unimplemented(instr);
            return;
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_extract(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::logical_format_map()]);
        let vf = nfd.get_vector_format(0);
        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rm = self.vreg_logic(instr.rm());
        if instr.mask(NEONExtractMask) == NEON_EXT {
            let index = instr.imm_neon_ext() as i32;
            self.ext(vf, &mut rd, &rn, &rm, index);
            self.set_vreg_logic(instr.rd(), rd);
        } else {
            self.visit_unimplemented(instr);
        }
    }

    fn visit_neon_perm(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rm = self.vreg_logic(instr.rm());

        match instr.mask(NEONPermMask) {
            NEON_TRN1 => self.trn1(vf, &mut rd, &rn, &rm),
            NEON_TRN2 => self.trn2(vf, &mut rd, &rn, &rm),
            NEON_UZP1 => self.uzp1(vf, &mut rd, &rn, &rm),
            NEON_UZP2 => self.uzp2(vf, &mut rd, &rn, &rm),
            NEON_ZIP1 => self.zip1(vf, &mut rd, &rn, &rm),
            NEON_ZIP2 => self.zip2(vf, &mut rd, &rn, &rm),
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_table(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::logical_format_map()]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rn2 = self.vreg_logic((instr.rn() + 1) % kNumberOfVRegisters as u32);
        let rn3 = self.vreg_logic((instr.rn() + 2) % kNumberOfVRegisters as u32);
        let rn4 = self.vreg_logic((instr.rn() + 3) % kNumberOfVRegisters as u32);
        let rm = self.vreg_logic(instr.rm());

        match instr.mask(NEONTableMask) {
            NEON_TBL_1v => self.tbl(vf, &mut rd, &[&rn], &rm),
            NEON_TBL_2v => self.tbl(vf, &mut rd, &[&rn, &rn2], &rm),
            NEON_TBL_3v => self.tbl(vf, &mut rd, &[&rn, &rn2, &rn3], &rm),
            NEON_TBL_4v => self.tbl(vf, &mut rd, &[&rn, &rn2, &rn3, &rn4], &rm),
            NEON_TBX_1v => self.tbx(vf, &mut rd, &[&rn], &rm),
            NEON_TBX_2v => self.tbx(vf, &mut rd, &[&rn, &rn2], &rm),
            NEON_TBX_3v => self.tbx(vf, &mut rd, &[&rn, &rn2, &rn3], &rm),
            NEON_TBX_4v => self.tbx(vf, &mut rd, &[&rn, &rn2, &rn3, &rn4], &rm),
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neonsha3(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rm = self.vreg_logic(instr.rm());
        let ra = self.vreg_logic(instr.ra());
        let mut temp = LogicVRegister::default();

        match instr.mask(NEONSHA3Mask) {
            NEON_BCAX => {
                self.bic(vf, &mut temp, &rm, &ra);
                let temp = temp;
                self.eor(vf, &mut rd, &rn, &temp);
            }
            NEON_EOR3 => {
                self.eor(vf, &mut temp, &rm, &ra);
                let temp = temp;
                self.eor(vf, &mut rd, &rn, &temp);
            }
            NEON_XAR => {
                let rot = instr.bits(15, 10) as i32;
                self.eor(VectorFormat::Format2D, &mut temp, &rn, &rm);
                let temp = temp;
                self.ror(VectorFormat::Format2D, &mut rd, &temp, rot);
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_sve_bit_perm(&mut self, instr: Instruction) {
        self.visit_unimplemented(instr);
    }

    fn visit_cpy(&mut self, instr: Instruction) {
        self.visit_unimplemented(instr);
    }

    fn visit_set(&mut self, instr: Instruction) {
        self.visit_unimplemented(instr);
    }

    fn visit_neon_by_indexed_element(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[]);
        let vf_r = nfd.get_vector_format(0);
        let mut vf = nfd.get_vector_format_from_map(NEONFormatDecoder::long_integer_format_map());

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());

        let mut rm_reg = instr.rm();
        let mut index = ((instr.neon_h() << 1) | instr.neon_l()) as i32;
        if instr.neon_size() == 1 {
            rm_reg &= 0xF;
            index = (index << 1) | instr.neon_m() as i32;
        }
        let q = instr.mask(NEON_Q) != 0;

        enum ByElement {
            Mul,
            Mla,
            Mls,
            Sqdmulh,
            Sqrdmulh,
            Smull,
            Smull2,
            Umull,
            Umull2,
            Smlal,
            Smlal2,
            Umlal,
            Umlal2,
            Smlsl,
            Smlsl2,
            Umlsl,
            Umlsl2,
            Sqdmull,
            Sqdmull2,
            Sqdmlal,
            Sqdmlal2,
            Sqdmlsl,
            Sqdmlsl2,
            Fmul,
            Fmla,
            Fmls,
            Fmulx,
        }

        let op = match instr.mask(NEONByIndexedElementMask) {
            NEON_MUL_byelement => {
                vf = vf_r;
                ByElement::Mul
            }
            NEON_MLA_byelement => {
                vf = vf_r;
                ByElement::Mla
            }
            NEON_MLS_byelement => {
                vf = vf_r;
                ByElement::Mls
            }
            NEON_SQDMULH_byelement => {
                vf = vf_r;
                ByElement::Sqdmulh
            }
            NEON_SQRDMULH_byelement => {
                vf = vf_r;
                ByElement::Sqrdmulh
            }
            NEON_SMULL_byelement => {
                if q {
                    ByElement::Smull2
                } else {
                    ByElement::Smull
                }
            }
            NEON_UMULL_byelement => {
                if q {
                    ByElement::Umull2
                } else {
                    ByElement::Umull
                }
            }
            NEON_SMLAL_byelement => {
                if q {
                    ByElement::Smlal2
                } else {
                    ByElement::Smlal
                }
            }
            NEON_UMLAL_byelement => {
                if q {
                    ByElement::Umlal2
                } else {
                    ByElement::Umlal
                }
            }
            NEON_SMLSL_byelement => {
                if q {
                    ByElement::Smlsl2
                } else {
                    ByElement::Smlsl
                }
            }
            NEON_UMLSL_byelement => {
                if q {
                    ByElement::Umlsl2
                } else {
                    ByElement::Umlsl
                }
            }
            NEON_SQDMULL_byelement => {
                if q {
                    ByElement::Sqdmull2
                } else {
                    ByElement::Sqdmull
                }
            }
            NEON_SQDMLAL_byelement => {
                if q {
                    ByElement::Sqdmlal2
                } else {
                    ByElement::Sqdmlal
                }
            }
            NEON_SQDMLSL_byelement => {
                if q {
                    ByElement::Sqdmlsl2
                } else {
                    ByElement::Sqdmlsl
                }
            }
            _ => {
                index = instr.neon_h() as i32;
                if (instr.fp_type() & 1) == 0 {
                    index = (index << 1) | instr.neon_l() as i32;
                }

                vf = nfd.get_vector_format_from_map(NEONFormatDecoder::fp_format_map());

                match instr.mask(NEONByIndexedElementFPMask) {
                    NEON_FMUL_byelement => ByElement::Fmul,
                    NEON_FMLA_byelement => ByElement::Fmla,
                    NEON_FMLS_byelement => ByElement::Fmls,
                    NEON_FMULX_byelement => ByElement::Fmulx,
                    _ => {
                        self.visit_unimplemented(instr);
                        return;
                    }
                }
            }
        };

        let rm = self.vreg_logic(rm_reg);
        match op {
            ByElement::Mul => self.mul_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Mla => self.mla_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Mls => self.mls_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqdmulh => self.sqdmulh_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqrdmulh => self.sqrdmulh_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Smull => self.smull_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Smull2 => self.smull2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Umull => self.umull_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Umull2 => self.umull2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Smlal => self.smlal_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Smlal2 => self.smlal2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Umlal => self.umlal_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Umlal2 => self.umlal2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Smlsl => self.smlsl_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Smlsl2 => self.smlsl2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Umlsl => self.umlsl_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Umlsl2 => self.umlsl2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqdmull => self.sqdmull_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqdmull2 => self.sqdmull2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqdmlal => self.sqdmlal_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqdmlal2 => self.sqdmlal2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqdmlsl => self.sqdmlsl_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Sqdmlsl2 => self.sqdmlsl2_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Fmul => self.fmul_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Fmla => self.fmla_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Fmls => self.fmls_by_element(vf, &mut rd, &rn, &rm, index),
            ByElement::Fmulx => self.fmulx_by_element(vf, &mut rd, &rn, &rm, index),
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_shift_immediate(&mut self, instr: Instruction) {
        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let fpcr_rounding = FPRounding::FPTieEven;

        let nfd = NEONFormatDecoder::new(instr, &[shift_immediate_format_map()]);
        let mut vf = nfd.get_vector_format(0);
        let vf_l = nfd.get_vector_format_from_map(shift_immediate_long_format_map());

        let highest_set_bit = highest_set_bit_position(instr.imm_neon_immh() as u64);
        let immhimmb = instr.imm_neon_immh_immb() as i32;
        let right_shift = (16 << highest_set_bit) - immhimmb;
        let left_shift = immhimmb - (8 << highest_set_bit);
        let q = instr.mask(NEON_Q) != 0;

        match instr.mask(NEONShiftImmediateMask) {
            NEON_SHL => self.shl(vf, &mut rd, &rn, left_shift),
            NEON_SLI => self.sli(vf, &mut rd, &rn, left_shift),
            NEON_SQSHLU => self.sqshlu(vf, &mut rd, &rn, left_shift),
            NEON_SRI => self.sri(vf, &mut rd, &rn, right_shift),
            NEON_SSHR => self.sshr(vf, &mut rd, &rn, right_shift),
            NEON_USHR => self.ushr(vf, &mut rd, &rn, right_shift),
            NEON_SRSHR => {
                self.sshr(vf, &mut rd, &rn, right_shift);
                rd.round(vf);
            }
            NEON_URSHR => {
                self.ushr(vf, &mut rd, &rn, right_shift);
                rd.round(vf);
            }
            NEON_SSRA => self.ssra(vf, &mut rd, &rn, right_shift),
            NEON_USRA => self.usra(vf, &mut rd, &rn, right_shift),
            NEON_SRSRA => self.srsra(vf, &mut rd, &rn, right_shift),
            NEON_URSRA => self.ursra(vf, &mut rd, &rn, right_shift),
            NEON_SQSHL_imm => self.sqshl(vf, &mut rd, &rn, left_shift),
            NEON_UQSHL_imm => self.uqshl(vf, &mut rd, &rn, left_shift),
            NEON_SCVTF_imm => self.scvtf(vf, &mut rd, &rn, right_shift, fpcr_rounding),
            NEON_UCVTF_imm => self.ucvtf(vf, &mut rd, &rn, right_shift, fpcr_rounding),
            NEON_FCVTZS_imm => self.fcvts(vf, &mut rd, &rn, FPRounding::FPZero, right_shift),
            NEON_FCVTZU_imm => self.fcvtu(vf, &mut rd, &rn, FPRounding::FPZero, right_shift),
            NEON_SSHLL => {
                vf = vf_l;
                if q {
                    self.sshll2(vf, &mut rd, &rn, left_shift);
                } else {
                    self.sshll(vf, &mut rd, &rn, left_shift);
                }
            }
            NEON_USHLL => {
                vf = vf_l;
                if q {
                    self.ushll2(vf, &mut rd, &rn, left_shift);
                } else {
                    self.ushll(vf, &mut rd, &rn, left_shift);
                }
            }
            NEON_SHRN => {
                if q {
                    self.shrn2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.shrn(vf, &mut rd, &rn, right_shift);
                }
            }
            NEON_RSHRN => {
                if q {
                    self.rshrn2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.rshrn(vf, &mut rd, &rn, right_shift);
                }
            }
            NEON_UQSHRN => {
                if q {
                    self.uqshrn2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.uqshrn(vf, &mut rd, &rn, right_shift);
                }
            }
            NEON_UQRSHRN => {
                if q {
                    self.uqrshrn2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.uqrshrn(vf, &mut rd, &rn, right_shift);
                }
            }
            NEON_SQSHRN => {
                if q {
                    self.sqshrn2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.sqshrn(vf, &mut rd, &rn, right_shift);
                }
            }
            NEON_SQRSHRN => {
                if q {
                    self.sqrshrn2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.sqrshrn(vf, &mut rd, &rn, right_shift);
                }
            }
            NEON_SQSHRUN => {
                if q {
                    self.sqshrun2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.sqshrun(vf, &mut rd, &rn, right_shift);
                }
            }
            NEON_SQRSHRUN => {
                if q {
                    self.sqrshrun2(vf, &mut rd, &rn, right_shift);
                } else {
                    self.sqrshrun(vf, &mut rd, &rn, right_shift);
                }
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_modified_immediate(&mut self, instr: Instruction) {
        let mut rd = self.vreg_logic(instr.rd());
        let cmode = instr.neon_cmode();
        let cmode_3_1 = (cmode >> 1) & 7;
        let cmode_3 = (cmode >> 3) & 1;
        let cmode_2 = (cmode >> 2) & 1;
        let cmode_1 = (cmode >> 1) & 1;
        let cmode_0 = cmode & 1;
        let q = instr.neon_q();
        let op_bit = instr.neon_mod_imm_op();
        let imm8 = instr.imm_neon_abcdefgh() as u64;

        let mut imm: u64;
        let vform;
        match cmode_3_1 {
            0x0 | 0x1 | 0x2 | 0x3 => {
                vform = if q == 1 {
                    VectorFormat::Format4S
                } else {
                    VectorFormat::Format2S
                };
                imm = imm8 << (8 * cmode_3_1);
            }
            0x4 | 0x5 => {
                vform = if q == 1 {
                    VectorFormat::Format8H
                } else {
                    VectorFormat::Format4H
                };
                imm = imm8 << (8 * cmode_1);
            }
            0x6 => {
                vform = if q == 1 {
                    VectorFormat::Format4S
                } else {
                    VectorFormat::Format2S
                };
                if cmode_0 == 0 {
                    imm = imm8 << 8 | 0x000000FF;
                } else {
                    imm = imm8 << 16 | 0x0000FFFF;
                }
            }
            0x7 => {
                if cmode_0 == 0 && op_bit == 0 {
                    vform = if q == 1 {
                        VectorFormat::Format16B
                    } else {
                        VectorFormat::Format8B
                    };
                    imm = imm8;
                } else if cmode_0 == 0 && op_bit == 1 {
                    vform = if q == 1 {
                        VectorFormat::Format2D
                    } else {
                        VectorFormat::Format1D
                    };
                    imm = 0;
                    for i in 0..8 {
                        if (imm8 & (1u64 << i)) != 0 {
                            imm |= 0xFFu64 << (8 * i);
                        }
                    }
                } else if op_bit == 0 {
                    vform = if q == 1 {
                        VectorFormat::Format4S
                    } else {
                        VectorFormat::Format2S
                    };
                    imm = instr.imm_neon_fp32().to_bits() as u64;
                } else if q == 1 {
                    vform = VectorFormat::Format2D;
                    imm = instr.imm_neon_fp64().to_bits();
                } else {
                    debug_assert!((q == 0) && (op_bit == 1) && (cmode == 0xF));
                    self.visit_unallocated(instr);
                    return;
                }
            }
            _ => unreachable!(),
        }

        let op;
        if cmode_3 == 0 {
            if cmode_0 == 0 {
                op = if op_bit != 0 {
                    NEONModifiedImmediate_MVNI
                } else {
                    NEONModifiedImmediate_MOVI
                };
            } else {
                op = if op_bit != 0 {
                    NEONModifiedImmediate_BIC
                } else {
                    NEONModifiedImmediate_ORR
                };
            }
        } else if cmode_2 == 0 {
            if cmode_0 == 0 {
                op = if op_bit != 0 {
                    NEONModifiedImmediate_MVNI
                } else {
                    NEONModifiedImmediate_MOVI
                };
            } else {
                op = if op_bit != 0 {
                    NEONModifiedImmediate_BIC
                } else {
                    NEONModifiedImmediate_ORR
                };
            }
        } else if cmode_1 == 0 {
            op = if op_bit != 0 {
                NEONModifiedImmediate_MVNI
            } else {
                NEONModifiedImmediate_MOVI
            };
        } else {
            op = NEONModifiedImmediate_MOVI;
        }

        match op {
            NEONModifiedImmediate_ORR => {
                let previous = rd;
                self.orr_immediate(vform, &mut rd, &previous, imm);
            }
            NEONModifiedImmediate_BIC => {
                let previous = rd;
                self.bic_immediate(vform, &mut rd, &previous, imm);
            }
            NEONModifiedImmediate_MOVI => self.movi(vform, &mut rd, imm),
            NEONModifiedImmediate_MVNI => self.mvni(vform, &mut rd, imm),
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_scalar2_reg_misc(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::scalar_format_map()]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());

        if instr.mask(NEON2RegMiscOpcode) <= NEON_NEG_scalar_opcode {
            match instr.mask(NEONScalar2RegMiscMask) {
                NEON_CMEQ_zero_scalar => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Eq),
                NEON_CMGE_zero_scalar => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Ge),
                NEON_CMGT_zero_scalar => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Gt),
                NEON_CMLT_zero_scalar => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Lt),
                NEON_CMLE_zero_scalar => self.cmp_immediate(vf, &mut rd, &rn, 0, Condition::Le),
                NEON_ABS_scalar => self.abs(vf, &mut rd, &rn),
                NEON_SQABS_scalar => {
                    self.abs(vf, &mut rd, &rn);
                    rd.signed_saturate(vf);
                }
                NEON_NEG_scalar => self.neg(vf, &mut rd, &rn),
                NEON_SQNEG_scalar => {
                    self.neg(vf, &mut rd, &rn);
                    rd.signed_saturate(vf);
                }
                NEON_SUQADD_scalar => self.suqadd(vf, &mut rd, &rn),
                NEON_USQADD_scalar => self.usqadd(vf, &mut rd, &rn),
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
            self.set_vreg_logic(instr.rd(), rd);
            return;
        }

        let fpf = nfd.get_vector_format_from_map(NEONFormatDecoder::fp_scalar_format_map());
        let fpcr_rounding = FPRounding::FPTieEven;

        match instr.mask(NEONScalar2RegMiscFPMask) {
            NEON_FRECPE_scalar => self.frecpe(fpf, &mut rd, &rn, fpcr_rounding),
            NEON_FRECPX_scalar => self.frecpx(fpf, &mut rd, &rn),
            NEON_FRSQRTE_scalar => self.frsqrte(fpf, &mut rd, &rn),
            NEON_FCMGT_zero_scalar => self.fcmp_zero(fpf, &mut rd, &rn, Condition::Gt),
            NEON_FCMGE_zero_scalar => self.fcmp_zero(fpf, &mut rd, &rn, Condition::Ge),
            NEON_FCMEQ_zero_scalar => self.fcmp_zero(fpf, &mut rd, &rn, Condition::Eq),
            NEON_FCMLE_zero_scalar => self.fcmp_zero(fpf, &mut rd, &rn, Condition::Le),
            NEON_FCMLT_zero_scalar => self.fcmp_zero(fpf, &mut rd, &rn, Condition::Lt),
            NEON_SCVTF_scalar => self.scvtf(fpf, &mut rd, &rn, 0, fpcr_rounding),
            NEON_UCVTF_scalar => self.ucvtf(fpf, &mut rd, &rn, 0, fpcr_rounding),
            NEON_FCVTNS_scalar => self.fcvts(fpf, &mut rd, &rn, FPRounding::FPTieEven, 0),
            NEON_FCVTNU_scalar => self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPTieEven, 0),
            NEON_FCVTPS_scalar => self.fcvts(fpf, &mut rd, &rn, FPRounding::FPPositiveInfinity, 0),
            NEON_FCVTPU_scalar => self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPPositiveInfinity, 0),
            NEON_FCVTMS_scalar => self.fcvts(fpf, &mut rd, &rn, FPRounding::FPNegativeInfinity, 0),
            NEON_FCVTMU_scalar => self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPNegativeInfinity, 0),
            NEON_FCVTZS_scalar => self.fcvts(fpf, &mut rd, &rn, FPRounding::FPZero, 0),
            NEON_FCVTZU_scalar => self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPZero, 0),
            NEON_FCVTAS_scalar => self.fcvts(fpf, &mut rd, &rn, FPRounding::FPTieAway, 0),
            NEON_FCVTAU_scalar => self.fcvtu(fpf, &mut rd, &rn, FPRounding::FPTieAway, 0),
            NEON_FCVTXN_scalar => {
                debug_assert_eq!(instr.bit(22), 1);
                self.fcvtxn(VectorFormat::FormatS, &mut rd, &rn);
            }
            _ => match instr.mask(NEONScalar2RegMiscMask) {
                NEON_SQXTN_scalar => self.sqxtn(vf, &mut rd, &rn),
                NEON_UQXTN_scalar => self.uqxtn(vf, &mut rd, &rn),
                NEON_SQXTUN_scalar => self.sqxtun(vf, &mut rd, &rn),
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            },
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_scalar3_diff(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::long_scalar_format_map()]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rm = self.vreg_logic(instr.rm());
        match instr.mask(NEONScalar3DiffMask) {
            NEON_SQDMLAL_scalar => self.sqdmlal(vf, &mut rd, &rn, &rm),
            NEON_SQDMLSL_scalar => self.sqdmlsl(vf, &mut rd, &rn, &rm),
            NEON_SQDMULL_scalar => self.sqdmull(vf, &mut rd, &rn, &rm),
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_scalar3_same(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::scalar_format_map()]);
        let mut vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let rm = self.vreg_logic(instr.rm());

        if instr.mask(NEONScalar3SameFPFMask) == NEONScalar3SameFPFixed {
            vf = nfd.get_vector_format_from_map(NEONFormatDecoder::fp_scalar_format_map());
            match instr.mask(NEONScalar3SameFPMask) {
                NEON_FMULX_scalar => self.fmulx(vf, &mut rd, &rn, &rm),
                NEON_FACGE_scalar => self.fabscmp(vf, &mut rd, &rn, &rm, Condition::Ge),
                NEON_FACGT_scalar => self.fabscmp(vf, &mut rd, &rn, &rm, Condition::Gt),
                NEON_FCMEQ_scalar => self.fcmp(vf, &mut rd, &rn, &rm, Condition::Eq),
                NEON_FCMGE_scalar => self.fcmp(vf, &mut rd, &rn, &rm, Condition::Ge),
                NEON_FCMGT_scalar => self.fcmp(vf, &mut rd, &rn, &rm, Condition::Gt),
                NEON_FRECPS_scalar => self.frecps(vf, &mut rd, &rn, &rm),
                NEON_FRSQRTS_scalar => self.frsqrts(vf, &mut rd, &rn, &rm),
                NEON_FABD_scalar => self.fabd(vf, &mut rd, &rn, &rm),
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
        } else {
            match instr.mask(NEONScalar3SameMask) {
                NEON_ADD_scalar => self.add(vf, &mut rd, &rn, &rm),
                NEON_SUB_scalar => self.sub(vf, &mut rd, &rn, &rm),
                NEON_CMEQ_scalar => self.cmp(vf, &mut rd, &rn, &rm, Condition::Eq),
                NEON_CMGE_scalar => self.cmp(vf, &mut rd, &rn, &rm, Condition::Ge),
                NEON_CMGT_scalar => self.cmp(vf, &mut rd, &rn, &rm, Condition::Gt),
                NEON_CMHI_scalar => self.cmp(vf, &mut rd, &rn, &rm, Condition::Hi),
                NEON_CMHS_scalar => self.cmp(vf, &mut rd, &rn, &rm, Condition::Hs),
                NEON_CMTST_scalar => self.cmptst(vf, &mut rd, &rn, &rm),
                NEON_USHL_scalar => self.ushl(vf, &mut rd, &rn, &rm),
                NEON_SSHL_scalar => self.sshl(vf, &mut rd, &rn, &rm),
                NEON_SQDMULH_scalar => self.sqdmulh(vf, &mut rd, &rn, &rm),
                NEON_SQRDMULH_scalar => self.sqrdmulh(vf, &mut rd, &rn, &rm),
                NEON_UQADD_scalar => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQADD_scalar => {
                    self.add(vf, &mut rd, &rn, &rm);
                    rd.signed_saturate(vf);
                }
                NEON_UQSUB_scalar => {
                    self.sub(vf, &mut rd, &rn, &rm);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQSUB_scalar => {
                    self.sub(vf, &mut rd, &rn, &rm);
                    rd.signed_saturate(vf);
                }
                NEON_UQSHL_scalar => {
                    self.ushl(vf, &mut rd, &rn, &rm);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQSHL_scalar => {
                    self.sshl(vf, &mut rd, &rn, &rm);
                    rd.signed_saturate(vf);
                }
                NEON_URSHL_scalar => {
                    self.ushl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                }
                NEON_SRSHL_scalar => {
                    self.sshl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                }
                NEON_UQRSHL_scalar => {
                    self.ushl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                    rd.unsigned_saturate(vf);
                }
                NEON_SQRSHL_scalar => {
                    self.sshl(vf, &mut rd, &rn, &rm);
                    rd.round(vf);
                    rd.signed_saturate(vf);
                }
                _ => {
                    self.visit_unimplemented(instr);
                    return;
                }
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_scalar_by_indexed_element(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::long_scalar_format_map()]);
        let mut vf = nfd.get_vector_format(0);
        let vf_r = nfd.get_vector_format_from_map(NEONFormatDecoder::scalar_format_map());

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());

        let mut rm_reg = instr.rm();
        let mut index = ((instr.neon_h() << 1) | instr.neon_l()) as i32;
        if instr.neon_size() == 1 {
            rm_reg &= 0xF;
            index = (index << 1) | instr.neon_m() as i32;
        }

        enum ScalarByElement {
            Sqdmull,
            Sqdmlal,
            Sqdmlsl,
            Sqdmulh,
            Sqrdmulh,
            Fmul,
            Fmla,
            Fmls,
            Fmulx,
        }

        let op = match instr.mask(NEONScalarByIndexedElementMask) {
            NEON_SQDMULL_byelement_scalar => ScalarByElement::Sqdmull,
            NEON_SQDMLAL_byelement_scalar => ScalarByElement::Sqdmlal,
            NEON_SQDMLSL_byelement_scalar => ScalarByElement::Sqdmlsl,
            NEON_SQDMULH_byelement_scalar => {
                vf = vf_r;
                ScalarByElement::Sqdmulh
            }
            NEON_SQRDMULH_byelement_scalar => {
                vf = vf_r;
                ScalarByElement::Sqrdmulh
            }
            _ => {
                vf = nfd.get_vector_format_from_map(NEONFormatDecoder::fp_scalar_format_map());
                index = instr.neon_h() as i32;
                if (instr.fp_type() & 1) == 0 {
                    index = (index << 1) | instr.neon_l() as i32;
                }
                match instr.mask(NEONScalarByIndexedElementFPMask) {
                    NEON_FMUL_byelement_scalar => ScalarByElement::Fmul,
                    NEON_FMLA_byelement_scalar => ScalarByElement::Fmla,
                    NEON_FMLS_byelement_scalar => ScalarByElement::Fmls,
                    NEON_FMULX_byelement_scalar => ScalarByElement::Fmulx,
                    _ => {
                        self.visit_unimplemented(instr);
                        return;
                    }
                }
            }
        };

        let rm = self.vreg_logic(rm_reg);
        match op {
            ScalarByElement::Sqdmull => self.sqdmull_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Sqdmlal => self.sqdmlal_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Sqdmlsl => self.sqdmlsl_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Sqdmulh => self.sqdmulh_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Sqrdmulh => self.sqrdmulh_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Fmul => self.fmul_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Fmla => self.fmla_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Fmls => self.fmls_by_element(vf, &mut rd, &rn, &rm, index),
            ScalarByElement::Fmulx => self.fmulx_by_element(vf, &mut rd, &rn, &rm, index),
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_scalar_copy(&mut self, instr: Instruction) {
        let nfd =
            NEONFormatDecoder::new(instr, &[NEONFormatDecoder::triangular_scalar_format_map()]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());

        if instr.mask(NEONScalarCopyMask) == NEON_DUP_ELEMENT_scalar {
            let imm5 = instr.imm_neon5();
            let lsb = lowest_set_bit_position(imm5 as u64);
            let rn_index = (imm5 >> lsb) as i32;
            self.dup_element(vf, &mut rd, &rn, rn_index);
            self.set_vreg_logic(instr.rd(), rd);
        } else {
            self.visit_unimplemented(instr);
        }
    }

    fn visit_neon_scalar_pairwise(&mut self, instr: Instruction) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::fp_scalar_format_map()]);
        let vf = nfd.get_vector_format(0);

        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        match instr.mask(NEONScalarPairwiseMask) {
            NEON_ADDP_scalar => self.addp_scalar(vf, &mut rd, &rn),
            NEON_FADDP_scalar => self.faddp_scalar(vf, &mut rd, &rn),
            NEON_FMAXP_scalar => self.fmaxp_scalar(vf, &mut rd, &rn),
            NEON_FMAXNMP_scalar => self.fmaxnmp_scalar(vf, &mut rd, &rn),
            NEON_FMINP_scalar => self.fminp_scalar(vf, &mut rd, &rn),
            NEON_FMINNMP_scalar => self.fminnmp_scalar(vf, &mut rd, &rn),
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_scalar_shift_immediate(&mut self, instr: Instruction) {
        let mut rd = self.vreg_logic(instr.rd());
        let rn = self.vreg_logic(instr.rn());
        let fpcr_rounding = FPRounding::FPTieEven;

        let nfd = NEONFormatDecoder::new(instr, &[scalar_shift_immediate_format_map()]);
        let vf = nfd.get_vector_format(0);

        let highest_set_bit = highest_set_bit_position(instr.imm_neon_immh() as u64);
        let immhimmb = instr.imm_neon_immh_immb() as i32;
        let right_shift = (16 << highest_set_bit) - immhimmb;
        let left_shift = immhimmb - (8 << highest_set_bit);
        match instr.mask(NEONScalarShiftImmediateMask) {
            NEON_SHL_scalar => self.shl(vf, &mut rd, &rn, left_shift),
            NEON_SLI_scalar => self.sli(vf, &mut rd, &rn, left_shift),
            NEON_SQSHL_imm_scalar => self.sqshl(vf, &mut rd, &rn, left_shift),
            NEON_UQSHL_imm_scalar => self.uqshl(vf, &mut rd, &rn, left_shift),
            NEON_SQSHLU_scalar => self.sqshlu(vf, &mut rd, &rn, left_shift),
            NEON_SRI_scalar => self.sri(vf, &mut rd, &rn, right_shift),
            NEON_SSHR_scalar => self.sshr(vf, &mut rd, &rn, right_shift),
            NEON_USHR_scalar => self.ushr(vf, &mut rd, &rn, right_shift),
            NEON_SRSHR_scalar => {
                self.sshr(vf, &mut rd, &rn, right_shift);
                rd.round(vf);
            }
            NEON_URSHR_scalar => {
                self.ushr(vf, &mut rd, &rn, right_shift);
                rd.round(vf);
            }
            NEON_SSRA_scalar => self.ssra(vf, &mut rd, &rn, right_shift),
            NEON_USRA_scalar => self.usra(vf, &mut rd, &rn, right_shift),
            NEON_SRSRA_scalar => self.srsra(vf, &mut rd, &rn, right_shift),
            NEON_URSRA_scalar => self.ursra(vf, &mut rd, &rn, right_shift),
            NEON_UQSHRN_scalar => self.uqshrn(vf, &mut rd, &rn, right_shift),
            NEON_UQRSHRN_scalar => self.uqrshrn(vf, &mut rd, &rn, right_shift),
            NEON_SQSHRN_scalar => self.sqshrn(vf, &mut rd, &rn, right_shift),
            NEON_SQRSHRN_scalar => self.sqrshrn(vf, &mut rd, &rn, right_shift),
            NEON_SQSHRUN_scalar => self.sqshrun(vf, &mut rd, &rn, right_shift),
            NEON_SQRSHRUN_scalar => self.sqrshrun(vf, &mut rd, &rn, right_shift),
            NEON_FCVTZS_imm_scalar => {
                self.fcvts(vf, &mut rd, &rn, FPRounding::FPZero, right_shift)
            }
            NEON_FCVTZU_imm_scalar => {
                self.fcvtu(vf, &mut rd, &rn, FPRounding::FPZero, right_shift)
            }
            NEON_SCVTF_imm_scalar => self.scvtf(vf, &mut rd, &rn, right_shift, fpcr_rounding),
            NEON_UCVTF_imm_scalar => self.ucvtf(vf, &mut rd, &rn, right_shift, fpcr_rounding),
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }
        self.set_vreg_logic(instr.rd(), rd);
    }

    fn visit_neon_load_store_multi_struct(&mut self, instr: Instruction) {
        self.neon_load_store_multi_struct_helper(instr, AddrMode::Offset);
    }

    fn visit_neon_load_store_multi_struct_post_index(&mut self, instr: Instruction) {
        self.neon_load_store_multi_struct_helper(instr, AddrMode::PostIndex);
    }

    fn visit_neon_load_store_single_struct(&mut self, instr: Instruction) {
        self.neon_load_store_single_struct_helper(instr, AddrMode::Offset);
    }

    fn visit_neon_load_store_single_struct_post_index(&mut self, instr: Instruction) {
        self.neon_load_store_single_struct_helper(instr, AddrMode::PostIndex);
    }

    fn visit_min_max_immediate(&mut self, instr: Instruction) {
        if instr.bit(18) == 1 {
            self.simulate_unsigned_min_max(instr);
        } else {
            self.simulate_signed_min_max(instr);
        }
    }

    fn visit_load_store_acquire_release(&mut self, instr: Instruction) {
        let rt = instr.rt();
        let rn = instr.rn();
        let op = instr.mask(LoadStoreAcquireReleaseMask);

        match op {
            CAS_w | CASA_w | CASL_w | CASAL_w => {
                self.compare_and_swap_helper(instr, 4, false);
                return;
            }
            CAS_x | CASA_x | CASL_x | CASAL_x => {
                self.compare_and_swap_helper(instr, 8, false);
                return;
            }
            CASB | CASAB | CASLB | CASALB => {
                self.compare_and_swap_helper(instr, 1, false);
                return;
            }
            CASH | CASAH | CASLH | CASALH => {
                self.compare_and_swap_helper(instr, 2, false);
                return;
            }
            CASP_w | CASPA_w | CASPL_w | CASPAL_w => {
                self.compare_and_swap_helper(instr, 4, true);
                return;
            }
            CASP_x | CASPA_x | CASPL_x | CASPAL_x => {
                self.compare_and_swap_helper(instr, 8, true);
                return;
            }
            _ => (),
        }

        let is_exclusive = instr.bits(LoadStoreXNotExclusive_offset, LoadStoreXNotExclusive_offset)
            == 0;
        let is_load = instr.bits(LoadStoreXLoad_offset, LoadStoreXLoad_offset) != 0;
        let access_size = 1usize
            << instr.bits(
                LoadStoreXSizeLog2_offset + 1,
                LoadStoreXSizeLog2_offset,
            );
        let address = self.reg_with_mode(rn, Reg31Mode::Reg31IsStackPointer);
        debug_assert_eq!(address as usize % access_size, 0);

        if is_load {
            let value = self.memory_read(address, access_size);
            match access_size {
                8 => self.set_xreg(rt, value),
                _ => self.set_wreg(rt, value as u32),
            }
        } else if is_exclusive {
            let rs = instr.rs();
            debug_assert_ne!(rs, rt);
            debug_assert_ne!(rs, rn);
            let value = self.xreg(rt);
            self.memory_write(address, value, access_size);
            self.set_wreg(rs, 0);
        } else {
            let value = self.xreg(rt);
            self.memory_write(address, value, access_size);
        }
    }

    fn visit_atomic_memory(&mut self, instr: Instruction) {
        self.atomic_memory_helper(instr);
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

fn map_lp() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [23, 22, 30, 0, 0, 0],
        &[NF_4H, NF_8H, NF_2S, NF_4S, NF_1D, NF_2D],
    );
    &map
}

fn map_fcvtl() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([22, 0, 0, 0, 0, 0], &[NF_4S, NF_2D]);
    &map
}

fn map_fcvtn() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap =
        neon_format_map([22, 30, 0, 0, 0, 0], &[NF_4H, NF_8H, NF_2S, NF_4S]);
    &map
}

fn shift_immediate_format_map() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [22, 21, 20, 19, 30, 0],
        &[
            NF_UNDEF, NF_UNDEF, NF_8B, NF_16B, NF_4H, NF_8H, NF_4H, NF_8H, NF_2S, NF_4S, NF_2S,
            NF_4S, NF_2S, NF_4S, NF_2S, NF_4S, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF,
            NF_2D, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF,
            NF_2D,
        ],
    );
    &map
}

fn shift_immediate_long_format_map() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [22, 21, 20, 19, 0, 0],
        &[NF_UNDEF, NF_8H, NF_4S, NF_4S, NF_2D, NF_2D, NF_2D, NF_2D],
    );
    &map
}

fn scalar_shift_immediate_format_map() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [22, 21, 20, 19, 0, 0],
        &[
            NF_UNDEF, NF_B, NF_H, NF_H, NF_S, NF_S, NF_S, NF_S, NF_D, NF_D, NF_D, NF_D, NF_D,
            NF_D, NF_D, NF_D,
        ],
    );
    &map
}

impl Simulator {
    fn visit_neon3_same_fp(
        &mut self,
        op: Instr,
        vf: VectorFormat,
        rd: &mut LogicVRegister,
        rn: &LogicVRegister,
        rm: &LogicVRegister,
    ) -> bool {
        match op {
            NEON_FADD => self.fadd(vf, rd, rn, rm),
            NEON_FSUB => self.fsub(vf, rd, rn, rm),
            NEON_FMUL => self.fmul(vf, rd, rn, rm),
            NEON_FDIV => self.fdiv(vf, rd, rn, rm),
            NEON_FMAX => self.fmax(vf, rd, rn, rm),
            NEON_FMIN => self.fmin(vf, rd, rn, rm),
            NEON_FMAXNM => self.fmaxnm(vf, rd, rn, rm),
            NEON_FMINNM => self.fminnm(vf, rd, rn, rm),
            NEON_FMLA => self.fmla(vf, rd, rn, rm),
            NEON_FMLS => self.fmls(vf, rd, rn, rm),
            NEON_FMULX => self.fmulx(vf, rd, rn, rm),
            NEON_FACGE => self.fabscmp(vf, rd, rn, rm, Condition::Ge),
            NEON_FACGT => self.fabscmp(vf, rd, rn, rm, Condition::Gt),
            NEON_FCMEQ => self.fcmp(vf, rd, rn, rm, Condition::Eq),
            NEON_FCMGE => self.fcmp(vf, rd, rn, rm, Condition::Ge),
            NEON_FCMGT => self.fcmp(vf, rd, rn, rm, Condition::Gt),
            NEON_FRECPS => self.frecps(vf, rd, rn, rm),
            NEON_FRSQRTS => self.frsqrts(vf, rd, rn, rm),
            NEON_FABD => self.fabd(vf, rd, rn, rm),
            NEON_FADDP => self.faddp(vf, rd, rn, rm),
            NEON_FMAXP => self.fmaxp(vf, rd, rn, rm),
            NEON_FMAXNMP => self.fmaxnmp(vf, rd, rn, rm),
            NEON_FMINP => self.fminp(vf, rd, rn, rm),
            NEON_FMINNMP => self.fminnmp(vf, rd, rn, rm),
            _ => return false,
        }
        true
    }

    fn neon_load_store_multi_struct_helper(&mut self, instr: Instruction, addr_mode: AddrMode) {
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::load_store_format_map()]);
        let vf = nfd.get_vector_format(0);

        let mut addr_base = self.reg_with_mode(instr.rn(), Reg31Mode::Reg31IsStackPointer);
        let reg_size = register_size_in_bytes_from_format(vf) as u64;

        let mut reg = [0u32; 4];
        let mut addr = [0u64; 4];
        for i in 0..4 {
            reg[i] = (instr.rt() + i as u32) % kNumberOfVRegisters as u32;
            addr[i] = addr_base + (i as u64 * reg_size);
        }
        let mut count = 1;

        if (instr.bit(23) == 0) && (instr.bits(20, 16) != 0) {
            unreachable!();
        }

        let single = |sim: &mut Simulator, index: usize, load: bool| {
            let mut regs = [sim.vreg_logic(reg[index])];
            if load {
                sim.ld1(vf, &mut regs, addr[index]);
                sim.set_vreg_logic(reg[index], regs[0]);
            } else {
                sim.st1(vf, &mut regs, addr[index]);
            }
        };

        match instr.mask(NEONLoadStoreMultiStructPostIndexMask) {
            NEON_LD1_4v | NEON_LD1_4v_post => {
                single(self, 3, true);
                single(self, 2, true);
                single(self, 1, true);
                single(self, 0, true);
                count = 4;
            }
            NEON_LD1_3v | NEON_LD1_3v_post => {
                single(self, 2, true);
                single(self, 1, true);
                single(self, 0, true);
                count = 3;
            }
            NEON_LD1_2v | NEON_LD1_2v_post => {
                single(self, 1, true);
                single(self, 0, true);
                count = 2;
            }
            NEON_LD1_1v | NEON_LD1_1v_post => {
                single(self, 0, true);
            }
            NEON_ST1_4v | NEON_ST1_4v_post => {
                single(self, 3, false);
                single(self, 2, false);
                single(self, 1, false);
                single(self, 0, false);
                count = 4;
            }
            NEON_ST1_3v | NEON_ST1_3v_post => {
                single(self, 2, false);
                single(self, 1, false);
                single(self, 0, false);
                count = 3;
            }
            NEON_ST1_2v | NEON_ST1_2v_post => {
                single(self, 1, false);
                single(self, 0, false);
                count = 2;
            }
            NEON_ST1_1v | NEON_ST1_1v_post => {
                single(self, 0, false);
            }
            NEON_LD2 | NEON_LD2_post => {
                let mut regs = [self.vreg_logic(reg[0]), self.vreg_logic(reg[1])];
                self.ld2(vf, &mut regs, addr[0]);
                for i in 0..2 {
                    self.set_vreg_logic(reg[i], regs[i]);
                }
                count = 2;
            }
            NEON_ST2 | NEON_ST2_post => {
                let mut regs = [self.vreg_logic(reg[0]), self.vreg_logic(reg[1])];
                self.st2(vf, &mut regs, addr[0]);
                count = 2;
            }
            NEON_LD3 | NEON_LD3_post => {
                let mut regs = [
                    self.vreg_logic(reg[0]),
                    self.vreg_logic(reg[1]),
                    self.vreg_logic(reg[2]),
                ];
                self.ld3(vf, &mut regs, addr[0]);
                for i in 0..3 {
                    self.set_vreg_logic(reg[i], regs[i]);
                }
                count = 3;
            }
            NEON_ST3 | NEON_ST3_post => {
                let mut regs = [
                    self.vreg_logic(reg[0]),
                    self.vreg_logic(reg[1]),
                    self.vreg_logic(reg[2]),
                ];
                self.st3(vf, &mut regs, addr[0]);
                count = 3;
            }
            NEON_LD4 | NEON_LD4_post => {
                let mut regs = [
                    self.vreg_logic(reg[0]),
                    self.vreg_logic(reg[1]),
                    self.vreg_logic(reg[2]),
                    self.vreg_logic(reg[3]),
                ];
                self.ld4(vf, &mut regs, addr[0]);
                for i in 0..4 {
                    self.set_vreg_logic(reg[i], regs[i]);
                }
                count = 4;
            }
            NEON_ST4 | NEON_ST4_post => {
                let mut regs = [
                    self.vreg_logic(reg[0]),
                    self.vreg_logic(reg[1]),
                    self.vreg_logic(reg[2]),
                    self.vreg_logic(reg[3]),
                ];
                self.st4(vf, &mut regs, addr[0]);
                count = 4;
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        if addr_mode == AddrMode::PostIndex {
            let rm = instr.rm();
            addr_base += if rm == 31 {
                register_size_in_bytes_from_format(vf) as u64 * count as u64
            } else {
                self.xreg(rm)
            };
            self.set_reg_with_mode(instr.rn(), addr_base, Reg31Mode::Reg31IsStackPointer);
        } else {
            debug_assert_eq!(addr_mode, AddrMode::Offset);
        }
    }

    fn neon_load_store_single_struct_helper(&mut self, instr: Instruction, addr_mode: AddrMode) {
        let addr = self.reg_with_mode(instr.rn(), Reg31Mode::Reg31IsStackPointer);
        let rt = instr.rt();

        debug_assert!(instr.bit(23) != 0 || instr.bits(20, 16) == 0);

        let mut do_load = false;

        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::load_store_format_map()]);
        let vf_t = nfd.get_vector_format(0);

        let mut vf = VectorFormat::Format16B;
        let rt2 = (rt + 1) % kNumberOfVRegisters as u32;
        let rt3 = (rt2 + 1) % kNumberOfVRegisters as u32;
        let rt4 = (rt3 + 1) % kNumberOfVRegisters as u32;

        match instr.mask(NEONLoadStoreSingleStructPostIndexMask) {
            NEON_LD1_b | NEON_LD1_b_post | NEON_LD2_b | NEON_LD2_b_post | NEON_LD3_b
            | NEON_LD3_b_post | NEON_LD4_b | NEON_LD4_b_post => {
                do_load = true;
            }
            NEON_ST1_b | NEON_ST1_b_post | NEON_ST2_b | NEON_ST2_b_post | NEON_ST3_b
            | NEON_ST3_b_post | NEON_ST4_b | NEON_ST4_b_post => (),

            NEON_LD1_h | NEON_LD1_h_post | NEON_LD2_h | NEON_LD2_h_post | NEON_LD3_h
            | NEON_LD3_h_post | NEON_LD4_h | NEON_LD4_h_post => {
                do_load = true;
                vf = VectorFormat::Format8H;
            }
            NEON_ST1_h | NEON_ST1_h_post | NEON_ST2_h | NEON_ST2_h_post | NEON_ST3_h
            | NEON_ST3_h_post | NEON_ST4_h | NEON_ST4_h_post => {
                vf = VectorFormat::Format8H;
            }

            NEON_LD1_s | NEON_LD1_s_post | NEON_LD2_s | NEON_LD2_s_post | NEON_LD3_s
            | NEON_LD3_s_post | NEON_LD4_s | NEON_LD4_s_post => {
                do_load = true;
                vf = if (instr.neon_ls_size() & 1) == 0 {
                    VectorFormat::Format4S
                } else {
                    VectorFormat::Format2D
                };
            }
            NEON_ST1_s | NEON_ST1_s_post | NEON_ST2_s | NEON_ST2_s_post | NEON_ST3_s
            | NEON_ST3_s_post | NEON_ST4_s | NEON_ST4_s_post => {
                vf = if (instr.neon_ls_size() & 1) == 0 {
                    VectorFormat::Format4S
                } else {
                    VectorFormat::Format2D
                };
            }

            NEON_LD1R | NEON_LD1R_post => {
                vf = vf_t;
                let mut regs = [self.vreg_logic(rt)];
                self.ld1r(vf, &mut regs, addr);
                self.set_vreg_logic(rt, regs[0]);
                self.advance_single_struct(instr, addr, addr_mode, vf, 1);
                return;
            }
            NEON_LD2R | NEON_LD2R_post => {
                vf = vf_t;
                let mut regs = [self.vreg_logic(rt), self.vreg_logic(rt2)];
                self.ld2r(vf, &mut regs, addr);
                self.set_vreg_logic(rt, regs[0]);
                self.set_vreg_logic(rt2, regs[1]);
                self.advance_single_struct(instr, addr, addr_mode, vf, 2);
                return;
            }
            NEON_LD3R | NEON_LD3R_post => {
                vf = vf_t;
                let mut regs = [
                    self.vreg_logic(rt),
                    self.vreg_logic(rt2),
                    self.vreg_logic(rt3),
                ];
                self.ld3r(vf, &mut regs, addr);
                self.set_vreg_logic(rt, regs[0]);
                self.set_vreg_logic(rt2, regs[1]);
                self.set_vreg_logic(rt3, regs[2]);
                self.advance_single_struct(instr, addr, addr_mode, vf, 3);
                return;
            }
            NEON_LD4R | NEON_LD4R_post => {
                vf = vf_t;
                let mut regs = [
                    self.vreg_logic(rt),
                    self.vreg_logic(rt2),
                    self.vreg_logic(rt3),
                    self.vreg_logic(rt4),
                ];
                self.ld4r(vf, &mut regs, addr);
                self.set_vreg_logic(rt, regs[0]);
                self.set_vreg_logic(rt2, regs[1]);
                self.set_vreg_logic(rt3, regs[2]);
                self.set_vreg_logic(rt4, regs[3]);
                self.advance_single_struct(instr, addr, addr_mode, vf, 4);
                return;
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        let index_shift = lane_size_in_bytes_log2_from_format(vf);
        let lane = instr.neon_ls_index(index_shift);
        let scale;
        match instr.mask(NEONLoadStoreSingleLenMask) {
            NEONLoadStoreSingle1 => {
                scale = 1;
                let mut regs = [self.vreg_logic(rt)];
                if do_load {
                    self.ld1_index(vf, &mut regs, lane, addr);
                    self.set_vreg_logic(rt, regs[0]);
                } else {
                    self.st1_index(vf, &mut regs, lane, addr);
                }
            }
            NEONLoadStoreSingle2 => {
                scale = 2;
                let mut regs = [self.vreg_logic(rt), self.vreg_logic(rt2)];
                if do_load {
                    self.ld2_index(vf, &mut regs, lane, addr);
                    self.set_vreg_logic(rt, regs[0]);
                    self.set_vreg_logic(rt2, regs[1]);
                } else {
                    self.st2_index(vf, &mut regs, lane, addr);
                }
            }
            NEONLoadStoreSingle3 => {
                scale = 3;
                let mut regs = [
                    self.vreg_logic(rt),
                    self.vreg_logic(rt2),
                    self.vreg_logic(rt3),
                ];
                if do_load {
                    self.ld3_index(vf, &mut regs, lane, addr);
                    self.set_vreg_logic(rt, regs[0]);
                    self.set_vreg_logic(rt2, regs[1]);
                    self.set_vreg_logic(rt3, regs[2]);
                } else {
                    self.st3_index(vf, &mut regs, lane, addr);
                }
            }
            NEONLoadStoreSingle4 => {
                scale = 4;
                let mut regs = [
                    self.vreg_logic(rt),
                    self.vreg_logic(rt2),
                    self.vreg_logic(rt3),
                    self.vreg_logic(rt4),
                ];
                if do_load {
                    self.ld4_index(vf, &mut regs, lane, addr);
                    self.set_vreg_logic(rt, regs[0]);
                    self.set_vreg_logic(rt2, regs[1]);
                    self.set_vreg_logic(rt3, regs[2]);
                    self.set_vreg_logic(rt4, regs[3]);
                } else {
                    self.st4_index(vf, &mut regs, lane, addr);
                }
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        }

        self.advance_single_struct(instr, addr, addr_mode, vf, scale);
    }

    fn advance_single_struct(
        &mut self,
        instr: Instruction,
        addr: u64,
        addr_mode: AddrMode,
        vf: VectorFormat,
        scale: u64,
    ) {
        if addr_mode == AddrMode::PostIndex {
            let rm = instr.rm();
            let lane_size = lane_size_in_bytes_from_format(vf) as u64;
            let offset = if rm == 31 {
                scale * lane_size
            } else {
                self.xreg(rm)
            };
            self.set_reg_with_mode(
                instr.rn(),
                addr + offset,
                Reg31Mode::Reg31IsStackPointer,
            );
        }
    }

    fn simulate_signed_min_max(&mut self, instr: Instruction) {
        let wn = self.wreg(instr.rn()) as i32;
        let mut wm = self.wreg(instr.rm()) as i32;
        let xn = self.xreg(instr.rn()) as i64;
        let mut xm = self.xreg(instr.rm()) as i64;
        let imm = instr.signed_bits(17, 10);
        let dst = instr.rd();

        if instr.mask(MinMaxImmediateFMask) == MinMaxImmediateFixed {
            debug_assert_eq!(instr.bit(18), 0);
            wm = imm;
            if instr.sixty_four_bits() == 1 {
                xm = imm as i64;
            }

            match instr.mask(MinMaxImmediateMask) {
                SMAX_w_imm => self.set_wreg(dst, std::cmp::max(wn, wm) as u32),
                SMAX_x_imm => self.set_xreg(dst, std::cmp::max(xn, xm) as u64),
                SMIN_w_imm => self.set_wreg(dst, std::cmp::min(wn, wm) as u32),
                SMIN_x_imm => self.set_xreg(dst, std::cmp::min(xn, xm) as u64),
                _ => unreachable!(),
            }
        } else {
            match instr.mask(DataProcessing2SourceMask) {
                SMAX_w => self.set_wreg(dst, std::cmp::max(wn, wm) as u32),
                SMAX_x => self.set_xreg(dst, std::cmp::max(xn, xm) as u64),
                SMIN_w => self.set_wreg(dst, std::cmp::min(wn, wm) as u32),
                SMIN_x => self.set_xreg(dst, std::cmp::min(xn, xm) as u64),
                _ => unreachable!(),
            }
        }
    }

    fn simulate_unsigned_min_max(&mut self, instr: Instruction) {
        let mut xn = self.xreg(instr.rn());
        let mut xm = self.xreg(instr.rm());
        let imm = instr.bits(17, 10) as u64;
        let dst = instr.rd();

        if instr.mask(MinMaxImmediateFMask) == MinMaxImmediateFixed {
            debug_assert_eq!(instr.bit(18), 1);
            xm = imm;

            match instr.mask(MinMaxImmediateMask) {
                UMAX_w_imm => {
                    xn = xn as u32 as u64;
                    self.set_xreg(dst, std::cmp::max(xn, xm));
                }
                UMAX_x_imm => self.set_xreg(dst, std::cmp::max(xn, xm)),
                UMIN_w_imm => {
                    xn = xn as u32 as u64;
                    self.set_xreg(dst, std::cmp::min(xn, xm));
                }
                UMIN_x_imm => self.set_xreg(dst, std::cmp::min(xn, xm)),
                _ => unreachable!(),
            }
        } else {
            match instr.mask(DataProcessing2SourceMask) {
                UMAX_w => {
                    xm = xm as u32 as u64;
                    xn = xn as u32 as u64;
                    self.set_xreg(dst, std::cmp::max(xn, xm));
                }
                UMAX_x => self.set_xreg(dst, std::cmp::max(xn, xm)),
                UMIN_w => {
                    xm = xm as u32 as u64;
                    xn = xn as u32 as u64;
                    self.set_xreg(dst, std::cmp::min(xn, xm));
                }
                UMIN_x => self.set_xreg(dst, std::cmp::min(xn, xm)),
                _ => unreachable!(),
            }
        }
    }

    fn compare_and_swap_helper(&mut self, instr: Instruction, element_size: usize, pair: bool) {
        let rs = instr.rs();
        let rt = instr.rt();
        let rn = instr.rn();

        let address = self.reg_with_mode(rn, Reg31Mode::Reg31IsStackPointer);

        if pair {
            let rs2 = (rs + 1) % kNumberOfRegisters as u32;
            let rt2 = (rt + 1) % kNumberOfRegisters as u32;
            let comparevalue = self.reg_of_size(rs, element_size);
            let comparevalue2 = self.reg_of_size(rs2, element_size);
            let newvalue = self.reg_of_size(rt, element_size);
            let newvalue2 = self.reg_of_size(rt2, element_size);

            let data = self.memory_read(address, element_size);
            let data2 = self.memory_read(address + element_size as u64, element_size);
            if (data == comparevalue) && (data2 == comparevalue2) {
                self.memory_write(address, newvalue, element_size);
                self.memory_write(address + element_size as u64, newvalue2, element_size);
            }
            self.set_reg_of_size(rs, data, element_size);
            self.set_reg_of_size(rs2, data2, element_size);
            return;
        }

        let comparevalue = self.reg_of_size(rs, element_size);
        let newvalue = self.reg_of_size(rt, element_size);

        let data = self.memory_read(address, element_size);
        if data == comparevalue {
            self.memory_write(address, newvalue, element_size);
        }
        self.set_reg_of_size(rs, data, element_size);
    }

    fn atomic_memory_helper(&mut self, instr: Instruction) {
        let rs = instr.rs();
        let rt = instr.rt();
        let rn = instr.rn();

        let element_size = 1usize << instr.bits(31, 30);
        let address = self.reg_with_mode(rn, Reg31Mode::Reg31IsStackPointer);
        debug_assert_eq!(address as usize % element_size, 0);

        let value = self.reg_of_size(rs, element_size);
        let data = self.memory_read(address, element_size);

        if instr.bits(15, 12) == 8 {
            self.memory_write(address, value, element_size);
            self.set_reg_of_size(rt, data, element_size);
            return;
        }

        let sign_extend = |v: u64| -> i64 {
            let shift = 64 - 8 * element_size;
            ((v << shift) as i64) >> shift
        };

        let result = match instr.mask(AtomicMemorySimpleOpMask) {
            LDADDOp => data.wrapping_add(value),
            LDCLROp => data & !value,
            LDEOROp => data ^ value,
            LDSETOp => data | value,
            LDSMAXOp => {
                if sign_extend(data) > sign_extend(value) {
                    data
                } else {
                    value
                }
            }
            LDSMINOp => {
                if sign_extend(data) > sign_extend(value) {
                    value
                } else {
                    data
                }
            }
            LDUMAXOp => {
                if data > value {
                    data
                } else {
                    value
                }
            }
            LDUMINOp => {
                if data > value {
                    value
                } else {
                    data
                }
            }
            _ => {
                self.visit_unimplemented(instr);
                return;
            }
        };

        self.memory_write(address, result, element_size);
        self.set_reg_of_size(rt, data, element_size);
    }

    fn reg_of_size(&self, code: u32, size_in_bytes: usize) -> u64 {
        let value = self.xreg(code);
        if size_in_bytes == 8 {
            value
        } else {
            value & ((1u64 << (8 * size_in_bytes)) - 1)
        }
    }

    fn set_reg_of_size(&mut self, code: u32, value: u64, size_in_bytes: usize) {
        if size_in_bytes == 8 {
            self.set_xreg(code, value);
        } else {
            self.set_wreg(code, value as u32);
        }
    }
}

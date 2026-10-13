#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::constants_arm64::{SystemHint, SystemRegister};
use crate::codegen::arm64::decoder_arm64::DecoderVisitor;
use crate::codegen::arm64::instructions_arm64::{
    neon_format_map, Instruction, NEONFormat, NEONFormatDecoder, NEONFormatMap, SubstitutionMode,
};
use crate::codegen::arm64::register_arm64::{sp, xzr, CPURegister, RegisterType, VectorFormat};
use crate::codegen::arm64::utils_arm64::{count_set_bits, highest_set_bit_position};

pub struct DisassemblingDecoder {
    buffer: String,
}

impl Default for DisassemblingDecoder {
    fn default() -> DisassemblingDecoder {
        DisassemblingDecoder::new()
    }
}

impl DisassemblingDecoder {
    pub fn new() -> DisassemblingDecoder {
        DisassemblingDecoder {
            buffer: String::new(),
        }
    }

    pub fn get_output(&self) -> &str {
        &self.buffer
    }

    fn reset_output(&mut self) {
        self.buffer.clear();
    }

    fn append_to_output(&mut self, text: &str) {
        self.buffer.push_str(text);
    }

    fn append_register_name_to_output(&mut self, reg: CPURegister) {
        debug_assert!(reg.is_valid());
        let reg_char;

        if reg.is_register() {
            reg_char = if reg.is_64_bits() { 'x' } else { 'w' };
        } else {
            debug_assert!(reg.is_v_register());
            reg_char = match reg.size_in_bits() {
                x if x == kBRegSizeInBits as u32 => 'b',
                x if x == kHRegSizeInBits as u32 => 'h',
                x if x == kSRegSizeInBits as u32 => 's',
                x if x == kDRegSizeInBits as u32 => 'd',
                _ => {
                    debug_assert!(reg.is_128_bits());
                    'q'
                }
            };
        }

        if reg.is_v_register() || !(reg.aliases(*sp) || reg.aliases(*xzr)) {
            if reg.is_x() && (reg.code() == 27) {
                self.append_to_output("cp");
            } else if reg.is_x() && (reg.code() == 29) {
                self.append_to_output("fp");
            } else if reg.is_x() && (reg.code() == 30) {
                self.append_to_output("lr");
            } else {
                let text = format!("{}{}", reg_char, reg.code());
                self.append_to_output(&text);
            }
        } else if reg.aliases(*sp) {
            self.append_to_output(if reg.is_64_bits() { "sp" } else { "wsp" });
        } else {
            let text = format!("{}zr", reg_char);
            self.append_to_output(&text);
        }
    }

    fn format(&mut self, instr: Instruction, mnemonic: &str, form: &str) {
        self.reset_output();
        self.substitute(instr, mnemonic);
        if !form.is_empty() {
            self.buffer.push(' ');
            self.substitute(instr, form);
        }
    }

    fn rd_is_zr_or_sp(&self, instr: Instruction) -> bool {
        instr.rd() == kZeroRegCode as u32
    }

    fn rn_is_zr_or_sp(&self, instr: Instruction) -> bool {
        instr.rn() == kZeroRegCode as u32
    }

    fn rm_is_zr_or_sp(&self, instr: Instruction) -> bool {
        instr.rm() == kZeroRegCode as u32
    }

    fn ra_is_zr_or_sp(&self, instr: Instruction) -> bool {
        instr.ra() == kZeroRegCode as u32
    }

    fn is_movz_movn_imm(&self, reg_size: u32, value: u64) -> bool {
        debug_assert!(
            (reg_size == kXRegSizeInBits as u32)
                || ((reg_size == kWRegSizeInBits as u32) && (value <= 0xFFFFFFFF))
        );

        if reg_size == kXRegSizeInBits as u32 {
            if ((value & 0xFFFFFFFFFFFF0000) == 0)
                || ((value & 0xFFFFFFFF0000FFFF) == 0)
                || ((value & 0xFFFF0000FFFFFFFF) == 0)
                || ((value & 0x0000FFFFFFFFFFFF) == 0)
            {
                return true;
            }
            let value = !value;
            if ((value & 0xFFFFFFFFFFFF0000) == 0)
                || ((value & 0xFFFFFFFF0000FFFF) == 0)
                || ((value & 0xFFFF0000FFFFFFFF) == 0)
                || ((value & 0x0000FFFFFFFFFFFF) == 0)
            {
                return true;
            }
        } else {
            debug_assert_eq!(reg_size, kWRegSizeInBits as u32);
            let value32 = value as u32;
            if ((value32 & 0xFFFF0000) == 0) || ((value32 & 0x0000FFFF) == 0) {
                return true;
            }
            let value32 = !value32;
            if ((value32 & 0xFFFF0000) == 0) || ((value32 & 0x0000FFFF) == 0) {
                return true;
            }
        }
        false
    }

    fn substitute(&mut self, instr: Instruction, string: &str) {
        let bytes = string.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\'' {
                i += 1;
                i += self.substitute_field(instr, &string[i..]);
            } else {
                self.buffer.push(bytes[i] as char);
                i += 1;
            }
        }
    }

    fn substitute_field(&mut self, instr: Instruction, format: &str) -> usize {
        match format.as_bytes()[0] {
            b'R' | b'F' | b'V' | b'Z' | b'W' | b'X' | b'B' | b'H' | b'S' | b'D' | b'Q' => {
                self.substitute_register_field(instr, format)
            }
            b'I' => self.substitute_immediate_field(instr, format),
            b'L' => self.substitute_literal_field(instr, format),
            b'N' => self.substitute_shift_field(instr, format),
            b'P' => self.substitute_prefetch_field(instr, format),
            b'C' => self.substitute_condition_field(instr, format),
            b'E' => self.substitute_extend_field(instr, format),
            b'A' => self.substitute_pc_rel_address_field(instr, format),
            b'T' => self.substitute_branch_target_field(instr, format),
            b'O' => self.substitute_ls_reg_offset_field(instr, format),
            b'M' => self.substitute_barrier_field(instr, format),
            b'u' => self.substitute_int_field(instr, format),
            _ => unreachable!(),
        }
    }

    fn substitute_register_field(&mut self, instr: Instruction, format: &str) -> usize {
        let f = format.as_bytes();
        let mut reg_prefix = f[0];
        let mut reg_num;
        let mut field_len = 2;

        match f[1] {
            b'd' => {
                reg_num = instr.rd();
                if f.len() > 2 && f[2] == b'q' {
                    reg_prefix = if instr.neon_q() != 0 { b'X' } else { b'W' };
                    field_len = 3;
                }
            }
            b'n' => reg_num = instr.rn(),
            b'm' => {
                reg_num = instr.rm();
                if f.len() > 2 && (f[2] == b'r' || f[2] == b'b' || f[2] == b'z') {
                    field_len = 3;
                    let digits: String = format[3..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    field_len += digits.len();
                    let mut imm: i32 = digits.parse().unwrap_or(0);
                    if reg_num == 31 {
                        match f[2] {
                            b'z' => imm *= 1 << instr.neon_ls_size(),
                            b'r' => {
                                imm *= if instr.neon_q() == 0 {
                                    kDRegSize
                                } else {
                                    kQRegSize
                                }
                            }
                            _ => (),
                        }
                        let text = format!("#{}", imm);
                        self.append_to_output(&text);
                        return field_len;
                    }
                }
            }
            b'e' => reg_num = instr.rm() & 0xF,
            b'a' => reg_num = instr.ra(),
            b't' => {
                reg_num = instr.rt();
                if f[0] == b'V' {
                    if f.len() > 2 && (f[2] >= b'2') && (f[2] <= b'4') {
                        reg_num = (reg_num + (f[2] - b'1') as u32) % 32;
                        field_len = 3;
                    }
                } else if f.len() > 2 && f[2] == b'2' {
                    reg_num = instr.rt2();
                    field_len = 3;
                }
            }
            b's' => reg_num = instr.rs(),
            _ => unreachable!(),
        }

        if f.len() > 2 && f[2] == b's' {
            field_len = 3;
        }

        if f.len() > 2 && f[2] == b'+' {
            debug_assert!((reg_prefix == b'W') || (reg_prefix == b'X'));
            reg_num += 1;
            field_len += 1;
        }

        let reg_type;
        let reg_size;

        if reg_prefix == b'R' {
            reg_prefix = if instr.sixty_four_bits() != 0 {
                b'X'
            } else {
                b'W'
            };
        } else if reg_prefix == b'F' {
            reg_prefix = if (instr.fp_type() & 1) == 0 {
                b'S'
            } else {
                b'D'
            };
        }

        match reg_prefix {
            b'W' => {
                reg_type = RegisterType::Register;
                reg_size = kWRegSizeInBits;
            }
            b'X' => {
                reg_type = RegisterType::Register;
                reg_size = kXRegSizeInBits;
            }
            b'B' => {
                reg_type = RegisterType::VRegister;
                reg_size = kBRegSizeInBits;
            }
            b'H' => {
                reg_type = RegisterType::VRegister;
                reg_size = kHRegSizeInBits;
            }
            b'S' => {
                reg_type = RegisterType::VRegister;
                reg_size = kSRegSizeInBits;
            }
            b'D' => {
                reg_type = RegisterType::VRegister;
                reg_size = kDRegSizeInBits;
            }
            b'Q' => {
                reg_type = RegisterType::VRegister;
                reg_size = kQRegSizeInBits;
            }
            b'V' => {
                let text = format!("v{}", reg_num);
                self.append_to_output(&text);
                return field_len;
            }
            b'Z' => {
                let text = format!("z{}", reg_num);
                self.append_to_output(&text);
                return field_len;
            }
            _ => unreachable!(),
        }

        if (reg_type == RegisterType::Register)
            && (reg_num == kZeroRegCode as u32)
            && (f.len() > 2 && f[2] == b's')
        {
            reg_num = kSPRegInternalCode as u32;
        }

        let reg = CPURegister::create(reg_num as i32, reg_size as u32, reg_type);
        self.append_register_name_to_output(reg);

        field_len
    }

    fn substitute_immediate_field(&mut self, instr: Instruction, format: &str) -> usize {
        let f = format.as_bytes();
        debug_assert_eq!(f[0], b'I');

        match f[1] {
            b'M' => {
                if f[2] == b'i' {
                    let imm = if instr.bit(18) == 1 {
                        instr.bits(17, 10) as i32
                    } else {
                        instr.signed_bits(17, 10)
                    };
                    let text = format!("#{}", imm);
                    self.append_to_output(&text);
                    return 7;
                }
                if f[5] == b'I' || f[5] == b'N' {
                    let mut imm = (instr.imm_move_wide() as u64) << (16 * instr.shift_move_wide());
                    if f[5] == b'N' {
                        imm = !imm;
                    }
                    if instr.sixty_four_bits() == 0 {
                        imm &= 0xFFFFFFFF;
                    }
                    let text = format!("#0x{:x}", imm);
                    self.append_to_output(&text);
                } else {
                    debug_assert_eq!(f[5], b'L');
                    let text = format!("#0x{:x}", instr.imm_move_wide());
                    self.append_to_output(&text);
                    if instr.shift_move_wide() > 0 {
                        let text = format!(", lsl #{}", 16 * instr.shift_move_wide());
                        self.append_to_output(&text);
                    }
                }
                8
            }
            b'L' => match f[2] {
                b'L' => {
                    let text = format!("pc{:+}", instr.imm_l_literal() * kLoadLiteralScale as i32);
                    self.append_to_output(&text);
                    9
                }
                b'S' => {
                    if instr.imm_ls() != 0 {
                        let text = format!(", #{}", instr.imm_ls());
                        self.append_to_output(&text);
                    }
                    3
                }
                b'P' => {
                    if instr.imm_ls_pair() != 0 {
                        let scale = 1 << (f[3] - b'0');
                        let text = format!(", #{}", instr.imm_ls_pair() * scale);
                        self.append_to_output(&text);
                    }
                    4
                }
                b'U' => {
                    if instr.imm_ls_unsigned() != 0 {
                        let shift = instr.size_ls();
                        let text = format!(", #{}", instr.imm_ls_unsigned() << shift);
                        self.append_to_output(&text);
                    }
                    3
                }
                _ => unreachable!(),
            },
            b'C' => {
                let offset = (instr.imm_cond_branch() as i64) << 2;
                let sign = if offset >= 0 { '+' } else { '-' };
                let text = format!("#{}0x{:x}", sign, offset.unsigned_abs());
                self.append_to_output(&text);
                6
            }
            b'A' => {
                debug_assert!(instr.shift_add_sub() <= 1);
                let imm = (instr.imm_add_sub() as i64) << (12 * instr.shift_add_sub());
                let text = format!("#0x{:x} ({})", imm, imm);
                self.append_to_output(&text);
                7
            }
            b'F' => {
                if f[3] == b'F' {
                    let text = format!("#{}", 64 - instr.fp_scale());
                    self.append_to_output(&text);
                    8
                } else {
                    let value = if f[3] == b'S' {
                        instr.imm_fp32() as f64
                    } else {
                        instr.imm_fp64()
                    };
                    let text = format!("#0x{:x} ({:.4})", instr.imm_fp(), value);
                    self.append_to_output(&text);
                    9
                }
            }
            b'T' => {
                let text = format!("#0x{:x}", instr.imm_logical());
                self.append_to_output(&text);
                4
            }
            b'N' => {
                let nzcv = instr.nzcv() << Flags_offset;
                let text = format!(
                    "#{}{}{}{}",
                    if (nzcv & NFlag) == 0 { 'n' } else { 'N' },
                    if (nzcv & ZFlag) == 0 { 'z' } else { 'Z' },
                    if (nzcv & CFlag) == 0 { 'c' } else { 'C' },
                    if (nzcv & VFlag) == 0 { 'v' } else { 'V' }
                );
                self.append_to_output(&text);
                5
            }
            b'P' => {
                let text = format!("#{}", instr.imm_cond_cmp());
                self.append_to_output(&text);
                2
            }
            b'B' => self.substitute_bitfield_immediate_field(instr, format),
            b'E' => {
                let text = format!("#{}", instr.imm_s());
                self.append_to_output(&text);
                8
            }
            b'S' => {
                let text = format!(
                    "#{}",
                    (instr.imm_test_branch_bit5() << 5) | instr.imm_test_branch_bit40()
                );
                self.append_to_output(&text);
                2
            }
            b's' => match f[2] {
                b'1' => {
                    let mut shift = 16 << highest_set_bit_position(instr.imm_neon_immh() as u64);
                    shift -= instr.imm_neon_immh_immb();
                    let text = format!("#{}", shift);
                    self.append_to_output(&text);
                    3
                }
                b'2' => {
                    let mut shift = instr.imm_neon_immh_immb();
                    shift -= 8 << highest_set_bit_position(instr.imm_neon_immh() as u64);
                    let text = format!("#{}", shift);
                    self.append_to_output(&text);
                    3
                }
                _ => unreachable!(),
            },
            b'D' => {
                let text = format!("#0x{:x}", instr.imm_exception());
                self.append_to_output(&text);
                6
            }
            b'V' => match f[2] {
                b'E' => {
                    let text = format!("#{}", instr.imm_neon_ext());
                    self.append_to_output(&text);
                    9
                }
                b'B' => {
                    let mut vm_index = (instr.neon_h() << 1) | instr.neon_l();
                    if instr.neon_size() == 1 {
                        vm_index = (vm_index << 1) | instr.neon_m();
                    }
                    let text = format!("{}", vm_index);
                    self.append_to_output(&text);
                    "IVByElemIndex".len()
                }
                b'I' => {
                    if format.starts_with("IVInsIndex") {
                        let imm5 = instr.imm_neon5();
                        let imm4 = instr.imm_neon4();
                        let tz = imm5.trailing_zeros();
                        if tz <= 3 {
                            let rd_index = imm5 >> (tz + 1);
                            let rn_index = imm4 >> tz;
                            if format.starts_with("IVInsIndex1") {
                                let text = format!("{}", rd_index);
                                self.append_to_output(&text);
                                return "IVInsIndex1".len();
                            } else if format.starts_with("IVInsIndex2") {
                                let text = format!("{}", rn_index);
                                self.append_to_output(&text);
                                return "IVInsIndex2".len();
                            }
                        }
                        return 0;
                    }
                    unreachable!()
                }
                b'L' => {
                    let text = format!("{}", instr.neon_ls_index((f[8] - b'0') as u32));
                    self.append_to_output(&text);
                    9
                }
                b'M' => {
                    if format.starts_with("IVMIImmFPSingle") {
                        let text = format!(
                            "#0x{:x} ({:.4})",
                            instr.imm_neon_abcdefgh(),
                            instr.imm_neon_fp32()
                        );
                        self.append_to_output(&text);
                        "IVMIImmFPSingle".len()
                    } else if format.starts_with("IVMIImmFPDouble") {
                        let text = format!(
                            "#0x{:x} ({:.4})",
                            instr.imm_neon_abcdefgh(),
                            instr.imm_neon_fp64()
                        );
                        self.append_to_output(&text);
                        "IVMIImmFPDouble".len()
                    } else if format.starts_with("IVMIImm8") {
                        let imm8 = instr.imm_neon_abcdefgh() as u64;
                        let text = format!("#0x{:x}", imm8);
                        self.append_to_output(&text);
                        "IVMIImm8".len()
                    } else if format.starts_with("IVMIImm") {
                        let imm8 = instr.imm_neon_abcdefgh() as u64;
                        let mut imm: u64 = 0;
                        for i in 0..8 {
                            if (imm8 & (1u64 << i)) != 0 {
                                imm |= 0xFFu64 << (8 * i);
                            }
                        }
                        let text = format!("#0x{:x}", imm);
                        self.append_to_output(&text);
                        "IVMIImm".len()
                    } else if format.starts_with("IVMIShiftAmt1") {
                        let cmode = instr.neon_cmode();
                        let shift_amount = 8 * ((cmode >> 1) & 3);
                        let text = format!("#{}", shift_amount);
                        self.append_to_output(&text);
                        "IVMIShiftAmt1".len()
                    } else if format.starts_with("IVMIShiftAmt2") {
                        let cmode = instr.neon_cmode();
                        let shift_amount = 8 << (cmode & 1);
                        let text = format!("#{}", shift_amount);
                        self.append_to_output(&text);
                        "IVMIShiftAmt2".len()
                    } else {
                        unreachable!()
                    }
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    fn substitute_bitfield_immediate_field(&mut self, instr: Instruction, format: &str) -> usize {
        let f = format.as_bytes();
        debug_assert!((f[0] == b'I') && (f[1] == b'B'));
        let r = instr.imm_r();
        let s = instr.imm_s();

        match f[2] {
            b'r' => {
                let text = format!("#{}", r);
                self.append_to_output(&text);
                3
            }
            b's' => {
                if f[3] == b'+' {
                    let text = format!("#{}", s + 1);
                    self.append_to_output(&text);
                    5
                } else {
                    debug_assert_eq!(f[3], b'-');
                    let text = format!("#{}", s - r + 1);
                    self.append_to_output(&text);
                    7
                }
            }
            b'Z' => {
                debug_assert!((f[3] == b'-') && (f[4] == b'r'));
                let reg_size = if instr.sixty_four_bits() == 1 {
                    kXRegSizeInBits
                } else {
                    kWRegSizeInBits
                };
                let text = format!("#{}", reg_size as u32 - r);
                self.append_to_output(&text);
                5
            }
            _ => unreachable!(),
        }
    }

    fn substitute_literal_field(&mut self, instr: Instruction, format: &str) -> usize {
        debug_assert!(format.starts_with("LValue"));

        match instr.mask(LoadLiteralMask) {
            LDR_w_lit | LDR_x_lit | LDR_s_lit | LDR_d_lit => {
                let text = format!("(pc{:+})", instr.imm_l_literal() * kLoadLiteralScale as i32);
                self.append_to_output(&text);
            }
            _ => unreachable!(),
        }

        6
    }

    fn substitute_shift_field(&mut self, instr: Instruction, format: &str) -> usize {
        let f = format.as_bytes();
        debug_assert_eq!(f[0], b'N');
        debug_assert!(instr.shift_dp() <= 0x3);

        match f[1] {
            b'D' | b'L' => {
                if instr.imm_dp_shift() != 0 {
                    let shift_type = ["lsl", "lsr", "asr", "ror"];
                    let text = format!(
                        ", {} #{}",
                        shift_type[instr.shift_dp() as usize],
                        instr.imm_dp_shift()
                    );
                    self.append_to_output(&text);
                }
                3
            }
            _ => unreachable!(),
        }
    }

    fn substitute_condition_field(&mut self, instr: Instruction, format: &str) -> usize {
        let f = format.as_bytes();
        debug_assert_eq!(f[0], b'C');
        let condition_code = [
            "eq", "ne", "hs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le",
            "al", "nv",
        ];
        let cond = match f[1] {
            b'B' => instr.condition_branch(),
            b'I' => negate_condition(condition_from_bits(instr.condition())) as u32,
            _ => instr.condition(),
        };
        self.append_to_output(condition_code[cond as usize]);
        4
    }

    fn substitute_pc_rel_address_field(&mut self, instr: Instruction, format: &str) -> usize {
        debug_assert!(format.starts_with("AddrPCRel"));

        let offset = instr.imm_pc_rel();

        let sign = if offset < 0 { '-' } else { '+' };
        let text = format!("#{}0x{:x}", sign, offset.unsigned_abs());
        self.append_to_output(&text);
        13
    }

    fn substitute_branch_target_field(&mut self, instr: Instruction, format: &str) -> usize {
        debug_assert!(format.starts_with("TImm"));
        let f = format.as_bytes();

        let mut offset: i64 = match f[5] {
            b'n' => instr.imm_uncond_branch() as i64,
            b'o' => instr.imm_cond_branch() as i64,
            b'm' => instr.imm_cmp_branch() as i64,
            b'e' => instr.imm_test_branch() as i64,
            _ => unreachable!(),
        };
        offset *= kInstrSize as i64;
        let sign = if offset < 0 { '-' } else { '+' };
        let text = format!("#{}0x{:x}", sign, offset.unsigned_abs());
        self.append_to_output(&text);
        8
    }

    fn substitute_extend_field(&mut self, instr: Instruction, format: &str) -> usize {
        debug_assert!(format.starts_with("Ext"));
        debug_assert!(instr.extend_mode() <= 7);

        let extend_mode = [
            "uxtb", "uxth", "uxtw", "uxtx", "sxtb", "sxth", "sxtw", "sxtx",
        ];

        if ((instr.rd() == kZeroRegCode as u32) || (instr.rn() == kZeroRegCode as u32))
            && (((instr.extend_mode() == Extend::Uxtw as u32) && (instr.sixty_four_bits() == 0))
                || (instr.extend_mode() == Extend::Uxtx as u32))
        {
            if instr.imm_extend_shift() > 0 {
                let text = format!(", lsl #{}", instr.imm_extend_shift());
                self.append_to_output(&text);
            }
        } else {
            let text = format!(", {}", extend_mode[instr.extend_mode() as usize]);
            self.append_to_output(&text);
            if instr.imm_extend_shift() > 0 {
                let text = format!(" #{}", instr.imm_extend_shift());
                self.append_to_output(&text);
            }
        }
        3
    }

    fn substitute_ls_reg_offset_field(&mut self, instr: Instruction, format: &str) -> usize {
        debug_assert!(format.starts_with("Offsetreg"));
        let extend_mode = [
            "undefined",
            "undefined",
            "uxtw",
            "lsl",
            "undefined",
            "undefined",
            "sxtw",
            "sxtx",
        ];

        let shift = instr.imm_shift_ls();
        let ext = instr.extend_mode();
        let reg_type = if (ext == Extend::Uxtw as u32) || (ext == Extend::Sxtw as u32) {
            'w'
        } else {
            'x'
        };

        let rm = instr.rm();
        if rm == kZeroRegCode as u32 {
            let text = format!("{}zr", reg_type);
            self.append_to_output(&text);
        } else {
            let text = format!("{}{}", reg_type, rm);
            self.append_to_output(&text);
        }

        if !((ext == Extend::Uxtx as u32) && (shift == 0)) {
            let text = format!(", {}", extend_mode[ext as usize]);
            self.append_to_output(&text);
            if shift != 0 {
                let text = format!(" #{}", instr.size_ls());
                self.append_to_output(&text);
            }
        }
        9
    }

    fn substitute_prefetch_field(&mut self, instr: Instruction, format: &str) -> usize {
        debug_assert_eq!(format.as_bytes()[0], b'P');

        let prefetch_mode = instr.prefetch_mode();

        let ls = if (prefetch_mode & 0x10) != 0 {
            "st"
        } else {
            "ld"
        };
        let level = ((prefetch_mode >> 1) & 3) + 1;
        let ks = if (prefetch_mode & 1) != 0 {
            "strm"
        } else {
            "keep"
        };

        let text = format!("p{}l{}{}", ls, level, ks);
        self.append_to_output(&text);
        6
    }

    fn substitute_barrier_field(&mut self, instr: Instruction, format: &str) -> usize {
        debug_assert_eq!(format.as_bytes()[0], b'M');

        let options = [
            ["sy (0b0000)", "oshld", "oshst", "osh"],
            ["sy (0b0100)", "nshld", "nshst", "nsh"],
            ["sy (0b1000)", "ishld", "ishst", "ish"],
            ["sy (0b1100)", "ld", "st", "sy"],
        ];
        let domain = instr.imm_barrier_domain();
        let barrier_type = instr.imm_barrier_type();

        self.append_to_output(options[domain as usize][barrier_type as usize]);
        1
    }

    fn substitute_int_field(&mut self, instr: Instruction, format: &str) -> usize {
        let f = format.as_bytes();
        debug_assert_eq!(f[0], b'u');
        let msb = ((f[1] - b'0') as u32 * 10) + (f[2] - b'0') as u32;
        let lsb = ((f[3] - b'0') as u32 * 10) + (f[4] - b'0') as u32;
        debug_assert!(msb >= lsb);
        debug_assert!(msb <= 32);
        let chunk_width = msb - lsb + 1;
        debug_assert!((chunk_width > 0) && (chunk_width < 32));
        let bits = instr.bits(msb, lsb);

        let text = format!("{}", bits);
        self.append_to_output(&text);
        5
    }

    fn disassemble_neon_polynomial_mul(&mut self, instr: Instruction) {
        let q = instr.bit(30) != 0;
        let mut mnemonic = if q { "pmull2" } else { "pmull" };
        let mut form = "";
        let size = instr.neon_size();
        if size == 0 {
            if !q {
                form = "'Vd.8h, 'Vn.8b, 'Vm.8b";
            } else {
                form = "'Vd.8h, 'Vn.16b, 'Vm.16b";
            }
        } else if size == 3 {
            if !q {
                form = "'Vd.1q, 'Vn.1d, 'Vm.1d";
            } else {
                form = "'Vd.1q, 'Vn.2d, 'Vm.2d";
            }
        } else {
            mnemonic = "undefined";
        }
        self.format(instr, mnemonic, form);
    }
}

fn visit_neon_2_reg_misc_map_lp_ta() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [23, 22, 30, 0, 0, 0],
        &[NF_4H, NF_8H, NF_2S, NF_4S, NF_1D, NF_2D],
    );
    &map
}

fn visit_neon_2_reg_misc_map_cvt_ta() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([22, 0, 0, 0, 0, 0], &[NF_4S, NF_2D]);
    &map
}

fn visit_neon_2_reg_misc_map_cvt_tb() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap =
        neon_format_map([22, 30, 0, 0, 0, 0], &[NF_4H, NF_8H, NF_2S, NF_4S]);
    &map
}

fn visit_neon_by_indexed_element_map_ta() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([23, 22, 0, 0, 0, 0], &[NF_UNDEF, NF_4S, NF_2D]);
    &map
}

fn visit_neon_modified_immediate_map_b() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([30, 0, 0, 0, 0, 0], &[NF_8B, NF_16B]);
    &map
}

fn visit_neon_modified_immediate_map_h() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([30, 0, 0, 0, 0, 0], &[NF_4H, NF_8H]);
    &map
}

fn visit_neon_modified_immediate_map_s() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([30, 0, 0, 0, 0, 0], &[NF_2S, NF_4S]);
    &map
}

fn visit_neon_scalar_pairwise_map() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([22, 0, 0, 0, 0, 0], &[NF_2S, NF_2D]);
    &map
}

fn visit_neon_scalar_shift_immediate_map_shift() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [22, 21, 20, 19, 0, 0],
        &[
            NF_UNDEF, NF_B, NF_H, NF_H, NF_S, NF_S, NF_S, NF_S, NF_D, NF_D, NF_D, NF_D, NF_D, NF_D,
            NF_D, NF_D,
        ],
    );
    &map
}

fn visit_neon_scalar_shift_immediate_map_shift_narrow() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [21, 20, 19, 0, 0, 0],
        &[NF_UNDEF, NF_H, NF_S, NF_S, NF_D, NF_D, NF_D, NF_D],
    );
    &map
}

fn visit_neon_shift_immediate_map_shift_ta() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [22, 21, 20, 19, 0, 0],
        &[NF_UNDEF, NF_8H, NF_4S, NF_4S, NF_2D, NF_2D, NF_2D, NF_2D],
    );
    &map
}

fn visit_neon_shift_immediate_map_shift_tb() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map(
        [22, 21, 20, 19, 30, 0],
        &[
            NF_UNDEF, NF_UNDEF, NF_8B, NF_16B, NF_4H, NF_8H, NF_4H, NF_8H, NF_2S, NF_4S, NF_2S,
            NF_4S, NF_2S, NF_4S, NF_2S, NF_4S, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D,
            NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D, NF_UNDEF, NF_2D,
        ],
    );
    &map
}

fn visit_neon_table_map_b() -> &'static NEONFormatMap {
    use NEONFormat::*;
    static map: NEONFormatMap = neon_format_map([30, 0, 0, 0, 0, 0], &[NF_8B, NF_16B]);
    &map
}

impl DecoderVisitor for DisassemblingDecoder {
    fn visit_add_sub_immediate(&mut self, instr: Instruction) {
        let rd_is_zr = self.rd_is_zr_or_sp(instr);
        let stack_op = (rd_is_zr || self.rn_is_zr_or_sp(instr)) && (instr.imm_add_sub() == 0);
        let mut mnemonic: &str;
        let mut form = "'Rds, 'Rns, 'IAddSub";
        let form_cmp = "'Rns, 'IAddSub";
        let form_mov = "'Rds, 'Rns";

        match instr.mask(AddSubImmediateMask) {
            ADD_w_imm | ADD_x_imm => {
                mnemonic = "add";
                if stack_op {
                    mnemonic = "mov";
                    form = form_mov;
                }
            }
            ADDS_w_imm | ADDS_x_imm => {
                mnemonic = "adds";
                if rd_is_zr {
                    mnemonic = "cmn";
                    form = form_cmp;
                }
            }
            SUB_w_imm | SUB_x_imm => {
                mnemonic = "sub";
            }
            SUBS_w_imm | SUBS_x_imm => {
                mnemonic = "subs";
                if rd_is_zr {
                    mnemonic = "cmp";
                    form = form_cmp;
                }
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_add_sub_shifted(&mut self, instr: Instruction) {
        let rd_is_zr = self.rd_is_zr_or_sp(instr);
        let rn_is_zr = self.rn_is_zr_or_sp(instr);
        let mut mnemonic: &str;
        let mut form = "'Rd, 'Rn, 'Rm'NDP";
        let form_cmp = "'Rn, 'Rm'NDP";
        let form_neg = "'Rd, 'Rm'NDP";

        match instr.mask(AddSubShiftedMask) {
            ADD_w_shift | ADD_x_shift => {
                mnemonic = "add";
            }
            ADDS_w_shift | ADDS_x_shift => {
                mnemonic = "adds";
                if rd_is_zr {
                    mnemonic = "cmn";
                    form = form_cmp;
                }
            }
            SUB_w_shift | SUB_x_shift => {
                mnemonic = "sub";
                if rn_is_zr {
                    mnemonic = "neg";
                    form = form_neg;
                }
            }
            SUBS_w_shift | SUBS_x_shift => {
                mnemonic = "subs";
                if rd_is_zr {
                    mnemonic = "cmp";
                    form = form_cmp;
                } else if rn_is_zr {
                    mnemonic = "negs";
                    form = form_neg;
                }
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_add_sub_extended(&mut self, instr: Instruction) {
        let rd_is_zr = self.rd_is_zr_or_sp(instr);
        let mut mnemonic: &str;
        let mode = instr.extend_mode();
        let mut form = if (mode == Extend::Uxtx as u32) || (mode == Extend::Sxtx as u32) {
            "'Rds, 'Rns, 'Xm'Ext"
        } else {
            "'Rds, 'Rns, 'Wm'Ext"
        };
        let form_cmp = if (mode == Extend::Uxtx as u32) || (mode == Extend::Sxtx as u32) {
            "'Rns, 'Xm'Ext"
        } else {
            "'Rns, 'Wm'Ext"
        };

        match instr.mask(AddSubExtendedMask) {
            ADD_w_ext | ADD_x_ext => {
                mnemonic = "add";
            }
            ADDS_w_ext | ADDS_x_ext => {
                mnemonic = "adds";
                if rd_is_zr {
                    mnemonic = "cmn";
                    form = form_cmp;
                }
            }
            SUB_w_ext | SUB_x_ext => {
                mnemonic = "sub";
            }
            SUBS_w_ext | SUBS_x_ext => {
                mnemonic = "subs";
                if rd_is_zr {
                    mnemonic = "cmp";
                    form = form_cmp;
                }
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_add_sub_with_carry(&mut self, instr: Instruction) {
        let rn_is_zr = self.rn_is_zr_or_sp(instr);
        let mut mnemonic: &str;
        let mut form = "'Rd, 'Rn, 'Rm";
        let form_neg = "'Rd, 'Rm";

        match instr.mask(AddSubWithCarryMask) {
            ADC_w | ADC_x => {
                mnemonic = "adc";
            }
            ADCS_w | ADCS_x => {
                mnemonic = "adcs";
            }
            SBC_w | SBC_x => {
                mnemonic = "sbc";
                if rn_is_zr {
                    mnemonic = "ngc";
                    form = form_neg;
                }
            }
            SBCS_w | SBCS_x => {
                mnemonic = "sbcs";
                if rn_is_zr {
                    mnemonic = "ngcs";
                    form = form_neg;
                }
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_logical_immediate(&mut self, instr: Instruction) {
        let rd_is_zr = self.rd_is_zr_or_sp(instr);
        let rn_is_zr = self.rn_is_zr_or_sp(instr);
        let mut mnemonic: &str;
        let mut form = "'Rds, 'Rn, 'ITri";

        if instr.imm_logical() == 0 {
            // The immediate encoded in the instruction is not in the expected format.
            self.format(instr, "unallocated", "(LogicalImmediate)");
            return;
        }

        match instr.mask(LogicalImmediateMask) {
            AND_w_imm | AND_x_imm => {
                mnemonic = "and";
            }
            ORR_w_imm | ORR_x_imm => {
                mnemonic = "orr";
                let reg_size = if instr.sixty_four_bits() == 1 {
                    kXRegSizeInBits as u32
                } else {
                    kWRegSizeInBits as u32
                };
                if rn_is_zr && !self.is_movz_movn_imm(reg_size, instr.imm_logical()) {
                    mnemonic = "mov";
                    form = "'Rds, 'ITri";
                }
            }
            EOR_w_imm | EOR_x_imm => {
                mnemonic = "eor";
            }
            ANDS_w_imm | ANDS_x_imm => {
                mnemonic = "ands";
                if rd_is_zr {
                    mnemonic = "tst";
                    form = "'Rn, 'ITri";
                }
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_min_max_immediate(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Rd, 'Rn, 'IMinMax";

        match instr.mask(MinMaxImmediateMask) {
            SMAX_w_imm | SMAX_x_imm => {
                mnemonic = "smax";
            }
            UMAX_w_imm | UMAX_x_imm => {
                mnemonic = "umax";
            }
            SMIN_w_imm | SMIN_x_imm => {
                mnemonic = "smin";
            }
            UMIN_w_imm | UMIN_x_imm => {
                mnemonic = "umin";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_logical_shifted(&mut self, instr: Instruction) {
        let rd_is_zr = self.rd_is_zr_or_sp(instr);
        let rn_is_zr = self.rn_is_zr_or_sp(instr);
        let mut mnemonic: &str;
        let mut form = "'Rd, 'Rn, 'Rm'NLo";

        match instr.mask(LogicalShiftedMask) {
            AND_w | AND_x => {
                mnemonic = "and";
            }
            BIC_w | BIC_x => {
                mnemonic = "bic";
            }
            EOR_w | EOR_x => {
                mnemonic = "eor";
            }
            EON_w | EON_x => {
                mnemonic = "eon";
            }
            BICS_w | BICS_x => {
                mnemonic = "bics";
            }
            ANDS_w | ANDS_x => {
                mnemonic = "ands";
                if rd_is_zr {
                    mnemonic = "tst";
                    form = "'Rn, 'Rm'NLo";
                }
            }
            ORR_w | ORR_x => {
                mnemonic = "orr";
                if rn_is_zr
                    && (instr.imm_dp_shift() == 0)
                    && (instr.shift_dp() == Shift::Lsl as u32)
                {
                    mnemonic = "mov";
                    form = "'Rd, 'Rm";
                }
            }
            ORN_w | ORN_x => {
                mnemonic = "orn";
                if rn_is_zr {
                    mnemonic = "mvn";
                    form = "'Rd, 'Rm'NLo";
                }
            }
            _ => {
                unreachable!();
            }
        }

        self.format(instr, mnemonic, form);
    }

    fn visit_conditional_compare_register(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Rn, 'Rm, 'INzcv, 'Cond";

        match instr.mask(ConditionalCompareRegisterMask) {
            CCMN_w | CCMN_x => {
                mnemonic = "ccmn";
            }
            CCMP_w | CCMP_x => {
                mnemonic = "ccmp";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_conditional_compare_immediate(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Rn, 'IP, 'INzcv, 'Cond";

        match instr.mask(ConditionalCompareImmediateMask) {
            CCMN_w_imm | CCMN_x_imm => {
                mnemonic = "ccmn";
            }
            CCMP_w_imm | CCMP_x_imm => {
                mnemonic = "ccmp";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_conditional_select(&mut self, instr: Instruction) {
        let rnm_is_zr = self.rn_is_zr_or_sp(instr) && self.rm_is_zr_or_sp(instr);
        let rn_is_rm = instr.rn() == instr.rm();
        let mut mnemonic: &str;
        let mut form = "'Rd, 'Rn, 'Rm, 'Cond";
        let form_test = "'Rd, 'CInv";
        let form_update = "'Rd, 'Rn, 'CInv";

        let cond = condition_from_bits(instr.condition());
        let invertible_cond = (cond != Condition::Al) && (cond != Condition::Nv);

        match instr.mask(ConditionalSelectMask) {
            CSEL_w | CSEL_x => {
                mnemonic = "csel";
            }
            CSINC_w | CSINC_x => {
                mnemonic = "csinc";
                if rnm_is_zr && invertible_cond {
                    mnemonic = "cset";
                    form = form_test;
                } else if rn_is_rm && invertible_cond {
                    mnemonic = "cinc";
                    form = form_update;
                }
            }
            CSINV_w | CSINV_x => {
                mnemonic = "csinv";
                if rnm_is_zr && invertible_cond {
                    mnemonic = "csetm";
                    form = form_test;
                } else if rn_is_rm && invertible_cond {
                    mnemonic = "cinv";
                    form = form_update;
                }
            }
            CSNEG_w | CSNEG_x => {
                mnemonic = "csneg";
                if rn_is_rm && invertible_cond {
                    mnemonic = "cneg";
                    form = form_update;
                }
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_bitfield(&mut self, instr: Instruction) {
        let s = instr.imm_s();
        let r = instr.imm_r();
        let rd_size_minus_1 = (if instr.sixty_four_bits() == 1 {
            kXRegSizeInBits as u32
        } else {
            kWRegSizeInBits as u32
        }) - 1;
        let mut mnemonic = "";
        let mut form = "";
        let form_shift_right = "'Rd, 'Rn, 'IBr";
        let form_extend = "'Rd, 'Wn";
        let form_bfiz = "'Rd, 'Rn, 'IBZ-r, 'IBs+1";
        let form_bfx = "'Rd, 'Rn, 'IBr, 'IBs-r+1";
        let form_lsl = "'Rd, 'Rn, 'IBZ-r";

        match instr.mask(BitfieldMask) {
            SBFM_w | SBFM_x => {
                mnemonic = "sbfx";
                form = form_bfx;
                if r == 0 {
                    form = form_extend;
                    if s == 7 {
                        mnemonic = "sxtb";
                    } else if s == 15 {
                        mnemonic = "sxth";
                    } else if (s == 31) && (instr.sixty_four_bits() == 1) {
                        mnemonic = "sxtw";
                    } else {
                        form = form_bfx;
                    }
                } else if s == rd_size_minus_1 {
                    mnemonic = "asr";
                    form = form_shift_right;
                } else if s < r {
                    mnemonic = "sbfiz";
                    form = form_bfiz;
                }
            }
            UBFM_w | UBFM_x => {
                mnemonic = "ubfx";
                form = form_bfx;
                if r == 0 {
                    form = form_extend;
                    if s == 7 {
                        mnemonic = "uxtb";
                    } else if s == 15 {
                        mnemonic = "uxth";
                    } else {
                        form = form_bfx;
                    }
                }
                if s == rd_size_minus_1 {
                    mnemonic = "lsr";
                    form = form_shift_right;
                } else if r == s + 1 {
                    mnemonic = "lsl";
                    form = form_lsl;
                } else if s < r {
                    mnemonic = "ubfiz";
                    form = form_bfiz;
                }
            }
            BFM_w | BFM_x => {
                mnemonic = "bfxil";
                form = form_bfx;
                if s < r {
                    mnemonic = "bfi";
                    form = form_bfiz;
                }
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_extract(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let mut form = "'Rd, 'Rn, 'Rm, 'IExtract";

        match instr.mask(ExtractMask) {
            EXTR_w | EXTR_x => {
                if instr.rn() == instr.rm() {
                    mnemonic = "ror";
                    form = "'Rd, 'Rn, 'IExtract";
                } else {
                    mnemonic = "extr";
                }
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_pc_rel_addressing(&mut self, instr: Instruction) {
        match instr.mask(PCRelAddressingMask) {
            ADR => {
                self.format(instr, "adr", "'Xd, 'AddrPCRelByte");

                // ADRP is not implemented.
            }
            _ => {
                self.format(instr, "unimplemented", "(PCRelAddressing)");
            }
        }
    }

    fn visit_conditional_branch(&mut self, instr: Instruction) {
        match instr.mask(ConditionalBranchMask) {
            B_cond => {
                self.format(instr, "b.'CBrn", "'TImmCond");
            }
            BC_cond => {
                self.format(instr, "bc.'CBrn", "'TImmCond");
            }
            _ => {
                unreachable!();
            }
        }
    }

    fn visit_unconditional_branch_to_register(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Xn";

        match instr.mask(UnconditionalBranchToRegisterMask) {
            BR => {
                mnemonic = "br";
            }
            BLR => {
                mnemonic = "blr";
            }
            RET => {
                mnemonic = "ret";
                if instr.rn() == kLinkRegCode {
                    form = "";
                }
            }
            _ => {
                form = "(UnconditionalBranchToRegister)";
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_unconditional_branch(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'TImmUncn";

        match instr.mask(UnconditionalBranchMask) {
            B => {
                mnemonic = "b";
            }
            BL => {
                mnemonic = "bl";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_data_processing_1_source(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Rd, 'Rn";

        match instr.mask(DataProcessing1SourceMask) {
            RBIT_w | RBIT_x => {
                mnemonic = "rbit";
            }
            REV16_w | REV16_x => {
                mnemonic = "rev16";
            }
            REV_w | REV_x => {
                mnemonic = "rev";
            }
            CLZ_w | CLZ_x => {
                mnemonic = "clz";
            }
            CLS_w | CLS_x => {
                mnemonic = "cls";
            }
            CTZ_w | CTZ_x => {
                mnemonic = "ctz";
            }
            CNT_w | CNT_x => {
                mnemonic = "cnt";
            }
            ABS_w | ABS_x => {
                mnemonic = "abs";
            }
            REV32_x => {
                mnemonic = "rev32";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_data_processing_2_source(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Rd, 'Rn, 'Rm";

        match instr.mask(DataProcessing2SourceMask) {
            UDIV_w | UDIV_x => {
                mnemonic = "udiv";
            }
            SDIV_w | SDIV_x => {
                mnemonic = "sdiv";
            }
            LSLV_w | LSLV_x => {
                mnemonic = "lsl";
            }
            LSRV_w | LSRV_x => {
                mnemonic = "lsr";
            }
            ASRV_w | ASRV_x => {
                mnemonic = "asr";
            }
            RORV_w | RORV_x => {
                mnemonic = "ror";
            }
            SMAX_w | SMAX_x => {
                mnemonic = "smax";
            }
            UMAX_w | UMAX_x => {
                mnemonic = "umax";
            }
            SMIN_w | SMIN_x => {
                mnemonic = "smin";
            }
            UMIN_w | UMIN_x => {
                mnemonic = "umin";
            }
            _ => {
                form = "(DataProcessing2Source)";
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_data_processing_3_source(&mut self, instr: Instruction) {
        let ra_is_zr = self.ra_is_zr_or_sp(instr);
        let mut mnemonic: &str;
        let mut form = "'Xd, 'Wn, 'Wm, 'Xa";
        let form_rrr = "'Rd, 'Rn, 'Rm";
        let form_rrrr = "'Rd, 'Rn, 'Rm, 'Ra";
        let form_xww = "'Xd, 'Wn, 'Wm";
        let form_xxx = "'Xd, 'Xn, 'Xm";

        match instr.mask(DataProcessing3SourceMask) {
            MADD_w | MADD_x => {
                mnemonic = "madd";
                form = form_rrrr;
                if ra_is_zr {
                    mnemonic = "mul";
                    form = form_rrr;
                }
            }
            MSUB_w | MSUB_x => {
                mnemonic = "msub";
                form = form_rrrr;
                if ra_is_zr {
                    mnemonic = "mneg";
                    form = form_rrr;
                }
            }
            SMADDL_x => {
                mnemonic = "smaddl";
                if ra_is_zr {
                    mnemonic = "smull";
                    form = form_xww;
                }
            }
            SMSUBL_x => {
                mnemonic = "smsubl";
                if ra_is_zr {
                    mnemonic = "smnegl";
                    form = form_xww;
                }
            }
            UMADDL_x => {
                mnemonic = "umaddl";
                if ra_is_zr {
                    mnemonic = "umull";
                    form = form_xww;
                }
            }
            UMSUBL_x => {
                mnemonic = "umsubl";
                if ra_is_zr {
                    mnemonic = "umnegl";
                    form = form_xww;
                }
            }
            SMULH_x => {
                mnemonic = "smulh";
                form = form_xxx;
            }
            UMULH_x => {
                mnemonic = "umulh";
                form = form_xxx;
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_compare_branch(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Rt, 'TImmCmpa";

        match instr.mask(CompareBranchMask) {
            CBZ_w | CBZ_x => {
                mnemonic = "cbz";
            }
            CBNZ_w | CBNZ_x => {
                mnemonic = "cbnz";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_test_branch(&mut self, instr: Instruction) {
        let mnemonic: &str;
        // If the top bit of the immediate is clear, the tested register is
        // disassembled as Wt, otherwise Xt. As the top bit of the immediate is
        // encoded in bit 31 of the instruction, we can reuse the Rt form, which
        // uses bit 31 (normally "sf") to choose the register size.
        let form = "'Rt, 'IS, 'TImmTest";

        match instr.mask(TestBranchMask) {
            TBZ => {
                mnemonic = "tbz";
            }
            TBNZ => {
                mnemonic = "tbnz";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_move_wide_immediate(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let mut form = "'Rd, 'IMoveImm";

        // Print the shift separately for movk, to make it clear which half word will
        // be overwritten. Movn and movz print the computed immediate, which includes
        // shift calculation.
        match instr.mask(MoveWideImmediateMask) {
            MOVN_w | MOVN_x => {
                mnemonic = "movn";
            }
            MOVZ_w | MOVZ_x => {
                mnemonic = "movz";
            }
            MOVK_w | MOVK_x => {
                mnemonic = "movk";
                form = "'Rd, 'IMoveLSL";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_pre_index(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStorePreIndex)";

        match instr.mask(LoadStorePreIndexMask) {
            STRB_w_pre => {
                mnemonic = "strb";
                form = "'Wt, ['Xns'ILS]!";
            }
            STRH_w_pre => {
                mnemonic = "strh";
                form = "'Wt, ['Xns'ILS]!";
            }
            STR_w_pre => {
                mnemonic = "str";
                form = "'Wt, ['Xns'ILS]!";
            }
            STR_x_pre => {
                mnemonic = "str";
                form = "'Xt, ['Xns'ILS]!";
            }
            LDRB_w_pre => {
                mnemonic = "ldrb";
                form = "'Wt, ['Xns'ILS]!";
            }
            LDRH_w_pre => {
                mnemonic = "ldrh";
                form = "'Wt, ['Xns'ILS]!";
            }
            LDR_w_pre => {
                mnemonic = "ldr";
                form = "'Wt, ['Xns'ILS]!";
            }
            LDR_x_pre => {
                mnemonic = "ldr";
                form = "'Xt, ['Xns'ILS]!";
            }
            LDRSB_x_pre => {
                mnemonic = "ldrsb";
                form = "'Xt, ['Xns'ILS]!";
            }
            LDRSH_x_pre => {
                mnemonic = "ldrsh";
                form = "'Xt, ['Xns'ILS]!";
            }
            LDRSW_x_pre => {
                mnemonic = "ldrsw";
                form = "'Xt, ['Xns'ILS]!";
            }
            LDRSB_w_pre => {
                mnemonic = "ldrsb";
                form = "'Wt, ['Xns'ILS]!";
            }
            LDRSH_w_pre => {
                mnemonic = "ldrsh";
                form = "'Wt, ['Xns'ILS]!";
            }
            STR_b_pre => {
                mnemonic = "str";
                form = "'Bt, ['Xns'ILS]!";
            }
            STR_h_pre => {
                mnemonic = "str";
                form = "'Ht, ['Xns'ILS]!";
            }
            STR_s_pre => {
                mnemonic = "str";
                form = "'St, ['Xns'ILS]!";
            }
            STR_d_pre => {
                mnemonic = "str";
                form = "'Dt, ['Xns'ILS]!";
            }
            LDR_b_pre => {
                mnemonic = "ldr";
                form = "'Bt, ['Xns'ILS]!";
            }
            LDR_h_pre => {
                mnemonic = "ldr";
                form = "'Ht, ['Xns'ILS]!";
            }
            LDR_s_pre => {
                mnemonic = "ldr";
                form = "'St, ['Xns'ILS]!";
            }
            LDR_d_pre => {
                mnemonic = "ldr";
                form = "'Dt, ['Xns'ILS]!";
            }
            STR_q_pre => {
                mnemonic = "str";
                form = "'Qt, ['Xns'ILS]!";
            }
            LDR_q_pre => {
                mnemonic = "ldr";
                form = "'Qt, ['Xns'ILS]!";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_post_index(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStorePostIndex)";

        match instr.mask(LoadStorePostIndexMask) {
            STRB_w_post => {
                mnemonic = "strb";
                form = "'Wt, ['Xns]'ILS";
            }
            STRH_w_post => {
                mnemonic = "strh";
                form = "'Wt, ['Xns]'ILS";
            }
            STR_w_post => {
                mnemonic = "str";
                form = "'Wt, ['Xns]'ILS";
            }
            STR_x_post => {
                mnemonic = "str";
                form = "'Xt, ['Xns]'ILS";
            }
            LDRB_w_post => {
                mnemonic = "ldrb";
                form = "'Wt, ['Xns]'ILS";
            }
            LDRH_w_post => {
                mnemonic = "ldrh";
                form = "'Wt, ['Xns]'ILS";
            }
            LDR_w_post => {
                mnemonic = "ldr";
                form = "'Wt, ['Xns]'ILS";
            }
            LDR_x_post => {
                mnemonic = "ldr";
                form = "'Xt, ['Xns]'ILS";
            }
            LDRSB_x_post => {
                mnemonic = "ldrsb";
                form = "'Xt, ['Xns]'ILS";
            }
            LDRSH_x_post => {
                mnemonic = "ldrsh";
                form = "'Xt, ['Xns]'ILS";
            }
            LDRSW_x_post => {
                mnemonic = "ldrsw";
                form = "'Xt, ['Xns]'ILS";
            }
            LDRSB_w_post => {
                mnemonic = "ldrsb";
                form = "'Wt, ['Xns]'ILS";
            }
            LDRSH_w_post => {
                mnemonic = "ldrsh";
                form = "'Wt, ['Xns]'ILS";
            }
            STR_b_post => {
                mnemonic = "str";
                form = "'Bt, ['Xns]'ILS";
            }
            STR_h_post => {
                mnemonic = "str";
                form = "'Ht, ['Xns]'ILS";
            }
            STR_s_post => {
                mnemonic = "str";
                form = "'St, ['Xns]'ILS";
            }
            STR_d_post => {
                mnemonic = "str";
                form = "'Dt, ['Xns]'ILS";
            }
            LDR_b_post => {
                mnemonic = "ldr";
                form = "'Bt, ['Xns]'ILS";
            }
            LDR_h_post => {
                mnemonic = "ldr";
                form = "'Ht, ['Xns]'ILS";
            }
            LDR_s_post => {
                mnemonic = "ldr";
                form = "'St, ['Xns]'ILS";
            }
            LDR_d_post => {
                mnemonic = "ldr";
                form = "'Dt, ['Xns]'ILS";
            }
            STR_q_post => {
                mnemonic = "str";
                form = "'Qt, ['Xns]'ILS";
            }
            LDR_q_post => {
                mnemonic = "ldr";
                form = "'Qt, ['Xns]'ILS";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_unsigned_offset(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStoreUnsignedOffset)";

        match instr.mask(LoadStoreUnsignedOffsetMask) {
            STRB_w_unsigned => {
                mnemonic = "strb";
                form = "'Wt, ['Xns'ILU]";
            }
            STRH_w_unsigned => {
                mnemonic = "strh";
                form = "'Wt, ['Xns'ILU]";
            }
            STR_w_unsigned => {
                mnemonic = "str";
                form = "'Wt, ['Xns'ILU]";
            }
            STR_x_unsigned => {
                mnemonic = "str";
                form = "'Xt, ['Xns'ILU]";
            }
            LDRB_w_unsigned => {
                mnemonic = "ldrb";
                form = "'Wt, ['Xns'ILU]";
            }
            LDRH_w_unsigned => {
                mnemonic = "ldrh";
                form = "'Wt, ['Xns'ILU]";
            }
            LDR_w_unsigned => {
                mnemonic = "ldr";
                form = "'Wt, ['Xns'ILU]";
            }
            LDR_x_unsigned => {
                mnemonic = "ldr";
                form = "'Xt, ['Xns'ILU]";
            }
            LDRSB_x_unsigned => {
                mnemonic = "ldrsb";
                form = "'Xt, ['Xns'ILU]";
            }
            LDRSH_x_unsigned => {
                mnemonic = "ldrsh";
                form = "'Xt, ['Xns'ILU]";
            }
            LDRSW_x_unsigned => {
                mnemonic = "ldrsw";
                form = "'Xt, ['Xns'ILU]";
            }
            LDRSB_w_unsigned => {
                mnemonic = "ldrsb";
                form = "'Wt, ['Xns'ILU]";
            }
            LDRSH_w_unsigned => {
                mnemonic = "ldrsh";
                form = "'Wt, ['Xns'ILU]";
            }
            STR_b_unsigned => {
                mnemonic = "str";
                form = "'Bt, ['Xns'ILU]";
            }
            STR_h_unsigned => {
                mnemonic = "str";
                form = "'Ht, ['Xns'ILU]";
            }
            STR_s_unsigned => {
                mnemonic = "str";
                form = "'St, ['Xns'ILU]";
            }
            STR_d_unsigned => {
                mnemonic = "str";
                form = "'Dt, ['Xns'ILU]";
            }
            LDR_b_unsigned => {
                mnemonic = "ldr";
                form = "'Bt, ['Xns'ILU]";
            }
            LDR_h_unsigned => {
                mnemonic = "ldr";
                form = "'Ht, ['Xns'ILU]";
            }
            LDR_s_unsigned => {
                mnemonic = "ldr";
                form = "'St, ['Xns'ILU]";
            }
            LDR_d_unsigned => {
                mnemonic = "ldr";
                form = "'Dt, ['Xns'ILU]";
            }
            STR_q_unsigned => {
                mnemonic = "str";
                form = "'Qt, ['Xns'ILU]";
            }
            LDR_q_unsigned => {
                mnemonic = "ldr";
                form = "'Qt, ['Xns'ILU]";
            }
            PRFM_unsigned => {
                mnemonic = "prfm";
                form = "'PrefOp, ['Xns'ILU]";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_register_offset(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStoreRegisterOffset)";

        match instr.mask(LoadStoreRegisterOffsetMask) {
            STRB_w_reg => {
                mnemonic = "strb";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            STRH_w_reg => {
                mnemonic = "strh";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            STR_w_reg => {
                mnemonic = "str";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            STR_x_reg => {
                mnemonic = "str";
                form = "'Xt, ['Xns, 'Offsetreg]";
            }
            LDRB_w_reg => {
                mnemonic = "ldrb";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            LDRH_w_reg => {
                mnemonic = "ldrh";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            LDR_w_reg => {
                mnemonic = "ldr";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            LDR_x_reg => {
                mnemonic = "ldr";
                form = "'Xt, ['Xns, 'Offsetreg]";
            }
            LDRSB_x_reg => {
                mnemonic = "ldrsb";
                form = "'Xt, ['Xns, 'Offsetreg]";
            }
            LDRSH_x_reg => {
                mnemonic = "ldrsh";
                form = "'Xt, ['Xns, 'Offsetreg]";
            }
            LDRSW_x_reg => {
                mnemonic = "ldrsw";
                form = "'Xt, ['Xns, 'Offsetreg]";
            }
            LDRSB_w_reg => {
                mnemonic = "ldrsb";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            LDRSH_w_reg => {
                mnemonic = "ldrsh";
                form = "'Wt, ['Xns, 'Offsetreg]";
            }
            STR_b_reg => {
                mnemonic = "str";
                form = "'Bt, ['Xns, 'Offsetreg]";
            }
            STR_h_reg => {
                mnemonic = "str";
                form = "'Ht, ['Xns, 'Offsetreg]";
            }
            STR_s_reg => {
                mnemonic = "str";
                form = "'St, ['Xns, 'Offsetreg]";
            }
            STR_d_reg => {
                mnemonic = "str";
                form = "'Dt, ['Xns, 'Offsetreg]";
            }
            LDR_b_reg => {
                mnemonic = "ldr";
                form = "'Bt, ['Xns, 'Offsetreg]";
            }
            LDR_h_reg => {
                mnemonic = "ldr";
                form = "'Ht, ['Xns, 'Offsetreg]";
            }
            LDR_s_reg => {
                mnemonic = "ldr";
                form = "'St, ['Xns, 'Offsetreg]";
            }
            LDR_d_reg => {
                mnemonic = "ldr";
                form = "'Dt, ['Xns, 'Offsetreg]";
            }
            STR_q_reg => {
                mnemonic = "str";
                form = "'Qt, ['Xns, 'Offsetreg]";
            }
            LDR_q_reg => {
                mnemonic = "ldr";
                form = "'Qt, ['Xns, 'Offsetreg]";
            }
            PRFM_reg => {
                mnemonic = "prfm";
                form = "'PrefOp, ['Xns, 'Offsetreg]";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_unscaled_offset(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStoreUnscaledOffset)";

        match instr.mask(LoadStoreUnscaledOffsetMask) {
            STURB_w => {
                mnemonic = "sturb";
                form = "'Wt, ['Xns'ILS]";
            }
            STURH_w => {
                mnemonic = "sturh";
                form = "'Wt, ['Xns'ILS]";
            }
            STUR_w => {
                mnemonic = "stur";
                form = "'Wt, ['Xns'ILS]";
            }
            STUR_x => {
                mnemonic = "stur";
                form = "'Xt, ['Xns'ILS]";
            }
            LDURB_w => {
                mnemonic = "ldurb";
                form = "'Wt, ['Xns'ILS]";
            }
            LDURH_w => {
                mnemonic = "ldurh";
                form = "'Wt, ['Xns'ILS]";
            }
            LDUR_w => {
                mnemonic = "ldur";
                form = "'Wt, ['Xns'ILS]";
            }
            LDUR_x => {
                mnemonic = "ldur";
                form = "'Xt, ['Xns'ILS]";
            }
            LDURSB_x => {
                mnemonic = "ldursb";
                form = "'Xt, ['Xns'ILS]";
            }
            LDURSH_x => {
                mnemonic = "ldursh";
                form = "'Xt, ['Xns'ILS]";
            }
            LDURSW_x => {
                mnemonic = "ldursw";
                form = "'Xt, ['Xns'ILS]";
            }
            LDURSB_w => {
                mnemonic = "ldursb";
                form = "'Wt, ['Xns'ILS]";
            }
            LDURSH_w => {
                mnemonic = "ldursh";
                form = "'Wt, ['Xns'ILS]";
            }
            STUR_b => {
                mnemonic = "stur";
                form = "'Bt, ['Xns'ILS]";
            }
            STUR_h => {
                mnemonic = "stur";
                form = "'Ht, ['Xns'ILS]";
            }
            STUR_s => {
                mnemonic = "stur";
                form = "'St, ['Xns'ILS]";
            }
            STUR_d => {
                mnemonic = "stur";
                form = "'Dt, ['Xns'ILS]";
            }
            LDUR_b => {
                mnemonic = "ldur";
                form = "'Bt, ['Xns'ILS]";
            }
            LDUR_h => {
                mnemonic = "ldur";
                form = "'Ht, ['Xns'ILS]";
            }
            LDUR_s => {
                mnemonic = "ldur";
                form = "'St, ['Xns'ILS]";
            }
            LDUR_d => {
                mnemonic = "ldur";
                form = "'Dt, ['Xns'ILS]";
            }
            STUR_q => {
                mnemonic = "stur";
                form = "'Qt, ['Xns'ILS]";
            }
            LDUR_q => {
                mnemonic = "ldur";
                form = "'Qt, ['Xns'ILS]";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_literal(&mut self, instr: Instruction) {
        let mut mnemonic = "ldr";
        let mut form = "(LoadLiteral)";

        match instr.mask(LoadLiteralMask) {
            LDR_w_lit => {
                form = "'Wt, 'ILLiteral 'LValue";
            }
            LDR_x_lit => {
                form = "'Xt, 'ILLiteral 'LValue";
            }
            LDR_s_lit => {
                form = "'St, 'ILLiteral 'LValue";
            }
            LDR_d_lit => {
                form = "'Dt, 'ILLiteral 'LValue";
            }
            _ => {
                mnemonic = "unimplemented";
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_pair_post_index(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStorePairPostIndex)";

        match instr.mask(LoadStorePairPostIndexMask) {
            STP_w_post => {
                mnemonic = "stp";
                form = "'Wt, 'Wt2, ['Xns]'ILP2";
            }
            LDP_w_post => {
                mnemonic = "ldp";
                form = "'Wt, 'Wt2, ['Xns]'ILP2";
            }
            LDPSW_x_post => {
                mnemonic = "ldpsw";
                form = "'Xt, 'Xt2, ['Xns]'ILP2";
            }
            STP_x_post => {
                mnemonic = "stp";
                form = "'Xt, 'Xt2, ['Xns]'ILP3";
            }
            LDP_x_post => {
                mnemonic = "ldp";
                form = "'Xt, 'Xt2, ['Xns]'ILP3";
            }
            STP_s_post => {
                mnemonic = "stp";
                form = "'St, 'St2, ['Xns]'ILP2";
            }
            LDP_s_post => {
                mnemonic = "ldp";
                form = "'St, 'St2, ['Xns]'ILP2";
            }
            STP_d_post => {
                mnemonic = "stp";
                form = "'Dt, 'Dt2, ['Xns]'ILP3";
            }
            LDP_d_post => {
                mnemonic = "ldp";
                form = "'Dt, 'Dt2, ['Xns]'ILP3";
            }
            LDP_q_post => {
                mnemonic = "ldp";
                form = "'Qt, 'Qt2, ['Xns]'ILP4";
            }
            STP_q_post => {
                mnemonic = "stp";
                form = "'Qt, 'Qt2, ['Xns]'ILP4";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_pair_pre_index(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStorePairPreIndex)";

        match instr.mask(LoadStorePairPreIndexMask) {
            STP_w_pre => {
                mnemonic = "stp";
                form = "'Wt, 'Wt2, ['Xns'ILP2]!";
            }
            LDP_w_pre => {
                mnemonic = "ldp";
                form = "'Wt, 'Wt2, ['Xns'ILP2]!";
            }
            LDPSW_x_pre => {
                mnemonic = "ldpsw";
                form = "'Xt, 'Xt2, ['Xns'ILP2]!";
            }
            STP_x_pre => {
                mnemonic = "stp";
                form = "'Xt, 'Xt2, ['Xns'ILP3]!";
            }
            LDP_x_pre => {
                mnemonic = "ldp";
                form = "'Xt, 'Xt2, ['Xns'ILP3]!";
            }
            STP_s_pre => {
                mnemonic = "stp";
                form = "'St, 'St2, ['Xns'ILP2]!";
            }
            LDP_s_pre => {
                mnemonic = "ldp";
                form = "'St, 'St2, ['Xns'ILP2]!";
            }
            STP_d_pre => {
                mnemonic = "stp";
                form = "'Dt, 'Dt2, ['Xns'ILP3]!";
            }
            LDP_d_pre => {
                mnemonic = "ldp";
                form = "'Dt, 'Dt2, ['Xns'ILP3]!";
            }
            LDP_q_pre => {
                mnemonic = "ldp";
                form = "'Qt, 'Qt2, ['Xns'ILP4]!";
            }
            STP_q_pre => {
                mnemonic = "stp";
                form = "'Qt, 'Qt2, ['Xns'ILP4]!";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_pair_offset(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(LoadStorePairOffset)";

        match instr.mask(LoadStorePairOffsetMask) {
            STP_w_off => {
                mnemonic = "stp";
                form = "'Wt, 'Wt2, ['Xns'ILP2]";
            }
            LDP_w_off => {
                mnemonic = "ldp";
                form = "'Wt, 'Wt2, ['Xns'ILP2]";
            }
            LDPSW_x_off => {
                mnemonic = "ldpsw";
                form = "'Xt, 'Xt2, ['Xns'ILP2]";
            }
            STP_x_off => {
                mnemonic = "stp";
                form = "'Xt, 'Xt2, ['Xns'ILP3]";
            }
            LDP_x_off => {
                mnemonic = "ldp";
                form = "'Xt, 'Xt2, ['Xns'ILP3]";
            }
            STP_s_off => {
                mnemonic = "stp";
                form = "'St, 'St2, ['Xns'ILP2]";
            }
            LDP_s_off => {
                mnemonic = "ldp";
                form = "'St, 'St2, ['Xns'ILP2]";
            }
            STP_d_off => {
                mnemonic = "stp";
                form = "'Dt, 'Dt2, ['Xns'ILP3]";
            }
            LDP_d_off => {
                mnemonic = "ldp";
                form = "'Dt, 'Dt2, ['Xns'ILP3]";
            }
            LDP_q_off => {
                mnemonic = "ldp";
                form = "'Qt, 'Qt2, ['Xns'ILP4]";
            }
            STP_q_off => {
                mnemonic = "stp";
                form = "'Qt, 'Qt2, ['Xns'ILP4]";
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_load_store_acquire_release(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form: &str;

        match instr.mask(LoadStoreAcquireReleaseMask) {
            STLXR_b => {
                mnemonic = "stlxrb";
                form = "'Ws, 'Wt, ['Xns]";
            }
            STLXR_h => {
                mnemonic = "stlxrh";
                form = "'Ws, 'Wt, ['Xns]";
            }
            STLXR_w => {
                mnemonic = "stlxr";
                form = "'Ws, 'Wt, ['Xns]";
            }
            STLXR_x => {
                mnemonic = "stlxr";
                form = "'Ws, 'Xt, ['Xns]";
            }
            LDAXR_b => {
                mnemonic = "ldaxrb";
                form = "'Wt, ['Xns]";
            }
            LDAXR_h => {
                mnemonic = "ldaxrh";
                form = "'Wt, ['Xns]";
            }
            LDAXR_w => {
                mnemonic = "ldaxr";
                form = "'Wt, ['Xns]";
            }
            LDAXR_x => {
                mnemonic = "ldaxr";
                form = "'Xt, ['Xns]";
            }
            STLR_b => {
                mnemonic = "stlrb";
                form = "'Wt, ['Xns]";
            }
            STLR_h => {
                mnemonic = "stlrh";
                form = "'Wt, ['Xns]";
            }
            STLR_w => {
                mnemonic = "stlr";
                form = "'Wt, ['Xns]";
            }
            STLR_x => {
                mnemonic = "stlr";
                form = "'Xt, ['Xns]";
            }
            LDAR_b => {
                mnemonic = "ldarb";
                form = "'Wt, ['Xns]";
            }
            LDAR_h => {
                mnemonic = "ldarh";
                form = "'Wt, ['Xns]";
            }
            LDAR_w => {
                mnemonic = "ldar";
                form = "'Wt, ['Xns]";
            }
            LDAR_x => {
                mnemonic = "ldar";
                form = "'Xt, ['Xns]";
            }
            CAS_w => {
                mnemonic = "cas";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CAS_x => {
                mnemonic = "cas";
                form = "'Xs, 'Xt, ['Xns]";
            }
            CASA_w => {
                mnemonic = "casa";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASA_x => {
                mnemonic = "casa";
                form = "'Xs, 'Xt, ['Xns]";
            }
            CASL_w => {
                mnemonic = "casl";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASL_x => {
                mnemonic = "casl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            CASAL_w => {
                mnemonic = "casal";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASAL_x => {
                mnemonic = "casal";
                form = "'Xs, 'Xt, ['Xns]";
            }
            CASB => {
                mnemonic = "casb";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASAB => {
                mnemonic = "casab";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASLB => {
                mnemonic = "caslb";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASALB => {
                mnemonic = "casalb";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASH => {
                mnemonic = "cash";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASAH => {
                mnemonic = "casah";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASLH => {
                mnemonic = "caslh";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASALH => {
                mnemonic = "casalh";
                form = "'Ws, 'Wt, ['Xns]";
            }
            CASP_w => {
                mnemonic = "casp";
                form = "'Ws, 'Ws+, 'Wt, 'Wt+, ['Xns]";
            }
            CASP_x => {
                mnemonic = "casp";
                form = "'Xs, 'Xs+, 'Xt, 'Xt+, ['Xns]";
            }
            CASPA_w => {
                mnemonic = "caspa";
                form = "'Ws, 'Ws+, 'Wt, 'Wt+, ['Xns]";
            }
            CASPA_x => {
                mnemonic = "caspa";
                form = "'Xs, 'Xs+, 'Xt, 'Xt+, ['Xns]";
            }
            CASPL_w => {
                mnemonic = "caspl";
                form = "'Ws, 'Ws+, 'Wt, 'Wt+, ['Xns]";
            }
            CASPL_x => {
                mnemonic = "caspl";
                form = "'Xs, 'Xs+, 'Xt, 'Xt+, ['Xns]";
            }
            CASPAL_w => {
                mnemonic = "caspal";
                form = "'Ws, 'Ws+, 'Wt, 'Wt+, ['Xns]";
            }
            CASPAL_x => {
                mnemonic = "caspal";
                form = "'Xs, 'Xs+, 'Xt, 'Xt+, ['Xns]";
            }
            _ => {
                form = "(LoadStoreAcquireRelease)";
            }
        }

        match instr.mask(LoadStoreAcquireReleaseMask) {
            CASP_w | CASP_x | CASPA_w | CASPA_x | CASPL_w | CASPL_x | CASPAL_w | CASPAL_x => {
                if (instr.rs() % 2 == 1) || (instr.rt() % 2 == 1) {
                    mnemonic = "unallocated";
                    form = "(LoadStoreExclusive)";
                }
            }
            _ => (),
        }

        self.format(instr, mnemonic, form);
    }

    fn visit_atomic_memory(&mut self, instr: Instruction) {
        let mut mnemonic: &str;
        let mut form = "'Ws, 'Wt, ['Xns]";

        match instr.mask(AtomicMemoryMask) {
            LDADDB => {
                mnemonic = "addb";
            }
            LDADDAB => {
                mnemonic = "addab";
            }
            LDADDLB => {
                mnemonic = "addlb";
            }
            LDADDALB => {
                mnemonic = "addalb";
            }
            LDADDH => {
                mnemonic = "addh";
            }
            LDADDAH => {
                mnemonic = "addah";
            }
            LDADDLH => {
                mnemonic = "addlh";
            }
            LDADDALH => {
                mnemonic = "addalh";
            }
            LDADD_w => {
                mnemonic = "add";
            }
            LDADDA_w => {
                mnemonic = "adda";
            }
            LDADDL_w => {
                mnemonic = "addl";
            }
            LDADDAL_w => {
                mnemonic = "addal";
            }
            LDADD_x => {
                mnemonic = "add";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDADDA_x => {
                mnemonic = "adda";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDADDL_x => {
                mnemonic = "addl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDADDAL_x => {
                mnemonic = "addal";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDCLRB => {
                mnemonic = "clrb";
            }
            LDCLRAB => {
                mnemonic = "clrab";
            }
            LDCLRLB => {
                mnemonic = "clrlb";
            }
            LDCLRALB => {
                mnemonic = "clralb";
            }
            LDCLRH => {
                mnemonic = "clrh";
            }
            LDCLRAH => {
                mnemonic = "clrah";
            }
            LDCLRLH => {
                mnemonic = "clrlh";
            }
            LDCLRALH => {
                mnemonic = "clralh";
            }
            LDCLR_w => {
                mnemonic = "clr";
            }
            LDCLRA_w => {
                mnemonic = "clra";
            }
            LDCLRL_w => {
                mnemonic = "clrl";
            }
            LDCLRAL_w => {
                mnemonic = "clral";
            }
            LDCLR_x => {
                mnemonic = "clr";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDCLRA_x => {
                mnemonic = "clra";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDCLRL_x => {
                mnemonic = "clrl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDCLRAL_x => {
                mnemonic = "clral";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDEORB => {
                mnemonic = "eorb";
            }
            LDEORAB => {
                mnemonic = "eorab";
            }
            LDEORLB => {
                mnemonic = "eorlb";
            }
            LDEORALB => {
                mnemonic = "eoralb";
            }
            LDEORH => {
                mnemonic = "eorh";
            }
            LDEORAH => {
                mnemonic = "eorah";
            }
            LDEORLH => {
                mnemonic = "eorlh";
            }
            LDEORALH => {
                mnemonic = "eoralh";
            }
            LDEOR_w => {
                mnemonic = "eor";
            }
            LDEORA_w => {
                mnemonic = "eora";
            }
            LDEORL_w => {
                mnemonic = "eorl";
            }
            LDEORAL_w => {
                mnemonic = "eoral";
            }
            LDEOR_x => {
                mnemonic = "eor";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDEORA_x => {
                mnemonic = "eora";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDEORL_x => {
                mnemonic = "eorl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDEORAL_x => {
                mnemonic = "eoral";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSETB => {
                mnemonic = "setb";
            }
            LDSETAB => {
                mnemonic = "setab";
            }
            LDSETLB => {
                mnemonic = "setlb";
            }
            LDSETALB => {
                mnemonic = "setalb";
            }
            LDSETH => {
                mnemonic = "seth";
            }
            LDSETAH => {
                mnemonic = "setah";
            }
            LDSETLH => {
                mnemonic = "setlh";
            }
            LDSETALH => {
                mnemonic = "setalh";
            }
            LDSET_w => {
                mnemonic = "set";
            }
            LDSETA_w => {
                mnemonic = "seta";
            }
            LDSETL_w => {
                mnemonic = "setl";
            }
            LDSETAL_w => {
                mnemonic = "setal";
            }
            LDSET_x => {
                mnemonic = "set";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSETA_x => {
                mnemonic = "seta";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSETL_x => {
                mnemonic = "setl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSETAL_x => {
                mnemonic = "setal";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMAXB => {
                mnemonic = "smaxb";
            }
            LDSMAXAB => {
                mnemonic = "smaxab";
            }
            LDSMAXLB => {
                mnemonic = "smaxlb";
            }
            LDSMAXALB => {
                mnemonic = "smaxalb";
            }
            LDSMAXH => {
                mnemonic = "smaxh";
            }
            LDSMAXAH => {
                mnemonic = "smaxah";
            }
            LDSMAXLH => {
                mnemonic = "smaxlh";
            }
            LDSMAXALH => {
                mnemonic = "smaxalh";
            }
            LDSMAX_w => {
                mnemonic = "smax";
            }
            LDSMAXA_w => {
                mnemonic = "smaxa";
            }
            LDSMAXL_w => {
                mnemonic = "smaxl";
            }
            LDSMAXAL_w => {
                mnemonic = "smaxal";
            }
            LDSMAX_x => {
                mnemonic = "smax";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMAXA_x => {
                mnemonic = "smaxa";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMAXL_x => {
                mnemonic = "smaxl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMAXAL_x => {
                mnemonic = "smaxal";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMINB => {
                mnemonic = "sminb";
            }
            LDSMINAB => {
                mnemonic = "sminab";
            }
            LDSMINLB => {
                mnemonic = "sminlb";
            }
            LDSMINALB => {
                mnemonic = "sminalb";
            }
            LDSMINH => {
                mnemonic = "sminh";
            }
            LDSMINAH => {
                mnemonic = "sminah";
            }
            LDSMINLH => {
                mnemonic = "sminlh";
            }
            LDSMINALH => {
                mnemonic = "sminalh";
            }
            LDSMIN_w => {
                mnemonic = "smin";
            }
            LDSMINA_w => {
                mnemonic = "smina";
            }
            LDSMINL_w => {
                mnemonic = "sminl";
            }
            LDSMINAL_w => {
                mnemonic = "sminal";
            }
            LDSMIN_x => {
                mnemonic = "smin";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMINA_x => {
                mnemonic = "smina";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMINL_x => {
                mnemonic = "sminl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDSMINAL_x => {
                mnemonic = "sminal";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMAXB => {
                mnemonic = "umaxb";
            }
            LDUMAXAB => {
                mnemonic = "umaxab";
            }
            LDUMAXLB => {
                mnemonic = "umaxlb";
            }
            LDUMAXALB => {
                mnemonic = "umaxalb";
            }
            LDUMAXH => {
                mnemonic = "umaxh";
            }
            LDUMAXAH => {
                mnemonic = "umaxah";
            }
            LDUMAXLH => {
                mnemonic = "umaxlh";
            }
            LDUMAXALH => {
                mnemonic = "umaxalh";
            }
            LDUMAX_w => {
                mnemonic = "umax";
            }
            LDUMAXA_w => {
                mnemonic = "umaxa";
            }
            LDUMAXL_w => {
                mnemonic = "umaxl";
            }
            LDUMAXAL_w => {
                mnemonic = "umaxal";
            }
            LDUMAX_x => {
                mnemonic = "umax";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMAXA_x => {
                mnemonic = "umaxa";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMAXL_x => {
                mnemonic = "umaxl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMAXAL_x => {
                mnemonic = "umaxal";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMINB => {
                mnemonic = "uminb";
            }
            LDUMINAB => {
                mnemonic = "uminab";
            }
            LDUMINLB => {
                mnemonic = "uminlb";
            }
            LDUMINALB => {
                mnemonic = "uminalb";
            }
            LDUMINH => {
                mnemonic = "uminh";
            }
            LDUMINAH => {
                mnemonic = "uminah";
            }
            LDUMINLH => {
                mnemonic = "uminlh";
            }
            LDUMINALH => {
                mnemonic = "uminalh";
            }
            LDUMIN_w => {
                mnemonic = "umin";
            }
            LDUMINA_w => {
                mnemonic = "umina";
            }
            LDUMINL_w => {
                mnemonic = "uminl";
            }
            LDUMINAL_w => {
                mnemonic = "uminal";
            }
            LDUMIN_x => {
                mnemonic = "umin";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMINA_x => {
                mnemonic = "umina";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMINL_x => {
                mnemonic = "uminl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            LDUMINAL_x => {
                mnemonic = "uminal";
                form = "'Xs, 'Xt, ['Xns]";

                // SWP has the same semantics as ldadd etc but without the store aliases.
            }
            SWPB => {
                mnemonic = "swpb";
            }
            SWPAB => {
                mnemonic = "swpab";
            }
            SWPLB => {
                mnemonic = "swplb";
            }
            SWPALB => {
                mnemonic = "swpalb";
            }
            SWPH => {
                mnemonic = "swph";
            }
            SWPAH => {
                mnemonic = "swpah";
            }
            SWPLH => {
                mnemonic = "swplh";
            }
            SWPALH => {
                mnemonic = "swpalh";
            }
            SWP_w => {
                mnemonic = "swp";
            }
            SWPA_w => {
                mnemonic = "swpa";
            }
            SWPL_w => {
                mnemonic = "swpl";
            }
            SWPAL_w => {
                mnemonic = "swpal";
            }
            SWP_x => {
                mnemonic = "swp";
                form = "'Xs, 'Xt, ['Xns]";
            }
            SWPA_x => {
                mnemonic = "swpa";
                form = "'Xs, 'Xt, ['Xns]";
            }
            SWPL_x => {
                mnemonic = "swpl";
                form = "'Xs, 'Xt, ['Xns]";
            }
            SWPAL_x => {
                mnemonic = "swpal";
                form = "'Xs, 'Xt, ['Xns]";
            }
            _ => {
                mnemonic = "unimplemented";
                form = "(AtomicMemory)";
            }
        }

        let mut prefix = "";
        match instr.mask(AtomicMemoryMask) {
            LDADDAB | LDADDALB | LDADDAH | LDADDALH | LDADDA_w | LDADDAL_w | LDADDA_x
            | LDADDAL_x => {
                prefix = "ld";
            }
            LDADDB | LDADDLB | LDADDH | LDADDLH | LDADD_w | LDADDL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDADD_x | LDADDL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            LDCLRAB | LDCLRALB | LDCLRAH | LDCLRALH | LDCLRA_w | LDCLRAL_w | LDCLRA_x
            | LDCLRAL_x => {
                prefix = "ld";
            }
            LDCLRB | LDCLRLB | LDCLRH | LDCLRLH | LDCLR_w | LDCLRL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDCLR_x | LDCLRL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            LDEORAB | LDEORALB | LDEORAH | LDEORALH | LDEORA_w | LDEORAL_w | LDEORA_x
            | LDEORAL_x => {
                prefix = "ld";
            }
            LDEORB | LDEORLB | LDEORH | LDEORLH | LDEOR_w | LDEORL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDEOR_x | LDEORL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            LDSETAB | LDSETALB | LDSETAH | LDSETALH | LDSETA_w | LDSETAL_w | LDSETA_x
            | LDSETAL_x => {
                prefix = "ld";
            }
            LDSETB | LDSETLB | LDSETH | LDSETLH | LDSET_w | LDSETL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDSET_x | LDSETL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            LDSMAXAB | LDSMAXALB | LDSMAXAH | LDSMAXALH | LDSMAXA_w | LDSMAXAL_w | LDSMAXA_x
            | LDSMAXAL_x => {
                prefix = "ld";
            }
            LDSMAXB | LDSMAXLB | LDSMAXH | LDSMAXLH | LDSMAX_w | LDSMAXL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDSMAX_x | LDSMAXL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            LDSMINAB | LDSMINALB | LDSMINAH | LDSMINALH | LDSMINA_w | LDSMINAL_w | LDSMINA_x
            | LDSMINAL_x => {
                prefix = "ld";
            }
            LDSMINB | LDSMINLB | LDSMINH | LDSMINLH | LDSMIN_w | LDSMINL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDSMIN_x | LDSMINL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            LDUMAXAB | LDUMAXALB | LDUMAXAH | LDUMAXALH | LDUMAXA_w | LDUMAXAL_w | LDUMAXA_x
            | LDUMAXAL_x => {
                prefix = "ld";
            }
            LDUMAXB | LDUMAXLB | LDUMAXH | LDUMAXLH | LDUMAX_w | LDUMAXL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDUMAX_x | LDUMAXL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            LDUMINAB | LDUMINALB | LDUMINAH | LDUMINALH | LDUMINA_w | LDUMINAL_w | LDUMINA_x
            | LDUMINAL_x => {
                prefix = "ld";
            }
            LDUMINB | LDUMINLB | LDUMINH | LDUMINLH | LDUMIN_w | LDUMINL_w => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Ws, ['Xns]";
                }
            }
            LDUMIN_x | LDUMINL_x => {
                prefix = "ld";
                let rt = instr.rt();
                if rt == kZeroRegCode as u32 {
                    prefix = "st";
                    form = "'Xs, ['Xns]";
                }
            }
            _ => (),
        }

        let buffer = format!("{}{}", prefix, mnemonic);
        if !prefix.is_empty() {
            mnemonic = &buffer;
        }

        self.format(instr, mnemonic, form);
    }

    fn visit_fp_compare(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Fn, 'Fm";
        let form_zero = "'Fn, #0.0";

        match instr.mask(FPCompareMask) {
            FCMP_s_zero | FCMP_d_zero => {
                form = form_zero;
            }
            FCMP_s | FCMP_d => {
                mnemonic = "fcmp";
            }
            _ => {
                form = "(FPCompare)";
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_conditional_compare(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Fn, 'Fm, 'INzcv, 'Cond";

        match instr.mask(FPConditionalCompareMask) {
            FCCMP_s | FCCMP_d => {
                mnemonic = "fccmp";
            }
            FCCMPE_s | FCCMPE_d => {
                mnemonic = "fccmpe";
            }
            _ => {
                form = "(FPConditionalCompare)";
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_conditional_select(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Fd, 'Fn, 'Fm, 'Cond";

        match instr.mask(FPConditionalSelectMask) {
            FCSEL_s | FCSEL_d => {
                mnemonic = "fcsel";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_data_processing_1_source(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Fd, 'Fn";

        match instr.mask(FPDataProcessing1SourceMask) {
            FMOV_s | FMOV_d => {
                mnemonic = "fmov";
            }
            FABS_s | FABS_d => {
                mnemonic = "fabs";
            }
            FNEG_s | FNEG_d => {
                mnemonic = "fneg";
            }
            FSQRT_s | FSQRT_d => {
                mnemonic = "fsqrt";
            }
            FRINTN_s | FRINTN_d => {
                mnemonic = "frintn";
            }
            FRINTP_s | FRINTP_d => {
                mnemonic = "frintp";
            }
            FRINTM_s | FRINTM_d => {
                mnemonic = "frintm";
            }
            FRINTZ_s | FRINTZ_d => {
                mnemonic = "frintz";
            }
            FRINTA_s | FRINTA_d => {
                mnemonic = "frinta";
            }
            FRINTX_s | FRINTX_d => {
                mnemonic = "frintx";
            }
            FRINTI_s | FRINTI_d => {
                mnemonic = "frinti";
            }
            FCVT_ds => {
                mnemonic = "fcvt";
                form = "'Dd, 'Sn";
            }
            FCVT_sd => {
                mnemonic = "fcvt";
                form = "'Sd, 'Dn";
            }
            FCVT_hs => {
                mnemonic = "fcvt";
                form = "'Hd, 'Sn";
            }
            FCVT_sh => {
                mnemonic = "fcvt";
                form = "'Sd, 'Hn";
            }
            FCVT_dh => {
                mnemonic = "fcvt";
                form = "'Dd, 'Hn";
            }
            FCVT_hd => {
                mnemonic = "fcvt";
                form = "'Hd, 'Dn";
            }
            _ => {
                form = "(FPDataProcessing1Source)";
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_data_processing_2_source(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Fd, 'Fn, 'Fm";

        match instr.mask(FPDataProcessing2SourceMask) {
            FMUL_s | FMUL_d => {
                mnemonic = "fmul";
            }
            FDIV_s | FDIV_d => {
                mnemonic = "fdiv";
            }
            FADD_s | FADD_d => {
                mnemonic = "fadd";
            }
            FSUB_s | FSUB_d => {
                mnemonic = "fsub";
            }
            FMAX_s | FMAX_d => {
                mnemonic = "fmax";
            }
            FMIN_s | FMIN_d => {
                mnemonic = "fmin";
            }
            FMAXNM_s | FMAXNM_d => {
                mnemonic = "fmaxnm";
            }
            FMINNM_s | FMINNM_d => {
                mnemonic = "fminnm";
            }
            FNMUL_s | FNMUL_d => {
                mnemonic = "fnmul";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_data_processing_3_source(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Fd, 'Fn, 'Fm, 'Fa";

        match instr.mask(FPDataProcessing3SourceMask) {
            FMADD_s | FMADD_d => {
                mnemonic = "fmadd";
            }
            FMSUB_s | FMSUB_d => {
                mnemonic = "fmsub";
            }
            FNMADD_s | FNMADD_d => {
                mnemonic = "fnmadd";
            }
            FNMSUB_s | FNMSUB_d => {
                mnemonic = "fnmsub";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_immediate(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form: &str;

        match instr.mask(FPImmediateMask) {
            FMOV_s_imm => {
                mnemonic = "fmov";
                form = "'Sd, 'IFPSingle";
            }
            FMOV_d_imm => {
                mnemonic = "fmov";
                form = "'Dd, 'IFPDouble";
            }
            _ => {
                unreachable!();
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_integer_convert(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(FPIntegerConvert)";
        let form_rf = "'Rd, 'Fn";
        let form_fr = "'Fd, 'Rn";

        match instr.mask(FPIntegerConvertMask) {
            FMOV_ws | FMOV_xd => {
                mnemonic = "fmov";
                form = form_rf;
            }
            FMOV_sw | FMOV_dx => {
                mnemonic = "fmov";
                form = form_fr;
            }
            FMOV_d1_x => {
                mnemonic = "fmov";
                form = "'Vd.D[1], 'Rn";
            }
            FMOV_x_d1 => {
                mnemonic = "fmov";
                form = "'Rd, 'Vn.D[1]";
            }
            FCVTAS_ws | FCVTAS_xs | FCVTAS_wd | FCVTAS_xd => {
                mnemonic = "fcvtas";
                form = form_rf;
            }
            FCVTAU_ws | FCVTAU_xs | FCVTAU_wd | FCVTAU_xd => {
                mnemonic = "fcvtau";
                form = form_rf;
            }
            FCVTMS_ws | FCVTMS_xs | FCVTMS_wd | FCVTMS_xd => {
                mnemonic = "fcvtms";
                form = form_rf;
            }
            FCVTMU_ws | FCVTMU_xs | FCVTMU_wd | FCVTMU_xd => {
                mnemonic = "fcvtmu";
                form = form_rf;
            }
            FCVTNS_ws | FCVTNS_xs | FCVTNS_wd | FCVTNS_xd => {
                mnemonic = "fcvtns";
                form = form_rf;
            }
            FCVTNU_ws | FCVTNU_xs | FCVTNU_wd | FCVTNU_xd => {
                mnemonic = "fcvtnu";
                form = form_rf;
            }
            FCVTZU_xd | FCVTZU_ws | FCVTZU_wd | FCVTZU_xs => {
                mnemonic = "fcvtzu";
                form = form_rf;
            }
            FCVTZS_xd | FCVTZS_wd | FCVTZS_xs | FCVTZS_ws => {
                mnemonic = "fcvtzs";
                form = form_rf;
            }
            FCVTPU_xd | FCVTPU_ws | FCVTPU_wd | FCVTPU_xs => {
                mnemonic = "fcvtpu";
                form = form_rf;
            }
            FCVTPS_xd | FCVTPS_wd | FCVTPS_xs | FCVTPS_ws => {
                mnemonic = "fcvtps";
                form = form_rf;
            }
            SCVTF_sw | SCVTF_sx | SCVTF_dw | SCVTF_dx => {
                mnemonic = "scvtf";
                form = form_fr;
            }
            UCVTF_sw | UCVTF_sx | UCVTF_dw | UCVTF_dx => {
                mnemonic = "ucvtf";
                form = form_fr;
            }
            FJCVTZS => {
                mnemonic = "fjcvtzs";
                form = form_rf;
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_fp_fixed_point_convert(&mut self, instr: Instruction) {
        let mut mnemonic = "";
        let mut form = "'Rd, 'Fn, 'IFPFBits";
        let form_fr = "'Fd, 'Rn, 'IFPFBits";

        match instr.mask(FPFixedPointConvertMask) {
            FCVTZS_ws_fixed | FCVTZS_xs_fixed | FCVTZS_wd_fixed | FCVTZS_xd_fixed => {
                mnemonic = "fcvtzs";
            }
            FCVTZU_ws_fixed | FCVTZU_xs_fixed | FCVTZU_wd_fixed | FCVTZU_xd_fixed => {
                mnemonic = "fcvtzu";
            }
            SCVTF_sw_fixed | SCVTF_sx_fixed | SCVTF_dw_fixed | SCVTF_dx_fixed => {
                mnemonic = "scvtf";
                form = form_fr;
            }
            UCVTF_sw_fixed | UCVTF_sx_fixed | UCVTF_dw_fixed | UCVTF_dx_fixed => {
                mnemonic = "ucvtf";
                form = form_fr;
            }
            _ => (),
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_system(&mut self, instr: Instruction) {
        // Some system instructions hijack their Op and Cp fields to represent a
        // range of immediates instead of indicating a different instruction. This
        // makes the decoding tricky.
        let mut mnemonic = "unimplemented";
        let mut form = "(System)";
        if instr.mask(SystemPAuthFMask) == SystemPAuthFixed {
            match instr.mask(SystemPAuthMask) {
                PACIB1716 => {
                    mnemonic = "pacib1716";
                    form = "";
                }
                AUTIB1716 => {
                    mnemonic = "autib1716";
                    form = "";
                }
                PACIBSP => {
                    mnemonic = "pacibsp";
                    form = "";
                }
                AUTIBSP => {
                    mnemonic = "autibsp";
                    form = "";
                }
                _ => (),
            }
        } else if instr.mask(SystemSysRegFMask) == SystemSysRegFixed {
            match instr.mask(SystemSysRegMask) {
                MRS => {
                    mnemonic = "mrs";
                    match instr.imm_system_register() as i32 {
                        x if x == SystemRegister::NZCV as i32 => {
                            form = "'Xt, nzcv";
                        }
                        x if x == SystemRegister::FPCR as i32 => {
                            form = "'Xt, fpcr";
                        }
                        _ => {
                            form = "'Xt, (unknown)";
                        }
                    }
                }
                MSR => {
                    mnemonic = "msr";
                    match instr.imm_system_register() as i32 {
                        x if x == SystemRegister::NZCV as i32 => {
                            form = "nzcv, 'Xt";
                        }
                        x if x == SystemRegister::FPCR as i32 => {
                            form = "fpcr, 'Xt";
                        }
                        _ => {
                            form = "(unknown), 'Xt";
                        }
                    }
                }
                _ => (),
            }
        } else if instr.mask(SystemHintFMask) == SystemHintFixed {
            debug_assert!(instr.mask(SystemHintMask) == HINT);
            form = "";
            match instr.imm_hint() as i32 {
                x if x == SystemHint::Nop as i32 => {
                    mnemonic = "nop";
                }
                x if x == SystemHint::Yield as i32 => {
                    mnemonic = "yield";
                }
                x if x == SystemHint::Csdb as i32 => {
                    mnemonic = "csdb";
                }
                x if x == SystemHint::Bti as i32 => {
                    mnemonic = "bti";
                }
                x if x == SystemHint::BtiC as i32 => {
                    mnemonic = "bti c";
                }
                x if x == SystemHint::BtiJ as i32 => {
                    mnemonic = "bti j";
                }
                x if x == SystemHint::BtiJc as i32 => {
                    mnemonic = "bti jc";
                }
                _ => {
                    // Fall back to 'hint #<imm7>'.
                    form = "'IH";
                    mnemonic = "hint";
                }
            }
        } else if instr.mask(MemBarrierFMask) == MemBarrierFixed {
            match instr.mask(MemBarrierMask) {
                DMB => {
                    mnemonic = "dmb";
                    form = "'M";
                }
                DSB => {
                    mnemonic = "dsb";
                    form = "'M";
                }
                ISB => {
                    mnemonic = "isb";
                    form = "";
                }
                _ => (),
            }
        }

        self.format(instr, mnemonic, form);
    }

    fn visit_exception(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'IDebug";

        match instr.mask(ExceptionMask) {
            HLT => {
                mnemonic = "hlt";
            }
            BRK => {
                mnemonic = "brk";
            }
            SVC => {
                mnemonic = "svc";
            }
            HVC => {
                mnemonic = "hvc";
            }
            SMC => {
                mnemonic = "smc";
            }
            DCPS1 => {
                mnemonic = "dcps1";
                form = "{'IDebug}";
            }
            DCPS2 => {
                mnemonic = "dcps2";
                form = "{'IDebug}";
            }
            DCPS3 => {
                mnemonic = "dcps3";
                form = "{'IDebug}";
            }
            _ => {
                form = "(Exception)";
            }
        }
        self.format(instr, mnemonic, form);
    }

    fn visit_neon3_same(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Vd.%s, 'Vn.%s, 'Vm.%s";
        let mut nfd = NEONFormatDecoder::new(instr, &[]);

        if instr.mask(NEON3SameLogicalFMask) == NEON3SameLogicalFixed {
            match instr.mask(NEON3SameLogicalMask) {
                NEON_AND => {
                    mnemonic = "and";
                }
                NEON_ORR => {
                    mnemonic = "orr";
                    if instr.rm() == instr.rn() {
                        mnemonic = "mov";
                        form = "'Vd.%s, 'Vn.%s";
                    }
                }
                NEON_ORN => {
                    mnemonic = "orn";
                }
                NEON_EOR => {
                    mnemonic = "eor";
                }
                NEON_BIC => {
                    mnemonic = "bic";
                }
                NEON_BIF => {
                    mnemonic = "bif";
                }
                NEON_BIT => {
                    mnemonic = "bit";
                }
                NEON_BSL => {
                    mnemonic = "bsl";
                }
                _ => {
                    form = "(NEON3Same)";
                }
            }
            nfd.set_format_maps(&[NEONFormatDecoder::logical_format_map()]);
        } else {
            const mnemonics: [&str; 128] = [
                "shadd",
                "uhadd",
                "shadd",
                "uhadd",
                "sqadd",
                "uqadd",
                "sqadd",
                "uqadd",
                "srhadd",
                "urhadd",
                "srhadd",
                "urhadd",
                "",
                "",
                "",
                "",
                "shsub",
                "uhsub",
                "shsub",
                "uhsub",
                "sqsub",
                "uqsub",
                "sqsub",
                "uqsub",
                "cmgt",
                "cmhi",
                "cmgt",
                "cmhi",
                "cmge",
                "cmhs",
                "cmge",
                "cmhs",
                "sshl",
                "ushl",
                "sshl",
                "ushl",
                "sqshl",
                "uqshl",
                "sqshl",
                "uqshl",
                "srshl",
                "urshl",
                "srshl",
                "urshl",
                "sqrshl",
                "uqrshl",
                "sqrshl",
                "uqrshl",
                "smax",
                "umax",
                "smax",
                "umax",
                "smin",
                "umin",
                "smin",
                "umin",
                "sabd",
                "uabd",
                "sabd",
                "uabd",
                "saba",
                "uaba",
                "saba",
                "uaba",
                "add",
                "sub",
                "add",
                "sub",
                "cmtst",
                "cmeq",
                "cmtst",
                "cmeq",
                "mla",
                "mls",
                "mla",
                "mls",
                "mul",
                "pmul",
                "mul",
                "pmul",
                "smaxp",
                "umaxp",
                "smaxp",
                "umaxp",
                "sminp",
                "uminp",
                "sminp",
                "uminp",
                "sqdmulh",
                "sqrdmulh",
                "sqdmulh",
                "sqrdmulh",
                "addp",
                "unallocated",
                "addp",
                "unallocated",
                "fmaxnm",
                "fmaxnmp",
                "fminnm",
                "fminnmp",
                "fmla",
                "unallocated",
                "fmls",
                "unallocated",
                "fadd",
                "faddp",
                "fsub",
                "fabd",
                "fmulx",
                "fmul",
                "unallocated",
                "unallocated",
                "fcmeq",
                "fcmge",
                "unallocated",
                "fcmgt",
                "unallocated",
                "facge",
                "unallocated",
                "facgt",
                "fmax",
                "fmaxp",
                "fmin",
                "fminp",
                "frecps",
                "fdiv",
                "frsqrts",
                "unallocated",
            ];

            // Operation is determined by the opcode bits (15-11), the top bit of
            // size (23) and the U bit (29).
            let index = (instr.bits(15, 11) << 2) | (instr.bit(23) << 1) | instr.bit(29);
            debug_assert!((index as usize) < mnemonics.len());
            mnemonic = mnemonics[index as usize];
            // Assert that index is not one of the previously handled logical
            // instructions.

            if instr.mask(NEON3SameFPFMask) == NEON3SameFPFixed {
                nfd.set_format_maps(&[NEONFormatDecoder::fp_format_map()]);
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon3_same_hp(&mut self, instr: Instruction) {
        let mnemonic: &str;
        let form = "'Vd.%s, 'Vn.%s, 'Vm.%s";
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::fphp_format_map()]);

        const mnemonics: [&str; 32] = [
            "fmaxnm", "fmaxnmp", "fminnm", "fminnmp", "fmla", "uqadd", "fmls", "uqadd", "fadd",
            "faddp", "fsub", "fabd", "fmulx", "fmul", "fmul", "fmul", "fcmeq", "fcmge", "shsub",
            "fcmgt", "sqsub", "facge", "sqsub", "facgt", "fmax", "fmaxp", "fmin", "fminp",
            "frecps", "fdiv", "frsqrts", "fdiv",
        ];

        // Operation is determined by the opcode bits (13-11), the top bit of
        // size (23) and the U bit (29).
        let index = (instr.bits(13, 11) << 2) | (instr.bit(23) << 1) | instr.bit(29);
        debug_assert!((index as usize) < mnemonics.len());
        mnemonic = mnemonics[index as usize];

        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon2_reg_misc(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Vd.%s, 'Vn.%s";
        let form_cmp_zero = "'Vd.%s, 'Vn.%s, #0";
        let form_fcmp_zero = "'Vd.%s, 'Vn.%s, #0.0";
        let mut nfd = NEONFormatDecoder::new(instr, &[]);

        if instr.mask(NEON2RegMiscOpcode) <= NEON_NEG_opcode {
            // These instructions all use a two bit size field, except NOT and RBIT,
            // which use the field to encode the operation.
            match instr.mask(NEON2RegMiscMask) {
                NEON_REV64 => {
                    mnemonic = "rev64";
                }
                NEON_REV32 => {
                    mnemonic = "rev32";
                }
                NEON_REV16 => {
                    mnemonic = "rev16";
                }
                NEON_SADDLP => {
                    mnemonic = "saddlp";
                    nfd.set_format_map(0, visit_neon_2_reg_misc_map_lp_ta());
                }
                NEON_UADDLP => {
                    mnemonic = "uaddlp";
                    nfd.set_format_map(0, visit_neon_2_reg_misc_map_lp_ta());
                }
                NEON_SUQADD => {
                    mnemonic = "suqadd";
                }
                NEON_USQADD => {
                    mnemonic = "usqadd";
                }
                NEON_CLS => {
                    mnemonic = "cls";
                }
                NEON_CLZ => {
                    mnemonic = "clz";
                }
                NEON_CNT => {
                    mnemonic = "cnt";
                }
                NEON_SADALP => {
                    mnemonic = "sadalp";
                    nfd.set_format_map(0, visit_neon_2_reg_misc_map_lp_ta());
                }
                NEON_UADALP => {
                    mnemonic = "uadalp";
                    nfd.set_format_map(0, visit_neon_2_reg_misc_map_lp_ta());
                }
                NEON_SQABS => {
                    mnemonic = "sqabs";
                }
                NEON_SQNEG => {
                    mnemonic = "sqneg";
                }
                NEON_CMGT_zero => {
                    mnemonic = "cmgt";
                    form = form_cmp_zero;
                }
                NEON_CMGE_zero => {
                    mnemonic = "cmge";
                    form = form_cmp_zero;
                }
                NEON_CMEQ_zero => {
                    mnemonic = "cmeq";
                    form = form_cmp_zero;
                }
                NEON_CMLE_zero => {
                    mnemonic = "cmle";
                    form = form_cmp_zero;
                }
                NEON_CMLT_zero => {
                    mnemonic = "cmlt";
                    form = form_cmp_zero;
                }
                NEON_ABS => {
                    mnemonic = "abs";
                }
                NEON_NEG => {
                    mnemonic = "neg";
                }
                NEON_RBIT_NOT => {
                    match instr.fp_type() {
                        0 => {
                            mnemonic = "mvn";
                        }
                        1 => {
                            mnemonic = "rbit";
                        }
                        _ => {
                            form = "(NEON2RegMisc)";
                        }
                    }
                    nfd.set_format_maps(&[NEONFormatDecoder::logical_format_map()]);
                }
                _ => (),
            }
        } else {
            // These instructions all use a one bit size field, except XTN, SQXTUN,
            // SHLL, SQXTN and UQXTN, which use a two bit size field.
            if instr.mask(NEON2RegMiscHPFixed) == NEON2RegMiscHPFixed {
                nfd.set_format_maps(&[NEONFormatDecoder::fphp_format_map()]);
            } else {
                nfd.set_format_maps(&[NEONFormatDecoder::fp_format_map()]);
            }
            match instr.mask(NEON2RegMiscFPMask ^ NEON2RegMiscHPFixed) {
                NEON_FABS => {
                    mnemonic = "fabs";
                }
                NEON_FNEG => {
                    mnemonic = "fneg";
                }
                NEON_FCVTN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "fcvtn2"
                    } else {
                        "fcvtn"
                    };
                    nfd.set_format_map(0, visit_neon_2_reg_misc_map_cvt_tb());
                    nfd.set_format_map(1, visit_neon_2_reg_misc_map_cvt_ta());
                }
                NEON_FCVTXN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "fcvtxn2"
                    } else {
                        "fcvtxn"
                    };
                    nfd.set_format_map(0, visit_neon_2_reg_misc_map_cvt_tb());
                    nfd.set_format_map(1, visit_neon_2_reg_misc_map_cvt_ta());
                }
                NEON_FCVTL => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "fcvtl2"
                    } else {
                        "fcvtl"
                    };
                    nfd.set_format_map(0, visit_neon_2_reg_misc_map_cvt_ta());
                    nfd.set_format_map(1, visit_neon_2_reg_misc_map_cvt_tb());
                }
                NEON_FRINTN => {
                    mnemonic = "frintn";
                }
                NEON_FRINTA => {
                    mnemonic = "frinta";
                }
                NEON_FRINTP => {
                    mnemonic = "frintp";
                }
                NEON_FRINTM => {
                    mnemonic = "frintm";
                }
                NEON_FRINTX => {
                    mnemonic = "frintx";
                }
                NEON_FRINTZ => {
                    mnemonic = "frintz";
                }
                NEON_FRINTI => {
                    mnemonic = "frinti";
                }
                NEON_FCVTNS => {
                    mnemonic = "fcvtns";
                }
                NEON_FCVTNU => {
                    mnemonic = "fcvtnu";
                }
                NEON_FCVTPS => {
                    mnemonic = "fcvtps";
                }
                NEON_FCVTPU => {
                    mnemonic = "fcvtpu";
                }
                NEON_FCVTMS => {
                    mnemonic = "fcvtms";
                }
                NEON_FCVTMU => {
                    mnemonic = "fcvtmu";
                }
                NEON_FCVTZS => {
                    mnemonic = "fcvtzs";
                }
                NEON_FCVTZU => {
                    mnemonic = "fcvtzu";
                }
                NEON_FCVTAS => {
                    mnemonic = "fcvtas";
                }
                NEON_FCVTAU => {
                    mnemonic = "fcvtau";
                }
                NEON_FSQRT => {
                    mnemonic = "fsqrt";
                }
                NEON_SCVTF => {
                    mnemonic = "scvtf";
                }
                NEON_UCVTF => {
                    mnemonic = "ucvtf";
                }
                NEON_URSQRTE => {
                    mnemonic = "ursqrte";
                }
                NEON_URECPE => {
                    mnemonic = "urecpe";
                }
                NEON_FRSQRTE => {
                    mnemonic = "frsqrte";
                }
                NEON_FRECPE => {
                    mnemonic = "frecpe";
                }
                NEON_FCMGT_zero => {
                    mnemonic = "fcmgt";
                    form = form_fcmp_zero;
                }
                NEON_FCMGE_zero => {
                    mnemonic = "fcmge";
                    form = form_fcmp_zero;
                }
                NEON_FCMEQ_zero => {
                    mnemonic = "fcmeq";
                    form = form_fcmp_zero;
                }
                NEON_FCMLE_zero => {
                    mnemonic = "fcmle";
                    form = form_fcmp_zero;
                }
                NEON_FCMLT_zero => {
                    mnemonic = "fcmlt";
                    form = form_fcmp_zero;
                }
                _ => {
                    if (NEON_XTN_opcode <= instr.mask(NEON2RegMiscOpcode))
                        && (instr.mask(NEON2RegMiscOpcode) <= NEON_UQXTN_opcode)
                    {
                        nfd.set_format_map(0, NEONFormatDecoder::integer_format_map());
                        nfd.set_format_map(1, NEONFormatDecoder::long_integer_format_map());

                        match instr.mask(NEON2RegMiscMask) {
                            NEON_XTN => {
                                mnemonic = "xtn";
                            }
                            NEON_SQXTN => {
                                mnemonic = "sqxtn";
                            }
                            NEON_UQXTN => {
                                mnemonic = "uqxtn";
                            }
                            NEON_SQXTUN => {
                                mnemonic = "sqxtun";
                            }
                            NEON_SHLL => {
                                mnemonic = "shll";
                                nfd.set_format_map(0, NEONFormatDecoder::long_integer_format_map());
                                nfd.set_format_map(1, NEONFormatDecoder::integer_format_map());
                                match instr.neon_size() {
                                    0 => {
                                        form = "'Vd.%s, 'Vn.%s, #8";
                                    }
                                    1 => {
                                        form = "'Vd.%s, 'Vn.%s, #16";
                                    }
                                    2 => {
                                        form = "'Vd.%s, 'Vn.%s, #32";
                                    }
                                    _ => {
                                        self.format(instr, "unallocated", "(NEON2RegMisc)");
                                        return;
                                    }
                                }
                            }
                            _ => (),
                        }
                        let mnemonic_text = nfd.mnemonic(mnemonic);
                        let form_text = nfd.substitute(
                            form,
                            [
                                SubstitutionMode::Format,
                                SubstitutionMode::Format,
                                SubstitutionMode::Format,
                                SubstitutionMode::Format,
                            ],
                        );
                        self.format(instr, &mnemonic_text, &form_text);
                        return;
                    } else {
                        form = "(NEON2RegMisc)";
                    }
                }
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon3_different(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Vd.%s, 'Vn.%s, 'Vm.%s";

        let mut nfd = NEONFormatDecoder::new(instr, &[]);
        nfd.set_format_map(0, NEONFormatDecoder::long_integer_format_map());

        // Ignore the Q bit. Appending a "2" suffix is handled later.
        match instr.mask(NEON3DifferentMask) & !NEON_Q {
            NEON_PMULL => {
                self.disassemble_neon_polynomial_mul(instr);
                return;
            }
            NEON_SABAL => {
                mnemonic = "sabal";
            }
            NEON_SABDL => {
                mnemonic = "sabdl";
            }
            NEON_SADDL => {
                mnemonic = "saddl";
            }
            NEON_SMLAL => {
                mnemonic = "smlal";
            }
            NEON_SMLSL => {
                mnemonic = "smlsl";
            }
            NEON_SMULL => {
                mnemonic = "smull";
            }
            NEON_SSUBL => {
                mnemonic = "ssubl";
            }
            NEON_SQDMLAL => {
                mnemonic = "sqdmlal";
            }
            NEON_SQDMLSL => {
                mnemonic = "sqdmlsl";
            }
            NEON_SQDMULL => {
                mnemonic = "sqdmull";
            }
            NEON_UABAL => {
                mnemonic = "uabal";
            }
            NEON_UABDL => {
                mnemonic = "uabdl";
            }
            NEON_UADDL => {
                mnemonic = "uaddl";
            }
            NEON_UMLAL => {
                mnemonic = "umlal";
            }
            NEON_UMLSL => {
                mnemonic = "umlsl";
            }
            NEON_UMULL => {
                mnemonic = "umull";
            }
            NEON_USUBL => {
                mnemonic = "usubl";
            }
            NEON_SADDW => {
                mnemonic = "saddw";
                nfd.set_format_map(1, NEONFormatDecoder::long_integer_format_map());
            }
            NEON_SSUBW => {
                mnemonic = "ssubw";
                nfd.set_format_map(1, NEONFormatDecoder::long_integer_format_map());
            }
            NEON_UADDW => {
                mnemonic = "uaddw";
                nfd.set_format_map(1, NEONFormatDecoder::long_integer_format_map());
            }
            NEON_USUBW => {
                mnemonic = "usubw";
                nfd.set_format_map(1, NEONFormatDecoder::long_integer_format_map());
            }
            NEON_ADDHN => {
                mnemonic = "addhn";
                nfd.set_format_maps(&[NEONFormatDecoder::long_integer_format_map()]);
                nfd.set_format_map(0, NEONFormatDecoder::integer_format_map());
            }
            NEON_RADDHN => {
                mnemonic = "raddhn";
                nfd.set_format_maps(&[NEONFormatDecoder::long_integer_format_map()]);
                nfd.set_format_map(0, NEONFormatDecoder::integer_format_map());
            }
            NEON_RSUBHN => {
                mnemonic = "rsubhn";
                nfd.set_format_maps(&[NEONFormatDecoder::long_integer_format_map()]);
                nfd.set_format_map(0, NEONFormatDecoder::integer_format_map());
            }
            NEON_SUBHN => {
                mnemonic = "subhn";
                nfd.set_format_maps(&[NEONFormatDecoder::long_integer_format_map()]);
                nfd.set_format_map(0, NEONFormatDecoder::integer_format_map());
            }
            _ => {
                form = "(NEON3Different)";
            }
        }
        let mnemonic_text = nfd.mnemonic(mnemonic);
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, &mnemonic_text, &form_text);
    }

    fn visit_neon3_extension(&mut self, instr: Instruction) {
        let form: &str;
        let mut mnemonic = "unimplemented";

        match instr.mask(NEON3ExtensionMask) {
            NEON_SDOT => {
                if instr.neon_size() != 2 {
                    self.visit_unallocated(instr);
                    return;
                }

                form = if instr.bit(30) == 1 {
                    "'Vd.4s, 'Vn.16b, 'Vm.16b"
                } else {
                    "'Vd.2s, 'Vn.8b, 'Vm.8b"
                };
                mnemonic = "sdot";
            }
            _ => {
                form = "(NEON3Extension)";
            }
        }

        self.format(instr, mnemonic, form);
    }

    fn visit_neon_across_lanes(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "%sd, 'Vn.%s";

        let mut nfd = NEONFormatDecoder::new(
            instr,
            &[
                NEONFormatDecoder::scalar_format_map(),
                NEONFormatDecoder::integer_format_map(),
            ],
        );

        if instr.mask(NEONAcrossLanesFPFMask) == NEONAcrossLanesFPFixed {
            nfd.set_format_map(0, NEONFormatDecoder::fp_scalar_format_map());
            nfd.set_format_map(1, NEONFormatDecoder::fp_format_map());
            match instr.mask(NEONAcrossLanesFPMask) {
                NEON_FMAXV => {
                    mnemonic = "fmaxv";
                }
                NEON_FMINV => {
                    mnemonic = "fminv";
                }
                NEON_FMAXNMV => {
                    mnemonic = "fmaxnmv";
                }
                NEON_FMINNMV => {
                    mnemonic = "fminnmv";
                }
                _ => {
                    form = "(NEONAcrossLanes)";
                }
            }
        } else if instr.mask(NEONAcrossLanesFMask) == NEONAcrossLanesFixed {
            match instr.mask(NEONAcrossLanesMask) {
                NEON_ADDV => {
                    mnemonic = "addv";
                }
                NEON_SMAXV => {
                    mnemonic = "smaxv";
                }
                NEON_SMINV => {
                    mnemonic = "sminv";
                }
                NEON_UMAXV => {
                    mnemonic = "umaxv";
                }
                NEON_UMINV => {
                    mnemonic = "uminv";
                }
                NEON_SADDLV => {
                    mnemonic = "saddlv";
                    nfd.set_format_map(0, NEONFormatDecoder::long_scalar_format_map());
                }
                NEON_UADDLV => {
                    mnemonic = "uaddlv";
                    nfd.set_format_map(0, NEONFormatDecoder::long_scalar_format_map());
                }
                _ => {
                    form = "(NEONAcrossLanes)";
                }
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Placeholder,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_by_indexed_element(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut l_instr = false;
        let mut fp_instr = false;

        let form = "'Vd.%s, 'Vn.%s, 'Ve.%s['IVByElemIndex]";

        let mut nfd = NEONFormatDecoder::new(
            instr,
            &[
                visit_neon_by_indexed_element_map_ta(),
                NEONFormatDecoder::integer_format_map(),
                NEONFormatDecoder::scalar_format_map(),
            ],
        );

        match instr.mask(NEONByIndexedElementMask) {
            NEON_SMULL_byelement => {
                mnemonic = "smull";
                l_instr = true;
            }
            NEON_UMULL_byelement => {
                mnemonic = "umull";
                l_instr = true;
            }
            NEON_SMLAL_byelement => {
                mnemonic = "smlal";
                l_instr = true;
            }
            NEON_UMLAL_byelement => {
                mnemonic = "umlal";
                l_instr = true;
            }
            NEON_SMLSL_byelement => {
                mnemonic = "smlsl";
                l_instr = true;
            }
            NEON_UMLSL_byelement => {
                mnemonic = "umlsl";
                l_instr = true;
            }
            NEON_SQDMULL_byelement => {
                mnemonic = "sqdmull";
                l_instr = true;
            }
            NEON_SQDMLAL_byelement => {
                mnemonic = "sqdmlal";
                l_instr = true;
            }
            NEON_SQDMLSL_byelement => {
                mnemonic = "sqdmlsl";
                l_instr = true;
            }
            NEON_MUL_byelement => {
                mnemonic = "mul";
            }
            NEON_MLA_byelement => {
                mnemonic = "mla";
            }
            NEON_MLS_byelement => {
                mnemonic = "mls";
            }
            NEON_SQDMULH_byelement => {
                mnemonic = "sqdmulh";
            }
            NEON_SQRDMULH_byelement => {
                mnemonic = "sqrdmulh";
            }
            _ => match instr.mask(NEONByIndexedElementFPMask) {
                NEON_FMUL_byelement => {
                    mnemonic = "fmul";
                    fp_instr = true;
                }
                NEON_FMLA_byelement => {
                    mnemonic = "fmla";
                    fp_instr = true;
                }
                NEON_FMLS_byelement => {
                    mnemonic = "fmls";
                    fp_instr = true;
                }
                NEON_FMULX_byelement => {
                    mnemonic = "fmulx";
                    fp_instr = true;
                }
                _ => (),
            },
        }

        if l_instr {
            let mnemonic_text = nfd.mnemonic(mnemonic);
            let form_text = nfd.substitute(
                form,
                [
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                ],
            );
            self.format(instr, &mnemonic_text, &form_text);
        } else if fp_instr {
            nfd.set_format_map(0, NEONFormatDecoder::fp_format_map());
            let form_text = nfd.substitute(
                form,
                [
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                ],
            );
            self.format(instr, mnemonic, &form_text);
        } else {
            nfd.set_format_map(0, NEONFormatDecoder::integer_format_map());
            let form_text = nfd.substitute(
                form,
                [
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                    SubstitutionMode::Format,
                ],
            );
            self.format(instr, mnemonic, &form_text);
        }
    }

    fn visit_neon_copy(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(NEONCopy)";

        let mut nfd = NEONFormatDecoder::new(
            instr,
            &[
                NEONFormatDecoder::triangular_format_map(),
                NEONFormatDecoder::triangular_scalar_format_map(),
            ],
        );

        if instr.mask(NEONCopyInsElementMask) == NEON_INS_ELEMENT {
            mnemonic = "mov";
            nfd.set_format_map(0, NEONFormatDecoder::triangular_scalar_format_map());
            form = "'Vd.%s['IVInsIndex1], 'Vn.%s['IVInsIndex2]";
        } else if instr.mask(NEONCopyInsGeneralMask) == NEON_INS_GENERAL {
            mnemonic = "mov";
            nfd.set_format_map(0, NEONFormatDecoder::triangular_scalar_format_map());
            if nfd.get_vector_format(0) == VectorFormat::FormatD {
                form = "'Vd.%s['IVInsIndex1], 'Xn";
            } else {
                form = "'Vd.%s['IVInsIndex1], 'Wn";
            }
        } else if instr.mask(NEONCopyUmovMask) == NEON_UMOV {
            if instr.mask(NEON_Q) != 0 || ((instr.imm_neon5() & 7) == 4) {
                mnemonic = "mov";
            } else {
                mnemonic = "umov";
            }
            nfd.set_format_map(0, NEONFormatDecoder::triangular_scalar_format_map());
            if nfd.get_vector_format(0) == VectorFormat::FormatD {
                form = "'Xd, 'Vn.%s['IVInsIndex1]";
            } else {
                form = "'Wd, 'Vn.%s['IVInsIndex1]";
            }
        } else if instr.mask(NEONCopySmovMask) == NEON_SMOV {
            mnemonic = "smov";
            nfd.set_format_map(0, NEONFormatDecoder::triangular_scalar_format_map());
            form = "'Rdq, 'Vn.%s['IVInsIndex1]";
        } else if instr.mask(NEONCopyDupElementMask) == NEON_DUP_ELEMENT {
            mnemonic = "dup";
            form = "'Vd.%s, 'Vn.%s['IVInsIndex1]";
        } else if instr.mask(NEONCopyDupGeneralMask) == NEON_DUP_GENERAL {
            mnemonic = "dup";
            if nfd.get_vector_format(0) == VectorFormat::Format2D {
                form = "'Vd.%s, 'Xn";
            } else {
                form = "'Vd.%s, 'Wn";
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_extract(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(NEONExtract)";
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::logical_format_map()]);
        if instr.mask(NEONExtractMask) == NEON_EXT {
            mnemonic = "ext";
            form = "'Vd.%s, 'Vn.%s, 'Vm.%s, 'IVExtract";
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_load_store_multi_struct(&mut self, instr: Instruction) {
        let mut mnemonic = "";
        let mut form = "";
        let form_1v = "{'Vt.%s}, ['Xns]";
        let form_2v = "{'Vt.%s, 'Vt2.%s}, ['Xns]";
        let form_3v = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s}, ['Xns]";
        let form_4v = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s, 'Vt4.%s}, ['Xns]";
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::load_store_format_map()]);

        match instr.mask(NEONLoadStoreMultiStructMask) {
            NEON_LD1_1v => {
                mnemonic = "ld1";
                form = form_1v;
            }
            NEON_LD1_2v => {
                mnemonic = "ld1";
                form = form_2v;
            }
            NEON_LD1_3v => {
                mnemonic = "ld1";
                form = form_3v;
            }
            NEON_LD1_4v => {
                mnemonic = "ld1";
                form = form_4v;
            }
            NEON_LD2 => {
                mnemonic = "ld2";
                form = form_2v;
            }
            NEON_LD3 => {
                mnemonic = "ld3";
                form = form_3v;
            }
            NEON_LD4 => {
                mnemonic = "ld4";
                form = form_4v;
            }
            NEON_ST1_1v => {
                mnemonic = "st1";
                form = form_1v;
            }
            NEON_ST1_2v => {
                mnemonic = "st1";
                form = form_2v;
            }
            NEON_ST1_3v => {
                mnemonic = "st1";
                form = form_3v;
            }
            NEON_ST1_4v => {
                mnemonic = "st1";
                form = form_4v;
            }
            NEON_ST2 => {
                mnemonic = "st2";
                form = form_2v;
            }
            NEON_ST3 => {
                mnemonic = "st3";
                form = form_3v;
            }
            NEON_ST4 => {
                mnemonic = "st4";
                form = form_4v;
            }
            _ => {}
        }

        // Work out unallocated encodings.
        let mut allocated = mnemonic != "";
        match instr.mask(NEONLoadStoreMultiStructMask) {
            NEON_LD2 | NEON_LD3 | NEON_LD4 | NEON_ST2 | NEON_ST3 | NEON_ST4 => {
                // LD[2-4] and ST[2-4] cannot use .1d format.
                allocated = (instr.neon_q() != 0) || (instr.neon_ls_size() != 3);
            }
            _ => {}
        }
        if allocated {
        } else {
            mnemonic = "unallocated";
            form = "(NEONLoadStoreMultiStruct)";
        }

        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_load_store_multi_struct_post_index(&mut self, instr: Instruction) {
        let mut mnemonic = "";
        let mut form = "";
        let form_1v = "{'Vt.%s}, ['Xns], 'Xmr1";
        let form_2v = "{'Vt.%s, 'Vt2.%s}, ['Xns], 'Xmr2";
        let form_3v = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s}, ['Xns], 'Xmr3";
        let form_4v = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s, 'Vt4.%s}, ['Xns], 'Xmr4";
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::load_store_format_map()]);

        match instr.mask(NEONLoadStoreMultiStructPostIndexMask) {
            NEON_LD1_1v_post => {
                mnemonic = "ld1";
                form = form_1v;
            }
            NEON_LD1_2v_post => {
                mnemonic = "ld1";
                form = form_2v;
            }
            NEON_LD1_3v_post => {
                mnemonic = "ld1";
                form = form_3v;
            }
            NEON_LD1_4v_post => {
                mnemonic = "ld1";
                form = form_4v;
            }
            NEON_LD2_post => {
                mnemonic = "ld2";
                form = form_2v;
            }
            NEON_LD3_post => {
                mnemonic = "ld3";
                form = form_3v;
            }
            NEON_LD4_post => {
                mnemonic = "ld4";
                form = form_4v;
            }
            NEON_ST1_1v_post => {
                mnemonic = "st1";
                form = form_1v;
            }
            NEON_ST1_2v_post => {
                mnemonic = "st1";
                form = form_2v;
            }
            NEON_ST1_3v_post => {
                mnemonic = "st1";
                form = form_3v;
            }
            NEON_ST1_4v_post => {
                mnemonic = "st1";
                form = form_4v;
            }
            NEON_ST2_post => {
                mnemonic = "st2";
                form = form_2v;
            }
            NEON_ST3_post => {
                mnemonic = "st3";
                form = form_3v;
            }
            NEON_ST4_post => {
                mnemonic = "st4";
                form = form_4v;
            }
            _ => {}
        }

        // Work out unallocated encodings.
        let mut allocated = mnemonic != "";
        match instr.mask(NEONLoadStoreMultiStructPostIndexMask) {
            NEON_LD2_post | NEON_LD3_post | NEON_LD4_post | NEON_ST2_post | NEON_ST3_post
            | NEON_ST4_post => {
                // LD[2-4] and ST[2-4] cannot use .1d format.
                allocated = (instr.neon_q() != 0) || (instr.neon_ls_size() != 3);
            }
            _ => {}
        }
        if allocated {
        } else {
            mnemonic = "unallocated";
            form = "(NEONLoadStoreMultiStructPostIndex)";
        }

        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_load_store_single_struct(&mut self, instr: Instruction) {
        let mut mnemonic = "";
        let mut form = "";

        let form_1b = "{'Vt.b}['IVLSLane0], ['Xns]";
        let form_1h = "{'Vt.h}['IVLSLane1], ['Xns]";
        let form_1s = "{'Vt.s}['IVLSLane2], ['Xns]";
        let form_1d = "{'Vt.d}['IVLSLane3], ['Xns]";
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::load_store_format_map()]);

        match instr.mask(NEONLoadStoreSingleStructMask) {
            NEON_LD1_b => {
                mnemonic = "ld1";
                form = form_1b;
            }
            NEON_LD1_h => {
                mnemonic = "ld1";
                form = form_1h;
            }
            NEON_LD1_s => {
                mnemonic = "ld1";

                form = if (instr.neon_ls_size() & 1) == 0 {
                    form_1s
                } else {
                    form_1d
                };
            }
            NEON_ST1_b => {
                mnemonic = "st1";
                form = form_1b;
            }
            NEON_ST1_h => {
                mnemonic = "st1";
                form = form_1h;
            }
            NEON_ST1_s => {
                mnemonic = "st1";

                form = if (instr.neon_ls_size() & 1) == 0 {
                    form_1s
                } else {
                    form_1d
                };
            }
            NEON_LD1R => {
                mnemonic = "ld1r";
                form = "{'Vt.%s}, ['Xns]";
            }
            NEON_LD2_b | NEON_ST2_b => {
                mnemonic = if instr.neon_load() == 1 { "ld2" } else { "st2" };
                form = "{'Vt.b, 'Vt2.b}['IVLSLane0], ['Xns]";
            }
            NEON_LD2_h | NEON_ST2_h => {
                mnemonic = if instr.neon_load() == 1 { "ld2" } else { "st2" };
                form = "{'Vt.h, 'Vt2.h}['IVLSLane1], ['Xns]";
            }
            NEON_LD2_s | NEON_ST2_s => {
                mnemonic = if instr.neon_load() == 1 { "ld2" } else { "st2" };
                if (instr.neon_ls_size() & 1) == 0 {
                    form = "{'Vt.s, 'Vt2.s}['IVLSLane2], ['Xns]";
                } else {
                    form = "{'Vt.d, 'Vt2.d}['IVLSLane3], ['Xns]";
                }
            }
            NEON_LD2R => {
                mnemonic = "ld2r";
                form = "{'Vt.%s, 'Vt2.%s}, ['Xns]";
            }
            NEON_LD3_b | NEON_ST3_b => {
                mnemonic = if instr.neon_load() == 1 { "ld3" } else { "st3" };
                form = "{'Vt.b, 'Vt2.b, 'Vt3.b}['IVLSLane0], ['Xns]";
            }
            NEON_LD3_h | NEON_ST3_h => {
                mnemonic = if instr.neon_load() == 1 { "ld3" } else { "st3" };
                form = "{'Vt.h, 'Vt2.h, 'Vt3.h}['IVLSLane1], ['Xns]";
            }
            NEON_LD3_s | NEON_ST3_s => {
                mnemonic = if instr.neon_load() == 1 { "ld3" } else { "st3" };
                if (instr.neon_ls_size() & 1) == 0 {
                    form = "{'Vt.s, 'Vt2.s, 'Vt3.s}['IVLSLane2], ['Xns]";
                } else {
                    form = "{'Vt.d, 'Vt2.d, 'Vt3.d}['IVLSLane3], ['Xns]";
                }
            }
            NEON_LD3R => {
                mnemonic = "ld3r";
                form = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s}, ['Xns]";
            }
            NEON_LD4_b | NEON_ST4_b => {
                mnemonic = if instr.neon_load() == 1 { "ld4" } else { "st4" };
                form = "{'Vt.b, 'Vt2.b, 'Vt3.b, 'Vt4.b}['IVLSLane0], ['Xns]";
            }
            NEON_LD4_h | NEON_ST4_h => {
                mnemonic = if instr.neon_load() == 1 { "ld4" } else { "st4" };
                form = "{'Vt.h, 'Vt2.h, 'Vt3.h, 'Vt4.h}['IVLSLane1], ['Xns]";
            }
            NEON_LD4_s | NEON_ST4_s => {
                mnemonic = if instr.neon_load() == 1 { "ld4" } else { "st4" };
                if (instr.neon_ls_size() & 1) == 0 {
                    form = "{'Vt.s, 'Vt2.s, 'Vt3.s, 'Vt4.s}['IVLSLane2], ['Xns]";
                } else {
                    form = "{'Vt.d, 'Vt2.d, 'Vt3.d, 'Vt4.d}['IVLSLane3], ['Xns]";
                }
            }
            NEON_LD4R => {
                mnemonic = "ld4r";
                form = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s, 'Vt4.%s}, ['Xns]";
            }
            _ => {}
        }

        // Work out unallocated encodings.
        let mut allocated = mnemonic != "";
        match instr.mask(NEONLoadStoreSingleStructMask) {
            NEON_LD1_h | NEON_LD2_h | NEON_LD3_h | NEON_LD4_h | NEON_ST1_h | NEON_ST2_h
            | NEON_ST3_h | NEON_ST4_h => {
                debug_assert!(allocated);
                allocated = (instr.neon_ls_size() & 1) == 0;
            }
            NEON_LD1_s | NEON_LD2_s | NEON_LD3_s | NEON_LD4_s | NEON_ST1_s | NEON_ST2_s
            | NEON_ST3_s | NEON_ST4_s => {
                debug_assert!(allocated);
                allocated = (instr.neon_ls_size() <= 1)
                    && ((instr.neon_ls_size() == 0) || (instr.neon_s() == 0));
            }
            NEON_LD1R | NEON_LD2R | NEON_LD3R | NEON_LD4R => {
                debug_assert!(allocated);
                allocated = instr.neon_s() == 0;
            }
            _ => {}
        }
        if allocated {
        } else {
            mnemonic = "unallocated";
            form = "(NEONLoadStoreSingleStruct)";
        }

        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_load_store_single_struct_post_index(&mut self, instr: Instruction) {
        let mut mnemonic = "";
        let mut form = "";

        let form_1b = "{'Vt.b}['IVLSLane0], ['Xns], 'Xmb1";
        let form_1h = "{'Vt.h}['IVLSLane1], ['Xns], 'Xmb2";
        let form_1s = "{'Vt.s}['IVLSLane2], ['Xns], 'Xmb4";
        let form_1d = "{'Vt.d}['IVLSLane3], ['Xns], 'Xmb8";
        let nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::load_store_format_map()]);

        match instr.mask(NEONLoadStoreSingleStructPostIndexMask) {
            NEON_LD1_b_post => {
                mnemonic = "ld1";
                form = form_1b;
            }
            NEON_LD1_h_post => {
                mnemonic = "ld1";
                form = form_1h;
            }
            NEON_LD1_s_post => {
                mnemonic = "ld1";

                form = if (instr.neon_ls_size() & 1) == 0 {
                    form_1s
                } else {
                    form_1d
                };
            }
            NEON_ST1_b_post => {
                mnemonic = "st1";
                form = form_1b;
            }
            NEON_ST1_h_post => {
                mnemonic = "st1";
                form = form_1h;
            }
            NEON_ST1_s_post => {
                mnemonic = "st1";

                form = if (instr.neon_ls_size() & 1) == 0 {
                    form_1s
                } else {
                    form_1d
                };
            }
            NEON_LD1R_post => {
                mnemonic = "ld1r";
                form = "{'Vt.%s}, ['Xns], 'Xmz1";
            }
            NEON_LD2_b_post | NEON_ST2_b_post => {
                mnemonic = if instr.neon_load() == 1 { "ld2" } else { "st2" };
                form = "{'Vt.b, 'Vt2.b}['IVLSLane0], ['Xns], 'Xmb2";
            }
            NEON_ST2_h_post | NEON_LD2_h_post => {
                mnemonic = if instr.neon_load() == 1 { "ld2" } else { "st2" };
                form = "{'Vt.h, 'Vt2.h}['IVLSLane1], ['Xns], 'Xmb4";
            }
            NEON_LD2_s_post | NEON_ST2_s_post => {
                mnemonic = if instr.neon_load() == 1 { "ld2" } else { "st2" };
                if (instr.neon_ls_size() & 1) == 0 {
                    form = "{'Vt.s, 'Vt2.s}['IVLSLane2], ['Xns], 'Xmb8";
                } else {
                    form = "{'Vt.d, 'Vt2.d}['IVLSLane3], ['Xns], 'Xmb16";
                }
            }
            NEON_LD2R_post => {
                mnemonic = "ld2r";
                form = "{'Vt.%s, 'Vt2.%s}, ['Xns], 'Xmz2";
            }
            NEON_LD3_b_post | NEON_ST3_b_post => {
                mnemonic = if instr.neon_load() == 1 { "ld3" } else { "st3" };
                form = "{'Vt.b, 'Vt2.b, 'Vt3.b}['IVLSLane0], ['Xns], 'Xmb3";
            }
            NEON_LD3_h_post | NEON_ST3_h_post => {
                mnemonic = if instr.neon_load() == 1 { "ld3" } else { "st3" };
                form = "{'Vt.h, 'Vt2.h, 'Vt3.h}['IVLSLane1], ['Xns], 'Xmb6";
            }
            NEON_LD3_s_post | NEON_ST3_s_post => {
                mnemonic = if instr.neon_load() == 1 { "ld3" } else { "st3" };
                if (instr.neon_ls_size() & 1) == 0 {
                    form = "{'Vt.s, 'Vt2.s, 'Vt3.s}['IVLSLane2], ['Xns], 'Xmb12";
                } else {
                    form = "{'Vt.d, 'Vt2.d, 'Vt3.d}['IVLSLane3], ['Xns], 'Xmb24";
                }
            }
            NEON_LD3R_post => {
                mnemonic = "ld3r";
                form = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s}, ['Xns], 'Xmz3";
            }
            NEON_LD4_b_post | NEON_ST4_b_post => {
                mnemonic = if instr.neon_load() == 1 { "ld4" } else { "st4" };
                form = "{'Vt.b, 'Vt2.b, 'Vt3.b, 'Vt4.b}['IVLSLane0], ['Xns], 'Xmb4";
            }
            NEON_LD4_h_post | NEON_ST4_h_post => {
                mnemonic = if instr.neon_load() == 1 { "ld4" } else { "st4" };
                form = "{'Vt.h, 'Vt2.h, 'Vt3.h, 'Vt4.h}['IVLSLane1], ['Xns], 'Xmb8";
            }
            NEON_LD4_s_post | NEON_ST4_s_post => {
                mnemonic = if instr.neon_load() == 1 { "ld4" } else { "st4" };
                if (instr.neon_ls_size() & 1) == 0 {
                    form = "{'Vt.s, 'Vt2.s, 'Vt3.s, 'Vt4.s}['IVLSLane2], ['Xns], 'Xmb16";
                } else {
                    form = "{'Vt.d, 'Vt2.d, 'Vt3.d, 'Vt4.d}['IVLSLane3], ['Xns], 'Xmb32";
                }
            }
            NEON_LD4R_post => {
                mnemonic = "ld4r";
                form = "{'Vt.%s, 'Vt2.%s, 'Vt3.%s, 'Vt4.%s}, ['Xns], 'Xmz4";
            }
            _ => {}
        }

        // Work out unallocated encodings.
        let mut allocated = mnemonic != "";
        match instr.mask(NEONLoadStoreSingleStructPostIndexMask) {
            NEON_LD1_h_post | NEON_LD2_h_post | NEON_LD3_h_post | NEON_LD4_h_post
            | NEON_ST1_h_post | NEON_ST2_h_post | NEON_ST3_h_post | NEON_ST4_h_post => {
                debug_assert!(allocated);
                allocated = (instr.neon_ls_size() & 1) == 0;
            }
            NEON_LD1_s_post | NEON_LD2_s_post | NEON_LD3_s_post | NEON_LD4_s_post
            | NEON_ST1_s_post | NEON_ST2_s_post | NEON_ST3_s_post | NEON_ST4_s_post => {
                debug_assert!(allocated);
                allocated = (instr.neon_ls_size() <= 1)
                    && ((instr.neon_ls_size() == 0) || (instr.neon_s() == 0));
            }
            NEON_LD1R_post | NEON_LD2R_post | NEON_LD3R_post | NEON_LD4R_post => {
                debug_assert!(allocated);
                allocated = instr.neon_s() == 0;
            }
            _ => {}
        }
        if allocated {
        } else {
            mnemonic = "unallocated";
            form = "(NEONLoadStoreSingleStructPostIndex)";
        }

        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_modified_immediate(&mut self, instr: Instruction) {
        let mut mnemonic: &str;
        let mut form = "'Vt.%s, 'IVMIImm8, lsl 'IVMIShiftAmt1";

        let cmode = instr.neon_cmode();
        let cmode_3 = (cmode >> 3) & 1;
        let cmode_2 = (cmode >> 2) & 1;
        let cmode_1 = (cmode >> 1) & 1;
        let cmode_0 = cmode & 1;
        let q = instr.neon_q();
        let op = instr.neon_mod_imm_op();

        let mut nfd = NEONFormatDecoder::new(instr, &[visit_neon_modified_immediate_map_b()]);

        if cmode_3 == 0 {
            if cmode_0 == 0 {
                mnemonic = if op == 1 { "mvni" } else { "movi" };
            } else {
                // cmode<0> == '1'.
                mnemonic = if op == 1 { "bic" } else { "orr" };
            }
            nfd.set_format_map(0, visit_neon_modified_immediate_map_s());
        } else {
            // cmode<3> == '1'.
            if cmode_2 == 0 {
                if cmode_0 == 0 {
                    mnemonic = if op == 1 { "mvni" } else { "movi" };
                } else {
                    // cmode<0> == '1'.
                    mnemonic = if op == 1 { "bic" } else { "orr" };
                }
                nfd.set_format_map(0, visit_neon_modified_immediate_map_h());
            } else {
                // cmode<2> == '1'.
                if cmode_1 == 0 {
                    mnemonic = if op == 1 { "mvni" } else { "movi" };
                    form = "'Vt.%s, 'IVMIImm8, msl 'IVMIShiftAmt2";
                    nfd.set_format_map(0, visit_neon_modified_immediate_map_s());
                } else {
                    // cmode<1> == '1'.
                    if cmode_0 == 0 {
                        mnemonic = "movi";
                        if op == 0 {
                            form = "'Vt.%s, 'IVMIImm8";
                        } else {
                            form = if q == 0 {
                                "'Dd, 'IVMIImm"
                            } else {
                                "'Vt.2d, 'IVMIImm"
                            };
                        }
                    } else {
                        // cmode<0> == '1'
                        mnemonic = "fmov";
                        if op == 0 {
                            form = "'Vt.%s, 'IVMIImmFPSingle";
                            nfd.set_format_map(0, visit_neon_modified_immediate_map_s());
                        } else {
                            if q == 1 {
                                form = "'Vt.2d, 'IVMIImmFPDouble";
                            } else {
                                mnemonic = "unallocated";
                                form = "(NEONModifiedImmediate)";
                            }
                        }
                    }
                }
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neonsha3(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Vd.%s, 'Vn.%s, 'Vm.%s, 'Va.%s";
        let nfd = NEONFormatDecoder::new(instr, &[]);

        match instr.mask(NEONSHA3Mask) {
            NEON_BCAX => {
                mnemonic = "bcax";
            }
            NEON_EOR3 => {
                mnemonic = "eor3";
            }
            NEON_XAR => {
                mnemonic = "xar";
                form = "'Vd.2d, 'Vn.2d, 'Vm.2d, #'u1510";
            }
            _ => {
                form = "(NEONSHA3)";
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_perm(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Vd.%s, 'Vn.%s, 'Vm.%s";
        let nfd = NEONFormatDecoder::new(instr, &[]);

        match instr.mask(NEONPermMask) {
            NEON_TRN1 => {
                mnemonic = "trn1";
            }
            NEON_TRN2 => {
                mnemonic = "trn2";
            }
            NEON_UZP1 => {
                mnemonic = "uzp1";
            }
            NEON_UZP2 => {
                mnemonic = "uzp2";
            }
            NEON_ZIP1 => {
                mnemonic = "zip1";
            }
            NEON_ZIP2 => {
                mnemonic = "zip2";
            }
            _ => {
                form = "(NEONPerm)";
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_scalar2_reg_misc(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "%sd, %sn";
        let form_0 = "%sd, %sn, #0";
        let form_fp0 = "%sd, %sn, #0.0";

        let mut nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::scalar_format_map()]);

        if instr.mask(NEON2RegMiscOpcode) <= NEON_NEG_scalar_opcode {
            // These instructions all use a two bit size field, except NOT and RBIT,
            // which use the field to encode the operation.
            match instr.mask(NEONScalar2RegMiscMask) {
                NEON_CMGT_zero_scalar => {
                    mnemonic = "cmgt";
                    form = form_0;
                }
                NEON_CMGE_zero_scalar => {
                    mnemonic = "cmge";
                    form = form_0;
                }
                NEON_CMLE_zero_scalar => {
                    mnemonic = "cmle";
                    form = form_0;
                }
                NEON_CMLT_zero_scalar => {
                    mnemonic = "cmlt";
                    form = form_0;
                }
                NEON_CMEQ_zero_scalar => {
                    mnemonic = "cmeq";
                    form = form_0;
                }
                NEON_NEG_scalar => {
                    mnemonic = "neg";
                }
                NEON_SQNEG_scalar => {
                    mnemonic = "sqneg";
                }
                NEON_ABS_scalar => {
                    mnemonic = "abs";
                }
                NEON_SQABS_scalar => {
                    mnemonic = "sqabs";
                }
                NEON_SUQADD_scalar => {
                    mnemonic = "suqadd";
                }
                NEON_USQADD_scalar => {
                    mnemonic = "usqadd";
                }
                _ => {
                    form = "(NEONScalar2RegMisc)";
                }
            }
        } else {
            // These instructions all use a one bit size field, except SQXTUN, SQXTN
            // and UQXTN, which use a two bit size field.
            nfd.set_format_maps(&[NEONFormatDecoder::fp_scalar_format_map()]);
            match instr.mask(NEONScalar2RegMiscFPMask) {
                NEON_FRSQRTE_scalar => {
                    mnemonic = "frsqrte";
                }
                NEON_FRECPE_scalar => {
                    mnemonic = "frecpe";
                }
                NEON_SCVTF_scalar => {
                    mnemonic = "scvtf";
                }
                NEON_UCVTF_scalar => {
                    mnemonic = "ucvtf";
                }
                NEON_FCMGT_zero_scalar => {
                    mnemonic = "fcmgt";
                    form = form_fp0;
                }
                NEON_FCMGE_zero_scalar => {
                    mnemonic = "fcmge";
                    form = form_fp0;
                }
                NEON_FCMLE_zero_scalar => {
                    mnemonic = "fcmle";
                    form = form_fp0;
                }
                NEON_FCMLT_zero_scalar => {
                    mnemonic = "fcmlt";
                    form = form_fp0;
                }
                NEON_FCMEQ_zero_scalar => {
                    mnemonic = "fcmeq";
                    form = form_fp0;
                }
                NEON_FRECPX_scalar => {
                    mnemonic = "frecpx";
                }
                NEON_FCVTNS_scalar => {
                    mnemonic = "fcvtns";
                }
                NEON_FCVTNU_scalar => {
                    mnemonic = "fcvtnu";
                }
                NEON_FCVTPS_scalar => {
                    mnemonic = "fcvtps";
                }
                NEON_FCVTPU_scalar => {
                    mnemonic = "fcvtpu";
                }
                NEON_FCVTMS_scalar => {
                    mnemonic = "fcvtms";
                }
                NEON_FCVTMU_scalar => {
                    mnemonic = "fcvtmu";
                }
                NEON_FCVTZS_scalar => {
                    mnemonic = "fcvtzs";
                }
                NEON_FCVTZU_scalar => {
                    mnemonic = "fcvtzu";
                }
                NEON_FCVTAS_scalar => {
                    mnemonic = "fcvtas";
                }
                NEON_FCVTAU_scalar => {
                    mnemonic = "fcvtau";
                }
                NEON_FCVTXN_scalar => {
                    nfd.set_format_map(0, NEONFormatDecoder::long_scalar_format_map());
                    mnemonic = "fcvtxn";
                }
                _ => {
                    nfd.set_format_map(0, NEONFormatDecoder::scalar_format_map());
                    nfd.set_format_map(1, NEONFormatDecoder::long_scalar_format_map());
                    match instr.mask(NEONScalar2RegMiscMask) {
                        NEON_SQXTN_scalar => {
                            mnemonic = "sqxtn";
                        }
                        NEON_UQXTN_scalar => {
                            mnemonic = "uqxtn";
                        }
                        NEON_SQXTUN_scalar => {
                            mnemonic = "sqxtun";
                        }
                        _ => {
                            form = "(NEONScalar2RegMisc)";
                        }
                    }
                }
            }
        }
        self.format(instr, mnemonic, &nfd.substitute_placeholders(form));
    }

    fn visit_neon_scalar3_diff(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "%sd, %sn, %sm";
        let nfd = NEONFormatDecoder::new(
            instr,
            &[
                NEONFormatDecoder::long_scalar_format_map(),
                NEONFormatDecoder::scalar_format_map(),
            ],
        );

        match instr.mask(NEONScalar3DiffMask) {
            NEON_SQDMLAL_scalar => {
                mnemonic = "sqdmlal";
            }
            NEON_SQDMLSL_scalar => {
                mnemonic = "sqdmlsl";
            }
            NEON_SQDMULL_scalar => {
                mnemonic = "sqdmull";
            }
            _ => {
                form = "(NEONScalar3Diff)";
            }
        }
        self.format(instr, mnemonic, &nfd.substitute_placeholders(form));
    }

    fn visit_neon_scalar3_same(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "%sd, %sn, %sm";
        let mut nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::scalar_format_map()]);

        if instr.mask(NEONScalar3SameFPFMask) == NEONScalar3SameFPFixed {
            nfd.set_format_maps(&[NEONFormatDecoder::fp_scalar_format_map()]);
            match instr.mask(NEONScalar3SameFPMask) {
                NEON_FACGE_scalar => {
                    mnemonic = "facge";
                }
                NEON_FACGT_scalar => {
                    mnemonic = "facgt";
                }
                NEON_FCMEQ_scalar => {
                    mnemonic = "fcmeq";
                }
                NEON_FCMGE_scalar => {
                    mnemonic = "fcmge";
                }
                NEON_FCMGT_scalar => {
                    mnemonic = "fcmgt";
                }
                NEON_FMULX_scalar => {
                    mnemonic = "fmulx";
                }
                NEON_FRECPS_scalar => {
                    mnemonic = "frecps";
                }
                NEON_FRSQRTS_scalar => {
                    mnemonic = "frsqrts";
                }
                NEON_FABD_scalar => {
                    mnemonic = "fabd";
                }
                _ => {
                    form = "(NEONScalar3Same)";
                }
            }
        } else {
            match instr.mask(NEONScalar3SameMask) {
                NEON_ADD_scalar => {
                    mnemonic = "add";
                }
                NEON_SUB_scalar => {
                    mnemonic = "sub";
                }
                NEON_CMEQ_scalar => {
                    mnemonic = "cmeq";
                }
                NEON_CMGE_scalar => {
                    mnemonic = "cmge";
                }
                NEON_CMGT_scalar => {
                    mnemonic = "cmgt";
                }
                NEON_CMHI_scalar => {
                    mnemonic = "cmhi";
                }
                NEON_CMHS_scalar => {
                    mnemonic = "cmhs";
                }
                NEON_CMTST_scalar => {
                    mnemonic = "cmtst";
                }
                NEON_UQADD_scalar => {
                    mnemonic = "uqadd";
                }
                NEON_SQADD_scalar => {
                    mnemonic = "sqadd";
                }
                NEON_UQSUB_scalar => {
                    mnemonic = "uqsub";
                }
                NEON_SQSUB_scalar => {
                    mnemonic = "sqsub";
                }
                NEON_USHL_scalar => {
                    mnemonic = "ushl";
                }
                NEON_SSHL_scalar => {
                    mnemonic = "sshl";
                }
                NEON_UQSHL_scalar => {
                    mnemonic = "uqshl";
                }
                NEON_SQSHL_scalar => {
                    mnemonic = "sqshl";
                }
                NEON_URSHL_scalar => {
                    mnemonic = "urshl";
                }
                NEON_SRSHL_scalar => {
                    mnemonic = "srshl";
                }
                NEON_UQRSHL_scalar => {
                    mnemonic = "uqrshl";
                }
                NEON_SQRSHL_scalar => {
                    mnemonic = "sqrshl";
                }
                NEON_SQDMULH_scalar => {
                    mnemonic = "sqdmulh";
                }
                NEON_SQRDMULH_scalar => {
                    mnemonic = "sqrdmulh";
                }
                _ => {
                    form = "(NEONScalar3Same)";
                }
            }
        }
        self.format(instr, mnemonic, &nfd.substitute_placeholders(form));
    }

    fn visit_neon_scalar_by_indexed_element(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "%sd, %sn, 'Ve.%s['IVByElemIndex]";
        let mut nfd = NEONFormatDecoder::new(instr, &[NEONFormatDecoder::scalar_format_map()]);
        let mut long_instr = false;

        match instr.mask(NEONScalarByIndexedElementMask) {
            NEON_SQDMULL_byelement_scalar => {
                mnemonic = "sqdmull";
                long_instr = true;
            }
            NEON_SQDMLAL_byelement_scalar => {
                mnemonic = "sqdmlal";
                long_instr = true;
            }
            NEON_SQDMLSL_byelement_scalar => {
                mnemonic = "sqdmlsl";
                long_instr = true;
            }
            NEON_SQDMULH_byelement_scalar => {
                mnemonic = "sqdmulh";
            }
            NEON_SQRDMULH_byelement_scalar => {
                mnemonic = "sqrdmulh";
            }
            _ => {
                nfd.set_format_map(0, NEONFormatDecoder::fp_scalar_format_map());
                match instr.mask(NEONScalarByIndexedElementFPMask) {
                    NEON_FMUL_byelement_scalar => {
                        mnemonic = "fmul";
                    }
                    NEON_FMLA_byelement_scalar => {
                        mnemonic = "fmla";
                    }
                    NEON_FMLS_byelement_scalar => {
                        mnemonic = "fmls";
                    }
                    NEON_FMULX_byelement_scalar => {
                        mnemonic = "fmulx";
                    }
                    _ => {
                        form = "(NEONScalarByIndexedElement)";
                    }
                }
            }
        }

        if long_instr {
            nfd.set_format_map(0, NEONFormatDecoder::long_scalar_format_map());
        }

        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Placeholder,
                SubstitutionMode::Placeholder,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_scalar_copy(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(NEONScalarCopy)";

        let nfd =
            NEONFormatDecoder::new(instr, &[NEONFormatDecoder::triangular_scalar_format_map()]);

        if instr.mask(NEONScalarCopyMask) == NEON_DUP_ELEMENT_scalar {
            mnemonic = "mov";
            form = "%sd, 'Vn.%s['IVInsIndex1]";
        }

        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Placeholder,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_scalar_pairwise(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "%sd, 'Vn.%s";

        let nfd = NEONFormatDecoder::new(
            instr,
            &[
                NEONFormatDecoder::fp_scalar_format_map(),
                visit_neon_scalar_pairwise_map(),
            ],
        );

        match instr.mask(NEONScalarPairwiseMask) {
            NEON_ADDP_scalar => {
                mnemonic = "addp";
            }
            NEON_FADDP_scalar => {
                mnemonic = "faddp";
            }
            NEON_FMAXP_scalar => {
                mnemonic = "fmaxp";
            }
            NEON_FMAXNMP_scalar => {
                mnemonic = "fmaxnmp";
            }
            NEON_FMINP_scalar => {
                mnemonic = "fminp";
            }
            NEON_FMINNMP_scalar => {
                mnemonic = "fminnmp";
            }
            _ => {
                form = "(NEONScalarPairwise)";
            }
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Placeholder,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_scalar_shift_immediate(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "%sd, %sn, 'Is1";
        let form_2 = "%sd, %sn, 'Is2";

        let mut nfd =
            NEONFormatDecoder::new(instr, &[visit_neon_scalar_shift_immediate_map_shift()]);

        if instr.imm_neon_immh() != 0 {
            // immh has to be non-zero.
            match instr.mask(NEONScalarShiftImmediateMask) {
                NEON_FCVTZU_imm_scalar => {
                    mnemonic = "fcvtzu";
                }
                NEON_FCVTZS_imm_scalar => {
                    mnemonic = "fcvtzs";
                }
                NEON_SCVTF_imm_scalar => {
                    mnemonic = "scvtf";
                }
                NEON_UCVTF_imm_scalar => {
                    mnemonic = "ucvtf";
                }
                NEON_SRI_scalar => {
                    mnemonic = "sri";
                }
                NEON_SSHR_scalar => {
                    mnemonic = "sshr";
                }
                NEON_USHR_scalar => {
                    mnemonic = "ushr";
                }
                NEON_SRSHR_scalar => {
                    mnemonic = "srshr";
                }
                NEON_URSHR_scalar => {
                    mnemonic = "urshr";
                }
                NEON_SSRA_scalar => {
                    mnemonic = "ssra";
                }
                NEON_USRA_scalar => {
                    mnemonic = "usra";
                }
                NEON_SRSRA_scalar => {
                    mnemonic = "srsra";
                }
                NEON_URSRA_scalar => {
                    mnemonic = "ursra";
                }
                NEON_SHL_scalar => {
                    mnemonic = "shl";
                    form = form_2;
                }
                NEON_SLI_scalar => {
                    mnemonic = "sli";
                    form = form_2;
                }
                NEON_SQSHLU_scalar => {
                    mnemonic = "sqshlu";
                    form = form_2;
                }
                NEON_SQSHL_imm_scalar => {
                    mnemonic = "sqshl";
                    form = form_2;
                }
                NEON_UQSHL_imm_scalar => {
                    mnemonic = "uqshl";
                    form = form_2;
                }
                NEON_UQSHRN_scalar => {
                    mnemonic = "uqshrn";
                    nfd.set_format_map(1, visit_neon_scalar_shift_immediate_map_shift_narrow());
                }
                NEON_UQRSHRN_scalar => {
                    mnemonic = "uqrshrn";
                    nfd.set_format_map(1, visit_neon_scalar_shift_immediate_map_shift_narrow());
                }
                NEON_SQSHRN_scalar => {
                    mnemonic = "sqshrn";
                    nfd.set_format_map(1, visit_neon_scalar_shift_immediate_map_shift_narrow());
                }
                NEON_SQRSHRN_scalar => {
                    mnemonic = "sqrshrn";
                    nfd.set_format_map(1, visit_neon_scalar_shift_immediate_map_shift_narrow());
                }
                NEON_SQSHRUN_scalar => {
                    mnemonic = "sqshrun";
                    nfd.set_format_map(1, visit_neon_scalar_shift_immediate_map_shift_narrow());
                }
                NEON_SQRSHRUN_scalar => {
                    mnemonic = "sqrshrun";
                    nfd.set_format_map(1, visit_neon_scalar_shift_immediate_map_shift_narrow());
                }
                _ => {
                    form = "(NEONScalarShiftImmediate)";
                }
            }
        } else {
            form = "(NEONScalarShiftImmediate)";
        }
        self.format(instr, mnemonic, &nfd.substitute_placeholders(form));
    }

    fn visit_neon_shift_immediate(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "'Vd.%s, 'Vn.%s, 'Is1";
        let form_shift_2 = "'Vd.%s, 'Vn.%s, 'Is2";
        let form_xtl = "'Vd.%s, 'Vn.%s";

        // 0001->8H, 001x->4S, 01xx->2D, all others undefined.

        // 00010->8B, 00011->16B, 001x0->4H, 001x1->8H,
        // 01xx0->2S, 01xx1->4S, 1xxx1->2D, all others undefined.

        let mut nfd = NEONFormatDecoder::new(instr, &[visit_neon_shift_immediate_map_shift_tb()]);

        if instr.imm_neon_immh() != 0 {
            // immh has to be non-zero.
            match instr.mask(NEONShiftImmediateMask) {
                NEON_SQSHLU => {
                    mnemonic = "sqshlu";
                    form = form_shift_2;
                }
                NEON_SQSHL_imm => {
                    mnemonic = "sqshl";
                    form = form_shift_2;
                }
                NEON_UQSHL_imm => {
                    mnemonic = "uqshl";
                    form = form_shift_2;
                }
                NEON_SHL => {
                    mnemonic = "shl";
                    form = form_shift_2;
                }
                NEON_SLI => {
                    mnemonic = "sli";
                    form = form_shift_2;
                }
                NEON_SCVTF_imm => {
                    mnemonic = "scvtf";
                }
                NEON_UCVTF_imm => {
                    mnemonic = "ucvtf";
                }
                NEON_FCVTZU_imm => {
                    mnemonic = "fcvtzu";
                }
                NEON_FCVTZS_imm => {
                    mnemonic = "fcvtzs";
                }
                NEON_SRI => {
                    mnemonic = "sri";
                }
                NEON_SSHR => {
                    mnemonic = "sshr";
                }
                NEON_USHR => {
                    mnemonic = "ushr";
                }
                NEON_SRSHR => {
                    mnemonic = "srshr";
                }
                NEON_URSHR => {
                    mnemonic = "urshr";
                }
                NEON_SSRA => {
                    mnemonic = "ssra";
                }
                NEON_USRA => {
                    mnemonic = "usra";
                }
                NEON_SRSRA => {
                    mnemonic = "srsra";
                }
                NEON_URSRA => {
                    mnemonic = "ursra";
                }
                NEON_SHRN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "shrn2"
                    } else {
                        "shrn"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_RSHRN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "rshrn2"
                    } else {
                        "rshrn"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_UQSHRN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "uqshrn2"
                    } else {
                        "uqshrn"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_UQRSHRN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "uqrshrn2"
                    } else {
                        "uqrshrn"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_SQSHRN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "sqshrn2"
                    } else {
                        "sqshrn"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_SQRSHRN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "sqrshrn2"
                    } else {
                        "sqrshrn"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_SQSHRUN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "sqshrun2"
                    } else {
                        "sqshrun"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_SQRSHRUN => {
                    mnemonic = if instr.mask(NEON_Q) != 0 {
                        "sqrshrun2"
                    } else {
                        "sqrshrun"
                    };
                    nfd.set_format_map(1, visit_neon_shift_immediate_map_shift_ta());
                }
                NEON_SSHLL => {
                    nfd.set_format_map(0, visit_neon_shift_immediate_map_shift_ta());
                    if instr.imm_neon_immb() == 0
                        && count_set_bits(instr.imm_neon_immh() as u64, 32) == 1
                    {
                        // sxtl variant.
                        form = form_xtl;
                        mnemonic = if instr.mask(NEON_Q) != 0 {
                            "sxtl2"
                        } else {
                            "sxtl"
                        };
                    } else {
                        // sshll variant.
                        form = form_shift_2;
                        mnemonic = if instr.mask(NEON_Q) != 0 {
                            "sshll2"
                        } else {
                            "sshll"
                        };
                    }
                }
                NEON_USHLL => {
                    nfd.set_format_map(0, visit_neon_shift_immediate_map_shift_ta());
                    if instr.imm_neon_immb() == 0
                        && count_set_bits(instr.imm_neon_immh() as u64, 32) == 1
                    {
                        // uxtl variant.
                        form = form_xtl;
                        mnemonic = if instr.mask(NEON_Q) != 0 {
                            "uxtl2"
                        } else {
                            "uxtl"
                        };
                    } else {
                        // ushll variant.
                        form = form_shift_2;
                        mnemonic = if instr.mask(NEON_Q) != 0 {
                            "ushll2"
                        } else {
                            "ushll"
                        };
                    }
                }
                _ => {
                    form = "(NEONShiftImmediate)";
                }
            }
        } else {
            form = "(NEONShiftImmediate)";
        }
        let form_text = nfd.substitute(
            form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_neon_table(&mut self, instr: Instruction) {
        let mut mnemonic = "unimplemented";
        let mut form = "(NEONTable)";
        let form_1v = "'Vd.%%s, {'Vn.16b}, 'Vm.%%s";
        let form_2v = "'Vd.%%s, {'Vn.16b, v%d.16b}, 'Vm.%%s";
        let form_3v = "'Vd.%%s, {'Vn.16b, v%d.16b, v%d.16b}, 'Vm.%%s";
        let form_4v = "'Vd.%%s, {'Vn.16b, v%d.16b, v%d.16b, v%d.16b}, 'Vm.%%s";

        let nfd = NEONFormatDecoder::new(instr, &[visit_neon_table_map_b()]);

        match instr.mask(NEONTableMask) {
            NEON_TBL_1v => {
                mnemonic = "tbl";
                form = form_1v;
            }
            NEON_TBL_2v => {
                mnemonic = "tbl";
                form = form_2v;
            }
            NEON_TBL_3v => {
                mnemonic = "tbl";
                form = form_3v;
            }
            NEON_TBL_4v => {
                mnemonic = "tbl";
                form = form_4v;
            }
            NEON_TBX_1v => {
                mnemonic = "tbx";
                form = form_1v;
            }
            NEON_TBX_2v => {
                mnemonic = "tbx";
                form = form_2v;
            }
            NEON_TBX_3v => {
                mnemonic = "tbx";
                form = form_3v;
            }
            NEON_TBX_4v => {
                mnemonic = "tbx";
                form = form_4v;
            }
            _ => {}
        }

        let reg_num = instr.rn();
        let mut re_form = String::new();
        let mut rest = form;
        let mut next = 1;
        while let Some(position) = rest.find("%d") {
            re_form.push_str(&rest[..position]);
            re_form.push_str(&format!(
                "{}",
                (reg_num + next) % kNumberOfVRegisters as u32
            ));
            next += 1;
            rest = &rest[position + 2..];
        }
        re_form.push_str(rest);
        let re_form = re_form.replace("%%s", "%s");

        let form_text = nfd.substitute(
            &re_form,
            [
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
                SubstitutionMode::Format,
            ],
        );
        self.format(instr, mnemonic, &form_text);
    }

    fn visit_sve_bit_perm(&mut self, instr: Instruction) {
        let _form = "'Zd.%s, 'Zn.%s, 'Zm.%s";
        let mut mnemonic = "unimplemented";

        debug_assert_eq!(instr.mask(SVEBitPermFMask), SVEBitPermFixed);

        match instr.mask(SVEBitPermMask) {
            BEXT => {
                mnemonic = "bext";
            }
            _ => {}
        }

        self.format(instr, mnemonic, "(SVEBitPerm)");
    }

    fn visit_unimplemented(&mut self, instr: Instruction) {
        self.format(instr, "unimplemented", "(Unimplemented)");
    }

    fn visit_unallocated(&mut self, instr: Instruction) {
        self.format(instr, "unallocated", "(Unallocated)");
    }

    fn visit_cpy(&mut self, instr: Instruction) {
        let mnemonic: &str;

        match instr.mask(CpyMask) {
            CPYP => {
                mnemonic = "cpyp";
            }
            CPYM => {
                mnemonic = "cpym";
            }
            CPYE => {
                mnemonic = "cpye";
            }
            _ => {
                unreachable!();
            }
        }
        let mut form = "['Xd]!, ['Xs]!, 'Xn!";

        let d = instr.rd();
        let n = instr.rn();
        let s = instr.rs();

        // Aliased registers and sp/zr are disallowed.
        if (d == n) || (d == s) || (n == s) || (d == 31) || (n == 31) || (s == 31) {
            form = "";
        }

        // Bits 31 and 30 must be zero.
        if instr.bits(31, 30) != 0 {
            form = "";
        }

        self.format(instr, mnemonic, form);
    }

    fn visit_set(&mut self, instr: Instruction) {
        let mnemonic: &str;

        match instr.mask(SetMask) {
            SETP => {
                mnemonic = "setp";
            }
            SETM => {
                mnemonic = "setm";
            }
            SETE => {
                mnemonic = "sete";
            }
            _ => {
                unreachable!();
            }
        }
        let mut form = "['Xd]!, 'Xn!, 'Xs";

        let d = instr.rd();
        let n = instr.rn();
        let s = instr.rs();

        // Aliased registers are disallowed, only rs can be zr/sp.
        if (d == n) || (d == s) || (n == s) || (d == 31) || (n == 31) {
            form = "";
        }

        // Bits 31 and 30 must be zero.
        if instr.bits(31, 30) != 0 {
            form = "";
        }

        self.format(instr, mnemonic, form);
    }
}

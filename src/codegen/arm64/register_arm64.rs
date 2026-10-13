#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::utils_arm64::count_set_bits;

pub const kCode_no_reg: i32 = -1;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RegisterType {
    Register,
    VRegister,
    NoRegister,
}

#[derive(Copy, Clone, Debug)]
pub struct CPURegister {
    reg_code: i32,
    reg_size: u32,
    reg_type: RegisterType,
    lane_count: i32,
}

impl CPURegister {
    pub const fn no_reg() -> CPURegister {
        CPURegister {
            reg_code: kCode_no_reg,
            reg_size: 0,
            reg_type: RegisterType::NoRegister,
            lane_count: 1,
        }
    }

    pub const fn create(code: i32, size: u32, reg_type: RegisterType) -> CPURegister {
        CPURegister {
            reg_code: code,
            reg_size: size,
            reg_type,
            lane_count: 1,
        }
    }

    #[inline]
    pub fn reg_type(self) -> RegisterType {
        self.reg_type
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.reg_code != kCode_no_reg
    }

    #[inline]
    pub fn code(self) -> i32 {
        debug_assert!(self.reg_code >= 0);
        self.reg_code
    }

    #[inline]
    pub fn size_in_bits(self) -> u32 {
        debug_assert!(self.is_valid());
        self.reg_size
    }

    #[inline]
    pub fn size_in_bytes(self) -> u32 {
        debug_assert!(self.is_valid());
        debug_assert_eq!(self.size_in_bits() % 8, 0);
        self.reg_size / 8
    }

    #[inline]
    pub fn is_32_bits(self) -> bool {
        debug_assert!(self.is_valid());
        self.reg_size == 32
    }

    #[inline]
    pub fn is_64_bits(self) -> bool {
        debug_assert!(self.is_valid());
        self.reg_size == 64
    }

    #[inline]
    pub fn is_none(self) -> bool {
        self.reg_type == RegisterType::NoRegister
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        debug_assert!(self.is_valid());
        self.is_register() && (self.code() == kZeroRegCode)
    }

    #[inline]
    pub fn is_sp(self) -> bool {
        debug_assert!(self.is_valid());
        self.is_register() && (self.code() == kSPRegInternalCode)
    }

    #[inline]
    pub fn is_register(self) -> bool {
        self.reg_type == RegisterType::Register
    }

    #[inline]
    pub fn is_v_register(self) -> bool {
        self.reg_type == RegisterType::VRegister
    }

    #[inline]
    pub fn is_w(self) -> bool {
        self.is_register() && self.is_32_bits()
    }

    #[inline]
    pub fn is_x(self) -> bool {
        self.is_register() && self.is_64_bits()
    }

    #[inline]
    pub fn is_q(self) -> bool {
        self.is_v_register() && self.reg_size == 128
    }

    #[inline]
    pub fn lane_count(self) -> i32 {
        self.lane_count
    }

    #[inline]
    pub fn is_scalar(self) -> bool {
        debug_assert!(self.is_valid());
        self.lane_count == 1
    }

    #[inline]
    pub fn is_vector(self) -> bool {
        debug_assert!(self.is_valid());
        self.lane_count > 1
    }

    #[inline]
    pub fn is_1b(self) -> bool {
        self.is_8_bits() && (self.lane_count == 1)
    }

    #[inline]
    pub fn is_1h(self) -> bool {
        self.is_16_bits() && (self.lane_count == 1)
    }

    #[inline]
    pub fn is_1s(self) -> bool {
        self.is_32_bits() && (self.lane_count == 1)
    }

    #[inline]
    pub fn is_1d(self) -> bool {
        self.is_64_bits() && (self.lane_count == 1)
    }

    #[inline]
    pub fn is_8_bits(self) -> bool {
        debug_assert!(self.is_valid());
        self.reg_size == 8
    }

    #[inline]
    pub fn is_16_bits(self) -> bool {
        debug_assert!(self.is_valid());
        self.reg_size == 16
    }

    #[inline]
    pub fn is_b(self) -> bool {
        self.is_v_register() && self.is_8_bits()
    }

    #[inline]
    pub fn is_h(self) -> bool {
        self.is_v_register() && self.is_16_bits()
    }

    #[inline]
    pub fn is_s(self) -> bool {
        self.is_v_register() && self.is_32_bits()
    }

    #[inline]
    pub fn is_d(self) -> bool {
        self.is_v_register() && self.is_64_bits()
    }

    #[inline]
    pub fn is_same_size_and_type(self, other: CPURegister) -> bool {
        (self.reg_size == other.reg_size) && (self.reg_type == other.reg_type)
    }

    pub fn is_fp_register(self) -> bool {
        self.is_s() || self.is_d()
    }

    pub fn max_code(self) -> i32 {
        if self.is_v_register() {
            return kNumberOfVRegisters as i32 - 1;
        }
        debug_assert!(self.is_register());
        kNumberOfRegisters as i32 - 1
    }

    pub fn is_even(self) -> bool {
        (self.code() % 2) == 0
    }

    #[inline]
    pub fn is_128_bits(self) -> bool {
        debug_assert!(self.is_valid());
        self.reg_size == 128
    }

    #[inline]
    pub fn aliases(self, other: CPURegister) -> bool {
        self.reg_code == other.reg_code && self.reg_type == other.reg_type
    }
}

impl PartialEq for CPURegister {
    fn eq(&self, other: &CPURegister) -> bool {
        self.reg_code == other.reg_code
            && self.reg_size == other.reg_size
            && self.reg_type == other.reg_type
    }
}

impl Eq for CPURegister {}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Register(CPURegister);

impl std::ops::Deref for Register {
    type Target = CPURegister;

    fn deref(&self) -> &CPURegister {
        &self.0
    }
}

impl From<Register> for CPURegister {
    fn from(reg: Register) -> CPURegister {
        reg.0
    }
}

impl Register {
    pub const fn no_reg() -> Register {
        Register(CPURegister::no_reg())
    }

    pub const fn create(code: i32, size: u32) -> Register {
        Register(CPURegister::create(code, size, RegisterType::Register))
    }

    pub fn x_reg_from_code(code: u32) -> Register {
        if code as i32 == kSPRegInternalCode {
            return sp;
        }
        debug_assert!((code as i32) < kNumberOfRegisters);
        Register::create(code as i32, kXRegSizeInBits as u32)
    }

    pub fn w_reg_from_code(code: u32) -> Register {
        if code as i32 == kSPRegInternalCode {
            return wsp;
        }
        debug_assert!((code as i32) < kNumberOfRegisters);
        Register::create(code as i32, kWRegSizeInBits as u32)
    }

    pub fn from_code(code: i32) -> Register {
        Register::create(code, kXRegSizeInBits as u32)
    }
}

pub const NoReg: Register = Register::no_reg();

macro_rules! define_registers {
    ($(($w:ident, $x:ident, $code:expr)),* $(,)?) => {
        $(
            pub const $w: Register = Register::create($code, kWRegSizeInBits as u32);
            pub const $x: Register = Register::create($code, kXRegSizeInBits as u32);
        )*
    };
}

define_registers!(
    (w0, x0, 0),
    (w1, x1, 1),
    (w2, x2, 2),
    (w3, x3, 3),
    (w4, x4, 4),
    (w5, x5, 5),
    (w6, x6, 6),
    (w7, x7, 7),
    (w8, x8, 8),
    (w9, x9, 9),
    (w10, x10, 10),
    (w11, x11, 11),
    (w12, x12, 12),
    (w13, x13, 13),
    (w14, x14, 14),
    (w15, x15, 15),
    (w16, x16, 16),
    (w17, x17, 17),
    (w18, x18, 18),
    (w19, x19, 19),
    (w20, x20, 20),
    (w21, x21, 21),
    (w22, x22, 22),
    (w23, x23, 23),
    (w24, x24, 24),
    (w25, x25, 25),
    (w26, x26, 26),
    (w27, x27, 27),
    (w28, x28, 28),
    (w29, x29, 29),
    (w30, x30, 30),
    (w31, x31, 31),
);

pub const wsp: Register = Register::create(kSPRegInternalCode, kWRegSizeInBits as u32);
pub const sp: Register = Register::create(kSPRegInternalCode, kXRegSizeInBits as u32);

pub const ip0: Register = x16;
pub const ip1: Register = x17;
pub const wip0: Register = w16;
pub const wip1: Register = w17;
pub const kRootRegister: Register = x26;
pub const rr: Register = x26;
pub const cp: Register = x27;
pub const fp: Register = x29;
pub const lr: Register = x30;
pub const xzr: Register = x31;
pub const wzr: Register = w31;
pub const padreg: Register = x31;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct VRegister(CPURegister);

impl std::ops::Deref for VRegister {
    type Target = CPURegister;

    fn deref(&self) -> &CPURegister {
        &self.0
    }
}

impl From<VRegister> for CPURegister {
    fn from(reg: VRegister) -> CPURegister {
        reg.0
    }
}

impl VRegister {
    pub const fn no_reg() -> VRegister {
        VRegister(CPURegister::no_reg())
    }

    pub const fn create(code: i32, size: u32, lane_count: i32) -> VRegister {
        VRegister(CPURegister {
            reg_code: code,
            reg_size: size,
            reg_type: RegisterType::VRegister,
            lane_count,
        })
    }

    pub fn b_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, kBRegSizeInBits as u32, 1)
    }

    pub fn h_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, kHRegSizeInBits as u32, 1)
    }

    pub fn s_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, kSRegSizeInBits as u32, 1)
    }

    pub fn d_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, kDRegSizeInBits as u32, 1)
    }

    pub fn q_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, kQRegSizeInBits as u32, 1)
    }
}

macro_rules! define_v_registers {
    ($(($b:ident, $h:ident, $s:ident, $d:ident, $q:ident, $v:ident, $code:expr)),* $(,)?) => {
        $(
            pub const $b: VRegister = VRegister::create($code, kBRegSizeInBits as u32, 1);
            pub const $h: VRegister = VRegister::create($code, kHRegSizeInBits as u32, 1);
            pub const $s: VRegister = VRegister::create($code, kSRegSizeInBits as u32, 1);
            pub const $d: VRegister = VRegister::create($code, kDRegSizeInBits as u32, 1);
            pub const $q: VRegister = VRegister::create($code, kQRegSizeInBits as u32, 1);
            pub const $v: VRegister = VRegister::create($code, kQRegSizeInBits as u32, 1);
        )*
    };
}

define_v_registers!(
    (b0, h0, s0, d0, q0, v0, 0),
    (b1, h1, s1, d1, q1, v1, 1),
    (b2, h2, s2, d2, q2, v2, 2),
    (b3, h3, s3, d3, q3, v3, 3),
    (b4, h4, s4, d4, q4, v4, 4),
    (b5, h5, s5, d5, q5, v5, 5),
    (b6, h6, s6, d6, q6, v6, 6),
    (b7, h7, s7, d7, q7, v7, 7),
    (b8, h8, s8, d8, q8, v8_, 8),
    (b9, h9, s9, d9, q9, v9, 9),
    (b10, h10, s10, d10, q10, v10, 10),
    (b11, h11, s11, d11, q11, v11, 11),
    (b12, h12, s12, d12, q12, v12, 12),
    (b13, h13, s13, d13, q13, v13, 13),
    (b14, h14, s14, d14, q14, v14, 14),
    (b15, h15, s15, d15, q15, v15, 15),
    (b16, h16, s16, d16, q16, v16, 16),
    (b17, h17, s17, d17, q17, v17, 17),
    (b18, h18, s18, d18, q18, v18, 18),
    (b19, h19, s19, d19, q19, v19, 19),
    (b20, h20, s20, d20, q20, v20, 20),
    (b21, h21, s21, d21, q21, v21, 21),
    (b22, h22, s22, d22, q22, v22, 22),
    (b23, h23, s23, d23, q23, v23, 23),
    (b24, h24, s24, d24, q24, v24, 24),
    (b25, h25, s25, d25, q25, v25, 25),
    (b26, h26, s26, d26, q26, v26, 26),
    (b27, h27, s27, d27, q27, v27, 27),
    (b28, h28, s28, d28, q28, v28, 28),
    (b29, h29, s29, d29, q29, v29, 29),
    (b30, h30, s30, d30, q30, v30, 30),
    (b31, h31, s31, d31, q31, v31, 31),
);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VectorFormat {
    FormatUndefined,
    Format8B,
    Format16B,
    Format4H,
    Format8H,
    Format2S,
    Format4S,
    Format1D,
    Format2D,
    FormatB,
    FormatH,
    FormatS,
    FormatD,
    Format1Q,
}

pub fn vector_format_half_width(vform: VectorFormat) -> VectorFormat {
    match vform {
        VectorFormat::Format8H => VectorFormat::Format8B,
        VectorFormat::Format4S => VectorFormat::Format4H,
        VectorFormat::Format2D => VectorFormat::Format2S,
        VectorFormat::Format1Q => VectorFormat::Format1D,
        VectorFormat::FormatH => VectorFormat::FormatB,
        VectorFormat::FormatS => VectorFormat::FormatH,
        VectorFormat::FormatD => VectorFormat::FormatS,
        _ => unreachable!(),
    }
}

pub fn vector_format_double_width(vform: VectorFormat) -> VectorFormat {
    match vform {
        VectorFormat::Format8B => VectorFormat::Format8H,
        VectorFormat::Format4H => VectorFormat::Format4S,
        VectorFormat::Format2S => VectorFormat::Format2D,
        VectorFormat::FormatB => VectorFormat::FormatH,
        VectorFormat::FormatH => VectorFormat::FormatS,
        VectorFormat::FormatS => VectorFormat::FormatD,
        _ => unreachable!(),
    }
}

pub fn vector_format_fill_q(vform: VectorFormat) -> VectorFormat {
    match vform {
        VectorFormat::FormatB | VectorFormat::Format8B | VectorFormat::Format16B => {
            VectorFormat::Format16B
        }
        VectorFormat::FormatH | VectorFormat::Format4H | VectorFormat::Format8H => {
            VectorFormat::Format8H
        }
        VectorFormat::FormatS | VectorFormat::Format2S | VectorFormat::Format4S => {
            VectorFormat::Format4S
        }
        VectorFormat::FormatD | VectorFormat::Format1D | VectorFormat::Format2D => {
            VectorFormat::Format2D
        }
        _ => unreachable!(),
    }
}

pub fn vector_format_half_width_double_lanes(vform: VectorFormat) -> VectorFormat {
    match vform {
        VectorFormat::Format4H => VectorFormat::Format8B,
        VectorFormat::Format8H => VectorFormat::Format16B,
        VectorFormat::Format2S => VectorFormat::Format4H,
        VectorFormat::Format4S => VectorFormat::Format8H,
        VectorFormat::Format1D => VectorFormat::Format2S,
        VectorFormat::Format2D => VectorFormat::Format4S,
        VectorFormat::Format1Q => VectorFormat::Format2D,
        _ => unreachable!(),
    }
}

pub fn scalar_format_from_lane_size(lane_size: u32) -> VectorFormat {
    match lane_size {
        8 => VectorFormat::FormatB,
        16 => VectorFormat::FormatH,
        32 => VectorFormat::FormatS,
        64 => VectorFormat::FormatD,
        _ => unreachable!(),
    }
}

pub fn scalar_format_from_format(vform: VectorFormat) -> VectorFormat {
    scalar_format_from_lane_size(lane_size_in_bits_from_format(vform))
}

pub fn register_size_in_bits_from_format(vform: VectorFormat) -> u32 {
    debug_assert_ne!(vform, VectorFormat::FormatUndefined);
    match vform {
        VectorFormat::FormatB => kBRegSizeInBits as u32,
        VectorFormat::FormatH => kHRegSizeInBits as u32,
        VectorFormat::FormatS => kSRegSizeInBits as u32,
        VectorFormat::FormatD => kDRegSizeInBits as u32,
        VectorFormat::Format8B
        | VectorFormat::Format4H
        | VectorFormat::Format2S
        | VectorFormat::Format1D => kDRegSizeInBits as u32,
        _ => kQRegSizeInBits as u32,
    }
}

pub fn register_size_in_bytes_from_format(vform: VectorFormat) -> u32 {
    let size_in_bits = register_size_in_bits_from_format(vform);
    debug_assert_eq!(size_in_bits % 8, 0);
    size_in_bits / 8
}

pub fn lane_size_in_bits_from_format(vform: VectorFormat) -> u32 {
    debug_assert_ne!(vform, VectorFormat::FormatUndefined);
    match vform {
        VectorFormat::FormatB | VectorFormat::Format8B | VectorFormat::Format16B => 8,
        VectorFormat::FormatH | VectorFormat::Format4H | VectorFormat::Format8H => 16,
        VectorFormat::FormatS | VectorFormat::Format2S | VectorFormat::Format4S => 32,
        VectorFormat::FormatD | VectorFormat::Format1D | VectorFormat::Format2D => 64,
        VectorFormat::Format1Q => 128,
        _ => unreachable!(),
    }
}

pub fn lane_size_in_bytes_from_format(vform: VectorFormat) -> u32 {
    lane_size_in_bits_from_format(vform) / 8
}

pub fn lane_size_in_bytes_log2_from_format(vform: VectorFormat) -> u32 {
    debug_assert_ne!(vform, VectorFormat::FormatUndefined);
    match vform {
        VectorFormat::FormatB | VectorFormat::Format8B | VectorFormat::Format16B => 0,
        VectorFormat::FormatH | VectorFormat::Format4H | VectorFormat::Format8H => 1,
        VectorFormat::FormatS | VectorFormat::Format2S | VectorFormat::Format4S => 2,
        VectorFormat::FormatD | VectorFormat::Format1D | VectorFormat::Format2D => 3,
        _ => unreachable!(),
    }
}

pub fn lane_count_from_neon_format(format: u32) -> i32 {
    match format & 0x40C00000 {
        0x00000000 => 8,
        0x40000000 => 16,
        0x00400000 => 4,
        0x40400000 => 8,
        0x00800000 => 2,
        0x40800000 => 4,
        0x00C00000 => 1,
        0x40C00000 => 2,
        _ => unreachable!(),
    }
}

pub fn lane_count_from_format(vform: VectorFormat) -> i32 {
    debug_assert_ne!(vform, VectorFormat::FormatUndefined);
    match vform {
        VectorFormat::Format16B => 16,
        VectorFormat::Format8B | VectorFormat::Format8H => 8,
        VectorFormat::Format4H | VectorFormat::Format4S => 4,
        VectorFormat::Format2S | VectorFormat::Format2D => 2,
        VectorFormat::Format1D
        | VectorFormat::Format1Q
        | VectorFormat::FormatB
        | VectorFormat::FormatH
        | VectorFormat::FormatS
        | VectorFormat::FormatD => 1,
        _ => unreachable!(),
    }
}

pub fn max_lane_count_from_format(vform: VectorFormat) -> i32 {
    debug_assert_ne!(vform, VectorFormat::FormatUndefined);
    match vform {
        VectorFormat::FormatB | VectorFormat::Format8B | VectorFormat::Format16B => 16,
        VectorFormat::FormatH | VectorFormat::Format4H | VectorFormat::Format8H => 8,
        VectorFormat::FormatS | VectorFormat::Format2S | VectorFormat::Format4S => 4,
        VectorFormat::FormatD | VectorFormat::Format1D | VectorFormat::Format2D => 2,
        _ => unreachable!(),
    }
}

pub fn is_vector_format(vform: VectorFormat) -> bool {
    debug_assert_ne!(vform, VectorFormat::FormatUndefined);
    !matches!(
        vform,
        VectorFormat::FormatB
            | VectorFormat::FormatH
            | VectorFormat::FormatS
            | VectorFormat::FormatD
    )
}

impl VRegister {
    pub fn v8b(self) -> VRegister {
        VRegister::create(self.code(), kDRegSizeInBits as u32, 8)
    }

    pub fn v16b(self) -> VRegister {
        VRegister::create(self.code(), kQRegSizeInBits as u32, 16)
    }

    pub fn v4h(self) -> VRegister {
        VRegister::create(self.code(), kDRegSizeInBits as u32, 4)
    }

    pub fn v8h(self) -> VRegister {
        VRegister::create(self.code(), kQRegSizeInBits as u32, 8)
    }

    pub fn v2s(self) -> VRegister {
        VRegister::create(self.code(), kDRegSizeInBits as u32, 2)
    }

    pub fn v4s(self) -> VRegister {
        VRegister::create(self.code(), kQRegSizeInBits as u32, 4)
    }

    pub fn v2d(self) -> VRegister {
        VRegister::create(self.code(), kQRegSizeInBits as u32, 2)
    }

    pub fn v1d(self) -> VRegister {
        VRegister::create(self.code(), kDRegSizeInBits as u32, 1)
    }

    pub fn v1q(self) -> VRegister {
        VRegister::create(self.code(), kQRegSizeInBits as u32, 1)
    }

    pub fn format(self, f: VectorFormat) -> VRegister {
        let reg_size = register_size_in_bits_from_format(f);
        let reg_count = if is_vector_format(f) {
            lane_count_from_format(f)
        } else {
            1
        };
        VRegister::create(self.code(), reg_size, reg_count)
    }

    #[inline]
    pub fn is_8b(self) -> bool {
        self.is_64_bits() && (self.lane_count() == 8)
    }

    #[inline]
    pub fn is_16b(self) -> bool {
        self.is_128_bits() && (self.lane_count() == 16)
    }

    #[inline]
    pub fn is_4h(self) -> bool {
        self.is_64_bits() && (self.lane_count() == 4)
    }

    #[inline]
    pub fn is_8h(self) -> bool {
        self.is_128_bits() && (self.lane_count() == 8)
    }

    #[inline]
    pub fn is_2s(self) -> bool {
        self.is_64_bits() && (self.lane_count() == 2)
    }

    #[inline]
    pub fn is_4s(self) -> bool {
        self.is_128_bits() && (self.lane_count() == 4)
    }

    #[inline]
    pub fn is_2d(self) -> bool {
        self.is_128_bits() && (self.lane_count() == 2)
    }

    #[inline]
    pub fn is_1q(self) -> bool {
        self.is_128_bits() && (self.lane_count() == 1)
    }

    #[inline]
    pub fn lane_size_in_bits(self) -> u32 {
        self.size_in_bits() / self.lane_count() as u32
    }

    #[inline]
    pub fn lane_size_in_bytes(self) -> u32 {
        self.lane_size_in_bits() / 8
    }

    #[inline]
    pub fn is_lane_size_b(self) -> bool {
        self.lane_size_in_bits() == kBRegSizeInBits as u32
    }

    #[inline]
    pub fn is_lane_size_h(self) -> bool {
        self.lane_size_in_bits() == kHRegSizeInBits as u32
    }

    #[inline]
    pub fn is_lane_size_s(self) -> bool {
        self.lane_size_in_bits() == kSRegSizeInBits as u32
    }

    #[inline]
    pub fn is_lane_size_d(self) -> bool {
        self.lane_size_in_bits() == kDRegSizeInBits as u32
    }

    #[inline]
    pub fn is_same_format(self, other: VRegister) -> bool {
        (self.size_in_bits() == other.size_in_bits())
            && (self.lane_count() == other.lane_count())
    }
}

pub fn are_aliased(regs: &[CPURegister]) -> bool {
    let mut number_of_valid_regs = 0;
    let mut number_of_valid_fpregs = 0;

    let mut unique_regs: u64 = 0;
    let mut unique_fpregs: u64 = 0;

    for reg in regs {
        if reg.is_register() {
            number_of_valid_regs += 1;
            unique_regs |= 1u64 << reg.code();
        } else if reg.is_v_register() {
            number_of_valid_fpregs += 1;
            unique_fpregs |= 1u64 << reg.code();
        } else {
            debug_assert!(!reg.is_valid());
        }
    }

    let number_of_unique_regs = count_set_bits(unique_regs, 64);
    let number_of_unique_fpregs = count_set_bits(unique_fpregs, 64);

    debug_assert!(number_of_valid_regs >= number_of_unique_regs);
    debug_assert!(number_of_valid_fpregs >= number_of_unique_fpregs);

    (number_of_valid_regs != number_of_unique_regs)
        || (number_of_valid_fpregs != number_of_unique_fpregs)
}

pub fn are_same_size_and_type(reg1: CPURegister, regs: &[CPURegister]) -> bool {
    debug_assert!(reg1.is_valid());
    regs.iter()
        .all(|reg| reg.is_valid() && reg.is_same_size_and_type(reg1))
}

pub fn are_consecutive(reg1: CPURegister, regs: &[CPURegister]) -> bool {
    debug_assert!(reg1.is_valid());
    let mut previous = reg1;
    for reg in regs {
        debug_assert!(reg.is_valid());
        if reg.code() != ((previous.code() + 1) % (reg1.max_code() + 1)) {
            return false;
        }
        previous = *reg;
    }
    true
}

pub fn are_even(regs: &[CPURegister]) -> bool {
    regs.iter().all(|reg| reg.is_valid() && reg.is_even())
}

pub fn vector_format_double_lanes(vform: VectorFormat) -> VectorFormat {
    debug_assert!(
        vform == VectorFormat::Format8B
            || vform == VectorFormat::Format4H
            || vform == VectorFormat::Format2S
    );
    match vform {
        VectorFormat::Format8B => VectorFormat::Format16B,
        VectorFormat::Format4H => VectorFormat::Format8H,
        VectorFormat::Format2S => VectorFormat::Format4S,
        _ => unreachable!(),
    }
}

pub fn vector_format_half_lanes(vform: VectorFormat) -> VectorFormat {
    debug_assert!(
        vform == VectorFormat::Format16B
            || vform == VectorFormat::Format8H
            || vform == VectorFormat::Format4S
            || vform == VectorFormat::Format2D
    );
    match vform {
        VectorFormat::Format16B => VectorFormat::Format8B,
        VectorFormat::Format8H => VectorFormat::Format4H,
        VectorFormat::Format4S => VectorFormat::Format2S,
        VectorFormat::Format2D => VectorFormat::Format1D,
        _ => unreachable!(),
    }
}

pub fn vector_format_fill_half_q(vform: VectorFormat) -> VectorFormat {
    vector_format_half_lanes(vector_format_fill_q(vform))
}

pub fn vector_format_fill_half_q_from_lane_size(lane_size: u32) -> VectorFormat {
    vector_format_fill_half_q(scalar_format_from_lane_size(lane_size))
}

pub fn max_int_from_format(vform: VectorFormat) -> i64 {
    (i64::MAX as u64 >> (64 - lane_size_in_bits_from_format(vform))) as i64
}

pub fn min_int_from_format(vform: VectorFormat) -> i64 {
    i64::MIN >> (64 - lane_size_in_bits_from_format(vform))
}

pub fn max_uint_from_format(vform: VectorFormat) -> u64 {
    u64::MAX >> (64 - lane_size_in_bits_from_format(vform))
}

pub fn are_same_format(reg1: VRegister, regs: &[VRegister]) -> bool {
    regs.iter().all(|reg| reg.is_valid() && reg1.is_same_format(*reg))
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct VOperand {
    reg: VRegister,
    index: i32,
}

pub const NO_LANE: i32 = -1;

impl VOperand {
    #[inline]
    pub fn reg(&self) -> VRegister {
        self.reg
    }

    #[inline]
    pub fn index(&self) -> i32 {
        debug_assert!(self.has_lane());
        self.index
    }

    #[inline]
    pub fn has_lane(&self) -> bool {
        self.index != NO_LANE
    }
}

impl From<VRegister> for VOperand {
    fn from(reg: VRegister) -> VOperand {
        VOperand {
            reg,
            index: NO_LANE,
        }
    }
}

impl VRegister {
    pub fn lane(self, index: i32) -> VOperand {
        debug_assert!(index >= 0);
        VOperand { reg: self, index }
    }
}

pub const fp_zero: VRegister = d15;
pub const fp_fixed1: VRegister = d27;
pub const fp_fixed2: VRegister = d28;
pub const fp_scratch: VRegister = d30;
pub const fp_scratch1: VRegister = d30;
pub const fp_scratch2: VRegister = d31;

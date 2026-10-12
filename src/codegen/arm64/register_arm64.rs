use crate::codegen::arm64::constants_arm64::*;

pub const K_CODE_NO_REG: i32 = -1;

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
            reg_code: K_CODE_NO_REG,
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
        self.reg_code != K_CODE_NO_REG
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
        self.is_register() && (self.code() == K_ZERO_REG_CODE)
    }

    #[inline]
    pub fn is_sp(self) -> bool {
        debug_assert!(self.is_valid());
        self.is_register() && (self.code() == K_SP_REG_INTERNAL_CODE)
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
        if code as i32 == K_SP_REG_INTERNAL_CODE {
            return SP;
        }
        debug_assert!((code as i32) < K_NUMBER_OF_REGISTERS);
        Register::create(code as i32, K_X_REG_SIZE_IN_BITS as u32)
    }

    pub fn w_reg_from_code(code: u32) -> Register {
        if code as i32 == K_SP_REG_INTERNAL_CODE {
            return WSP;
        }
        debug_assert!((code as i32) < K_NUMBER_OF_REGISTERS);
        Register::create(code as i32, K_W_REG_SIZE_IN_BITS as u32)
    }

    pub fn from_code(code: i32) -> Register {
        Register::create(code, K_X_REG_SIZE_IN_BITS as u32)
    }
}

pub const NO_REG: Register = Register::no_reg();

macro_rules! define_registers {
    ($(($w:ident, $x:ident, $code:expr)),* $(,)?) => {
        $(
            pub const $w: Register = Register::create($code, K_W_REG_SIZE_IN_BITS as u32);
            pub const $x: Register = Register::create($code, K_X_REG_SIZE_IN_BITS as u32);
        )*
    };
}

define_registers!(
    (W0, X0, 0),
    (W1, X1, 1),
    (W2, X2, 2),
    (W3, X3, 3),
    (W4, X4, 4),
    (W5, X5, 5),
    (W6, X6, 6),
    (W7, X7, 7),
    (W8, X8, 8),
    (W9, X9, 9),
    (W10, X10, 10),
    (W11, X11, 11),
    (W12, X12, 12),
    (W13, X13, 13),
    (W14, X14, 14),
    (W15, X15, 15),
    (W16, X16, 16),
    (W17, X17, 17),
    (W18, X18, 18),
    (W19, X19, 19),
    (W20, X20, 20),
    (W21, X21, 21),
    (W22, X22, 22),
    (W23, X23, 23),
    (W24, X24, 24),
    (W25, X25, 25),
    (W26, X26, 26),
    (W27, X27, 27),
    (W28, X28, 28),
    (W29, X29, 29),
    (W30, X30, 30),
    (W31, X31, 31),
);

pub const WSP: Register = Register::create(K_SP_REG_INTERNAL_CODE, K_W_REG_SIZE_IN_BITS as u32);
pub const SP: Register = Register::create(K_SP_REG_INTERNAL_CODE, K_X_REG_SIZE_IN_BITS as u32);

pub const IP0: Register = X16;
pub const IP1: Register = X17;
pub const WIP0: Register = W16;
pub const WIP1: Register = W17;
pub const K_ROOT_REGISTER: Register = X26;
pub const RR: Register = X26;
pub const CP: Register = X27;
pub const FP: Register = X29;
pub const LR: Register = X30;
pub const XZR: Register = X31;
pub const WZR: Register = W31;
pub const PADREG: Register = X31;

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
        VRegister::create(code as i32, K_B_REG_SIZE_IN_BITS as u32, 1)
    }

    pub fn h_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, K_H_REG_SIZE_IN_BITS as u32, 1)
    }

    pub fn s_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, K_S_REG_SIZE_IN_BITS as u32, 1)
    }

    pub fn d_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, K_D_REG_SIZE_IN_BITS as u32, 1)
    }

    pub fn q_reg_from_code(code: u32) -> VRegister {
        VRegister::create(code as i32, K_Q_REG_SIZE_IN_BITS as u32, 1)
    }
}

macro_rules! define_v_registers {
    ($(($b:ident, $h:ident, $s:ident, $d:ident, $q:ident, $v:ident, $code:expr)),* $(,)?) => {
        $(
            pub const $b: VRegister = VRegister::create($code, K_B_REG_SIZE_IN_BITS as u32, 1);
            pub const $h: VRegister = VRegister::create($code, K_H_REG_SIZE_IN_BITS as u32, 1);
            pub const $s: VRegister = VRegister::create($code, K_S_REG_SIZE_IN_BITS as u32, 1);
            pub const $d: VRegister = VRegister::create($code, K_D_REG_SIZE_IN_BITS as u32, 1);
            pub const $q: VRegister = VRegister::create($code, K_Q_REG_SIZE_IN_BITS as u32, 1);
            pub const $v: VRegister = VRegister::create($code, K_Q_REG_SIZE_IN_BITS as u32, 1);
        )*
    };
}

define_v_registers!(
    (B0, H0, S0, D0, Q0, V0, 0),
    (B1, H1, S1, D1, Q1, V1, 1),
    (B2, H2, S2, D2, Q2, V2, 2),
    (B3, H3, S3, D3, Q3, V3, 3),
    (B4, H4, S4, D4, Q4, V4, 4),
    (B5, H5, S5, D5, Q5, V5, 5),
    (B6, H6, S6, D6, Q6, V6, 6),
    (B7, H7, S7, D7, Q7, V7, 7),
    (B8, H8, S8, D8, Q8, V8_, 8),
    (B9, H9, S9, D9, Q9, V9, 9),
    (B10, H10, S10, D10, Q10, V10, 10),
    (B11, H11, S11, D11, Q11, V11, 11),
    (B12, H12, S12, D12, Q12, V12, 12),
    (B13, H13, S13, D13, Q13, V13, 13),
    (B14, H14, S14, D14, Q14, V14, 14),
    (B15, H15, S15, D15, Q15, V15, 15),
    (B16, H16, S16, D16, Q16, V16, 16),
    (B17, H17, S17, D17, Q17, V17, 17),
    (B18, H18, S18, D18, Q18, V18, 18),
    (B19, H19, S19, D19, Q19, V19, 19),
    (B20, H20, S20, D20, Q20, V20, 20),
    (B21, H21, S21, D21, Q21, V21, 21),
    (B22, H22, S22, D22, Q22, V22, 22),
    (B23, H23, S23, D23, Q23, V23, 23),
    (B24, H24, S24, D24, Q24, V24, 24),
    (B25, H25, S25, D25, Q25, V25, 25),
    (B26, H26, S26, D26, Q26, V26, 26),
    (B27, H27, S27, D27, Q27, V27, 27),
    (B28, H28, S28, D28, Q28, V28, 28),
    (B29, H29, S29, D29, Q29, V29, 29),
    (B30, H30, S30, D30, Q30, V30, 30),
    (B31, H31, S31, D31, Q31, V31, 31),
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
        VectorFormat::FormatB => K_B_REG_SIZE_IN_BITS as u32,
        VectorFormat::FormatH => K_H_REG_SIZE_IN_BITS as u32,
        VectorFormat::FormatS => K_S_REG_SIZE_IN_BITS as u32,
        VectorFormat::FormatD => K_D_REG_SIZE_IN_BITS as u32,
        VectorFormat::Format8B
        | VectorFormat::Format4H
        | VectorFormat::Format2S
        | VectorFormat::Format1D => K_D_REG_SIZE_IN_BITS as u32,
        _ => K_Q_REG_SIZE_IN_BITS as u32,
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
        VRegister::create(self.code(), K_D_REG_SIZE_IN_BITS as u32, 8)
    }

    pub fn v16b(self) -> VRegister {
        VRegister::create(self.code(), K_Q_REG_SIZE_IN_BITS as u32, 16)
    }

    pub fn v4h(self) -> VRegister {
        VRegister::create(self.code(), K_D_REG_SIZE_IN_BITS as u32, 4)
    }

    pub fn v8h(self) -> VRegister {
        VRegister::create(self.code(), K_Q_REG_SIZE_IN_BITS as u32, 8)
    }

    pub fn v2s(self) -> VRegister {
        VRegister::create(self.code(), K_D_REG_SIZE_IN_BITS as u32, 2)
    }

    pub fn v4s(self) -> VRegister {
        VRegister::create(self.code(), K_Q_REG_SIZE_IN_BITS as u32, 4)
    }

    pub fn v2d(self) -> VRegister {
        VRegister::create(self.code(), K_Q_REG_SIZE_IN_BITS as u32, 2)
    }

    pub fn v1d(self) -> VRegister {
        VRegister::create(self.code(), K_D_REG_SIZE_IN_BITS as u32, 1)
    }

    pub fn v1q(self) -> VRegister {
        VRegister::create(self.code(), K_Q_REG_SIZE_IN_BITS as u32, 1)
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
    pub fn is_lane_size_b(self) -> bool {
        self.lane_size_in_bits() == K_B_REG_SIZE_IN_BITS as u32
    }

    #[inline]
    pub fn is_lane_size_h(self) -> bool {
        self.lane_size_in_bits() == K_H_REG_SIZE_IN_BITS as u32
    }

    #[inline]
    pub fn is_lane_size_s(self) -> bool {
        self.lane_size_in_bits() == K_S_REG_SIZE_IN_BITS as u32
    }

    #[inline]
    pub fn is_lane_size_d(self) -> bool {
        self.lane_size_in_bits() == K_D_REG_SIZE_IN_BITS as u32
    }

    #[inline]
    pub fn is_same_format(self, other: VRegister) -> bool {
        (self.size_in_bits() == other.size_in_bits())
            && (self.lane_count() == other.lane_count())
    }
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

pub const FP_ZERO: VRegister = D15;
pub const FP_FIXED1: VRegister = D27;
pub const FP_FIXED2: VRegister = D28;
pub const FP_SCRATCH: VRegister = D30;
pub const FP_SCRATCH1: VRegister = D30;
pub const FP_SCRATCH2: VRegister = D31;

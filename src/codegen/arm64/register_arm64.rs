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
}

impl CPURegister {
    pub const fn no_reg() -> CPURegister {
        CPURegister {
            reg_code: K_CODE_NO_REG,
            reg_size: 0,
            reg_type: RegisterType::NoRegister,
        }
    }

    pub const fn create(code: i32, size: u32, reg_type: RegisterType) -> CPURegister {
        CPURegister {
            reg_code: code,
            reg_size: size,
            reg_type,
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

pub type Register = CPURegister;

impl CPURegister {
    pub const fn create_register(code: i32, size: u32) -> Register {
        CPURegister::create(code, size, RegisterType::Register)
    }

    pub fn x_reg_from_code(code: u32) -> Register {
        if code as i32 == K_SP_REG_INTERNAL_CODE {
            return SP;
        }
        debug_assert!((code as i32) < K_NUMBER_OF_REGISTERS);
        CPURegister::create_register(code as i32, K_X_REG_SIZE_IN_BITS as u32)
    }

    pub fn w_reg_from_code(code: u32) -> Register {
        if code as i32 == K_SP_REG_INTERNAL_CODE {
            return WSP;
        }
        debug_assert!((code as i32) < K_NUMBER_OF_REGISTERS);
        CPURegister::create_register(code as i32, K_W_REG_SIZE_IN_BITS as u32)
    }

    pub fn from_code(code: i32) -> Register {
        CPURegister::create_register(code, K_X_REG_SIZE_IN_BITS as u32)
    }
}

pub const NO_REG: Register = CPURegister::no_reg();

macro_rules! define_registers {
    ($(($w:ident, $x:ident, $code:expr)),* $(,)?) => {
        $(
            pub const $w: Register =
                CPURegister::create_register($code, K_W_REG_SIZE_IN_BITS as u32);
            pub const $x: Register =
                CPURegister::create_register($code, K_X_REG_SIZE_IN_BITS as u32);
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

pub const WSP: Register =
    CPURegister::create_register(K_SP_REG_INTERNAL_CODE, K_W_REG_SIZE_IN_BITS as u32);
pub const SP: Register =
    CPURegister::create_register(K_SP_REG_INTERNAL_CODE, K_X_REG_SIZE_IN_BITS as u32);

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

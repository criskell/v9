#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::register_arm64::{padreg, CPURegister, RegisterType};
use crate::codegen::arm64::utils_arm64::count_set_bits;

pub const kRegListSizeInBits: i32 = 64;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CPURegList {
    list: u64,
    size: u32,
    reg_type: RegisterType,
}

impl CPURegList {
    pub fn new(regs: &[CPURegister]) -> CPURegList {
        debug_assert!(!regs.is_empty());
        let mut list = 0u64;
        for reg in regs {
            if reg.is_valid() {
                list |= 1u64 << reg.code();
            }
        }
        let result = CPURegList {
            list,
            size: regs[0].size_in_bits(),
            reg_type: regs[0].reg_type(),
        };
        debug_assert!(result.is_valid());
        result
    }

    pub fn with_range(
        reg_type: RegisterType,
        size: u32,
        first_reg: i32,
        last_reg: i32,
    ) -> CPURegList {
        debug_assert!(
            ((reg_type == RegisterType::Register) && (last_reg < kNumberOfRegisters))
                || ((reg_type == RegisterType::VRegister)
                    && (last_reg < kNumberOfVRegisters))
        );
        debug_assert!(last_reg >= first_reg);
        let mut list = (1u64 << (last_reg + 1)) - 1;
        list &= !((1u64 << first_reg) - 1);
        let result = CPURegList {
            list,
            size,
            reg_type,
        };
        debug_assert!(result.is_valid());
        result
    }

    pub fn reg_type(&self) -> RegisterType {
        self.reg_type
    }

    pub fn bits(&self) -> u64 {
        self.list
    }

    #[inline]
    pub fn set_bits(&mut self, new_bits: u64) {
        self.list = new_bits;
        debug_assert!(self.is_valid());
    }

    pub fn combine(&mut self, other: CPURegList) {
        debug_assert_eq!(self.reg_type, other.reg_type);
        debug_assert_eq!(self.size, other.size);
        self.list |= other.bits();
    }

    pub fn remove(&mut self, other: CPURegList) {
        debug_assert_eq!(self.reg_type, other.reg_type);
        self.list &= !other.bits();
    }

    pub fn combine_register(&mut self, other: CPURegister) {
        debug_assert_eq!(self.reg_type, other.reg_type());
        debug_assert_eq!(self.size, other.size_in_bits());
        self.combine_code(other.code());
    }

    pub fn remove_register(&mut self, other: CPURegister) {
        if other.is_valid() && other.reg_type() == self.reg_type {
            self.remove_code(other.code());
        }
    }

    pub fn combine_code(&mut self, code: i32) {
        debug_assert!(self.is_valid());
        self.list |= 1u64 << code;
    }

    pub fn remove_code(&mut self, code: i32) {
        debug_assert!(self.is_valid());
        self.list &= !(1u64 << code);
    }

    pub fn pop_lowest_index(&mut self) -> CPURegister {
        if self.is_empty() {
            return CPURegister::no_reg();
        }
        let index = self.list.trailing_zeros() as i32;
        debug_assert!((self.list & (1u64 << index)) != 0);
        self.remove_code(index);
        CPURegister::create(index, self.size, self.reg_type)
    }

    pub fn pop_highest_index(&mut self) -> CPURegister {
        if self.is_empty() {
            return CPURegister::no_reg();
        }
        let index = 63 - self.list.leading_zeros() as i32;
        debug_assert!((self.list & (1u64 << index)) != 0);
        self.remove_code(index);
        CPURegister::create(index, self.size, self.reg_type)
    }

    pub fn align(&mut self) {
        if (self.count() % 2) != 0 {
            if self.includes_alias_of(*padreg) {
                self.remove_register(*padreg);
            } else {
                self.combine_register(*padreg);
            }
        }
        debug_assert_eq!(self.count() % 2, 0);
    }

    pub fn is_empty(&self) -> bool {
        self.list == 0
    }

    pub fn includes_alias_of(&self, other: CPURegister) -> bool {
        if other.is_none() || (other.reg_type() != self.reg_type) {
            return false;
        }
        (self.list & (1u64 << other.code())) != 0
    }

    pub fn count(&self) -> i32 {
        count_set_bits(self.list, kRegListSizeInBits)
    }

    pub fn register_size_in_bits(&self) -> u32 {
        self.size
    }

    pub fn register_size_in_bytes(&self) -> u32 {
        let size_in_bits = self.register_size_in_bits();
        debug_assert_eq!(size_in_bits % 8, 0);
        size_in_bits / 8
    }

    pub fn total_size_in_bytes(&self) -> u32 {
        self.register_size_in_bytes() * self.count() as u32
    }

    fn is_valid(&self) -> bool {
        const K_VALID_REGISTERS: u64 = 0x8000000ffffffff;
        const K_VALID_V_REGISTERS: u64 = 0x0000000ffffffff;
        match self.reg_type {
            RegisterType::Register => (self.list & K_VALID_REGISTERS) == self.list,
            RegisterType::VRegister => (self.list & K_VALID_V_REGISTERS) == self.list,
            RegisterType::NoRegister => self.list == 0,
        }
    }

    pub fn get_callee_saved(size: u32) -> CPURegList {
        CPURegList::with_range(RegisterType::Register, size, 19, 28)
    }

    pub fn get_callee_saved_d(size: u32) -> CPURegList {
        CPURegList::with_range(RegisterType::VRegister, size, 8, 15)
    }

    pub fn get_caller_saved(size: u32) -> CPURegList {
        let mut list = CPURegList::with_range(RegisterType::Register, size, 0, 17);
        list.combine_code(18);
        list
    }

    pub fn get_caller_saved_v(size: u32) -> CPURegList {
        let mut list = CPURegList::with_range(RegisterType::VRegister, size, 0, 7);
        list.combine(CPURegList::with_range(RegisterType::VRegister, size, 16, 31));
        list
    }
}


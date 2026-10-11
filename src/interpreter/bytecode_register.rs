//! Ported from `src/interpreter/bytecode-register.h`.
//!
//! A `Register` names a slot in the interpreter frame. The encoding is the
//! interesting part, and it is V8's:
//!
//! ```text
//!   index >= 0    a local, in the register file
//!   index <  0    a parameter, or one of the fixed frame slots
//! ```
//!
//! Parameters are *negative* because the interpreter frame has them on the
//! far side of the frame pointer -- the caller pushed them before the call,
//! so they sit above fp, while the register file grows below it. One signed
//! index addresses both, which is why `Reg` is a signed operand.
//!
//! Deviation: V8 derives the fixed slot indices from
//! `InterpreterFrameConstants`, because a `Register` really is an offset into
//! a machine stack frame that generated code also reads. `vx` interprets in
//! a `Vec<Tagged>` and has no machine frame, so it defines the layout here
//! instead -- keeping V8's *ordering*, which is what the encoding depends on.

use crate::interpreter::bytecode_operands::{OperandScale, OperandSize};

/// The frame layout, in register indices. Mirrors
/// `InterpreterFrameConstants`: the register file starts at index 0 and the
/// fixed slots and parameters run downwards from there.
///
/// ```text
///   index  0, 1, 2 ...   the register file (locals, temporaries)
///   index -1            feedback vector
///   index -2            bytecode array
///   index -3            current context
///   index -4            the closure being executed
///   index -5            argument count
///   index -6, -7 ...    parameters: receiver, then the declared ones
/// ```
const K_REGISTER_FILE_START_INDEX: i32 = 0;
const K_FEEDBACK_VECTOR_REGISTER_INDEX: i32 = -1;
const K_BYTECODE_ARRAY_REGISTER_INDEX: i32 = -2;
const K_CURRENT_CONTEXT_REGISTER_INDEX: i32 = -3;
const K_FUNCTION_CLOSURE_REGISTER_INDEX: i32 = -4;
const K_ARGUMENT_COUNT_REGISTER_INDEX: i32 = -5;
const K_FIRST_PARAM_REGISTER_INDEX: i32 = -6;

/// `Register::kInvalidIndex`
const K_INVALID_INDEX: i32 = i32::MAX;

/// `Register`
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Register(i32);

impl Register {
    #[inline]
    pub const fn new(index: i32) -> Register {
        Register(index)
    }

    #[inline]
    pub const fn index(self) -> i32 {
        self.0
    }

    /// `Register::is_parameter`
    #[inline]
    pub const fn is_parameter(self) -> bool {
        self.0 < 0
    }

    /// `Register::is_valid`
    #[inline]
    pub const fn is_valid(self) -> bool {
        self.0 != K_INVALID_INDEX
    }

    /// `Register::invalid_value`
    #[inline]
    pub const fn invalid_value() -> Register {
        Register(K_INVALID_INDEX)
    }

    /// `Register::FromParameterIndex`. Parameter 0 is the receiver (`this`),
    /// so the declared parameters start at 1.
    #[inline]
    pub const fn from_parameter_index(index: i32) -> Register {
        debug_assert!(index >= 0);
        Register(K_FIRST_PARAM_REGISTER_INDEX - index)
    }

    /// `Register::ToParameterIndex`
    #[inline]
    pub const fn to_parameter_index(self) -> i32 {
        debug_assert!(self.is_parameter());
        K_FIRST_PARAM_REGISTER_INDEX - self.0
    }

    /// `Register::receiver`: parameter 0, the `this` value.
    #[inline]
    pub const fn receiver() -> Register {
        Register::from_parameter_index(0)
    }

    #[inline]
    pub const fn is_receiver(self) -> bool {
        self.is_parameter() && self.to_parameter_index() == 0
    }

    /// `Register::function_closure`
    #[inline]
    pub const fn function_closure() -> Register {
        Register(K_FUNCTION_CLOSURE_REGISTER_INDEX)
    }

    /// `Register::current_context`
    #[inline]
    pub const fn current_context() -> Register {
        Register(K_CURRENT_CONTEXT_REGISTER_INDEX)
    }

    /// `Register::bytecode_array`
    #[inline]
    pub const fn bytecode_array() -> Register {
        Register(K_BYTECODE_ARRAY_REGISTER_INDEX)
    }

    /// `Register::feedback_vector`
    #[inline]
    pub const fn feedback_vector() -> Register {
        Register(K_FEEDBACK_VECTOR_REGISTER_INDEX)
    }

    /// `Register::argument_count`
    #[inline]
    pub const fn argument_count() -> Register {
        Register(K_ARGUMENT_COUNT_REGISTER_INDEX)
    }

    /// `Register::ToOperand`: the value actually encoded in the bytecode.
    ///
    /// Inverting the index is what makes the encoding compact: it maps the
    /// frame's slots onto small signed numbers around zero, so the common
    /// registers fit in one byte.
    #[inline]
    pub const fn to_operand(self) -> i32 {
        K_REGISTER_FILE_START_INDEX - self.0
    }

    /// `Register::FromOperand`
    #[inline]
    pub const fn from_operand(operand: i32) -> Register {
        Register(K_REGISTER_FILE_START_INDEX - operand)
    }

    /// `Register::SizeOfOperand`: the narrowest operand size this register
    /// can be encoded in, which is what decides whether the bytecode needs a
    /// `Wide` prefix.
    pub fn size_of_operand(self) -> OperandSize {
        let operand = self.to_operand();
        if operand >= i8::MIN as i32 && operand <= i8::MAX as i32 {
            OperandSize::Byte
        } else if operand >= i16::MIN as i32 && operand <= i16::MAX as i32 {
            OperandSize::Short
        } else {
            OperandSize::Quad
        }
    }

    /// The scale a bytecode needs in order to encode this register.
    pub fn operand_scale(self) -> OperandScale {
        match self.size_of_operand() {
            OperandSize::Byte | OperandSize::None => OperandScale::Single,
            OperandSize::Short => OperandScale::Double,
            OperandSize::Quad => OperandScale::Quadruple,
        }
    }
}

impl std::fmt::Debug for Register {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.is_valid() {
            return write!(f, "<invalid>");
        }
        match self.0 {
            K_FEEDBACK_VECTOR_REGISTER_INDEX => write!(f, "<feedback_vector>"),
            K_BYTECODE_ARRAY_REGISTER_INDEX => write!(f, "<bytecode_array>"),
            K_CURRENT_CONTEXT_REGISTER_INDEX => write!(f, "<context>"),
            K_FUNCTION_CLOSURE_REGISTER_INDEX => write!(f, "<closure>"),
            K_ARGUMENT_COUNT_REGISTER_INDEX => write!(f, "<argc>"),
            index if index < 0 => {
                let parameter = self.to_parameter_index();
                if parameter == 0 {
                    write!(f, "<this>")
                } else {
                    write!(f, "a{}", parameter - 1)
                }
            }
            index => write!(f, "r{index}"),
        }
    }
}

impl std::fmt::Display for Register {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// `RegisterList`: a run of consecutive registers.
///
/// Consecutive is the point. A call passes its arguments as one `RegList`
/// operand plus a count, so the interpreter can take a slice instead of
/// decoding one operand per argument -- which is also why the bytecode
/// generator has to allocate argument registers contiguously.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RegisterList {
    first_reg_index: i32,
    register_count: i32,
}

impl RegisterList {
    pub const fn empty() -> RegisterList {
        RegisterList {
            first_reg_index: K_INVALID_INDEX,
            register_count: 0,
        }
    }

    pub const fn new(first: Register, count: i32) -> RegisterList {
        RegisterList {
            first_reg_index: first.index(),
            register_count: count,
        }
    }

    /// A list holding exactly one register.
    pub const fn one(register: Register) -> RegisterList {
        RegisterList::new(register, 1)
    }

    #[inline]
    pub const fn register_count(self) -> i32 {
        self.register_count
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.register_count == 0
    }

    /// `RegisterList::first_register`
    #[inline]
    pub const fn first_register(self) -> Register {
        if self.register_count == 0 {
            Register::invalid_value()
        } else {
            Register::new(self.first_reg_index)
        }
    }

    /// `RegisterList::operator[]`
    #[inline]
    pub fn get(self, index: i32) -> Register {
        debug_assert!(index >= 0 && index < self.register_count);
        Register::new(self.first_reg_index + index)
    }

    /// `RegisterList::Truncate`
    pub fn truncate(self, new_count: i32) -> RegisterList {
        debug_assert!(new_count >= 0 && new_count <= self.register_count);
        RegisterList {
            first_reg_index: self.first_reg_index,
            register_count: new_count,
        }
    }

    /// `RegisterList::PopLeft`: drop the first register. A call uses this to
    /// separate the receiver from the arguments.
    pub fn pop_left(self) -> RegisterList {
        debug_assert!(self.register_count > 0);
        RegisterList {
            first_reg_index: self.first_reg_index + 1,
            register_count: self.register_count - 1,
        }
    }

    pub fn iter(self) -> impl Iterator<Item = Register> {
        (0..self.register_count).map(move |i| Register::new(self.first_reg_index + i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locals_are_non_negative_and_parameters_are_negative() {
        let local = Register::new(0);
        assert!(!local.is_parameter());
        assert_eq!(local.index(), 0);

        let receiver = Register::receiver();
        assert!(receiver.is_parameter());
        assert!(receiver.is_receiver());
        assert_eq!(receiver.to_parameter_index(), 0);

        let first_argument = Register::from_parameter_index(1);
        assert!(first_argument.is_parameter());
        assert!(!first_argument.is_receiver());
        assert_eq!(first_argument.to_parameter_index(), 1);
        assert!(
            first_argument.index() < receiver.index(),
            "parameters run downwards"
        );
    }

    #[test]
    fn parameter_indices_round_trip() {
        for index in 0..32 {
            let register = Register::from_parameter_index(index);
            assert_eq!(register.to_parameter_index(), index);
            assert!(register.is_parameter());
        }
    }

    #[test]
    fn the_operand_encoding_round_trips() {
        let registers = [
            Register::new(0),
            Register::new(1),
            Register::new(200),
            Register::receiver(),
            Register::from_parameter_index(3),
            Register::current_context(),
            Register::function_closure(),
            Register::feedback_vector(),
            Register::argument_count(),
        ];
        for register in registers {
            assert_eq!(
                Register::from_operand(register.to_operand()),
                register,
                "{register:?} did not round-trip through its operand"
            );
        }
    }

    #[test]
    fn the_fixed_frame_slots_are_distinct_and_below_the_register_file() {
        let slots = [
            Register::feedback_vector(),
            Register::bytecode_array(),
            Register::current_context(),
            Register::function_closure(),
            Register::argument_count(),
        ];
        for (i, a) in slots.iter().enumerate() {
            assert!(a.is_parameter(), "{a:?} should be below the register file");
            for b in slots.iter().skip(i + 1) {
                assert_ne!(a, b, "two frame slots collide");
            }
        }
        // ...and none of them collides with a parameter.
        for index in 0..64 {
            let parameter = Register::from_parameter_index(index);
            for slot in slots {
                assert_ne!(parameter, slot, "parameter {index} collides with {slot:?}");
            }
        }
    }

    #[test]
    fn a_register_needs_a_wider_operand_only_when_it_does_not_fit() {
        // The operand is the negated index, so the byte boundary falls where
        // `-index` leaves `i8` -- at 128, not 127.
        assert_eq!(Register::new(0).size_of_operand(), OperandSize::Byte);
        assert_eq!(Register::new(128).size_of_operand(), OperandSize::Byte);
        assert_eq!(Register::new(129).size_of_operand(), OperandSize::Short);
        assert_eq!(Register::new(32_768).size_of_operand(), OperandSize::Short);
        assert_eq!(Register::new(32_769).size_of_operand(), OperandSize::Quad);

        assert_eq!(Register::new(0).operand_scale(), OperandScale::Single);
        assert_eq!(Register::new(129).operand_scale(), OperandScale::Double);
        assert_eq!(
            Register::new(32_769).operand_scale(),
            OperandScale::Quadruple
        );

        // A parameter encodes as a positive operand, so it leaves a byte one
        // step earlier.
        assert_eq!(Register::receiver().size_of_operand(), OperandSize::Byte);
        assert_eq!(
            Register::from_parameter_index(121).size_of_operand(),
            OperandSize::Byte
        );
        assert_eq!(
            Register::from_parameter_index(122).size_of_operand(),
            OperandSize::Short
        );
    }

    #[test]
    fn register_lists_are_consecutive() {
        let list = RegisterList::new(Register::new(3), 4);
        assert_eq!(list.register_count(), 4);
        assert_eq!(list.first_register(), Register::new(3));
        assert_eq!(list.get(0), Register::new(3));
        assert_eq!(list.get(3), Register::new(6));
        assert_eq!(
            list.iter().collect::<Vec<_>>(),
            vec![
                Register::new(3),
                Register::new(4),
                Register::new(5),
                Register::new(6)
            ]
        );
    }

    #[test]
    fn popping_the_receiver_leaves_the_arguments() {
        // How a call separates the receiver from what follows it.
        let list = RegisterList::new(Register::new(2), 3);
        let arguments = list.pop_left();
        assert_eq!(arguments.register_count(), 2);
        assert_eq!(arguments.first_register(), Register::new(3));

        let truncated = list.truncate(1);
        assert_eq!(truncated.register_count(), 1);
        assert_eq!(truncated.first_register(), Register::new(2));
    }

    #[test]
    fn an_empty_list_has_no_first_register() {
        let empty = RegisterList::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.register_count(), 0);
        assert!(!empty.first_register().is_valid());
    }

    #[test]
    fn registers_print_readably() {
        assert_eq!(format!("{:?}", Register::new(0)), "r0");
        assert_eq!(format!("{:?}", Register::new(7)), "r7");
        assert_eq!(format!("{:?}", Register::receiver()), "<this>");
        assert_eq!(format!("{:?}", Register::from_parameter_index(1)), "a0");
        assert_eq!(format!("{:?}", Register::current_context()), "<context>");
        assert_eq!(format!("{:?}", Register::invalid_value()), "<invalid>");
    }
}

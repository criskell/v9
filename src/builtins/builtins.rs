//! Ported from `src/builtins/builtins.h`, in miniature.
//!
//! V8's builtins are generated code -- Torque or CSA compiled into the
//! snapshot -- and there are well over a thousand of them. `vx` has a handful
//! written in Rust, named by this enum and reached through a
//! `SharedFunctionInfo` whose `function_data` holds the id.

use crate::execution::isolate::Isolate;
use crate::interpreter::interpreter::{ExecutionResult, Thrown};
use crate::objects::tagged::Tagged;

/// `Builtin`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum Builtin {
    ConsoleLog = 0,
    MathSqrt = 1,
    MathFloor = 2,
    MathAbs = 3,
    MathMax = 4,
    MathMin = 5,
    MathPow = 6,
    StringLength = 7,
}

impl Builtin {
    pub fn from_id(id: i32) -> Option<Builtin> {
        use Builtin::*;
        Some(match id {
            0 => ConsoleLog,
            1 => MathSqrt,
            2 => MathFloor,
            3 => MathAbs,
            4 => MathMax,
            5 => MathMin,
            6 => MathPow,
            7 => StringLength,
            _ => return None,
        })
    }

    pub fn id(self) -> i32 {
        self as i32
    }
}

impl Isolate {
    /// Runs a builtin. `args` excludes the receiver.
    pub fn call_builtin(&mut self, builtin: Builtin, args: &[Tagged]) -> ExecutionResult {
        let argument = |index: usize| args.get(index).copied();
        match builtin {
            Builtin::ConsoleLog => {
                let parts: Vec<String> = args.iter().map(|value| self.to_display(*value)).collect();
                self.write_output(&parts.join(" "));
                Ok(self.undefined_value())
            }
            Builtin::MathSqrt => {
                let value = argument(0).map_or(f64::NAN, |value| self.to_number(value));
                Ok(self.new_number(value.sqrt()))
            }
            Builtin::MathFloor => {
                let value = argument(0).map_or(f64::NAN, |value| self.to_number(value));
                Ok(self.new_number(value.floor()))
            }
            Builtin::MathAbs => {
                let value = argument(0).map_or(f64::NAN, |value| self.to_number(value));
                Ok(self.new_number(value.abs()))
            }
            Builtin::MathMax => {
                // `Math.max()` with no arguments is -Infinity, which is the
                // identity for max -- not NaN.
                let mut result = f64::NEG_INFINITY;
                for value in args {
                    let number = self.to_number(*value);
                    if number.is_nan() {
                        return Ok(self.new_number(f64::NAN));
                    }
                    if number > result {
                        result = number;
                    }
                }
                Ok(self.new_number(result))
            }
            Builtin::MathMin => {
                let mut result = f64::INFINITY;
                for value in args {
                    let number = self.to_number(*value);
                    if number.is_nan() {
                        return Ok(self.new_number(f64::NAN));
                    }
                    if number < result {
                        result = number;
                    }
                }
                Ok(self.new_number(result))
            }
            Builtin::MathPow => {
                let base = argument(0).map_or(f64::NAN, |value| self.to_number(value));
                let exponent = argument(1).map_or(f64::NAN, |value| self.to_number(value));
                Ok(self.new_number(base.powf(exponent)))
            }
            Builtin::StringLength => {
                let value = argument(0).unwrap_or(self.undefined_value());
                match crate::objects::string::String::try_cast(&self.heap, value) {
                    Some(string) => {
                        let length = string.length(&self.heap);
                        Ok(crate::objects::smi::Smi::from_int(length))
                    }
                    None => {
                        let message = self.new_string("length expects a string").tagged();
                        Err(Thrown(message))
                    }
                }
            }
        }
    }

    /// How a value prints at the console. Differs from `ToString` in one
    /// place that matters for reading output: a string argument to
    /// `console.log` is printed bare, not quoted.
    pub fn to_display(&mut self, value: Tagged) -> String {
        self.to_string(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::smi::Smi;

    #[test]
    fn builtin_ids_round_trip() {
        for builtin in [
            Builtin::ConsoleLog,
            Builtin::MathSqrt,
            Builtin::MathFloor,
            Builtin::MathAbs,
            Builtin::MathMax,
            Builtin::MathMin,
            Builtin::MathPow,
            Builtin::StringLength,
        ] {
            assert_eq!(Builtin::from_id(builtin.id()), Some(builtin));
        }
        assert_eq!(Builtin::from_id(999), None);
    }

    #[test]
    fn math_functions_convert_their_arguments() {
        let mut isolate = Isolate::new();
        let four = Smi::from_int(4);
        let result = isolate.call_builtin(Builtin::MathSqrt, &[four]).unwrap();
        assert_eq!(isolate.to_number(result), 2.0);

        let text = isolate.new_string("9").tagged();
        let result = isolate.call_builtin(Builtin::MathSqrt, &[text]).unwrap();
        assert_eq!(isolate.to_number(result), 3.0);
    }

    #[test]
    fn max_with_no_arguments_is_negative_infinity() {
        // The identity for max, which is what the spec says -- not NaN.
        let mut isolate = Isolate::new();
        let result = isolate.call_builtin(Builtin::MathMax, &[]).unwrap();
        assert_eq!(isolate.to_number(result), f64::NEG_INFINITY);
        let result = isolate.call_builtin(Builtin::MathMin, &[]).unwrap();
        assert_eq!(isolate.to_number(result), f64::INFINITY);
    }

    #[test]
    fn max_is_poisoned_by_nan() {
        let mut isolate = Isolate::new();
        let nan = isolate.new_heap_number(f64::NAN).tagged();
        let result = isolate
            .call_builtin(Builtin::MathMax, &[Smi::from_int(1), nan])
            .unwrap();
        assert!(isolate.to_number(result).is_nan());
    }

    #[test]
    fn a_builtin_can_throw() {
        let mut isolate = Isolate::new();
        let result = isolate.call_builtin(Builtin::StringLength, &[Smi::from_int(1)]);
        assert!(result.is_err());
    }
}

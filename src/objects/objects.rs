use crate::execution::isolate::Isolate;
use crate::numbers::conversions::number_to_string;
use crate::objects::heap_number::number_value;
use crate::objects::instance_type;
use crate::objects::js_objects::JSObject;
use crate::objects::oddball::{Oddball, OddballKind};
use crate::objects::smi::Smi;
use crate::objects::string::String as VxString;
use crate::objects::tagged::Tagged;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ValueKind {
    Smi,
    HeapNumber,
    String,
    Boolean,
    Null,
    Undefined,
    Object,
    Function,
    Hole,
}

impl Isolate {
    pub fn value_kind(&self, value: Tagged) -> ValueKind {
        if value.is_smi() {
            return ValueKind::Smi;
        }
        let Some(object) = value.to_strong_heap_object() else {
            return ValueKind::Hole;
        };
        let instance_type = self.heap.instance_type(object);
        if instance_type::is_string(instance_type) {
            return ValueKind::String;
        }
        if instance_type::is_heap_number(instance_type) {
            return ValueKind::HeapNumber;
        }
        if instance_type::is_oddball(instance_type) {
            return match Oddball::unchecked_cast(object).kind(&self.heap) {
                Some(OddballKind::True) | Some(OddballKind::False) => ValueKind::Boolean,
                Some(OddballKind::Null) => ValueKind::Null,
                Some(OddballKind::Undefined) => ValueKind::Undefined,
                _ => ValueKind::Hole,
            };
        }
        if instance_type::is_callable(instance_type) {
            return ValueKind::Function;
        }
        ValueKind::Object
    }

    /// `Object::BooleanValue`.
    /// `-0`, `NaN`, `""`.
    pub fn to_boolean(&self, value: Tagged) -> bool {
        match self.value_kind(value) {
            ValueKind::Smi => Smi::to_int(value) != 0,
            ValueKind::HeapNumber => {
                let number = number_value(&self.heap, value).unwrap();
                number != 0.0 && !number.is_nan()
            }
            ValueKind::String => {
                VxString::unchecked_cast(value.to_strong_heap_object().unwrap()).length(&self.heap)
                    != 0
            }
            ValueKind::Boolean => value == self.true_value(),
            ValueKind::Null | ValueKind::Undefined | ValueKind::Hole => false,
            ValueKind::Object | ValueKind::Function => true,
        }
    }

    pub fn to_number(&self, value: Tagged) -> f64 {
        match self.value_kind(value) {
            ValueKind::Smi => Smi::to_int(value) as f64,
            ValueKind::HeapNumber => number_value(&self.heap, value).unwrap(),
            ValueKind::String => {
                let text = VxString::unchecked_cast(value.to_strong_heap_object().unwrap())
                    .to_rust_string(&self.heap);
                string_to_number(&text)
            }
            ValueKind::Boolean => {
                if value == self.true_value() {
                    1.0
                } else {
                    0.0
                }
            }
            ValueKind::Null | ValueKind::Undefined => {
                Oddball::unchecked_cast(value.to_strong_heap_object().unwrap())
                    .to_number_raw(&self.heap)
            }
            ValueKind::Object | ValueKind::Function | ValueKind::Hole => f64::NAN,
        }
    }

    /// `Object::ToString`
    pub fn to_string(&self, value: Tagged) -> std::string::String {
        match self.value_kind(value) {
            ValueKind::Smi => Smi::to_int(value).to_string(),
            ValueKind::HeapNumber => number_to_string(number_value(&self.heap, value).unwrap()),
            ValueKind::String => VxString::unchecked_cast(value.to_strong_heap_object().unwrap())
                .to_rust_string(&self.heap),
            ValueKind::Boolean | ValueKind::Null | ValueKind::Undefined => {
                let to_string = Oddball::unchecked_cast(value.to_strong_heap_object().unwrap())
                    .to_string(&self.heap);
                VxString::unchecked_cast(to_string.to_strong_heap_object().unwrap())
                    .to_rust_string(&self.heap)
            }
            ValueKind::Function => "function () { [native code] }".to_string(),
            ValueKind::Object => "[object Object]".to_string(),
            ValueKind::Hole => "<hole>".to_string(),
        }
    }

    /// `Object::TypeOf`
    pub fn type_of(&mut self, value: Tagged) -> Tagged {
        let name = match self.value_kind(value) {
            ValueKind::Smi | ValueKind::HeapNumber => "number",
            ValueKind::String => "string",
            ValueKind::Boolean => "boolean",
            ValueKind::Undefined | ValueKind::Hole => "undefined",
            ValueKind::Null => "object",
            ValueKind::Function => "function",
            ValueKind::Object => "object",
        };
        self.internalize_string(name).tagged()
    }

    pub fn add(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let left_is_string = self.value_kind(left) == ValueKind::String;
        let right_is_string = self.value_kind(right) == ValueKind::String;
        if left_is_string || right_is_string {
            let text = format!("{}{}", self.to_string(left), self.to_string(right));
            return self.new_string(&text).tagged();
        }
        if left.is_smi() && right.is_smi() {
            let sum = Smi::to_int(left) as i64 + Smi::to_int(right) as i64;
            return self.new_number_from_i64(sum);
        }
        let sum = self.to_number(left) + self.to_number(right);
        self.new_number(sum)
    }

    pub fn subtract(&mut self, left: Tagged, right: Tagged) -> Tagged {
        if left.is_smi() && right.is_smi() {
            let result = Smi::to_int(left) as i64 - Smi::to_int(right) as i64;
            return self.new_number_from_i64(result);
        }
        let result = self.to_number(left) - self.to_number(right);
        self.new_number(result)
    }

    pub fn multiply(&mut self, left: Tagged, right: Tagged) -> Tagged {
        if left.is_smi() && right.is_smi() {
            let result = Smi::to_int(left) as i64 * Smi::to_int(right) as i64;
            return self.new_number_from_i64(result);
        }
        let result = self.to_number(left) * self.to_number(right);
        self.new_number(result)
    }

    pub fn divide(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let result = self.to_number(left) / self.to_number(right);
        self.new_number(result)
    }

    pub fn modulus(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let result = self.to_number(left) % self.to_number(right);
        self.new_number(result)
    }

    pub fn exponentiate(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let result = self.to_number(left).powf(self.to_number(right));
        self.new_number(result)
    }

    pub fn bitwise_and(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let result = self.to_int32(left) & self.to_int32(right);
        Smi::from_int(result)
    }

    pub fn bitwise_or(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let result = self.to_int32(left) | self.to_int32(right);
        Smi::from_int(result)
    }

    pub fn bitwise_xor(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let result = self.to_int32(left) ^ self.to_int32(right);
        Smi::from_int(result)
    }

    pub fn bitwise_not(&mut self, value: Tagged) -> Tagged {
        Smi::from_int(!self.to_int32(value))
    }

    pub fn shift_left(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let count = (self.to_uint32(right) & 31) as u32;
        Smi::from_int(self.to_int32(left) << count)
    }

    pub fn shift_right(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let count = (self.to_uint32(right) & 31) as u32;
        Smi::from_int(self.to_int32(left) >> count)
    }

    pub fn shift_right_logical(&mut self, left: Tagged, right: Tagged) -> Tagged {
        let count = (self.to_uint32(right) & 31) as u32;
        let result = self.to_uint32(left) >> count;
        self.new_number(result as f64)
    }

    /// `Object::ToArrayLength`
    // Tries to convert an object to an array length. Returns true and sets the
    // output parameter if it succeeds.
    pub fn to_array_length(&self, value: Tagged) -> Option<u32> {
        if let Some(smi) = crate::objects::smi::Smi::try_to_int(value) {
            return if smi >= 0 { Some(smi as u32) } else { None };
        }
        let number = crate::objects::heap_number::number_value(&self.heap, value)?;
        if number < 0.0 || number.fract() != 0.0 || number > u32::MAX as f64 {
            return None;
        }
        Some(number as u32)
    }

    /// `Object::ToArrayIndex`
    // Tries to convert an object to an array index. Returns true and sets the
    // output parameter if it succeeds. Equivalent to ToArrayLength, but does
    // not allow kMaxUInt32.
    pub fn to_array_index(&self, value: Tagged) -> Option<u32> {
        match self.to_array_length(value) {
            Some(length) if length != u32::MAX => Some(length),
            _ => None,
        }
    }

    /// `String::AsArrayIndex`
    pub fn string_as_array_index(&self, value: Tagged) -> Option<u32> {
        let string = crate::objects::string::String::try_cast(&self.heap, value)?;
        let text = string.to_rust_string(&self.heap);
        if text.is_empty() || (text.len() > 1 && text.starts_with('0')) {
            return None;
        }
        let index: u32 = text.parse().ok()?;
        if index == u32::MAX {
            return None;
        }
        Some(index)
    }

    pub fn to_int32(&self, value: Tagged) -> i32 {
        if value.is_smi() {
            return Smi::to_int(value);
        }
        double_to_int32(self.to_number(value))
    }

    pub fn to_uint32(&self, value: Tagged) -> u32 {
        self.to_int32(value) as u32
    }

    pub fn strict_equals(&self, left: Tagged, right: Tagged) -> bool {
        let left_kind = self.value_kind(left);
        let right_kind = self.value_kind(right);

        let numeric = |kind| matches!(kind, ValueKind::Smi | ValueKind::HeapNumber);
        if numeric(left_kind) && numeric(right_kind) {
            let a = self.to_number(left);
            let b = self.to_number(right);
            return a == b;
        }
        if left_kind != right_kind {
            return false;
        }
        match left_kind {
            ValueKind::String => {
                let a = VxString::unchecked_cast(left.to_strong_heap_object().unwrap());
                let b = VxString::unchecked_cast(right.to_strong_heap_object().unwrap());
                a.equals(&self.heap, b)
            }
            _ => left == right,
        }
    }

    pub fn loose_equals(&mut self, left: Tagged, right: Tagged) -> bool {
        let left_kind = self.value_kind(left);
        let right_kind = self.value_kind(right);

        let nullish = |kind| matches!(kind, ValueKind::Null | ValueKind::Undefined);
        if nullish(left_kind) || nullish(right_kind) {
            return nullish(left_kind) && nullish(right_kind);
        }

        if left_kind == right_kind
            || (matches!(left_kind, ValueKind::Smi | ValueKind::HeapNumber)
                && matches!(right_kind, ValueKind::Smi | ValueKind::HeapNumber))
        {
            return self.strict_equals(left, right);
        }

        let a = self.to_number(left);
        let b = self.to_number(right);
        a == b
    }

    pub fn less_than(&mut self, left: Tagged, right: Tagged) -> bool {
        match self.compare(left, right) {
            Some(std::cmp::Ordering::Less) => true,
            _ => false,
        }
    }

    pub fn greater_than(&mut self, left: Tagged, right: Tagged) -> bool {
        matches!(self.compare(left, right), Some(std::cmp::Ordering::Greater))
    }

    pub fn less_than_or_equal(&mut self, left: Tagged, right: Tagged) -> bool {
        matches!(
            self.compare(left, right),
            Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal)
        )
    }

    pub fn greater_than_or_equal(&mut self, left: Tagged, right: Tagged) -> bool {
        matches!(
            self.compare(left, right),
            Some(std::cmp::Ordering::Greater) | Some(std::cmp::Ordering::Equal)
        )
    }

    fn compare(&mut self, left: Tagged, right: Tagged) -> Option<std::cmp::Ordering> {
        if self.value_kind(left) == ValueKind::String && self.value_kind(right) == ValueKind::String
        {
            let a = self.to_string(left);
            let b = self.to_string(right);
            return Some(a.cmp(&b));
        }
        let a = self.to_number(left);
        let b = self.to_number(right);
        a.partial_cmp(&b)
    }

    pub fn has_property(&mut self, object: Tagged, key: Tagged) -> bool {
        let Some(receiver) = object.to_strong_heap_object() else {
            return false;
        };
        if !instance_type::is_js_receiver(self.heap.instance_type(receiver)) {
            return false;
        }
        let name = self.to_string(key);
        let key = self.internalize_string(&name).tagged();
        self.lookup_property(JSObject::unchecked_cast(receiver), key)
            .is_some()
    }
}

/// `DoubleToInt32`
pub fn double_to_int32(value: f64) -> i32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    let truncated = value.trunc();
    let wrapped = truncated.rem_euclid(4294967296.0);
    let bits = wrapped as u32;
    bits as i32
}

pub fn string_to_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16)
            .map(|v| v as f64)
            .unwrap_or(f64::NAN);
    }
    match trimmed {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_falsy_values_and_nothing_else() {
        let mut isolate = Isolate::new();
        let empty = isolate.new_string("").tagged();
        let nan = isolate.new_heap_number(f64::NAN).tagged();
        let negative_zero = isolate.new_heap_number(-0.0).tagged();

        for value in [
            isolate.false_value(),
            isolate.undefined_value(),
            isolate.null_value(),
            Smi::from_int(0),
            empty,
            nan,
            negative_zero,
        ] {
            assert!(!isolate.to_boolean(value), "{value:?} should be falsy");
        }

        let text = isolate.new_string("0").tagged();
        let object = isolate.new_js_object().tagged();
        for value in [
            isolate.true_value(),
            Smi::from_int(1),
            Smi::from_int(-1),
            text,
            object,
        ] {
            assert!(isolate.to_boolean(value), "{value:?} should be truthy");
        }
    }

    #[test]
    fn plus_concatenates_when_either_side_is_a_string() {
        let mut isolate = Isolate::new();
        let one = Smi::from_int(1);
        let text = isolate.new_string("1").tagged();

        let sum = isolate.add(one, one);
        assert_eq!(Smi::to_int(sum), 2);

        let concatenated = isolate.add(one, text);
        assert_eq!(isolate.to_string(concatenated), "11");
        let other_way = isolate.add(text, one);
        assert_eq!(isolate.to_string(other_way), "11");
    }

    #[test]
    fn arithmetic_overflows_out_of_smi_range_into_a_heap_number() {
        let mut isolate = Isolate::new();
        let big = Smi::from_int(i32::MAX);
        let sum = isolate.add(big, Smi::from_int(1));
        assert!(!sum.is_smi(), "the result no longer fits in a Smi");
        assert_eq!(isolate.to_number(sum), i32::MAX as f64 + 1.0);
    }

    #[test]
    fn division_is_not_integer_division() {
        let mut isolate = Isolate::new();
        let result = isolate.divide(Smi::from_int(1), Smi::from_int(2));
        assert_eq!(isolate.to_number(result), 0.5);
        let infinity = isolate.divide(Smi::from_int(1), Smi::from_int(0));
        assert_eq!(isolate.to_number(infinity), f64::INFINITY);
    }

    #[test]
    fn modulus_keeps_the_sign_of_the_dividend() {
        let mut isolate = Isolate::new();
        let result = isolate.modulus(Smi::from_int(-7), Smi::from_int(3));
        assert_eq!(
            isolate.to_number(result),
            -1.0,
            "not 2, as rem_euclid gives"
        );
    }

    #[test]
    fn typeof_null_is_object() {
        let mut isolate = Isolate::new();
        let null = isolate.null_value();
        let answer = isolate.type_of(null);
        assert_eq!(isolate.to_string(answer), "object");

        let undefined = isolate.undefined_value();
        let answer = isolate.type_of(undefined);
        assert_eq!(isolate.to_string(answer), "undefined");

        let answer = isolate.type_of(Smi::from_int(1));
        assert_eq!(isolate.to_string(answer), "number");
    }

    #[test]
    fn strict_equality_compares_numbers_across_representations() {
        let mut isolate = Isolate::new();
        let boxed = isolate.new_heap_number(1.0).tagged();
        let smi = Smi::from_int(1);
        assert!(
            isolate.strict_equals(boxed, smi),
            "which representation a number has is not observable"
        );

        let nan = isolate.new_heap_number(f64::NAN).tagged();
        assert!(!isolate.strict_equals(nan, nan), "NaN !== NaN");
    }

    #[test]
    fn strict_equality_on_strings_is_by_content() {
        let mut isolate = Isolate::new();
        let a = isolate.new_string("key").tagged();
        let b = isolate.new_string("key").tagged();
        assert_ne!(a, b, "two separate string objects");
        assert!(isolate.strict_equals(a, b));
    }

    #[test]
    fn strict_equality_never_converts() {
        let mut isolate = Isolate::new();
        let one = Smi::from_int(1);
        let text = isolate.new_string("1").tagged();
        assert!(!isolate.strict_equals(one, text));
        assert!(isolate.loose_equals(one, text), "but == does");
    }

    #[test]
    fn loose_equality_treats_null_and_undefined_as_a_pair() {
        let mut isolate = Isolate::new();
        let null = isolate.null_value();
        let undefined = isolate.undefined_value();
        assert!(isolate.loose_equals(null, undefined));
        assert!(isolate.loose_equals(null, null));
        assert!(!isolate.loose_equals(null, Smi::from_int(0)));
        assert!(!isolate.loose_equals(undefined, Smi::from_int(0)));
        assert!(!isolate.loose_equals(null, isolate.false_value()));
    }

    #[test]
    fn the_empty_string_loosely_equals_zero() {
        let mut isolate = Isolate::new();
        let empty = isolate.new_string("").tagged();
        assert!(isolate.loose_equals(empty, Smi::from_int(0)));
        assert!(!isolate.strict_equals(empty, Smi::from_int(0)));
    }

    #[test]
    fn comparison_is_lexicographic_only_when_both_sides_are_strings() {
        let mut isolate = Isolate::new();
        let ten = isolate.new_string("10").tagged();
        let nine = isolate.new_string("9").tagged();
        assert!(isolate.less_than(ten, nine), "\"10\" < \"9\" as strings");
        assert!(!isolate.less_than(Smi::from_int(10), Smi::from_int(9)));
        assert!(isolate.less_than(ten, Smi::from_int(11)));
    }

    #[test]
    fn every_relational_operator_is_false_for_nan() {
        let mut isolate = Isolate::new();
        let nan = isolate.new_heap_number(f64::NAN).tagged();
        let one = Smi::from_int(1);
        assert!(!isolate.less_than(nan, one));
        assert!(!isolate.greater_than(nan, one));
        assert!(!isolate.less_than_or_equal(nan, one));
        assert!(!isolate.greater_than_or_equal(nan, one));
    }

    #[test]
    fn shifts_take_their_count_modulo_32() {
        let mut isolate = Isolate::new();
        let one = Smi::from_int(1);
        let shifted = isolate.shift_left(one, Smi::from_int(32));
        assert_eq!(Smi::to_int(shifted), 1, "1 << 32 is 1, not 0");
        let shifted = isolate.shift_left(one, Smi::from_int(3));
        assert_eq!(Smi::to_int(shifted), 8);
    }

    #[test]
    fn logical_shift_right_is_unsigned() {
        let mut isolate = Isolate::new();
        let negative = Smi::from_int(-1);
        let arithmetic = isolate.shift_right(negative, Smi::from_int(1));
        assert_eq!(Smi::to_int(arithmetic), -1, ">> propagates the sign");
        let logical = isolate.shift_right_logical(negative, Smi::from_int(1));
        assert_eq!(
            isolate.to_number(logical),
            2147483647.0,
            ">>> does not, so the result is positive"
        );
    }

    #[test]
    fn to_int32_wraps_rather_than_saturating() {
        assert_eq!(double_to_int32(1.9), 1, "truncates towards zero");
        assert_eq!(double_to_int32(-1.9), -1);
        assert_eq!(double_to_int32(4294967296.0), 0, "2^32 wraps to 0");
        assert_eq!(double_to_int32(4294967297.0), 1);
        assert_eq!(double_to_int32(f64::NAN), 0);
        assert_eq!(double_to_int32(f64::INFINITY), 0);
    }

    #[test]
    fn string_to_number_follows_the_spec_not_rust() {
        assert_eq!(string_to_number(""), 0.0, "Number(\"\") is 0");
        assert_eq!(string_to_number("  12  "), 12.0, "whitespace is trimmed");
        assert_eq!(string_to_number("0x10"), 16.0);
        assert_eq!(string_to_number("Infinity"), f64::INFINITY);
        assert!(string_to_number("12px").is_nan());
        assert_eq!(string_to_number("1.5"), 1.5);
        assert_eq!(string_to_number("-2"), -2.0);
    }

    #[test]
    fn numbers_stringify_as_javascript_prints_them() {
        let mut isolate = Isolate::new();
        assert_eq!(isolate.to_string(Smi::from_int(42)), "42");
        let half = isolate.new_heap_number(0.5).tagged();
        assert_eq!(isolate.to_string(half), "0.5");
        let integral = isolate.new_heap_number(3.0).tagged();
        assert_eq!(isolate.to_string(integral), "3", "not \"3.0\"");
        let negative_zero = isolate.new_heap_number(-0.0).tagged();
        assert_eq!(isolate.to_string(negative_zero), "0");
    }
}

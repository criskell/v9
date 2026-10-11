use crate::execution::isolate::Isolate;
use crate::interpreter::interpreter::{ExecutionResult, Thrown};
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

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

    ArrayPrototypePush = 8,
    ArrayPrototypePop = 9,
    ArrayPrototypeJoin = 10,
    ArrayPrototypeIndexOf = 11,
    StringPrototypeCharAt = 12,
    StringPrototypeIndexOf = 13,
    StringPrototypeSlice = 14,
    StringPrototypeToUpperCase = 15,
    StringPrototypeToLowerCase = 16,
    NumberPrototypeToString = 17,
    ObjectPrototypeHasOwnProperty = 18,
    ObjectPrototypeToString = 19,
    ObjectKeys = 20,
    FunctionPrototypeCall = 21,
    ObjectDefineProperty = 22,
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
            8 => ArrayPrototypePush,
            9 => ArrayPrototypePop,
            10 => ArrayPrototypeJoin,
            11 => ArrayPrototypeIndexOf,
            12 => StringPrototypeCharAt,
            13 => StringPrototypeIndexOf,
            14 => StringPrototypeSlice,
            15 => StringPrototypeToUpperCase,
            16 => StringPrototypeToLowerCase,
            17 => NumberPrototypeToString,
            18 => ObjectPrototypeHasOwnProperty,
            19 => ObjectPrototypeToString,
            20 => ObjectKeys,
            22 => ObjectDefineProperty,
            21 => FunctionPrototypeCall,
            _ => return None,
        })
    }

    pub fn id(self) -> i32 {
        self as i32
    }
}

impl Isolate {
    pub fn call_builtin(
        &mut self,
        builtin: Builtin,
        receiver: Tagged,
        args: &[Tagged],
    ) -> ExecutionResult {
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

            Builtin::ArrayPrototypePush => self.array_prototype_push(receiver, args),
            Builtin::ArrayPrototypePop => self.array_prototype_pop(receiver),
            Builtin::ArrayPrototypeJoin => self.array_prototype_join(receiver, argument(0)),
            Builtin::ArrayPrototypeIndexOf => self.array_prototype_index_of(receiver, argument(0)),

            Builtin::StringPrototypeCharAt => {
                let text = self.to_string(receiver);
                let index = argument(0).map_or(0.0, |value| self.to_number(value));
                let characters: Vec<char> = text.chars().collect();
                let result = if index < 0.0 || index as usize >= characters.len() {
                    String::new()
                } else {
                    characters[index as usize].to_string()
                };
                Ok(self.new_string(&result).tagged())
            }
            Builtin::StringPrototypeIndexOf => {
                let text = self.to_string(receiver);
                let needle = match argument(0) {
                    Some(value) => self.to_string(value),
                    None => "undefined".to_string(),
                };
                let found = match text.find(&needle) {
                    Some(byte_offset) => text[..byte_offset].chars().count() as i32,
                    None => -1,
                };
                Ok(Smi::from_int(found))
            }
            Builtin::StringPrototypeSlice => {
                let text = self.to_string(receiver);
                let characters: Vec<char> = text.chars().collect();
                let length = characters.len() as f64;
                let relative = |value: Option<Tagged>, default: f64| -> usize {
                    let raw = match value {
                        Some(value) => self.to_number(value),
                        None => default,
                    };
                    let resolved = if raw < 0.0 { length + raw } else { raw };
                    resolved.clamp(0.0, length) as usize
                };
                let start = relative(argument(0), 0.0);
                let end = relative(argument(1), length);
                let result: String = if start < end {
                    characters[start..end].iter().collect()
                } else {
                    String::new()
                };
                Ok(self.new_string(&result).tagged())
            }
            Builtin::StringPrototypeToUpperCase => {
                let text = self.to_string(receiver).to_uppercase();
                Ok(self.new_string(&text).tagged())
            }
            Builtin::StringPrototypeToLowerCase => {
                let text = self.to_string(receiver).to_lowercase();
                Ok(self.new_string(&text).tagged())
            }

            Builtin::NumberPrototypeToString => {
                let text = self.to_string(receiver);
                Ok(self.new_string(&text).tagged())
            }

            Builtin::ObjectPrototypeHasOwnProperty => {
                let name = match argument(0) {
                    Some(value) => value,
                    None => self.undefined_value(),
                };
                Ok(self.has_own_property(receiver, name))
            }
            Builtin::ObjectPrototypeToString => {
                let text = self.to_string(receiver);
                Ok(self.new_string(&text).tagged())
            }
            Builtin::ObjectKeys => self.object_keys(argument(0)),
            Builtin::ObjectDefineProperty => {
                self.object_define_property(argument(0), argument(1), argument(2))
            }

            Builtin::FunctionPrototypeCall => {
                let this_argument = argument(0).unwrap_or(self.undefined_value());
                let rest: Vec<Tagged> = args.iter().skip(1).copied().collect();
                let Some(callee) = receiver.to_strong_heap_object() else {
                    let message = self.new_string("call expects a function").tagged();
                    return Err(Thrown(message));
                };
                if !crate::objects::instance_type::is_callable(self.heap.instance_type(callee)) {
                    let message = self.new_string("call expects a function").tagged();
                    return Err(Thrown(message));
                }
                let callee = crate::objects::js_function::JSFunction::unchecked_cast(callee);
                self.execute(callee, this_argument, &rest)
            }
        }
    }

    pub fn to_display(&mut self, value: Tagged) -> String {
        self.to_string(value)
    }
}

impl Isolate {
    fn array_prototype_push(&mut self, receiver: Tagged, args: &[Tagged]) -> ExecutionResult {
        let array = match self.as_js_array(receiver) {
            Some(array) => array,
            None => return self.not_an_array("push"),
        };
        let scope = self.handle_scope();
        let array_handle = self.create_object_handle(array.object());
        for value in args {
            let value_handle = self.create_handle(*value);
            let array =
                crate::objects::js_array::JSArray::unchecked_cast(array_handle.object(self));
            let index = array.length_value(&self.heap);
            let value = value_handle.get(self);
            self.add_data_element(array.as_js_object(), index, value);
        }
        let array = crate::objects::js_array::JSArray::unchecked_cast(array_handle.object(self));
        let length = array.length_value(&self.heap);
        self.close_handle_scope(scope);
        Ok(Smi::from_int(length as i32))
    }

    fn array_prototype_pop(&mut self, receiver: Tagged) -> ExecutionResult {
        use crate::objects::elements::ElementsAccessor;
        let array = match self.as_js_array(receiver) {
            Some(array) => array,
            None => return self.not_an_array("pop"),
        };
        let length = array.length_value(&self.heap);
        if length == 0 {
            return Ok(self.undefined_value());
        }
        let object = array.as_js_object();
        let kind = object.get_elements_kind(&self.heap);
        let last = ElementsAccessor::for_kind(kind)
            .get(self, object, length - 1)
            .unwrap_or_else(|| self.undefined_value());
        let scope = self.handle_scope();
        let last_handle = self.create_handle(last);
        let array = self.as_js_array(receiver).expect("still an array");
        self.set_elements_length(array, length - 1);
        let last = last_handle.get(self);
        self.close_handle_scope(scope);
        Ok(last)
    }

    fn array_prototype_join(
        &mut self,
        receiver: Tagged,
        separator: Option<Tagged>,
    ) -> ExecutionResult {
        use crate::objects::elements::ElementsAccessor;
        let array = match self.as_js_array(receiver) {
            Some(array) => array,
            None => return self.not_an_array("join"),
        };
        let separator = match separator {
            Some(value) if !self.heap.is_undefined(value) => self.to_string(value),
            _ => ",".to_string(),
        };
        let length = array.length_value(&self.heap);
        let mut parts: Vec<String> = Vec::new();
        for index in 0..length {
            let array = self.as_js_array(receiver).expect("still an array");
            let object = array.as_js_object();
            let kind = object.get_elements_kind(&self.heap);
            let element = ElementsAccessor::for_kind(kind).get(self, object, index);
            parts.push(match element {
                Some(value) if !self.heap.is_undefined(value) && value != self.roots.null_value => {
                    self.to_string(value)
                }
                _ => String::new(),
            });
        }
        let joined = parts.join(&separator);
        Ok(self.new_string(&joined).tagged())
    }

    fn array_prototype_index_of(
        &mut self,
        receiver: Tagged,
        needle: Option<Tagged>,
    ) -> ExecutionResult {
        use crate::objects::elements::ElementsAccessor;
        let array = match self.as_js_array(receiver) {
            Some(array) => array,
            None => return self.not_an_array("indexOf"),
        };
        let needle = needle.unwrap_or(self.undefined_value());
        let length = array.length_value(&self.heap);
        for index in 0..length {
            let array = self.as_js_array(receiver).expect("still an array");
            let object = array.as_js_object();
            let kind = object.get_elements_kind(&self.heap);
            if let Some(value) = ElementsAccessor::for_kind(kind).get(self, object, index) {
                if self.strict_equals(value, needle) {
                    return Ok(Smi::from_int(index as i32));
                }
            }
        }
        Ok(Smi::from_int(-1))
    }

    fn object_define_property(
        &mut self,
        target: Option<Tagged>,
        key: Option<Tagged>,
        attributes: Option<Tagged>,
    ) -> ExecutionResult {
        use crate::objects::js_objects::JSObject;
        let target = target.unwrap_or(self.undefined_value());
        let key = key.unwrap_or(self.undefined_value());
        let attributes = attributes.unwrap_or(self.undefined_value());

        let Some(object) = target.to_strong_heap_object() else {
            let message = self
                .new_string("Object.defineProperty called on non-object")
                .tagged();
            return Err(Thrown(message));
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(object)) {
            let message = self
                .new_string("Object.defineProperty called on non-object")
                .tagged();
            return Err(Thrown(message));
        }

        let scope = self.handle_scope();
        let target_handle = self.create_object_handle(object);
        let key_text = self.to_string(key);
        let name = self.internalize_string(&key_text).tagged();
        let name_handle = self.create_handle(name);

        let descriptor = self.to_property_descriptor(attributes)?;
        let property_attributes = descriptor.to_attributes();

        let object = JSObject::unchecked_cast(target_handle.object(self));
        let name = name_handle.get(self);
        if descriptor.is_accessor_descriptor() {
            let null = self.null_value();
            let getter = descriptor.get().unwrap_or(null);
            let setter = descriptor.set().unwrap_or(null);
            self.define_accessor(object, name, getter, setter, property_attributes);
        } else {
            let value = descriptor.value().unwrap_or(self.undefined_value());
            self.add_property_with_attributes(object, name, value, property_attributes);
        }
        let result = target_handle.get(self);
        self.close_handle_scope(scope);
        Ok(result)
    }

    fn object_keys(&mut self, target: Option<Tagged>) -> ExecutionResult {
        use crate::objects::js_objects::JSObject;
        let target = target.unwrap_or(self.undefined_value());
        let Some(object) = target.to_strong_heap_object() else {
            let message = self.new_string("Object.keys expects an object").tagged();
            return Err(Thrown(message));
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(object)) {
            let message = self.new_string("Object.keys expects an object").tagged();
            return Err(Thrown(message));
        }
        let object = JSObject::unchecked_cast(object);
        let names = object.map(&self.heap).own_property_names(&self.heap);
        let array = self.new_js_array(names.len() as i32);
        let scope = self.handle_scope();
        let array_handle = self.create_object_handle(array.object());
        for (index, name) in names.iter().enumerate() {
            let name_handle = self.create_handle(*name);
            let array =
                crate::objects::js_array::JSArray::unchecked_cast(array_handle.object(self));
            let name = name_handle.get(self);
            self.add_data_element(array.as_js_object(), index as u32, name);
        }
        let result = array_handle.get(self);
        self.close_handle_scope(scope);
        Ok(result)
    }

    fn as_js_array(&self, receiver: Tagged) -> Option<crate::objects::js_array::JSArray> {
        let object = receiver.to_strong_heap_object()?;
        if self.heap.instance_type(object) != crate::objects::instance_type::JS_ARRAY_TYPE {
            return None;
        }
        Some(crate::objects::js_array::JSArray::unchecked_cast(object))
    }

    fn not_an_array(&mut self, name: &str) -> ExecutionResult {
        let message = self
            .new_string(&format!("Array.prototype.{name} called on a non-array"))
            .tagged();
        Err(Thrown(message))
    }

    fn has_own_property(&mut self, receiver: Tagged, name: Tagged) -> Tagged {
        use crate::objects::js_objects::JSObject;
        let Some(object) = receiver.to_strong_heap_object() else {
            return self.false_value();
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(object)) {
            return self.false_value();
        }
        let object = JSObject::unchecked_cast(object);
        let key = self.key_to_name_for_builtin(name);
        let found = self.get_own_property(object, key).is_some();
        self.boolean_value(found)
    }

    fn key_to_name_for_builtin(&mut self, key: Tagged) -> Tagged {
        if let Some(string) = crate::objects::string::String::try_cast(&self.heap, key) {
            if string.is_internalized(&self.heap) {
                return key;
            }
        }
        let text = self.to_string(key);
        self.internalize_string(&text).tagged()
    }
}

#[cfg(test)]
mod tests {
    fn undefined_receiver() -> Tagged {
        Tagged::ZERO
    }

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
        let result = isolate
            .call_builtin(Builtin::MathSqrt, undefined_receiver(), &[four])
            .unwrap();
        assert_eq!(isolate.to_number(result), 2.0);

        let text = isolate.new_string("9").tagged();
        let result = isolate
            .call_builtin(Builtin::MathSqrt, undefined_receiver(), &[text])
            .unwrap();
        assert_eq!(isolate.to_number(result), 3.0);
    }

    #[test]
    fn max_with_no_arguments_is_negative_infinity() {
        let mut isolate = Isolate::new();
        let result = isolate
            .call_builtin(Builtin::MathMax, undefined_receiver(), &[])
            .unwrap();
        assert_eq!(isolate.to_number(result), f64::NEG_INFINITY);
        let result = isolate
            .call_builtin(Builtin::MathMin, undefined_receiver(), &[])
            .unwrap();
        assert_eq!(isolate.to_number(result), f64::INFINITY);
    }

    #[test]
    fn max_is_poisoned_by_nan() {
        let mut isolate = Isolate::new();
        let nan = isolate.new_heap_number(f64::NAN).tagged();
        let result = isolate
            .call_builtin(
                Builtin::MathMax,
                undefined_receiver(),
                &[Smi::from_int(1), nan],
            )
            .unwrap();
        assert!(isolate.to_number(result).is_nan());
    }

    #[test]
    fn a_builtin_can_throw() {
        let mut isolate = Isolate::new();
        let result = isolate.call_builtin(
            Builtin::StringLength,
            undefined_receiver(),
            &[Smi::from_int(1)],
        );
        assert!(result.is_err());
    }
}

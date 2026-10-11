use crate::execution::isolate::Isolate;
use crate::objects::js_array::JSArray;
use crate::objects::js_objects::JSObject;
use crate::objects::tagged::Tagged;

impl Isolate {
    /// `Accessors::ArrayLengthGetter`
    pub(crate) fn array_length_getter(&self, holder: JSArray) -> Tagged {
        holder.length(&self.heap)
    }

    /// `Accessors::FunctionPrototypeGetter`
    pub(crate) fn function_prototype_getter(
        &mut self,
        holder: crate::objects::js_function::JSFunction,
    ) -> Tagged {
        self.function_prototype_object(holder).tagged()
    }

    /// `Accessors::FunctionPrototypeSetter`
    pub(crate) fn function_prototype_setter(
        &mut self,
        holder: crate::objects::js_function::JSFunction,
        value: Tagged,
    ) {
        holder.set_prototype_or_initial_map(&mut self.heap, value);
    }

    /// `Accessors::MakeFunctionPrototypeInfo`
    pub(crate) fn function_prototype_if_requested(
        &self,
        object: Tagged,
        name: Tagged,
    ) -> Option<crate::objects::js_function::JSFunction> {
        let receiver = object.to_strong_heap_object()?;
        if self.heap.instance_type(receiver) != crate::objects::instance_type::JS_FUNCTION_TYPE {
            return None;
        }
        let string = crate::objects::string::String::try_cast(&self.heap, name)?;
        if string.to_rust_string(&self.heap) != "prototype" {
            return None;
        }
        Some(crate::objects::js_function::JSFunction::unchecked_cast(
            receiver,
        ))
    }

    /// `Accessors::StringLengthGetter`
    pub(crate) fn string_length_getter(&self, holder: crate::objects::string::String) -> Tagged {
        crate::objects::smi::Smi::from_int(holder.length(&self.heap))
    }

    /// `Accessors::ArrayLengthSetter`
    pub(crate) fn array_length_setter(&mut self, object: JSObject, value: Tagged) {
        let array = JSArray::unchecked_cast(object.object());
        let Some(length) = self.to_array_length(value) else {
            return;
        };
        self.set_elements_length(array, length);
    }
}

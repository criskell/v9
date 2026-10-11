use crate::execution::isolate::Isolate;
use crate::objects::js_array::JSArray;
use crate::objects::js_objects::JSObject;
use crate::objects::tagged::Tagged;

impl Isolate {
    /// `Accessors::ArrayLengthGetter`
    pub(crate) fn array_length_getter(&self, holder: JSArray) -> Tagged {
        holder.length(&self.heap)
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

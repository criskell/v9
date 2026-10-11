use crate::execution::isolate::Isolate;
use crate::interpreter::interpreter::Thrown;
use crate::objects::js_objects::JSObject;
use crate::objects::property_details::{PropertyAttributes, DONT_DELETE, DONT_ENUM, NONE, READ_ONLY};
use crate::objects::tagged::Tagged;

/// `PropertyDescriptor`
#[derive(Default)]
pub struct PropertyDescriptor {
    value: Option<Tagged>,
    get: Option<Tagged>,
    set: Option<Tagged>,
    enumerable: Option<bool>,
    configurable: Option<bool>,
    writable: Option<bool>,
}

impl PropertyDescriptor {
    pub fn new() -> PropertyDescriptor {
        PropertyDescriptor::default()
    }

    /// `PropertyDescriptor::IsAccessorDescriptor`
    pub fn is_accessor_descriptor(&self) -> bool {
        self.get.is_some() || self.set.is_some()
    }

    /// `PropertyDescriptor::IsDataDescriptor`
    pub fn is_data_descriptor(&self) -> bool {
        self.value.is_some() || self.writable.is_some()
    }

    /// `PropertyDescriptor::ToAttributes`
    pub fn to_attributes(&self) -> PropertyAttributes {
        let mut attributes = NONE;
        if !self.enumerable.unwrap_or(false) {
            attributes |= DONT_ENUM;
        }
        if !self.configurable.unwrap_or(false) {
            attributes |= DONT_DELETE;
        }
        if self.is_data_descriptor() && !self.writable.unwrap_or(false) {
            attributes |= READ_ONLY;
        }
        attributes
    }

    pub fn get(&self) -> Option<Tagged> {
        self.get
    }

    pub fn set(&self) -> Option<Tagged> {
        self.set
    }

    pub fn value(&self) -> Option<Tagged> {
        self.value
    }
}

impl Isolate {
    /// `PropertyDescriptor::ToPropertyDescriptor`
    pub(crate) fn to_property_descriptor(
        &mut self,
        object: Tagged,
    ) -> Result<PropertyDescriptor, Thrown> {
        let Some(holder) = object.to_strong_heap_object() else {
            let message = self
                .new_string("Property description must be an object")
                .tagged();
            return Err(Thrown(message));
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(holder)) {
            let message = self
                .new_string("Property description must be an object")
                .tagged();
            return Err(Thrown(message));
        }
        let holder = JSObject::unchecked_cast(holder);

        let mut descriptor = PropertyDescriptor::new();
        descriptor.enumerable = self.descriptor_flag(holder, "enumerable");
        descriptor.configurable = self.descriptor_flag(holder, "configurable");
        descriptor.writable = self.descriptor_flag(holder, "writable");
        descriptor.value = self.descriptor_component(holder, "value");
        descriptor.get = self.descriptor_component(holder, "get");
        descriptor.set = self.descriptor_component(holder, "set");
        Ok(descriptor)
    }

    fn descriptor_flag(&mut self, holder: JSObject, name: &str) -> Option<bool> {
        let value = self.descriptor_component(holder, name)?;
        Some(self.to_boolean(value))
    }

    fn descriptor_component(&mut self, holder: JSObject, name: &str) -> Option<Tagged> {
        let name = self.internalize_string(name).tagged();
        self.get_own_property(holder, name)
    }
}

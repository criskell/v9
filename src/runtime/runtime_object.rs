use crate::execution::isolate::Isolate;
use crate::interpreter::interpreter::ExecutionResult;
use crate::objects::js_objects::JSObject;
use crate::objects::property_details::PropertyAttributes;
use crate::objects::tagged::Tagged;

/// `PropertyAttributesFromInt`
fn property_attributes_from_int(value: i32) -> PropertyAttributes {
    value as PropertyAttributes
}

impl Isolate {
    /// `Runtime_DefineAccessorPropertyUnchecked`
    pub(crate) fn runtime_define_accessor_property_unchecked(
        &mut self,
        args: &[Tagged],
    ) -> ExecutionResult {
        debug_assert_eq!(5, args.len());
        let obj = JSObject::unchecked_cast(
            args[0]
                .to_strong_heap_object()
                .expect("a JSObject receiver"),
        );
        let name = args[1];
        let getter = args[2];
        let setter = args[3];
        let attrs = property_attributes_from_int(crate::objects::smi::Smi::to_int(args[4]));

        self.define_accessor(obj, name, getter, setter, attrs);
        Ok(self.undefined_value())
    }
}

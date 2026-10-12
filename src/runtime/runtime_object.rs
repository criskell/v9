use crate::execution::isolate::Isolate;
use crate::interpreter::interpreter::ExecutionResult;
use crate::objects::js_objects::JSObject;
use crate::objects::property_details::PropertyAttributes;
use crate::objects::tagged::Tagged;

fn property_attributes_from_int(value: i32) -> PropertyAttributes {
    value as PropertyAttributes
}

impl Isolate {
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

    pub(crate) fn runtime_load_private_setter(&mut self, args: &[Tagged]) -> ExecutionResult {
        use crate::objects::r#struct::AccessorPair;
        debug_assert_eq!(1, args.len());
        let pair = AccessorPair::unchecked_cast(args[0].to_strong_heap_object().expect("a pair"));
        Ok(pair.setter(&self.heap))
    }

    pub(crate) fn runtime_load_private_getter(&mut self, args: &[Tagged]) -> ExecutionResult {
        use crate::objects::r#struct::AccessorPair;
        debug_assert_eq!(1, args.len());
        let pair = AccessorPair::unchecked_cast(args[0].to_strong_heap_object().expect("a pair"));
        Ok(pair.getter(&self.heap))
    }

    pub(crate) fn runtime_create_private_accessors(&mut self, args: &[Tagged]) -> ExecutionResult {
        debug_assert_eq!(2, args.len());
        let scope = self.handle_scope();
        let getter = self.create_handle(args[0]);
        let setter = self.create_handle(args[1]);
        let pair = self.new_accessor_pair();
        let getter = getter.get(self);
        let setter = setter.get(self);
        pair.set_components(&mut self.heap, getter, setter);
        let value = pair.tagged();
        self.close_handle_scope(scope);
        Ok(value)
    }

    pub(crate) fn runtime_to_fast_properties(&mut self, args: &[Tagged]) -> ExecutionResult {
        debug_assert_eq!(1, args.len());
        let scope = self.handle_scope();
        let object = self.create_handle(args[0]);
        let value = object.get(self);
        if let Some(heap_object) = value.to_strong_heap_object() {
            if crate::objects::instance_type::is_js_object(self.heap.instance_type(heap_object))
                && !self.is_global_object(JSObject::unchecked_cast(heap_object))
            {
                self.migrate_slow_to_fast(JSObject::unchecked_cast(heap_object), 0);
            }
        }
        let value = object.get(self);
        self.close_handle_scope(scope);
        Ok(value)
    }
}

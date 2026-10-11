use crate::execution::isolate::Isolate;
use crate::interpreter::interpreter::{ExecutionResult, Thrown};
use crate::objects::js_function::JSFunction;
use crate::objects::js_objects::JSObject;
use crate::objects::literal_objects::{ClassBoilerplate, ValueKind};
use crate::objects::property_details::DONT_ENUM;
use crate::objects::tagged::Tagged;
use crate::handles::handles::Handle;

impl Isolate {
    /// `Runtime_DefineClass`
    pub(crate) fn runtime_define_class(&mut self, args: &[Tagged]) -> ExecutionResult {
        debug_assert!(
            ClassBoilerplate::K_FIRST_DYNAMIC_ARGUMENT_INDEX as usize <= args.len()
        );
        let scope = self.handle_scope();
        let mut arguments = Vec::with_capacity(args.len());
        for value in args {
            arguments.push(self.create_handle(*value));
        }
        let result = self.define_class(&arguments);
        self.close_handle_scope(scope);
        result
    }

    /// `DefineClass`
    fn define_class(&mut self, args: &[Handle]) -> ExecutionResult {
        let super_class = args[ClassBoilerplate::K_PROTOTYPE_ARGUMENT_INDEX as usize].get(self);
        if !self.heap.is_the_hole(super_class) {
            let message = self.new_string("extends is not implemented").tagged();
            return Err(Thrown(message));
        }

        let prototype = self.create_class_prototype();
        let prototype = self.create_object_handle(prototype.object());

        self.init_class_constructor(args, prototype)?;
        self.init_class_prototype(args, prototype)?;
        Ok(prototype.get(self))
    }

    /// `CreateClassPrototype`
    fn create_class_prototype(&mut self) -> JSObject {
        self.new_js_object()
    }

    /// `InitClassConstructor`
    fn init_class_constructor(&mut self, args: &[Handle], prototype: Handle) -> ExecutionResult {
        let constructor = args[ClassBoilerplate::K_CONSTRUCTOR_ARGUMENT_INDEX as usize];
        let prototype_value = prototype.get(self);
        let function = JSFunction::unchecked_cast(constructor.object(self));
        function.set_prototype_or_initial_map(&mut self.heap, prototype_value);

        self.add_descriptors_by_template(args, constructor, true)
    }

    /// `InitClassPrototype`
    fn init_class_prototype(&mut self, args: &[Handle], prototype: Handle) -> ExecutionResult {
        let constructor_value =
            args[ClassBoilerplate::K_CONSTRUCTOR_ARGUMENT_INDEX as usize].get(self);
        let holder = JSObject::unchecked_cast(prototype.object(self));
        let name = self.internalize_string("constructor").tagged();
        self.add_property_with_attributes(holder, name, constructor_value, DONT_ENUM);

        self.add_descriptors_by_template(args, prototype, false)
    }

    /// `AddDescriptorsByTemplate`
    fn add_descriptors_by_template(
        &mut self,
        args: &[Handle],
        receiver: Handle,
        is_static: bool,
    ) -> ExecutionResult {
        let boilerplate = ClassBoilerplate::unchecked_cast(args[0].object(self));
        let number_of_members = boilerplate.number_of_members(&self.heap);

        for member in 0..number_of_members {
            let boilerplate = ClassBoilerplate::unchecked_cast(args[0].object(self));
            if boilerplate.is_static_at(&self.heap, member) != is_static {
                continue;
            }
            let value_kind = boilerplate.value_kind_at(&self.heap, member);
            let name = boilerplate.key_at(&self.heap, member);
            let name = self.create_handle(name);
            let value = args
                [ClassBoilerplate::K_FIRST_DYNAMIC_ARGUMENT_INDEX as usize + member as usize];

            let holder = JSObject::unchecked_cast(receiver.object(self));
            let name_value = name.get(self);
            let value_value = value.get(self);
            match value_kind {
                ValueKind::ConstantKind => {
                    self.add_property_with_attributes(
                        holder,
                        name_value,
                        value_value,
                        DONT_ENUM,
                    );
                }
                ValueKind::GetterKind => {
                    let null = self.null_value();
                    self.define_accessor(holder, name_value, value_value, null, DONT_ENUM);
                }
                ValueKind::SetterKind => {
                    let null = self.null_value();
                    self.define_accessor(holder, name_value, null, value_value, DONT_ENUM);
                }
            }
        }
        Ok(self.undefined_value())
    }
}

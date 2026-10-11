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
        let super_class = args[ClassBoilerplate::K_PROTOTYPE_ARGUMENT_INDEX as usize];
        let super_class_value = super_class.get(self);

        let prototype_parent;
        let constructor_parent;
        if self.heap.is_the_hole(super_class_value) {
            let initial = self.roots.object_prototype;
            prototype_parent = Some(self.create_handle(initial));
            constructor_parent = None;
        } else if self.heap.is_null(super_class_value) {
            prototype_parent = None;
            constructor_parent = None;
        } else if self.is_constructor(super_class_value) {
            let maybe_prototype_parent = self.get_super_class_prototype(super_class)?;
            let Some(parent) = maybe_prototype_parent.to_strong_heap_object() else {
                let message = self
                    .new_string("Class extends value does not have valid prototype property")
                    .tagged();
                return Err(Thrown(message));
            };
            if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(parent)) {
                let message = self
                    .new_string("Class extends value does not have valid prototype property")
                    .tagged();
                return Err(Thrown(message));
            }
            prototype_parent = Some(self.create_handle(maybe_prototype_parent));
            constructor_parent = Some(super_class);
        } else {
            let message = self
                .new_string("Class extends value is not a constructor or null")
                .tagged();
            return Err(Thrown(message));
        }

        let prototype = self.create_class_prototype();
        let prototype = self.create_object_handle(prototype.object());

        self.init_class_constructor(args, prototype, constructor_parent)?;
        self.init_class_prototype(args, prototype, prototype_parent)?;
        Ok(prototype.get(self))
    }

    /// `IsConstructor`
    fn is_constructor(&self, value: Tagged) -> bool {
        match value.to_strong_heap_object() {
            Some(object) => {
                self.heap.instance_type(object) == crate::objects::instance_type::JS_FUNCTION_TYPE
            }
            None => false,
        }
    }

    fn get_super_class_prototype(&mut self, super_class: Handle) -> Result<Tagged, Thrown> {
        let function = JSFunction::unchecked_cast(super_class.object(self));
        let prototype = self.function_prototype_object(function);
        Ok(prototype.tagged())
    }

    /// `CreateClassPrototype`
    fn create_class_prototype(&mut self) -> JSObject {
        self.new_js_object()
    }

    /// `InitClassConstructor`
    fn init_class_constructor(
        &mut self,
        args: &[Handle],
        prototype: Handle,
        constructor_parent: Option<Handle>,
    ) -> ExecutionResult {
        let constructor = args[ClassBoilerplate::K_CONSTRUCTOR_ARGUMENT_INDEX as usize];

        let function = JSFunction::unchecked_cast(constructor.object(self));
        let map = crate::objects::map::Map::unchecked_cast(self.heap.map(function.object()));
        let map = self.copy_map_shell(map);
        let map = self.create_object_handle(map.object());
        if let Some(parent) = constructor_parent {
            let parent_value = parent.get(self);
            let map = crate::objects::map::Map::unchecked_cast(map.object(self));
            map.set_prototype(&mut self.heap, parent_value);
        }
        let map_value = map.object(self);
        let function = JSFunction::unchecked_cast(constructor.object(self));
        self.heap.set_map(function.object(), map_value);

        let prototype_value = prototype.get(self);
        let function = JSFunction::unchecked_cast(constructor.object(self));
        function.set_prototype_or_initial_map(&mut self.heap, prototype_value);

        self.add_descriptors_by_template(args, constructor, true)
    }

    /// `InitClassPrototype`
    fn init_class_prototype(
        &mut self,
        args: &[Handle],
        prototype: Handle,
        prototype_parent: Option<Handle>,
    ) -> ExecutionResult {
        let parent_value = match prototype_parent {
            Some(parent) => parent.get(self),
            None => self.null_value(),
        };
        let holder = JSObject::unchecked_cast(prototype.object(self));
        let map = crate::objects::map::Map::unchecked_cast(self.heap.map(holder.object()));
        let map = self.copy_map_shell(map);
        let map = self.create_object_handle(map.object());
        let map_object = crate::objects::map::Map::unchecked_cast(map.object(self));
        map_object.set_prototype(&mut self.heap, parent_value);
        let map_value = map.object(self);
        let holder = JSObject::unchecked_cast(prototype.object(self));
        self.heap.set_map(holder.object(), map_value);

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

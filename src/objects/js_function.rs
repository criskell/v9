use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::js_objects::JSObject;
use crate::objects::tagged::Tagged;

object_type!(JSFunction);

impl JSFunction {
    pub const K_PROPERTIES_OR_HASH_OFFSET: i32 = JSObject::K_PROPERTIES_OR_HASH_OFFSET;
    pub const K_ELEMENTS_OFFSET: i32 = JSObject::K_ELEMENTS_OFFSET;
    pub const K_SHARED_FUNCTION_INFO_OFFSET: i32 = JSObject::K_HEADER_SIZE;
    pub const K_CONTEXT_OFFSET: i32 = Self::K_SHARED_FUNCTION_INFO_OFFSET + K_TAGGED_SIZE;
    pub const K_FEEDBACK_VECTOR_OFFSET: i32 = Self::K_CONTEXT_OFFSET + K_TAGGED_SIZE;
    pub const K_PROTOTYPE_OR_INITIAL_MAP_OFFSET: i32 =
        Self::K_FEEDBACK_VECTOR_OFFSET + K_TAGGED_SIZE;
    pub const K_SIZE: i32 = Self::K_PROTOTYPE_OR_INITIAL_MAP_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub fn as_js_object(self) -> JSObject {
        JSObject::unchecked_cast(self.object())
    }

    #[inline]
    pub fn raw_shared_function_info(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET))
    }

    /// `JSFunction::shared`
    /// `JSFunction::prototype_or_initial_map`
    #[inline]
    pub fn prototype_or_initial_map(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_PROTOTYPE_OR_INITIAL_MAP_OFFSET))
    }

    #[inline]
    pub fn set_prototype_or_initial_map(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_PROTOTYPE_OR_INITIAL_MAP_OFFSET), value);
    }

    #[inline]
    pub fn prototype_or_initial_map_address(self) -> crate::objects::tagged::Address {
        self.field(Self::K_PROTOTYPE_OR_INITIAL_MAP_OFFSET)
    }

    /// `JSFunction::has_initial_map`
    #[inline]
    pub fn has_initial_map(self, heap: &Heap) -> bool {
        match self.prototype_or_initial_map(heap).to_strong_heap_object() {
            Some(object) => heap.instance_type(object) == crate::objects::instance_type::MAP_TYPE,
            None => false,
        }
    }

    /// `JSFunction::initial_map`
    #[inline]
    pub fn initial_map(self, heap: &Heap) -> crate::objects::map::Map {
        debug_assert!(self.has_initial_map(heap));
        crate::objects::map::Map::unchecked_cast(
            self.prototype_or_initial_map(heap)
                .to_strong_heap_object()
                .unwrap(),
        )
    }

    #[inline]
    pub fn shared(self, heap: &Heap) -> crate::objects::shared_function_info::SharedFunctionInfo {
        crate::objects::shared_function_info::SharedFunctionInfo::unchecked_cast(
            self.raw_shared_function_info(heap)
                .to_strong_heap_object()
                .expect("a JSFunction always has a SharedFunctionInfo"),
        )
    }

    #[inline]
    pub fn set_shared_function_info(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET), value);
    }

    /// `JSFunction::context`
    #[inline]
    pub fn context(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_CONTEXT_OFFSET))
    }

    #[inline]
    pub fn set_context(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_CONTEXT_OFFSET), value);
    }

    #[inline]
    pub fn feedback_vector(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_FEEDBACK_VECTOR_OFFSET))
    }

    #[inline]
    pub fn set_feedback_vector(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_FEEDBACK_VECTOR_OFFSET), value);
    }
}

impl crate::execution::isolate::Isolate {
    /// `JSFunction::GetPrototype`
    pub(crate) fn function_prototype_object(
        &mut self,
        function: JSFunction,
    ) -> crate::objects::js_objects::JSObject {
        use crate::objects::js_objects::JSObject;
        let slot = function.prototype_or_initial_map(&self.heap);
        if let Some(object) = slot.to_strong_heap_object() {
            let instance_type = self.heap.instance_type(object);
            if instance_type == crate::objects::instance_type::MAP_TYPE {
                let map = crate::objects::map::Map::unchecked_cast(object);
                return JSObject::unchecked_cast(
                    map.prototype(&self.heap)
                        .to_strong_heap_object()
                        .expect("an initial map has a prototype"),
                );
            }
            if crate::objects::instance_type::is_js_receiver(instance_type) {
                return JSObject::unchecked_cast(object);
            }
        }

        let scope = self.handle_scope();
        let function_handle = self.create_object_handle(function.object());
        let prototype = self.new_js_object();
        let prototype_handle = self.create_object_handle(prototype.object());
        let function = JSFunction::unchecked_cast(function_handle.object(self));
        let prototype_value = prototype_handle.get(self);
        function.set_prototype_or_initial_map(&mut self.heap, prototype_value);
        let prototype = JSObject::unchecked_cast(prototype_handle.object(self));
        self.close_handle_scope(scope);
        prototype
    }

    /// `JSFunction::EnsureHasInitialMap`
    pub(crate) fn ensure_has_initial_map(
        &mut self,
        function: JSFunction,
    ) -> crate::objects::map::Map {
        use crate::objects::map::Map;
        if function.has_initial_map(&self.heap) {
            return function.initial_map(&self.heap);
        }

        let scope = self.handle_scope();
        let function_handle = self.create_object_handle(function.object());
        let prototype = self.function_prototype_object(function);
        let prototype_handle = self.create_object_handle(prototype.object());

        let map = self.new_js_object_map_for_constructor();
        let map_handle = self.create_object_handle(map.object());
        let prototype_value = prototype_handle.get(self);
        let map = Map::unchecked_cast(map_handle.object(self));
        map.set_prototype(&mut self.heap, prototype_value);

        let function = JSFunction::unchecked_cast(function_handle.object(self));
        let map_value = map_handle.get(self);
        function.set_prototype_or_initial_map(&mut self.heap, map_value);
        let map = Map::unchecked_cast(map_handle.object(self));
        self.close_handle_scope(scope);
        map
    }

    /// `Builtins::Construct`
    pub(crate) fn construct(
        &mut self,
        target: Tagged,
        args: &[Tagged],
    ) -> crate::interpreter::interpreter::ExecutionResult {
        use crate::interpreter::interpreter::Thrown;

        let Some(object) = target.to_strong_heap_object() else {
            let message = self.not_a_constructor_message(target);
            return Err(Thrown(message));
        };
        if self.heap.instance_type(object) != crate::objects::instance_type::JS_FUNCTION_TYPE {
            let message = self.not_a_constructor_message(target);
            return Err(Thrown(message));
        }
        let function = JSFunction::unchecked_cast(object);

        let scope = self.handle_scope();
        let function_handle = self.create_object_handle(function.object());
        let arg_handles: Vec<crate::handles::handles::Handle> = args
            .iter()
            .map(|value| self.create_handle(*value))
            .collect();

        let function = JSFunction::unchecked_cast(function_handle.object(self));
        let map = self.ensure_has_initial_map(function);
        let receiver = self.new_js_object_from_map(map);
        let receiver_handle = self.create_object_handle(receiver.object());

        let arguments: Vec<Tagged> = arg_handles.iter().map(|handle| handle.get(self)).collect();
        let function = JSFunction::unchecked_cast(function_handle.object(self));
        let receiver_value = receiver_handle.get(self);
        let result = self.execute(function, receiver_value, &arguments);

        let answer = match result {
            Ok(value) => match value.to_strong_heap_object() {
                Some(returned)
                    if crate::objects::instance_type::is_js_receiver(
                        self.heap.instance_type(returned),
                    ) =>
                {
                    Ok(value)
                }
                _ => Ok(receiver_handle.get(self)),
            },
            Err(thrown) => Err(thrown),
        };
        self.close_handle_scope(scope);
        answer
    }

    pub(crate) fn not_a_constructor_message(&mut self, target: Tagged) -> Tagged {
        let text = self.to_string(target);
        self.new_string(&format!("{text} is not a constructor"))
            .tagged()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_own_fields_come_after_the_js_object_header() {
        assert_eq!(
            JSFunction::K_SHARED_FUNCTION_INFO_OFFSET,
            JSObject::K_HEADER_SIZE
        );
        assert_eq!(JSFunction::K_SHARED_FUNCTION_INFO_OFFSET, 24);
        assert_eq!(JSFunction::K_CONTEXT_OFFSET, 32);
        assert_eq!(JSFunction::K_FEEDBACK_VECTOR_OFFSET, 40);
        assert_eq!(JSFunction::K_PROTOTYPE_OR_INITIAL_MAP_OFFSET, 48);
        assert_eq!(JSFunction::K_SIZE, 56);
        assert_eq!(
            JSFunction::K_PROPERTIES_OR_HASH_OFFSET,
            JSObject::K_PROPERTIES_OR_HASH_OFFSET
        );
        assert_eq!(JSFunction::K_ELEMENTS_OFFSET, JSObject::K_ELEMENTS_OFFSET);
    }
}

impl crate::execution::isolate::Isolate {
    /// `GetSuperConstructor`
    pub(crate) fn get_super_constructor(
        &mut self,
        active_function: Tagged,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        use crate::interpreter::interpreter::Thrown;
        let Some(object) = active_function.to_strong_heap_object() else {
            let message = self.not_a_constructor_message(active_function);
            return Err(Thrown(message));
        };
        let map = self.heap.map(object);
        Ok(crate::objects::map::Map::unchecked_cast(map).prototype(&self.heap))
    }
}

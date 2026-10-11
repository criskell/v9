use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::js_objects::JSObject;
use crate::objects::tagged::Tagged;

object_type!(JSFunction);

/// `JSFunction::PrototypeOrInitialMapData`
#[derive(Default)]
pub struct PrototypeOrInitialMapData {
    pub instance_prototype: Tagged,
    pub non_instance_prototype: Tagged,
    pub initial_map: Option<crate::objects::map::Map>,
    pub has_instance_prototype: bool,
    pub has_non_instance_prototype: bool,
    pub has_initial_map: bool,
}

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

    /// `JSFunction::TryGetPrototypeOrInitialMap`
    pub fn try_get_prototype_or_initial_map(
        self,
        heap: &Heap,
    ) -> Option<PrototypeOrInitialMapData> {
        use crate::objects::instance_type as it;
        use crate::objects::r#struct::Tuple2;

        let mut proto_or_map = self.prototype_or_initial_map(heap);
        if heap.is_the_hole(proto_or_map) {
            return None;
        }
        let mut out = PrototypeOrInitialMapData::default();
        let Some(object) = proto_or_map.to_strong_heap_object() else {
            return None;
        };
        if heap.instance_type(object) == it::TUPLE2_TYPE {
            let tuple = Tuple2::unchecked_cast(object);
            out.has_non_instance_prototype = true;
            out.non_instance_prototype = tuple.value2(heap);
            proto_or_map = tuple.value1(heap);
        }
        let Some(object) = proto_or_map.to_strong_heap_object() else {
            return Some(out);
        };
        if heap.instance_type(object) == it::MAP_TYPE {
            let initial_map = crate::objects::map::Map::unchecked_cast(object);
            out.instance_prototype = initial_map.prototype(heap);
            out.has_initial_map = true;
            out.initial_map = Some(initial_map);
            out.has_instance_prototype = !heap.is_null(out.instance_prototype);
            return Some(out);
        }
        out.instance_prototype = proto_or_map;
        out.has_instance_prototype = true;
        Some(out)
    }

    /// `JSFunction::has_initial_map`
    #[inline]
    pub fn has_initial_map(self, heap: &Heap) -> bool {
        match self.try_get_prototype_or_initial_map(heap) {
            Some(data) => data.has_initial_map,
            None => false,
        }
    }

    /// `JSFunction::initial_map`
    #[inline]
    pub fn initial_map(self, heap: &Heap) -> crate::objects::map::Map {
        self.try_get_prototype_or_initial_map(heap)
            .and_then(|data| data.initial_map)
            .expect("an initial map")
    }

    /// `JSFunction::has_prototype`
    #[inline]
    pub fn has_prototype(self, heap: &Heap) -> bool {
        self.try_get_prototype_or_initial_map(heap).is_some()
    }

    /// `JSFunction::has_non_instance_prototype`
    #[inline]
    pub fn has_non_instance_prototype(self, heap: &Heap) -> bool {
        match self.try_get_prototype_or_initial_map(heap) {
            Some(data) => data.has_non_instance_prototype,
            None => false,
        }
    }

    /// `JSFunction::prototype`
    #[inline]
    pub fn prototype(self, heap: &Heap) -> Tagged {
        let data = self
            .try_get_prototype_or_initial_map(heap)
            .expect("a prototype");
        if data.has_non_instance_prototype {
            return data.non_instance_prototype;
        }
        data.instance_prototype
    }

    /// `JSFunction::instance_prototype`
    #[inline]
    pub fn instance_prototype(self, heap: &Heap) -> Option<Tagged> {
        let data = self.try_get_prototype_or_initial_map(heap)?;
        data.has_instance_prototype
            .then_some(data.instance_prototype)
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
    /// `JSFunction::instance_prototype`
    pub(crate) fn function_prototype_object(
        &mut self,
        function: JSFunction,
    ) -> crate::objects::js_objects::JSObject {
        use crate::objects::js_objects::JSObject;
        if let Some(prototype) = function.instance_prototype(&self.heap) {
            if let Some(object) = prototype.to_strong_heap_object() {
                if crate::objects::instance_type::is_js_receiver(
                    self.heap.instance_type(object),
                ) {
                    return JSObject::unchecked_cast(object);
                }
            }
        }

        let scope = self.handle_scope();
        let function_handle = self.create_object_handle(function.object());
        let prototype = self.new_function_prototype();
        let prototype_handle = self.create_object_handle(prototype.object());
        let function = JSFunction::unchecked_cast(function_handle.object(self));
        let prototype_value = prototype_handle.get(self);
        self.set_function_prototype(function, prototype_value);
        let prototype = JSObject::unchecked_cast(prototype_handle.object(self));
        self.close_handle_scope(scope);
        prototype
    }

    /// `Factory::NewFunctionPrototype`
    fn new_function_prototype(&mut self) -> crate::objects::js_objects::JSObject {
        self.new_js_object()
    }

    /// `JSFunction::SetPrototype`
    pub(crate) fn set_function_prototype(&mut self, function: JSFunction, value: Tagged) {
        use crate::objects::r#struct::Tuple2;

        let is_receiver = match value.to_strong_heap_object() {
            Some(object) => {
                crate::objects::instance_type::is_js_receiver(self.heap.instance_type(object))
            }
            None => false,
        };

        if !is_receiver {
            let data = function.try_get_prototype_or_initial_map(&self.heap);
            if let Some(data) = &data {
                if data.has_non_instance_prototype {
                    let tuple = Tuple2::unchecked_cast(
                        function
                            .prototype_or_initial_map(&self.heap)
                            .to_strong_heap_object()
                            .expect("a Tuple2"),
                    );
                    tuple.set_value2(&mut self.heap, value);
                    return;
                }
            }
            let old_initial_map = data.and_then(|data| data.initial_map);
            let new_instance_prototype = self.roots.object_prototype;
            let new_prototype_or_initial_map = match old_initial_map {
                Some(map) if map.prototype(&self.heap) == new_instance_prototype => map.tagged(),
                _ => new_instance_prototype,
            };

            let scope = self.handle_scope();
            let function_handle = self.create_object_handle(function.object());
            let tuple = self.new_tuple2(new_prototype_or_initial_map, value);
            let tuple = self.create_object_handle(tuple.object());
            let function = JSFunction::unchecked_cast(function_handle.object(self));
            let tuple_value = tuple.get(self);
            function.set_prototype_or_initial_map(&mut self.heap, tuple_value);
            self.close_handle_scope(scope);
            return;
        }

        if function.has_initial_map(&self.heap) {
            let map = function.initial_map(&self.heap);
            if map.prototype(&self.heap) == value {
                function.set_prototype_or_initial_map(&mut self.heap, map.tagged());
                return;
            }
        }
        function.set_prototype_or_initial_map(&mut self.heap, value);
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
        let function = JSFunction::unchecked_cast(function_handle.object(self));
        let prototype_value = prototype_handle.get(self);
        let map = Map::unchecked_cast(map_handle.object(self));
        self.set_initial_map(function, map, prototype_value);
        let map = Map::unchecked_cast(map_handle.object(self));
        self.close_handle_scope(scope);
        map
    }

    /// `JSFunction::SetInitialMap`
    pub(crate) fn set_initial_map(
        &mut self,
        function: JSFunction,
        initial_map: crate::objects::map::Map,
        prototype: Tagged,
    ) {
        use crate::objects::r#struct::Tuple2;
        if initial_map.prototype(&self.heap) != prototype {
            initial_map.set_prototype(&mut self.heap, prototype);
        }

        let data = function.try_get_prototype_or_initial_map(&self.heap);
        let non_instance = data
            .map(|data| data.has_non_instance_prototype)
            .unwrap_or(false);
        if non_instance {
            let tuple = Tuple2::unchecked_cast(
                function
                    .prototype_or_initial_map(&self.heap)
                    .to_strong_heap_object()
                    .expect("a Tuple2"),
            );
            tuple.set_value1(&mut self.heap, initial_map.tagged());
        } else {
            function.set_prototype_or_initial_map(&mut self.heap, initial_map.tagged());
        }
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

use crate::execution::isolate::Isolate;
use crate::objects::fixed_array::FixedArray;
use crate::objects::js_objects::JSObject;
use crate::objects::tagged::Tagged;

impl Isolate {
    pub(crate) fn get_keys(&mut self, receiver: JSObject) -> Vec<Tagged> {
        use crate::objects::elements::ElementsAccessor;

        let scope = self.handle_scope();
        let holder = self.create_object_handle(receiver.object());
        let mut keys: Vec<Tagged> = Vec::new();
        loop {
            let current = JSObject::unchecked_cast(holder.object(self));
            let kind = current.get_elements_kind(&self.heap);
            let accessor = ElementsAccessor::for_kind(kind);
            let capacity = accessor.get_capacity(&self.heap, current.elements(&self.heap));
            for index in 0..capacity {
                let current = JSObject::unchecked_cast(holder.object(self));
                let elements = current.elements(&self.heap);
                if !accessor.has_entry(&self.heap, elements, index) {
                    continue;
                }
                let name = self.internalize_string(&index.to_string()).tagged();
                if !keys.contains(&name) {
                    keys.push(name);
                }
            }

            let current = JSObject::unchecked_cast(holder.object(self));
            for name in self.own_enumerable_names(current) {
                if !keys.contains(&name) {
                    keys.push(name);
                }
            }

            let current = JSObject::unchecked_cast(holder.object(self));
            let prototype = current.map(&self.heap).prototype(&self.heap);
            let Some(next) = prototype.to_strong_heap_object() else {
                break;
            };
            if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(next)) {
                break;
            }
            self.handles[holder.index()] = next.tagged();
        }
        self.close_handle_scope(scope);
        keys
    }

    pub(crate) fn own_enumerable_names_for_keys(&self, holder: JSObject) -> Vec<Tagged> {
        self.own_enumerable_names(holder)
    }

    fn own_enumerable_names(&self, holder: JSObject) -> Vec<Tagged> {
        use crate::objects::hash_table::HashTableBase;

        if !holder.has_fast_properties(&self.heap) {
            let dictionary = holder.property_dictionary(&self.heap);
            let mut entries: Vec<(u32, Tagged)> = dictionary
                .iterate_entries(&self.heap)
                .iter()
                .map(|entry| {
                    (
                        dictionary.details_at(&self.heap, *entry).dictionary_index(),
                        dictionary.key_at(&self.heap, *entry),
                    )
                })
                .filter(|(_, key)| HashTableBase::is_key(&self.heap, *key))
                .filter(|(_, key)| !self.is_symbol(*key))
                .collect();
            entries.sort_by_key(|(index, _)| *index);
            return entries.into_iter().map(|(_, key)| key).collect();
        }
        let map = holder.map(&self.heap);
        let descriptors = map.instance_descriptors(&self.heap);
        (0..map.number_of_own_descriptors(&self.heap))
            .filter(|index| descriptors.get_details(&self.heap, *index).is_enumerable())
            .map(|index| descriptors.get_key(&self.heap, index))
            .filter(|key| !self.is_symbol(*key))
            .collect()
    }

    fn is_symbol(&self, key: Tagged) -> bool {
        match key.to_strong_heap_object() {
            Some(object) => {
                self.heap.instance_type(object) == crate::objects::instance_type::SYMBOL_TYPE
            }
            None => false,
        }
    }

    pub(crate) fn initialize_or_change_enum_cache(
        &mut self,
        map: crate::objects::map::Map,
        keys: FixedArray,
    ) {
        use crate::objects::descriptor_array::EnumCache;

        let descriptors = map.instance_descriptors(&self.heap);
        let existing = descriptors.enum_cache(&self.heap);
        if let Some(object) = existing.to_strong_heap_object() {
            if self.heap.instance_type(object) == crate::objects::instance_type::ENUM_CACHE_TYPE {
                let cache = EnumCache::unchecked_cast(object);
                cache.set_keys(&mut self.heap, keys.tagged());
                return;
            }
        }
        let scope = self.handle_scope();
        let keys_handle = self.create_object_handle(keys.object());
        let map_handle = self.create_object_handle(map.object());
        let cache = self.new_enum_cache();
        let keys = keys_handle.get(self);
        cache.set_keys(&mut self.heap, keys);
        let empty = self.roots.empty_fixed_array;
        cache.set_indices(&mut self.heap, empty);
        let map = crate::objects::map::Map::unchecked_cast(map_handle.object(self));
        let descriptors = map.instance_descriptors(&self.heap);
        let cache_value = cache.tagged();
        descriptors.set_enum_cache(&mut self.heap, cache_value);
        self.close_handle_scope(scope);
    }

    fn new_enum_cache(&mut self) -> crate::objects::descriptor_array::EnumCache {
        use crate::objects::descriptor_array::EnumCache;
        let object = self.allocate_raw_with_retry_or_fail(
            EnumCache::K_SIZE as usize,
            crate::heap::heap::AllocationType::Old,
        );
        let map = self.roots.enum_cache_map.to_strong_heap_object().unwrap();
        self.heap.set_map(object, map);
        let cache = EnumCache::unchecked_cast(object);
        let empty = self.roots.empty_fixed_array;
        cache.set_keys(&mut self.heap, empty);
        cache.set_indices(&mut self.heap, empty);
        cache
    }
}

impl Isolate {
    pub(crate) fn for_in_enumerate(
        &mut self,
        receiver: Tagged,
    ) -> Result<Tagged, crate::interpreter::interpreter::Thrown> {
        use crate::interpreter::interpreter::Thrown;
        let Some(object) = receiver.to_strong_heap_object() else {
            let message = self.new_string("for...in needs an object").tagged();
            return Err(Thrown(message));
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(object)) {
            return Ok(self.roots.empty_fixed_array);
        }
        let scope = self.handle_scope();
        let receiver = self.create_object_handle(object);
        let object = JSObject::unchecked_cast(receiver.object(self));
        let keys = self.get_keys(object);
        let array = self.new_fixed_array(keys.len() as i32);
        let array_handle = self.create_object_handle(array.object());
        for (index, key) in keys.iter().enumerate() {
            let array = FixedArray::unchecked_cast(array_handle.object(self));
            array.set(&mut self.heap, index as i32, *key);
        }

        let object = JSObject::unchecked_cast(receiver.object(self));
        if object.has_fast_properties(&self.heap) {
            let map = object.map(&self.heap);
            if map.number_of_own_descriptors(&self.heap) > 0 {
                let array = FixedArray::unchecked_cast(array_handle.object(self));
                self.initialize_or_change_enum_cache(map, array);
            }
        }
        let result = array_handle.get(self);
        self.close_handle_scope(scope);
        Ok(result)
    }

    pub(crate) fn for_in_prepare(&mut self, enumerable: Tagged) -> (Tagged, Tagged, i32) {
        let Some(object) = enumerable.to_strong_heap_object() else {
            let empty = self.roots.empty_fixed_array;
            return (self.undefined_value(), empty, 0);
        };
        let array = FixedArray::unchecked_cast(object);
        let length = array.length(&self.heap);
        (self.undefined_value(), array.tagged(), length)
    }

    pub(crate) fn for_in_next(
        &mut self,
        receiver: Tagged,
        _cache_type: Tagged,
        cache_array: Tagged,
        index: i32,
    ) -> Result<Tagged, crate::interpreter::interpreter::Thrown> {
        let Some(array) = cache_array.to_strong_heap_object() else {
            return Ok(self.undefined_value());
        };
        let array = FixedArray::unchecked_cast(array);
        if index >= array.length(&self.heap) {
            return Ok(self.undefined_value());
        }
        let key = array.get(&self.heap, index);

        let Some(object) = receiver.to_strong_heap_object() else {
            return Ok(key);
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(object)) {
            return Ok(key);
        }
        let object = JSObject::unchecked_cast(object);
        if self.has_property_for_in(object, key) {
            Ok(key)
        } else {
            Ok(self.undefined_value())
        }
    }

    fn has_property_for_in(&mut self, object: JSObject, key: Tagged) -> bool {
        use crate::objects::elements::ElementsAccessor;
        if let Some(index) = self.string_as_array_index(key) {
            let kind = object.get_elements_kind(&self.heap);
            if ElementsAccessor::for_kind(kind).has_element(&self.heap, object, index) {
                return true;
            }
        }
        let it = crate::objects::lookup::LookupIterator::new(self, object.tagged(), key);
        it.is_found()
    }
}

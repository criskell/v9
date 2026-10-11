use crate::common::globals::StoreOrigin;
use crate::execution::isolate::Isolate;
use crate::objects::hash_table::K_NOT_FOUND;
use crate::objects::instance_type;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::property_details::{PropertyDetails, Representation};
use crate::objects::tagged::Tagged;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PropertyLocationInfo {
    pub holder_map: Map,
    pub holder: JSObject,
    pub details: PropertyDetails,
    pub dictionary_entry: i32,
}

impl Isolate {
    // -----------------------------------------------------------------------
    // Transitions
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Lookup
    // -----------------------------------------------------------------------

    pub fn lookup_own_property(
        &self,
        object: JSObject,
        name: Tagged,
    ) -> Option<PropertyLocationInfo> {
        let map = object.map(&self.heap);
        if map.is_dictionary_map(&self.heap) {
            let dictionary = object.property_dictionary(&self.heap);
            let entry = dictionary.find_entry(&self.heap, name);
            if entry == K_NOT_FOUND {
                return None;
            }
            return Some(PropertyLocationInfo {
                holder_map: map,
                holder: object,
                details: dictionary.details_at(&self.heap, entry),
                dictionary_entry: entry,
            });
        }
        let details = map.find_property_details(&self.heap, name)?;
        Some(PropertyLocationInfo {
            holder_map: map,
            holder: object,
            details,
            dictionary_entry: K_NOT_FOUND,
        })
    }

    pub fn lookup_property(&self, object: JSObject, name: Tagged) -> Option<PropertyLocationInfo> {
        let mut holder = object;
        loop {
            if let Some(found) = self.lookup_own_property(holder, name) {
                return Some(found);
            }
            let prototype = holder.map(&self.heap).prototype(&self.heap);
            if prototype == self.roots.null_value || prototype == self.roots.undefined_value {
                return None;
            }
            let prototype_object = prototype.to_strong_heap_object()?;
            if !instance_type::is_js_receiver(self.heap.instance_type(prototype_object)) {
                return None;
            }
            holder = JSObject::unchecked_cast(prototype_object);
        }
    }

    pub fn get_property(&self, object: JSObject, name: Tagged) -> Tagged {
        if let Some(length) = self.array_length_if_requested(object, name) {
            return length;
        }
        match self.get_property_raw(object, name) {
            Some(value) => self.unwrap_cell(object, value),
            None => self.undefined_value(),
        }
    }

    pub fn get_own_property(&self, object: JSObject, name: Tagged) -> Option<Tagged> {
        let value = self.get_own_property_raw(object, name)?;
        Some(self.unwrap_cell(object, value))
    }

    pub(crate) fn get_property_raw(&self, object: JSObject, name: Tagged) -> Option<Tagged> {
        let found = self.lookup_property(object, name)?;
        Some(self.property_value_at(&found))
    }

    pub(crate) fn get_own_property_raw(&self, object: JSObject, name: Tagged) -> Option<Tagged> {
        let found = self.lookup_own_property(object, name)?;
        Some(self.property_value_at(&found))
    }

    /// `LookupIterator::GetDataValue`
    #[inline]
    pub(crate) fn property_value_at(&self, found: &PropertyLocationInfo) -> Tagged {
        if found.dictionary_entry != K_NOT_FOUND {
            return found
                .holder
                .property_dictionary(&self.heap)
                .value_at(&self.heap, found.dictionary_entry);
        }
        found
            .holder
            .fast_property_at(&self.heap, found.details.field_index() as i32)
    }

    pub(crate) fn array_length_if_requested(
        &self,
        object: JSObject,
        name: Tagged,
    ) -> Option<Tagged> {
        use crate::objects::instance_type::JS_ARRAY_TYPE;
        if self.heap.instance_type(object.object()) != JS_ARRAY_TYPE {
            return None;
        }
        let string = crate::objects::string::String::try_cast(&self.heap, name)?;
        if string.to_rust_string(&self.heap) != "length" {
            return None;
        }
        let array = crate::objects::js_array::JSArray::unchecked_cast(object.object());
        Some(self.array_length_getter(array))
    }

    #[inline]
    fn unwrap_cell(&self, holder: JSObject, value: Tagged) -> Tagged {
        if !self.is_global_object(holder) {
            return value;
        }
        match value.to_strong_heap_object() {
            Some(object)
                if self.heap.instance_type(object) == instance_type::PROPERTY_CELL_TYPE =>
            {
                crate::objects::property_cell::PropertyCell::unchecked_cast(object)
                    .value(&self.heap)
            }
            _ => value,
        }
    }

    pub fn set_property(&mut self, object: JSObject, name: Tagged, value: Tagged) -> bool {
        self.set_property_with_origin(object, name, value, StoreOrigin::Named)
    }

    /// `Object::SetProperty`
    pub fn set_property_with_origin(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        store_origin: StoreOrigin,
    ) -> bool {
        if self.array_length_if_requested(object, name).is_some() {
            self.array_length_setter(object, value);
            return true;
        }
        if self.is_global_object(object) {
            self.store_global(name, value);
            return true;
        }
        self.set_property_ignoring_cells(object, name, value, store_origin)
    }

    pub(crate) fn set_property_ignoring_cells(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        store_origin: StoreOrigin,
    ) -> bool {
        let map = object.map(&self.heap);

        if map.is_dictionary_map(&self.heap) {
            return self.set_dictionary_property(object, name, value);
        }

        if let Some(details) = map.find_property_details(&self.heap, name) {
            if details.is_read_only() {
                return false;
            }
            let field_index = details.field_index() as i32;
            object.set_fast_property_at(&mut self.heap, field_index, value);
            self.widen_field_representation(map, name, value);
            return true;
        }

        if !map.is_extensible(&self.heap) {
            return false;
        }

        if self.too_many_fast_properties(map, store_origin) {
            let scope = self.handle_scope();
            let object_handle = self.create_object_handle(object.object());
            let name_handle = self.create_handle(name);
            let value_handle = self.create_handle(value);
            self.normalize_properties(object);
            let object = JSObject::unchecked_cast(object_handle.object(self));
            let name = name_handle.get(self);
            let value = value_handle.get(self);
            let result = self.set_dictionary_property(object, name, value);
            self.close_handle_scope(scope);
            return result;
        }

        let representation = Representation::for_value(value);
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let name_handle = self.create_handle(name);
        let value_handle = self.create_handle(value);

        let new_map = self.transition_to_data_property(map, name, representation);
        let new_map_handle = self.create_object_handle(new_map.object());
        let name = name_handle.get(self);
        let field_index = new_map
            .find_property_details(&self.heap, name)
            .expect("the transition target must own the property")
            .field_index() as i32;

        let inobject = new_map.inobject_properties(&self.heap);
        if field_index >= inobject {
            let object = JSObject::unchecked_cast(object_handle.object(self));
            self.grow_property_array(object, field_index - inobject + 1);
        }

        let object = JSObject::unchecked_cast(object_handle.object(self));
        let new_map = Map::unchecked_cast(new_map_handle.object(self));
        let value = value_handle.get(self);
        self.heap.set_map(object.object(), new_map.object());
        object.set_fast_property_at(&mut self.heap, field_index, value);
        self.close_handle_scope(scope);
        true
    }

    /// `JSObject::NormalizeProperties`
    pub(crate) fn normalize_properties(&mut self, object: JSObject) {
        let map = object.map(&self.heap);
        if map.is_dictionary_map(&self.heap) {
            return;
        }
        let number_of_own = map.number_of_own_descriptors(&self.heap);
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let map_handle = self.create_object_handle(map.object());
        let mut dictionary = self.new_name_dictionary(number_of_own as u32);
        for index in 0..number_of_own {
            let map = Map::unchecked_cast(map_handle.object(self));
            let descriptors = map.instance_descriptors(&self.heap);
            let key = descriptors.get_key(&self.heap, index);
            let details = descriptors.get_details(&self.heap, index);
            let object = JSObject::unchecked_cast(object_handle.object(self));
            let value = object.fast_property_at(&self.heap, details.field_index() as i32);
            let dictionary_handle = self.create_object_handle(dictionary.object());
            let slow_details = PropertyDetails::new_dictionary(
                details.kind(),
                details.attributes(),
                crate::objects::property_details::PropertyConstness::Mutable,
                0,
            );
            let dictionary_now = crate::objects::dictionary::NameDictionary::unchecked_cast(
                dictionary_handle.object(self),
            );
            dictionary = self.name_dictionary_add(dictionary_now, key, value, slow_details);
        }
        let dictionary_handle = self.create_object_handle(dictionary.object());
        let map = Map::unchecked_cast(map_handle.object(self));
        let new_map = self.normalize_map(map);
        let object = JSObject::unchecked_cast(object_handle.object(self));
        self.heap.set_map(object.object(), new_map.object());
        let dictionary = dictionary_handle.get(self);
        object.set_raw_properties_or_hash(&mut self.heap, dictionary);
        self.close_handle_scope(scope);
    }

    pub(crate) fn set_dictionary_property(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
    ) -> bool {
        let dictionary = object.property_dictionary(&self.heap);
        let entry = dictionary.find_entry(&self.heap, name);
        if entry != K_NOT_FOUND {
            if dictionary.details_at(&self.heap, entry).is_read_only() {
                return false;
            }
            dictionary.value_at_put(&mut self.heap, entry, value);
            return true;
        }
        if !object.map(&self.heap).is_extensible(&self.heap) {
            return false;
        }
        let details = PropertyDetails::new_dictionary(
            crate::objects::property_details::PropertyKind::Data,
            crate::objects::property_details::NONE,
            crate::objects::property_details::PropertyConstness::Mutable,
            0,
        );
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let updated = self
            .name_dictionary_add(dictionary, name, value, details)
            .tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        object.set_raw_properties_or_hash(&mut self.heap, updated);
        self.close_handle_scope(scope);
        true
    }

    /// `JSReceiver::DeleteProperty`
    pub(crate) fn delete_property(&mut self, object: JSObject, name: Tagged) -> bool {
        if self.lookup_own_property(object, name).is_none() {
            return true;
        }
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let name_handle = self.create_handle(name);
        self.normalize_properties(object);
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let name = name_handle.get(self);
        self.close_handle_scope(scope);
        let dictionary = object.property_dictionary(&self.heap);
        let entry = dictionary.find_entry(&self.heap, name);
        if entry == K_NOT_FOUND {
            return true;
        }
        if !dictionary.details_at(&self.heap, entry).is_configurable() {
            return false;
        }
        let updated = self.name_dictionary_delete(dictionary, entry).tagged();
        object.set_raw_properties_or_hash(&mut self.heap, updated);
        true
    }

    /// `JSObject::AddProperty`
    pub(crate) fn add_property_with_attributes(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
    ) -> bool {
        let map = object.map(&self.heap);
        if map.is_dictionary_map(&self.heap) {
            return self.set_dictionary_property(object, name, value);
        }
        if map.find_property_details(&self.heap, name).is_some() {
            return self.set_property_ignoring_cells(object, name, value, StoreOrigin::Named);
        }

        let representation = Representation::for_value(value);
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let value_handle = self.create_handle(value);
        let name_handle = self.create_handle(name);

        let new_map =
            self.transition_to_data_property_with_attributes(map, name, representation, attributes);
        let new_map_handle = self.create_object_handle(new_map.object());
        let name = name_handle.get(self);
        let field_index = new_map
            .find_property_details(&self.heap, name)
            .expect("the transition target must own the property")
            .field_index() as i32;

        let inobject = new_map.inobject_properties(&self.heap);
        if field_index >= inobject {
            let object = JSObject::unchecked_cast(object_handle.object(self));
            self.grow_property_array(object, field_index - inobject + 1);
        }

        let object = JSObject::unchecked_cast(object_handle.object(self));
        let new_map = Map::unchecked_cast(new_map_handle.object(self));
        let value = value_handle.get(self);
        self.heap.set_map(object.object(), new_map.object());
        object.set_fast_property_at(&mut self.heap, field_index, value);
        self.close_handle_scope(scope);
        true
    }

    pub(crate) fn add_property_by_str_with_attributes(
        &mut self,
        object: JSObject,
        name: &str,
        value: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
    ) -> bool {
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let value_handle = self.create_handle(value);
        let key = self.internalize_string(name).tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let value = value_handle.get(self);
        let result = self.add_property_with_attributes(object, key, value, attributes);
        self.close_handle_scope(scope);
        result
    }

    pub fn set_property_by_str(&mut self, object: JSObject, name: &str, value: Tagged) -> bool {
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let value_handle = self.create_handle(value);
        let key = self.internalize_string(name).tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let value = value_handle.get(self);
        let result = self.set_property(object, key, value);
        self.close_handle_scope(scope);
        result
    }

    pub fn get_property_by_str(&mut self, object: JSObject, name: &str) -> Tagged {
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let key = self.internalize_string(name).tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let result = self.get_property(object, key);
        self.close_handle_scope(scope);
        result
    }
}

#[cfg(test)]
mod tests {
    #[derive(Copy, Clone)]
    struct Rooted(usize);

    impl Rooted {
        fn new(isolate: &mut Isolate, object: JSObject) -> Rooted {
            isolate.handles.push(object.tagged());
            Rooted(isolate.handles.len() - 1)
        }

        fn get(self, isolate: &Isolate) -> JSObject {
            JSObject::unchecked_cast(isolate.handles[self.0].to_strong_heap_object().unwrap())
        }
    }

    use super::*;
    use crate::objects::descriptor_array::K_NOT_FOUND;
    use crate::objects::heap_number::number_value;
    use crate::objects::smi::Smi;
    use crate::objects::transitions::TransitionsAccessor;

    #[test]
    fn adding_a_property_moves_the_object_to_a_new_map() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        let map_before = object.get(&isolate).map(&isolate.heap);

        assert!(isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1)));

        let map_after = object.get(&isolate).map(&isolate.heap);
        assert_ne!(map_before, map_after, "the shape changed");
        assert_eq!(map_before.number_of_own_descriptors(&isolate.heap), 0);
        assert_eq!(map_after.number_of_own_descriptors(&isolate.heap), 1);
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            1
        );
    }

    #[test]
    fn the_new_map_points_back_at_the_old_one() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        let map0 = object.get(&isolate).map(&isolate.heap);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        let map1 = object.get(&isolate).map(&isolate.heap);
        isolate.set_property_by_str(object.get(&isolate), "y", Smi::from_int(2));
        let map2 = object.get(&isolate).map(&isolate.heap);

        assert_eq!(map2.back_pointer(&isolate.heap), Some(map1));
        assert_eq!(map1.back_pointer(&isolate.heap), Some(map0));
        assert_eq!(map0.back_pointer(&isolate.heap), None, "the root map");
    }

    #[test]
    fn objects_built_the_same_way_share_a_map() {
        let mut isolate = Isolate::new();
        let a = isolate.new_js_object();
        let a = Rooted::new(&mut isolate, a);
        let b = isolate.new_js_object();
        let b = Rooted::new(&mut isolate, b);

        for rooted in [a, b] {
            let object = rooted.get(&isolate);
            isolate.set_property_by_str(object, "x", Smi::from_int(1));
            let object = rooted.get(&isolate);
            isolate.set_property_by_str(object, "y", Smi::from_int(2));
        }

        assert_eq!(
            a.get(&isolate).map(&isolate.heap),
            b.get(&isolate).map(&isolate.heap),
            "this is the entire point of hidden classes"
        );
    }

    #[test]
    fn property_order_produces_different_shapes() {
        let mut isolate = Isolate::new();
        let xy = isolate.new_js_object();
        let xy = Rooted::new(&mut isolate, xy);
        isolate.set_property_by_str(xy.get(&isolate), "x", Smi::from_int(1));
        isolate.set_property_by_str(xy.get(&isolate), "y", Smi::from_int(2));

        let yx = isolate.new_js_object();
        let yx = Rooted::new(&mut isolate, yx);
        isolate.set_property_by_str(yx.get(&isolate), "y", Smi::from_int(2));
        isolate.set_property_by_str(yx.get(&isolate), "x", Smi::from_int(1));

        assert_ne!(
            xy.get(&isolate).map(&isolate.heap),
            yx.get(&isolate).map(&isolate.heap),
            "{{x, y}} and {{y, x}} are different shapes -- the classic \
             performance footgun"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(xy.get(&isolate), "x")),
            1
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(yx.get(&isolate), "x")),
            1
        );
        let x_in_xy = xy.get(&isolate).map(&isolate.heap);
        let x_key = isolate.internalize_string("x").tagged();
        assert_eq!(
            x_in_xy
                .find_property_details(&isolate.heap, x_key)
                .unwrap()
                .field_index(),
            0
        );
        assert_eq!(
            yx.get(&isolate)
                .map(&isolate.heap)
                .find_property_details(&isolate.heap, x_key)
                .unwrap()
                .field_index(),
            1
        );
    }

    #[test]
    fn a_shared_shape_is_reached_through_a_recorded_transition() {
        let mut isolate = Isolate::new();
        let map0 = isolate.roots.js_object_map();

        let a = isolate.new_js_object();
        let a = Rooted::new(&mut isolate, a);
        isolate.set_property_by_str(a.get(&isolate), "x", Smi::from_int(1));
        let map1 = a.get(&isolate).map(&isolate.heap);

        let x = isolate.internalize_string("x").tagged();
        assert_eq!(
            TransitionsAccessor::search_transition(&isolate.heap, map0, x),
            Some(map1),
            "the edge must be cached on the source map"
        );

        let b = isolate.new_js_object();
        let b = Rooted::new(&mut isolate, b);
        isolate.set_property_by_str(b.get(&isolate), "x", Smi::from_int(9));
        assert_eq!(b.get(&isolate).map(&isolate.heap), map1);
    }

    #[test]
    fn a_branching_shape_graph_keeps_both_paths() {
        let mut isolate = Isolate::new();
        let base = isolate.new_js_object();
        let base = Rooted::new(&mut isolate, base);
        isolate.set_property_by_str(base.get(&isolate), "x", Smi::from_int(1));
        let map1 = base.get(&isolate).map(&isolate.heap);

        let left = isolate.new_js_object();
        let left = Rooted::new(&mut isolate, left);
        isolate.set_property_by_str(left.get(&isolate), "x", Smi::from_int(1));
        isolate.set_property_by_str(left.get(&isolate), "y", Smi::from_int(2));

        let right = isolate.new_js_object();
        let right = Rooted::new(&mut isolate, right);
        isolate.set_property_by_str(right.get(&isolate), "x", Smi::from_int(1));
        isolate.set_property_by_str(right.get(&isolate), "z", Smi::from_int(3));

        assert_ne!(
            left.get(&isolate).map(&isolate.heap),
            right.get(&isolate).map(&isolate.heap)
        );
        assert_eq!(
            TransitionsAccessor::number_of_transitions(&isolate.heap, map1),
            2,
            "one shape, two successors"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(left.get(&isolate), "y")),
            2
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(right.get(&isolate), "z")),
            3
        );
        assert_eq!(
            isolate.get_property_by_str(left.get(&isolate), "z"),
            isolate.undefined_value(),
            "shapes do not leak into each other"
        );
    }

    #[test]
    fn descriptor_arrays_are_shared_down_a_chain_rather_than_copied() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "a", Smi::from_int(1));
        let map1 = object.get(&isolate).map(&isolate.heap);
        let descriptors1 = map1.instance_descriptors(&isolate.heap);

        isolate.set_property_by_str(object.get(&isolate), "b", Smi::from_int(2));
        let map2 = object.get(&isolate).map(&isolate.heap);
        let descriptors2 = map2.instance_descriptors(&isolate.heap);

        assert_eq!(
            descriptors1, descriptors2,
            "the child should append into the parent's slack, not copy"
        );
        assert!(!map1.owns_descriptors(&isolate.heap));
        assert!(map2.owns_descriptors(&isolate.heap));
        assert_eq!(map1.number_of_own_descriptors(&isolate.heap), 1);
        assert_eq!(map2.number_of_own_descriptors(&isolate.heap), 2);
        let b = isolate.internalize_string("b").tagged();
        assert_eq!(
            map1.find_descriptor(&isolate.heap, b),
            K_NOT_FOUND,
            "the parent must not see the child's property"
        );
    }

    #[test]
    fn properties_spill_into_a_backing_store_when_the_inline_slots_run_out() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        let inline_slots = object
            .get(&isolate)
            .map(&isolate.heap)
            .inobject_properties(&isolate.heap);
        assert_eq!(inline_slots, Map::K_INITIAL_INOBJECT_SLACK);

        let count = inline_slots + 3;
        for i in 0..count {
            let name = format!("p{i}");
            assert!(isolate.set_property_by_str(object.get(&isolate), &name, Smi::from_int(i)));
        }

        for i in 0..count {
            let name = format!("p{i}");
            assert_eq!(
                Smi::to_int(isolate.get_property_by_str(object.get(&isolate), &name)),
                i,
                "property {name} was lost"
            );
        }

        let array = object.get(&isolate).property_array(&isolate.heap);
        assert!(
            array.length(&isolate.heap) >= count - inline_slots,
            "the overflow must have a backing store"
        );
        assert_eq!(
            object
                .get(&isolate)
                .map(&isolate.heap)
                .number_of_own_descriptors(&isolate.heap),
            count
        );
    }

    #[test]
    fn overwriting_a_property_does_not_change_the_shape() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        let map = object.get(&isolate).map(&isolate.heap);

        assert!(isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(2)));
        assert_eq!(
            object.get(&isolate).map(&isolate.heap),
            map,
            "a write is not a shape change"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            2
        );
    }

    #[test]
    fn assigning_a_wider_value_generalizes_the_field_representation() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));

        let map = object.get(&isolate).map(&isolate.heap);
        let x = isolate.internalize_string("x").tagged();
        assert_eq!(
            map.find_property_details(&isolate.heap, x)
                .unwrap()
                .representation(),
            Representation::Smi
        );

        let boxed = isolate.new_heap_number(1.5).tagged();
        assert!(isolate.set_property_by_str(object.get(&isolate), "x", boxed));

        assert_eq!(
            map.find_property_details(&isolate.heap, x)
                .unwrap()
                .representation(),
            Representation::Tagged,
            "a field that has held both a Smi and an object.get(&isolate) is tagged"
        );
        assert!(map.is_unstable(&isolate.heap));
        let read_back = isolate.get_property_by_str(object.get(&isolate), "x");
        assert_eq!(number_value(&isolate.heap, read_back), Some(1.5));
    }

    #[test]
    fn a_missing_property_reads_as_undefined() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        assert_eq!(
            isolate.get_property_by_str(object.get(&isolate), "nope"),
            isolate.undefined_value()
        );
        assert_eq!(
            isolate.get_own_property(object.get(&isolate), isolate.roots.empty_string),
            None
        );
    }

    #[test]
    fn lookup_walks_the_prototype_chain() {
        let mut isolate = Isolate::new();
        let prototype = isolate.new_js_object();
        let prototype = Rooted::new(&mut isolate, prototype);
        isolate.set_property_by_str(prototype.get(&isolate), "inherited", Smi::from_int(7));

        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "own", Smi::from_int(1));
        let map = object.get(&isolate).map(&isolate.heap);
        let prototype_value = prototype.get(&isolate).tagged();
        map.set_prototype(&mut isolate.heap, prototype_value);

        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "inherited")),
            7
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "own")),
            1
        );

        let inherited = isolate.internalize_string("inherited").tagged();
        assert_eq!(
            isolate.get_own_property(object.get(&isolate), inherited),
            None
        );

        let found = isolate
            .lookup_property(object.get(&isolate), inherited)
            .unwrap();
        assert_eq!(found.holder, prototype.get(&isolate));
        assert_eq!(found.holder_map, prototype.get(&isolate).map(&isolate.heap));
    }

    #[test]
    fn an_own_property_shadows_an_inherited_one() {
        let mut isolate = Isolate::new();
        let prototype = isolate.new_js_object();
        let prototype = Rooted::new(&mut isolate, prototype);
        isolate.set_property_by_str(prototype.get(&isolate), "x", Smi::from_int(7));

        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "marker", Smi::zero());
        let map = object.get(&isolate).map(&isolate.heap);
        let prototype_value = prototype.get(&isolate).tagged();
        map.set_prototype(&mut isolate.heap, prototype_value);

        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            7
        );
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            1,
            "the own property wins"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(prototype.get(&isolate), "x")),
            7,
            "and the prototype.get(&isolate) is untouched"
        );
    }

    #[test]
    fn a_non_extensible_object_refuses_new_properties_but_allows_writes() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        object
            .get(&isolate)
            .map(&isolate.heap)
            .set_is_extensible(&mut isolate.heap, false);

        assert!(
            !isolate.set_property_by_str(object.get(&isolate), "y", Smi::from_int(2)),
            "Object.preventExtensions blocks additions"
        );
        assert!(
            isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(3)),
            "existing properties are still writable"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            3
        );
    }

    #[test]
    fn a_long_shape_chain_stays_consistent() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.handles.push(object.get(&isolate).tagged());
        const N: i32 = 40;
        for i in 0..N {
            assert!(isolate.set_property_by_str(
                object.get(&isolate),
                &format!("f{i}"),
                Smi::from_int(i * 3)
            ));
        }

        let map = object.get(&isolate).map(&isolate.heap);
        assert_eq!(map.number_of_own_descriptors(&isolate.heap), N);
        assert_eq!(map.own_property_names(&isolate.heap).len() as i32, N);

        for i in 0..N {
            assert_eq!(
                Smi::to_int(isolate.get_property_by_str(object.get(&isolate), &format!("f{i}"))),
                i * 3
            );
        }

        let mut steps = 0;
        let mut current = map;
        while let Some(parent) = current.back_pointer(&isolate.heap) {
            steps += 1;
            current = parent;
            assert!(
                steps <= N,
                "the back-pointer chain should be exactly N long"
            );
        }
        assert_eq!(steps, N);
        assert_eq!(current, isolate.roots.js_object_map());
    }

    #[test]
    fn a_thousand_identical_objects_produce_one_shape_chain() {
        let mut isolate = Isolate::new();
        let mut maps = std::collections::HashSet::new();
        for i in 0..1000 {
            let object = isolate.new_js_object();
            let object = Rooted::new(&mut isolate, object);
            isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(i));
            isolate.set_property_by_str(object.get(&isolate), "y", Smi::from_int(i));
            maps.insert(object.get(&isolate).map(&isolate.heap));
        }
        assert_eq!(maps.len(), 1, "1000 objects, 1 hidden class");
    }
}

impl Isolate {
    #[inline]
    pub fn is_global_object(&self, object: JSObject) -> bool {
        self.heap.instance_type(object.object()) == instance_type::JS_GLOBAL_OBJECT_TYPE
    }

    pub fn global_cell(&self, name: Tagged) -> Option<crate::objects::property_cell::PropertyCell> {
        use crate::objects::property_cell::PropertyCell;
        let global = self.global_object();
        let value = self.get_own_property_raw(global, name)?;
        let object = value.to_strong_heap_object()?;
        if self.heap.instance_type(object) == instance_type::PROPERTY_CELL_TYPE {
            Some(PropertyCell::unchecked_cast(object))
        } else {
            None
        }
    }

    pub fn declare_global_cell(
        &mut self,
        name: Tagged,
        value: Tagged,
    ) -> crate::objects::property_cell::PropertyCell {
        if let Some(cell) = self.global_cell(name) {
            return cell;
        }
        let scope = self.handle_scope();
        let name_handle = self.create_handle(name);
        let cell = self.new_property_cell(name, value);
        let cell_handle = self.create_object_handle(cell.object());
        let name = name_handle.get(self);
        let cell_value = cell_handle.get(self);
        let global = self.global_object();
        self.set_property_ignoring_cells(global, name, cell_value, StoreOrigin::Named);
        let cell =
            crate::objects::property_cell::PropertyCell::unchecked_cast(cell_handle.object(self));
        self.close_handle_scope(scope);
        cell
    }

    pub fn set_global_cell_value(
        &mut self,
        cell: crate::objects::property_cell::PropertyCell,
        value: Tagged,
    ) {
        use crate::objects::property_cell::updated_cell_type;
        let updated = updated_cell_type(&self.heap, cell, value);
        let details = cell.property_details(&self.heap).with_cell_type(updated);
        cell.set_property_details(&mut self.heap, details);
        cell.set_value(&mut self.heap, value);
    }

    pub fn load_global(&self, name: Tagged) -> Option<Tagged> {
        Some(self.global_cell(name)?.value(&self.heap))
    }

    pub fn store_global(&mut self, name: Tagged, value: Tagged) {
        match self.global_cell(name) {
            Some(cell) => self.set_global_cell_value(cell, value),
            None => {
                self.declare_global_cell(name, value);
            }
        }
    }
}

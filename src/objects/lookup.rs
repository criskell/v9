//! Ported from `src/objects/lookup.h` / `lookup.cc`.
//!
//! Finding a property, and storing one. Lookup is the mirror image of a map
//! transition: the map's descriptors say which slot a property occupies, and
//! reading it is a map check plus one indexed load. A miss walks the
//! prototype chain.
//!
//! Adding a property is not done here -- that is a shape change, and it
//! lives with the shapes in [`crate::objects::map`]. This file decides
//! *whether* a property is already there, and `set_property` calls
//! `Map::TransitionToDataProperty` when it is not.

use crate::execution::isolate::Isolate;
use crate::objects::instance_type;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::property_details::{PropertyDetails, Representation};
use crate::objects::tagged::Tagged;

/// Where a property was found, for the inline cache to remember.
///
/// This is the information V8's `LookupIterator` hands to the IC: the map
/// that was checked and the slot to read. With both, a repeat of the same
/// load needs no lookup at all.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PropertyLocationInfo {
    /// The map the property was found on. For an own property that is the
    /// receiver's map; for an inherited one, a prototype's.
    pub holder_map: Map,
    /// The object the value actually lives on.
    pub holder: JSObject,
    pub details: PropertyDetails,
}

impl Isolate {
    // -----------------------------------------------------------------------
    // Transitions
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Lookup
    // -----------------------------------------------------------------------

    /// Finds an own property of `object`, without consulting its prototype.
    pub fn lookup_own_property(
        &self,
        object: JSObject,
        name: Tagged,
    ) -> Option<PropertyLocationInfo> {
        let map = object.map(&self.heap);
        let details = map.find_property_details(&self.heap, name)?;
        Some(PropertyLocationInfo {
            holder_map: map,
            holder: object,
            details,
        })
    }

    /// `LookupIterator`, reduced to data properties on the prototype chain.
    ///
    /// Deviation: no accessors, no proxies, no interceptors, no indexed
    /// elements. Those are each a branch in V8's state machine; `vx` has the
    /// data-property path only. See `PORTING.md`.
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

    /// `Object::GetProperty`: the value, or `undefined` if there is none.
    pub fn get_property(&self, object: JSObject, name: Tagged) -> Tagged {
        match self.lookup_property(object, name) {
            Some(found) => found
                .holder
                .fast_property_at(&self.heap, found.details.field_index() as i32),
            None => self.undefined_value(),
        }
    }

    /// Own property only -- what `Object.getOwnPropertyDescriptor` reads.
    pub fn get_own_property(&self, object: JSObject, name: Tagged) -> Option<Tagged> {
        let found = self.lookup_own_property(object, name)?;
        Some(
            found
                .holder
                .fast_property_at(&self.heap, found.details.field_index() as i32),
        )
    }

    /// `Object::SetProperty` for the data-property case: overwrite the
    /// property if the object already has it, otherwise transition to a map
    /// that does.
    ///
    /// Returns `false` if the property could not be added because the object
    /// is not extensible (`Object.preventExtensions`), which is the one way
    /// this can fail without throwing.
    pub fn set_property(&mut self, object: JSObject, name: Tagged, value: Tagged) -> bool {
        let map = object.map(&self.heap);

        // Already present: write the slot the map already assigned.
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

        // New property: find or create the map that has it.
        let representation = Representation::for_value(value);
        let new_map = self.transition_to_data_property(map, name, representation);
        let field_index = new_map
            .find_property_details(&self.heap, name)
            .expect("the transition target must own the property")
            .field_index() as i32;

        // Make room out of object if the inline slots are used up. This has
        // to happen *before* the map is swapped in: between the two, the
        // object's map would promise a field that does not exist yet.
        let inobject = new_map.inobject_properties(&self.heap);
        if field_index >= inobject {
            self.grow_property_array(object, field_index - inobject + 1);
        }

        self.heap.set_map(object.object(), new_map.object());
        object.set_fast_property_at(&mut self.heap, field_index, value);
        true
    }

    // -- convenience for tests and for the builtins -------------------------

    /// `o[name] = value` with a Rust string key, internalizing the name.
    pub fn set_property_by_str(&mut self, object: JSObject, name: &str, value: Tagged) -> bool {
        let key = self.internalize_string(name).tagged();
        self.set_property(object, key, value)
    }

    pub fn get_property_by_str(&mut self, object: JSObject, name: &str) -> Tagged {
        let key = self.internalize_string(name).tagged();
        self.get_property(object, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::descriptor_array::K_NOT_FOUND;
    use crate::objects::heap_number::number_value;
    use crate::objects::smi::Smi;
    use crate::objects::transitions::TransitionsAccessor;

    #[test]
    fn adding_a_property_moves_the_object_to_a_new_map() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let map_before = object.map(&isolate.heap);

        assert!(isolate.set_property_by_str(object, "x", Smi::from_int(1)));

        let map_after = object.map(&isolate.heap);
        assert_ne!(map_before, map_after, "the shape changed");
        assert_eq!(map_before.number_of_own_descriptors(&isolate.heap), 0);
        assert_eq!(map_after.number_of_own_descriptors(&isolate.heap), 1);
        assert_eq!(Smi::to_int(isolate.get_property_by_str(object, "x")), 1);
    }

    #[test]
    fn the_new_map_points_back_at_the_old_one() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let map0 = object.map(&isolate.heap);
        isolate.set_property_by_str(object, "x", Smi::from_int(1));
        let map1 = object.map(&isolate.heap);
        isolate.set_property_by_str(object, "y", Smi::from_int(2));
        let map2 = object.map(&isolate.heap);

        assert_eq!(map2.back_pointer(&isolate.heap), Some(map1));
        assert_eq!(map1.back_pointer(&isolate.heap), Some(map0));
        assert_eq!(map0.back_pointer(&isolate.heap), None, "the root map");
    }

    #[test]
    fn objects_built_the_same_way_share_a_map() {
        let mut isolate = Isolate::new();
        let a = isolate.new_js_object();
        let b = isolate.new_js_object();

        for object in [a, b] {
            isolate.set_property_by_str(object, "x", Smi::from_int(1));
            isolate.set_property_by_str(object, "y", Smi::from_int(2));
        }

        assert_eq!(
            a.map(&isolate.heap),
            b.map(&isolate.heap),
            "this is the entire point of hidden classes"
        );
    }

    #[test]
    fn property_order_produces_different_shapes() {
        let mut isolate = Isolate::new();
        let xy = isolate.new_js_object();
        isolate.set_property_by_str(xy, "x", Smi::from_int(1));
        isolate.set_property_by_str(xy, "y", Smi::from_int(2));

        let yx = isolate.new_js_object();
        isolate.set_property_by_str(yx, "y", Smi::from_int(2));
        isolate.set_property_by_str(yx, "x", Smi::from_int(1));

        assert_ne!(
            xy.map(&isolate.heap),
            yx.map(&isolate.heap),
            "{{x, y}} and {{y, x}} are different shapes -- the classic \
             performance footgun"
        );
        // Both still read back correctly.
        assert_eq!(Smi::to_int(isolate.get_property_by_str(xy, "x")), 1);
        assert_eq!(Smi::to_int(isolate.get_property_by_str(yx, "x")), 1);
        // ...but the field indices differ.
        let x_in_xy = xy.map(&isolate.heap);
        let x_key = isolate.internalize_string("x").tagged();
        assert_eq!(
            x_in_xy
                .find_property_details(&isolate.heap, x_key)
                .unwrap()
                .field_index(),
            0
        );
        assert_eq!(
            yx.map(&isolate.heap)
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
        isolate.set_property_by_str(a, "x", Smi::from_int(1));
        let map1 = a.map(&isolate.heap);

        let x = isolate.internalize_string("x").tagged();
        assert_eq!(
            TransitionsAccessor::search_transition(&isolate.heap, map0, x),
            Some(map1),
            "the edge must be cached on the source map"
        );

        // A second object follows the edge rather than building a new map.
        let b = isolate.new_js_object();
        isolate.set_property_by_str(b, "x", Smi::from_int(9));
        assert_eq!(b.map(&isolate.heap), map1);
    }

    #[test]
    fn a_branching_shape_graph_keeps_both_paths() {
        let mut isolate = Isolate::new();
        let base = isolate.new_js_object();
        isolate.set_property_by_str(base, "x", Smi::from_int(1));
        let map1 = base.map(&isolate.heap);

        let left = isolate.new_js_object();
        isolate.set_property_by_str(left, "x", Smi::from_int(1));
        isolate.set_property_by_str(left, "y", Smi::from_int(2));

        let right = isolate.new_js_object();
        isolate.set_property_by_str(right, "x", Smi::from_int(1));
        isolate.set_property_by_str(right, "z", Smi::from_int(3));

        assert_ne!(left.map(&isolate.heap), right.map(&isolate.heap));
        assert_eq!(
            TransitionsAccessor::number_of_transitions(&isolate.heap, map1),
            2,
            "one shape, two successors"
        );
        assert_eq!(Smi::to_int(isolate.get_property_by_str(left, "y")), 2);
        assert_eq!(Smi::to_int(isolate.get_property_by_str(right, "z")), 3);
        assert_eq!(
            isolate.get_property_by_str(left, "z"),
            isolate.undefined_value(),
            "shapes do not leak into each other"
        );
    }

    #[test]
    fn descriptor_arrays_are_shared_down_a_chain_rather_than_copied() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "a", Smi::from_int(1));
        let map1 = object.map(&isolate.heap);
        let descriptors1 = map1.instance_descriptors(&isolate.heap);

        isolate.set_property_by_str(object, "b", Smi::from_int(2));
        let map2 = object.map(&isolate.heap);
        let descriptors2 = map2.instance_descriptors(&isolate.heap);

        assert_eq!(
            descriptors1, descriptors2,
            "the child should append into the parent's slack, not copy"
        );
        // Ownership moved to the tip of the chain.
        assert!(!map1.owns_descriptors(&isolate.heap));
        assert!(map2.owns_descriptors(&isolate.heap));
        // ...and the parent still sees only its own prefix.
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
        let inline_slots = object.map(&isolate.heap).inobject_properties(&isolate.heap);
        assert_eq!(inline_slots, Map::K_INITIAL_INOBJECT_SLACK);

        // One more property than fits inline.
        let count = inline_slots + 3;
        for i in 0..count {
            let name = format!("p{i}");
            assert!(isolate.set_property_by_str(object, &name, Smi::from_int(i)));
        }

        // Everything still reads back, inline or not.
        for i in 0..count {
            let name = format!("p{i}");
            assert_eq!(
                Smi::to_int(isolate.get_property_by_str(object, &name)),
                i,
                "property {name} was lost"
            );
        }

        let array = object.property_array(&isolate.heap);
        assert!(
            array.length(&isolate.heap) >= count - inline_slots,
            "the overflow must have a backing store"
        );
        assert_eq!(
            object
                .map(&isolate.heap)
                .number_of_own_descriptors(&isolate.heap),
            count
        );
    }

    #[test]
    fn overwriting_a_property_does_not_change_the_shape() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "x", Smi::from_int(1));
        let map = object.map(&isolate.heap);

        assert!(isolate.set_property_by_str(object, "x", Smi::from_int(2)));
        assert_eq!(
            object.map(&isolate.heap),
            map,
            "a write is not a shape change"
        );
        assert_eq!(Smi::to_int(isolate.get_property_by_str(object, "x")), 2);
    }

    #[test]
    fn assigning_a_wider_value_generalizes_the_field_representation() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "x", Smi::from_int(1));

        let map = object.map(&isolate.heap);
        let x = isolate.internalize_string("x").tagged();
        assert_eq!(
            map.find_property_details(&isolate.heap, x)
                .unwrap()
                .representation(),
            Representation::Smi
        );

        let boxed = isolate.new_heap_number(1.5).tagged();
        assert!(isolate.set_property_by_str(object, "x", boxed));

        assert_eq!(
            map.find_property_details(&isolate.heap, x)
                .unwrap()
                .representation(),
            Representation::Tagged,
            "a field that has held both a Smi and an object is tagged"
        );
        assert!(map.is_unstable(&isolate.heap));
        let read_back = isolate.get_property_by_str(object, "x");
        assert_eq!(number_value(&isolate.heap, read_back), Some(1.5));
    }

    #[test]
    fn a_missing_property_reads_as_undefined() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        assert_eq!(
            isolate.get_property_by_str(object, "nope"),
            isolate.undefined_value()
        );
        assert_eq!(
            isolate.get_own_property(object, isolate.roots.empty_string),
            None
        );
    }

    #[test]
    fn lookup_walks_the_prototype_chain() {
        let mut isolate = Isolate::new();
        let prototype = isolate.new_js_object();
        isolate.set_property_by_str(prototype, "inherited", Smi::from_int(7));

        // A fresh map whose prototype is that object.
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "own", Smi::from_int(1));
        let map = object.map(&isolate.heap);
        map.set_prototype(&mut isolate.heap, prototype.tagged());

        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object, "inherited")),
            7
        );
        assert_eq!(Smi::to_int(isolate.get_property_by_str(object, "own")), 1);

        // Own-property lookup does not see it.
        let inherited = isolate.internalize_string("inherited").tagged();
        assert_eq!(isolate.get_own_property(object, inherited), None);

        // The holder reported is the prototype, not the receiver.
        let found = isolate.lookup_property(object, inherited).unwrap();
        assert_eq!(found.holder, prototype);
        assert_eq!(found.holder_map, prototype.map(&isolate.heap));
    }

    #[test]
    fn an_own_property_shadows_an_inherited_one() {
        let mut isolate = Isolate::new();
        let prototype = isolate.new_js_object();
        isolate.set_property_by_str(prototype, "x", Smi::from_int(7));

        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "marker", Smi::zero());
        let map = object.map(&isolate.heap);
        map.set_prototype(&mut isolate.heap, prototype.tagged());

        assert_eq!(Smi::to_int(isolate.get_property_by_str(object, "x")), 7);
        isolate.set_property_by_str(object, "x", Smi::from_int(1));
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object, "x")),
            1,
            "the own property wins"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(prototype, "x")),
            7,
            "and the prototype is untouched"
        );
    }

    #[test]
    fn a_non_extensible_object_refuses_new_properties_but_allows_writes() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "x", Smi::from_int(1));
        object
            .map(&isolate.heap)
            .set_is_extensible(&mut isolate.heap, false);

        assert!(
            !isolate.set_property_by_str(object, "y", Smi::from_int(2)),
            "Object.preventExtensions blocks additions"
        );
        assert!(
            isolate.set_property_by_str(object, "x", Smi::from_int(3)),
            "existing properties are still writable"
        );
        assert_eq!(Smi::to_int(isolate.get_property_by_str(object, "x")), 3);
    }

    #[test]
    fn a_long_shape_chain_stays_consistent() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        const N: i32 = 40;
        for i in 0..N {
            assert!(isolate.set_property_by_str(object, &format!("f{i}"), Smi::from_int(i * 3)));
        }

        let map = object.map(&isolate.heap);
        assert_eq!(map.number_of_own_descriptors(&isolate.heap), N);
        assert_eq!(map.own_property_names(&isolate.heap).len() as i32, N);

        for i in 0..N {
            assert_eq!(
                Smi::to_int(isolate.get_property_by_str(object, &format!("f{i}"))),
                i * 3
            );
        }

        // Walking the back pointers gets us all the way home.
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
            isolate.set_property_by_str(object, "x", Smi::from_int(i));
            isolate.set_property_by_str(object, "y", Smi::from_int(i));
            maps.insert(object.map(&isolate.heap));
        }
        assert_eq!(maps.len(), 1, "1000 objects, 1 hidden class");
    }
}

//! Ported from `src/objects/map-updater.h` / `map-updater.cc`.
//!
//! When a field turns out to hold a wider kind of value than its map says --
//! a slot that held a Smi is assigned an object -- the recorded
//! representation is no longer true, and anything that specialised on it is
//! now wrong. `MapUpdater` is V8's machinery for fixing that: it builds a new
//! map with the generalised field, deprecates the old one, and migrates
//! objects to the new one as they are touched.
//!
//! Deviation, tracked in `PORTING.md`: `vx` widens the descriptor **in
//! place** and marks the map unstable, rather than building a replacement.
//! That is sound only because nothing specialises on representation yet --
//! there is no optimizing tier to invalidate and no deoptimisation to
//! trigger. The moment one exists, this has to become the real thing: an
//! in-place widening would silently leave optimized code trusting a
//! representation that no longer holds.

use crate::execution::isolate::Isolate;
use crate::objects::descriptor_array::{DescriptorArray, K_NOT_FOUND};
use crate::objects::map::Map;
use crate::objects::property_details::Representation;
use crate::objects::tagged::Tagged;

impl Isolate {
    /// `MapUpdater::ReconfigureToDataField`, in the narrow sense `vx` needs:
    /// a field that has held a Smi and is now assigned an object has to be
    /// recorded as tagged, or a future optimisation would trust a stale
    /// representation.
    ///
    /// Deviation: V8 generalises by building a *new* map and deprecating the
    /// old one, so that code which already specialised on the narrow
    /// representation gets invalidated. `vx` widens the descriptor in place,
    /// which is sound only because nothing specialises on representation yet.
    /// The map is marked unstable to record that it happened.
    pub(crate) fn widen_field_representation(&mut self, map: Map, name: Tagged, value: Tagged) {
        let index = map.find_descriptor(&self.heap, name);
        if index == K_NOT_FOUND {
            return;
        }
        let descriptors: DescriptorArray = map.instance_descriptors(&self.heap);
        let details = descriptors.get_details(&self.heap, index);
        let current = details.representation();
        let widened = current.generalize(Representation::for_value(value));
        if widened != current {
            descriptors.set_details(&mut self.heap, index, details.with_representation(widened));
            map.set_is_unstable(&mut self.heap, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::property_details::default_data_field;
    use crate::objects::smi::Smi;

    #[test]
    fn widening_only_happens_when_the_value_is_actually_wider() {
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
        // Another Smi keeps the narrow representation, and the map stable.
        isolate.set_property_by_str(object, "x", Smi::from_int(2));
        assert_eq!(
            map.find_property_details(&isolate.heap, x)
                .unwrap()
                .representation(),
            Representation::Smi
        );
        assert!(!map.is_unstable(&isolate.heap));
    }

    #[test]
    fn the_field_index_and_attributes_survive_a_widening() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "a", Smi::from_int(1));
        isolate.set_property_by_str(object, "b", Smi::from_int(2));
        let map = object.map(&isolate.heap);
        let b = isolate.internalize_string("b").tagged();
        let before = map.find_property_details(&isolate.heap, b).unwrap();

        let boxed = isolate.new_heap_number(1.5).tagged();
        isolate.set_property_by_str(object, "b", boxed);

        let after = map.find_property_details(&isolate.heap, b).unwrap();
        assert_eq!(after.representation(), Representation::Tagged);
        assert_eq!(
            after.field_index(),
            before.field_index(),
            "the slot is fixed"
        );
        assert_eq!(after.attributes(), before.attributes());
        assert_eq!(after.kind(), before.kind());

        // The sibling descriptor is untouched.
        let a = isolate.internalize_string("a").tagged();
        assert_eq!(
            map.find_property_details(&isolate.heap, a)
                .unwrap()
                .representation(),
            Representation::Smi
        );
        let _ = default_data_field(0, Representation::Smi);
        let _: Option<DescriptorArray> = None;
        let _: i32 = K_NOT_FOUND;
        let _: Tagged = b;
    }
}

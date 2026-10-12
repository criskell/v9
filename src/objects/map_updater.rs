use crate::execution::isolate::Isolate;
use crate::objects::descriptor_array::{DescriptorArray, K_NOT_FOUND};
use crate::objects::map::Map;
use crate::objects::property_details::Representation;
use crate::objects::tagged::Tagged;

impl Isolate {
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
    use crate::objects::js_objects::JSObject;
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
    use crate::objects::property_details::{data_field, NONE};
    use crate::objects::smi::Smi;

    #[test]
    fn widening_only_happens_when_the_value_is_actually_wider() {
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
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(2));
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
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "a", Smi::from_int(1));
        isolate.set_property_by_str(object.get(&isolate), "b", Smi::from_int(2));
        let map = object.get(&isolate).map(&isolate.heap);
        let b = isolate.internalize_string("b").tagged();
        let before = map.find_property_details(&isolate.heap, b).unwrap();

        let boxed = isolate.new_heap_number(1.5).tagged();
        isolate.set_property_by_str(object.get(&isolate), "b", boxed);

        let after = map.find_property_details(&isolate.heap, b).unwrap();
        assert_eq!(after.representation(), Representation::Tagged);
        assert_eq!(
            after.field_index(),
            before.field_index(),
            "the slot is fixed"
        );
        assert_eq!(after.attributes(), before.attributes());
        assert_eq!(after.kind(), before.kind());

        let a = isolate.internalize_string("a").tagged();
        assert_eq!(
            map.find_property_details(&isolate.heap, a)
                .unwrap()
                .representation(),
            Representation::Smi
        );
        let _ = data_field(0, Representation::Smi, NONE);
        let _: Option<DescriptorArray> = None;
        let _: i32 = K_NOT_FOUND;
        let _: Tagged = b;
    }
}

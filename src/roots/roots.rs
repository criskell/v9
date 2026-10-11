use crate::objects::map::Map;
use crate::objects::tagged::Tagged;

macro_rules! roots_table {
    ($($(#[$doc:meta])* $field:ident),* $(,)?) => {
        pub struct RootsTable {
            $($(#[$doc])* pub $field: Tagged,)*
        }

        impl Default for RootsTable {
            fn default() -> RootsTable {
                RootsTable { $($field: Tagged::ZERO,)* }
            }
        }

        impl RootsTable {
            pub fn iterate(&mut self, visit: &mut impl FnMut(&mut Tagged)) {
                $(visit(&mut self.$field);)*
            }

            pub fn named(&self) -> Vec<(&'static str, Tagged)> {
                vec![$((stringify!($field), self.$field),)*]
            }
        }
    };
}

roots_table!(
    meta_map,
    heap_number_map,
    oddball_map,
    fixed_array_map,
    property_array_map,
    descriptor_array_map,
    transition_array_map,
    feedback_vector_map,
    feedback_metadata_map,
    property_cell_map,
    scope_info_map,
    function_context_map,
    bytecode_array_map,
    shared_function_info_map,
    filler_map,
    seq_one_byte_string_map,
    seq_two_byte_string_map,
    internalized_one_byte_string_map,
    internalized_two_byte_string_map,
    js_object_map,
    js_array_map,
    js_function_map,
    js_native_function_map,
    js_global_object_map,
    undefined_value,
    null_value,
    true_value,
    false_value,
    the_hole_value,
    uninitialized_value,
    megamorphic_symbol,
    empty_fixed_array,
    empty_property_array,
    empty_descriptor_array,
    empty_string,
    object_prototype,
    function_prototype,
    array_prototype,
    global_object,
);

impl RootsTable {
    #[inline]
    pub fn meta_map(&self) -> Map {
        self.as_map(self.meta_map)
    }

    #[inline]
    pub fn js_object_map(&self) -> Map {
        self.as_map(self.js_object_map)
    }

    #[inline]
    pub fn transition_array_map(&self) -> Map {
        self.as_map(self.transition_array_map)
    }

    #[inline]
    pub fn descriptor_array_map(&self) -> Map {
        self.as_map(self.descriptor_array_map)
    }

    #[inline]
    fn as_map(&self, value: Tagged) -> Map {
        Map::unchecked_cast(
            value
                .to_strong_heap_object()
                .expect("root map read before the root table was set up"),
        )
    }
}

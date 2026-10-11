//! Ported from `src/roots/roots.h`.
//!
//! The root set: every object the engine itself needs to be able to name.
//! Maps, the oddballs, the shared empty arrays. Two jobs:
//!
//! 1. They are the entry points the garbage collector traces from. Anything
//!    not reachable from a root (or from the stack) is garbage.
//! 2. They give the rest of the engine a place to find `undefined` without
//!    threading it through every signature.
//!
//! V8 generates this list with macros over `READ_ONLY_ROOT_LIST`; `vx` uses
//! one macro for the same reason -- the list has to be declared once and then
//! iterated by the GC, and keeping those in sync by hand is how roots get
//! dropped.

use crate::objects::map::Map;
use crate::objects::tagged::Tagged;

macro_rules! roots_table {
    ($($(#[$doc:meta])* $field:ident),* $(,)?) => {
        /// `ReadOnlyRoots` / `RootsTable`.
        ///
        /// Every slot starts as Smi zero, which is a safe non-reference value,
        /// so a half-built root table is still walkable by the GC.
        pub struct RootsTable {
            $($(#[$doc])* pub $field: Tagged,)*
        }

        impl Default for RootsTable {
            fn default() -> RootsTable {
                RootsTable { $($field: Tagged::ZERO,)* }
            }
        }

        impl RootsTable {
            /// `RootsTable::Iterate`: hands every root slot to the visitor so
            /// a moving collector can update it.
            pub fn iterate(&mut self, visit: &mut impl FnMut(&mut Tagged)) {
                $(visit(&mut self.$field);)*
            }

            /// The roots paired with their names, for `--print-roots`.
            pub fn named(&self) -> Vec<(&'static str, Tagged)> {
                vec![$((stringify!($field), self.$field),)*]
            }
        }
    };
}

roots_table!(
    /// The map of every `Map`, including itself. The root of the type system:
    /// `meta_map.map == meta_map`.
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
    bytecode_array_map,
    shared_function_info_map,
    /// The map stamped on a region of free space, so the heap stays walkable.
    filler_map,
    seq_one_byte_string_map,
    seq_two_byte_string_map,
    internalized_one_byte_string_map,
    internalized_two_byte_string_map,
    /// The initial map for an object literal: no properties, a few spare
    /// inline slots. Every `{}` starts here and transitions away.
    js_object_map,
    js_array_map,
    js_function_map,
    js_native_function_map,
    js_global_object_map,
    undefined_value,
    null_value,
    true_value,
    false_value,
    /// Never visible to JavaScript: marks an uninitialized `let` binding and
    /// an absent element.
    the_hole_value,
    /// An inline cache slot nothing has reached yet.
    uninitialized_value,
    /// `FeedbackVector::MegamorphicSentinel`: this site has seen too many
    /// shapes and has stopped caching. V8 uses a `Symbol`; `vx` has no
    /// symbols, so this is an oddball distinguished by identity.
    megamorphic_symbol,
    empty_fixed_array,
    empty_property_array,
    empty_descriptor_array,
    empty_string,
    /// The prototype every plain object gets.
    object_prototype,
    function_prototype,
    array_prototype,
    /// The global object -- the receiver for unqualified names.
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

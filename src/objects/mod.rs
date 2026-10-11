//! Mirrors `src/objects/` in V8: the object model -- the layout of everything
//! that lives in the heap, and the tagged word that refers to it.

#[macro_use]
pub mod object_macros;

pub mod descriptor_array;
pub mod fixed_array;
pub mod heap_number;
pub mod instance_type;
pub mod js_function;
pub mod js_objects;
pub mod lookup;
pub mod map;
pub mod map_updater;
pub mod map_word;
pub mod name;
pub mod oddball;
pub mod property_details;
pub mod smi;
pub mod string;
pub mod string_table;
pub mod tagged;
pub mod transitions;

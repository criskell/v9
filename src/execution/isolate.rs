//! Ported from `src/execution/isolate.h` (the parts `vx` has reached).
//!
//! An `Isolate` is one independent JavaScript world: its own heap, its own
//! roots, its own string table. V8's is enormous because it also owns the
//! compiler tiers, the debugger, the stack and the embedder plumbing; `vx`'s
//! grows as those arrive.
//!
//! It exists mostly to solve a borrow problem honestly. Allocation can move
//! objects, so anything live across an allocation has to be reachable from
//! something the collector traces. Putting the heap, the roots and the string
//! table in one struct means the collector can be a method on it and see all
//! of them at once -- no juggling of `&mut` borrows, and no way to forget a
//! root set.

use std::collections::HashMap;

use crate::heap::heap::{AllocationType, Heap};
use crate::objects::descriptor_array::DescriptorArray;
use crate::objects::fixed_array::{FixedArray, PropertyArray};
use crate::objects::instance_type::*;
use crate::objects::map::Map;
use crate::objects::oddball::{Oddball, OddballKind};
use crate::objects::string::{String as VxString, K_EMPTY_HASH_FIELD};
use crate::objects::tagged::Tagged;

pub struct Isolate {
    pub heap: Heap,
    pub roots: RootsTable,
    /// `StringTable`: the one internalized copy of each distinct name.
    ///
    /// Internalizing is what lets every property lookup compare names by
    /// address instead of by content. The Rust-side key is a `String` so that
    /// a lookup can be done without allocating in the heap first.
    string_table: HashMap<std::string::String, Tagged>,
    /// `HandleScope`, flattened. Values parked here are traced as roots, so
    /// they survive an allocation that triggers a collection. Code that holds
    /// more than one object across an allocation puts them here.
    pub handles: Vec<Tagged>,

    /// Scavenger bookkeeping: how far the collector has scanned the objects
    /// it promoted into old space during the collection in progress.
    ///
    /// Promotion appends to old space, and a promoted object's own references
    /// still have to be traced, so the collector needs a scan pointer there
    /// as well as in to-space. Parked on the isolate rather than passed
    /// around because the two drain loops feed each other.
    pub(crate) promoted_scan: crate::objects::tagged::Address,
}

pub use crate::roots::roots::RootsTable;

impl Isolate {
    /// `Isolate::New` followed by `Isolate::Init`: builds the heap and
    /// bootstraps the root set.
    pub fn new() -> Isolate {
        let mut isolate = Isolate {
            heap: Heap::new(),
            roots: RootsTable::default(),
            string_table: HashMap::new(),
            handles: Vec::new(),
            promoted_scan: 0,
        };
        isolate.setup_read_only_roots();
        isolate
    }

    pub fn with_heap(heap: Heap) -> Isolate {
        let mut isolate = Isolate {
            heap,
            roots: RootsTable::default(),
            string_table: HashMap::new(),
            handles: Vec::new(),
            promoted_scan: 0,
        };
        isolate.setup_read_only_roots();
        isolate
    }

    /// `Heap::CreateReadOnlyHeapObjects`: the bootstrap.
    ///
    /// The ordering here is forced, and it is the one genuinely circular part
    /// of starting an engine: a `Map` is a heap object, so it needs a map, so
    /// the first map has to be its own. V8 does exactly this -- allocate the
    /// meta map, then point it at itself.
    fn setup_read_only_roots(&mut self) {
        // 1. The meta map, which describes itself.
        let meta = self
            .heap
            .allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        self.heap.set_map(meta, meta);
        let meta_map = Map::unchecked_cast(meta);
        meta_map.set_instance_type(&mut self.heap, MAP_TYPE);
        meta_map.set_instance_size(&mut self.heap, Map::K_SIZE);
        meta_map.set_inobject_properties_start_in_words(
            &mut self.heap,
            Map::K_SIZE / crate::common::globals::K_TAGGED_SIZE,
        );
        self.roots.meta_map = meta.tagged();

        // 2. The maps for the structures the rest of the bootstrap needs.
        //    Their descriptor arrays are filled in once an empty one exists.
        self.roots.descriptor_array_map = self.new_bare_map(DESCRIPTOR_ARRAY_TYPE, 0).tagged();
        self.roots.fixed_array_map = self.new_bare_map(FIXED_ARRAY_TYPE, 0).tagged();
        self.roots.property_array_map = self.new_bare_map(PROPERTY_ARRAY_TYPE, 0).tagged();
        self.roots.transition_array_map = self.new_bare_map(TRANSITION_ARRAY_TYPE, 0).tagged();
        self.roots.feedback_vector_map = self.new_bare_map(FEEDBACK_VECTOR_TYPE, 0).tagged();
        self.roots.filler_map = self.new_bare_map(FILLER_TYPE, 0).tagged();

        // 3. The shared empty collections. `empty_descriptor_array` has to
        //    come first: every map points at it until it owns properties.
        let empty_descriptors = self.allocate_descriptor_array_raw(0);
        self.roots.empty_descriptor_array = empty_descriptors.tagged();
        let empty_fixed_array = self.allocate_fixed_array_raw(0);
        self.roots.empty_fixed_array = empty_fixed_array.tagged();
        let empty_property_array = self.allocate_property_array_raw(0);
        self.roots.empty_property_array = empty_property_array.tagged();

        // Retrofit the maps built in step 2, which were created before an
        // empty descriptor array existed.
        let built_so_far = [
            self.roots.meta_map,
            self.roots.descriptor_array_map,
            self.roots.fixed_array_map,
            self.roots.property_array_map,
            self.roots.transition_array_map,
            self.roots.feedback_vector_map,
            self.roots.filler_map,
        ];
        for map in built_so_far {
            let map = Map::unchecked_cast(map.to_strong_heap_object().unwrap());
            map.set_instance_descriptors(&mut self.heap, empty_descriptors.tagged());
        }

        // 4. String maps, then the empty string.
        self.roots.seq_one_byte_string_map =
            self.new_bare_map(SEQ_ONE_BYTE_STRING_TYPE, 0).tagged();
        self.roots.seq_two_byte_string_map =
            self.new_bare_map(SEQ_TWO_BYTE_STRING_TYPE, 0).tagged();
        self.roots.internalized_one_byte_string_map = self
            .new_bare_map(INTERNALIZED_ONE_BYTE_STRING_TYPE, 0)
            .tagged();
        self.roots.internalized_two_byte_string_map = self
            .new_bare_map(INTERNALIZED_TWO_BYTE_STRING_TYPE, 0)
            .tagged();
        self.roots.empty_string = self.internalize_string("").tagged();

        // 5. Numbers and the oddballs.
        self.roots.heap_number_map = self
            .new_bare_map(
                HEAP_NUMBER_TYPE,
                crate::objects::heap_number::HeapNumber::K_SIZE,
            )
            .tagged();
        self.roots.oddball_map = self.new_bare_map(ODDBALL_TYPE, Oddball::K_SIZE).tagged();

        self.roots.undefined_value = self
            .new_oddball(OddballKind::Undefined, "undefined", f64::NAN, "undefined")
            .tagged();
        self.roots.null_value = self
            .new_oddball(OddballKind::Null, "null", 0.0, "object")
            .tagged();
        self.roots.true_value = self
            .new_oddball(OddballKind::True, "true", 1.0, "boolean")
            .tagged();
        self.roots.false_value = self
            .new_oddball(OddballKind::False, "false", 0.0, "boolean")
            .tagged();
        self.roots.the_hole_value = self
            .new_oddball(OddballKind::TheHole, "hole", f64::NAN, "undefined")
            .tagged();
        self.roots.uninitialized_value = self
            .new_oddball(
                OddballKind::Uninitialized,
                "uninitialized",
                f64::NAN,
                "undefined",
            )
            .tagged();

        // 6. The JS object maps. A fresh object literal gets a few spare
        //    inline slots so the first properties need no backing store.
        self.roots.js_object_map = self
            .new_js_object_map(JS_OBJECT_TYPE, Map::K_INITIAL_INOBJECT_SLACK)
            .tagged();
        self.roots.js_array_map = self.new_js_object_map(JS_ARRAY_TYPE, 0).tagged();
        self.roots.js_global_object_map = self
            .new_js_object_map(JS_GLOBAL_OBJECT_TYPE, Map::K_INITIAL_INOBJECT_SLACK)
            .tagged();
        self.roots.js_function_map = self.new_js_object_map(JS_FUNCTION_TYPE, 0).tagged();
        self.roots.js_native_function_map =
            self.new_js_object_map(JS_NATIVE_FUNCTION_TYPE, 0).tagged();

        // Functions are callable; the map bit is what `typeof` reads.
        for map in [
            self.roots.js_function_map,
            self.roots.js_native_function_map,
        ] {
            let map = Map::unchecked_cast(map.to_strong_heap_object().unwrap());
            map.set_is_callable(&mut self.heap, true);
            map.set_is_constructor(&mut self.heap, true);
        }
    }

    /// A map with no descriptors, used during the bootstrap before an empty
    /// descriptor array exists.
    fn new_bare_map(&mut self, instance_type: InstanceType, instance_size: i32) -> Map {
        let meta = self.roots.meta_map;
        let object = self
            .heap
            .allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        self.heap
            .set_map(object, meta.to_strong_heap_object().unwrap());
        let map = Map::unchecked_cast(object);
        map.set_instance_type(&mut self.heap, instance_type);
        map.set_instance_size(&mut self.heap, instance_size);
        map.set_instance_descriptors(&mut self.heap, self.roots.empty_descriptor_array);
        map.set_number_of_own_descriptors(&mut self.heap, 0);
        map.set_owns_descriptors(&mut self.heap, true);
        map.set_is_extensible(&mut self.heap, true);
        map.set_prototype(&mut self.heap, self.roots.null_value);
        // Smi zero == no transitions, the `Uninitialized` encoding.
        map.set_raw_transitions(&mut self.heap, Tagged::ZERO);
        map
    }

    fn new_oddball(
        &mut self,
        kind: OddballKind,
        to_string: &str,
        to_number_raw: f64,
        type_of: &str,
    ) -> Oddball {
        let oddball_map = self.roots.oddball_map.to_strong_heap_object().unwrap();
        let object = self
            .heap
            .allocate_raw_or_fail(Oddball::K_SIZE as usize, AllocationType::ReadOnly);
        self.heap.set_map(object, oddball_map);
        let oddball = Oddball::unchecked_cast(object);
        oddball.set_kind(&mut self.heap, kind);
        oddball.set_to_number_raw(&mut self.heap, to_number_raw);
        let to_string = self.internalize_string(to_string).tagged();
        oddball.set_to_string(&mut self.heap, to_string);
        let type_of = self.internalize_string(type_of).tagged();
        oddball.set_type_of(&mut self.heap, type_of);
        // `to_number` would need a HeapNumber for NaN, which does not exist
        // this early; the raw double above is what conversions actually read.
        oddball.set_to_number(&mut self.heap, Tagged::ZERO);
        oddball
    }

    // -- roots, as typed accessors -----------------------------------------

    #[inline]
    pub fn undefined_value(&self) -> Tagged {
        self.roots.undefined_value
    }

    #[inline]
    pub fn null_value(&self) -> Tagged {
        self.roots.null_value
    }

    #[inline]
    pub fn true_value(&self) -> Tagged {
        self.roots.true_value
    }

    #[inline]
    pub fn false_value(&self) -> Tagged {
        self.roots.false_value
    }

    #[inline]
    pub fn the_hole_value(&self) -> Tagged {
        self.roots.the_hole_value
    }

    #[inline]
    pub fn boolean_value(&self, value: bool) -> Tagged {
        if value {
            self.roots.true_value
        } else {
            self.roots.false_value
        }
    }

    #[inline]
    pub fn empty_fixed_array(&self) -> FixedArray {
        FixedArray::unchecked_cast(
            self.roots
                .empty_fixed_array
                .to_strong_heap_object()
                .unwrap(),
        )
    }

    #[inline]
    pub fn empty_property_array(&self) -> PropertyArray {
        PropertyArray::unchecked_cast(
            self.roots
                .empty_property_array
                .to_strong_heap_object()
                .unwrap(),
        )
    }

    #[inline]
    pub fn empty_descriptor_array(&self) -> DescriptorArray {
        DescriptorArray::unchecked_cast(
            self.roots
                .empty_descriptor_array
                .to_strong_heap_object()
                .unwrap(),
        )
    }

    // -- the string table ---------------------------------------------------

    /// `StringTable::LookupString`: the single internalized copy of `s`.
    ///
    /// Internalized strings go in read-only space. They are effectively
    /// immortal (a property name, once seen, tends to be seen again) and
    /// keeping them immovable means a `DescriptorArray` key and an inline
    /// cache's cached name never need updating by the collector.
    ///
    /// Deviation: V8 internalizes into old space and relies on a write
    /// barrier plus string forwarding (`ThinString`) to internalize a string
    /// that already exists. `vx` has neither yet, so it copies.
    pub fn internalize_string(&mut self, s: &str) -> VxString {
        if let Some(existing) = self.string_table.get(s) {
            return VxString::unchecked_cast(existing.to_strong_heap_object().unwrap());
        }
        let one_byte = VxString::is_one_byte_content(s);
        let map = if one_byte {
            self.roots.internalized_one_byte_string_map
        } else {
            self.roots.internalized_two_byte_string_map
        };
        let string = self.allocate_string_raw(s, map, one_byte, AllocationType::ReadOnly);
        self.string_table.insert(s.to_owned(), string.tagged());
        string
    }

    /// Whether `s` has already been internalized. For tests and diagnostics.
    pub fn is_internalized(&self, s: &str) -> bool {
        self.string_table.contains_key(s)
    }

    pub fn string_table_size(&self) -> usize {
        self.string_table.len()
    }

    /// Hands every string-table entry to the visitor, so the collector can
    /// update them. Internalized strings do not move today, but the table is
    /// traced anyway: it is a root set, and treating it as one now is what
    /// keeps moving them later a local change.
    pub fn iterate_string_table(&mut self, visit: &mut impl FnMut(&mut Tagged)) {
        for value in self.string_table.values_mut() {
            visit(value);
        }
    }

    pub(crate) fn allocate_string_raw(
        &mut self,
        s: &str,
        map: Tagged,
        one_byte: bool,
        allocation: AllocationType,
    ) -> VxString {
        let length = VxString::utf16_length(s);
        let size = if one_byte {
            VxString::size_for_one_byte(length)
        } else {
            VxString::size_for_two_byte(length)
        };
        let object = self.heap.allocate_raw_or_fail(size as usize, allocation);
        self.heap
            .set_map(object, map.to_strong_heap_object().unwrap());
        let string = VxString::unchecked_cast(object);
        string.set_length(&mut self.heap, length);
        string
            .as_name()
            .set_raw_hash_field(&mut self.heap, K_EMPTY_HASH_FIELD);
        string.write_chars(&mut self.heap, s);
        string
    }

    // -- bootstrap-only raw allocations -------------------------------------

    fn allocate_fixed_array_raw(&mut self, length: i32) -> FixedArray {
        let object = self.heap.allocate_raw_or_fail(
            FixedArray::size_for(length) as usize,
            AllocationType::ReadOnly,
        );
        self.heap.set_map(
            object,
            self.roots.fixed_array_map.to_strong_heap_object().unwrap(),
        );
        let array = FixedArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        array
    }

    fn allocate_property_array_raw(&mut self, length: i32) -> PropertyArray {
        let object = self.heap.allocate_raw_or_fail(
            PropertyArray::size_for(length) as usize,
            AllocationType::ReadOnly,
        );
        self.heap.set_map(
            object,
            self.roots
                .property_array_map
                .to_strong_heap_object()
                .unwrap(),
        );
        let array = PropertyArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        array
    }

    fn allocate_descriptor_array_raw(&mut self, capacity: i32) -> DescriptorArray {
        let object = self.heap.allocate_raw_or_fail(
            DescriptorArray::size_for(capacity) as usize,
            AllocationType::ReadOnly,
        );
        self.heap.set_map(
            object,
            self.roots
                .descriptor_array_map
                .to_strong_heap_object()
                .unwrap(),
        );
        let array = DescriptorArray::unchecked_cast(object);
        array.set_number_of_all_descriptors(&mut self.heap, capacity);
        array.set_number_of_descriptors(&mut self.heap, 0);
        array.set_enum_cache(&mut self.heap, Tagged::ZERO);
        array
    }

    /// A map for ordinary JavaScript objects, with `inobject_properties`
    /// spare inline slots after the `JSObject` header.
    fn new_js_object_map(&mut self, instance_type: InstanceType, inobject_properties: i32) -> Map {
        use crate::objects::js_objects::JSObject;
        let header_words = JSObject::K_HEADER_SIZE / crate::common::globals::K_TAGGED_SIZE;
        let instance_size =
            (header_words + inobject_properties) * crate::common::globals::K_TAGGED_SIZE;
        let map = self.new_bare_map(instance_type, instance_size);
        map.set_inobject_properties_start_in_words(&mut self.heap, header_words);
        map.set_prototype(&mut self.heap, self.roots.null_value);
        map
    }
}

impl Default for Isolate {
    fn default() -> Isolate {
        Isolate::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::instance_type;

    #[test]
    fn the_meta_map_is_its_own_map() {
        let isolate = Isolate::new();
        let meta = isolate.roots.meta_map();
        assert_eq!(
            isolate.heap.map(meta.object()),
            meta.object(),
            "the type system has to bottom out somewhere"
        );
        assert_eq!(meta.instance_type(&isolate.heap), MAP_TYPE);
    }

    #[test]
    fn every_root_is_populated_after_bootstrap() {
        let isolate = Isolate::new();
        for (name, value) in isolate.roots.named() {
            // `global_object` and the prototypes are set up by the context,
            // not the read-only bootstrap.
            if matches!(
                name,
                "global_object" | "object_prototype" | "function_prototype" | "array_prototype"
            ) {
                continue;
            }
            assert!(
                value.is_strong(),
                "root `{name}` was left unset by the bootstrap"
            );
        }
    }

    #[test]
    fn every_root_map_is_a_map() {
        let isolate = Isolate::new();
        for (name, value) in isolate.roots.named() {
            if !name.ends_with("_map") {
                continue;
            }
            let object = value.to_strong_heap_object().expect(name);
            assert_eq!(
                isolate.heap.instance_type(object),
                MAP_TYPE,
                "root `{name}` should be a Map"
            );
        }
    }

    #[test]
    fn everything_immortal_lives_in_read_only_space() {
        let isolate = Isolate::new();
        for (name, value) in isolate.roots.named() {
            let Some(object) = value.to_strong_heap_object() else {
                continue;
            };
            assert!(
                isolate.heap.in_read_only_space(object),
                "root `{name}` must be immovable"
            );
        }
    }

    #[test]
    fn the_oddballs_know_how_to_convert_themselves() {
        let isolate = Isolate::new();
        let heap = &isolate.heap;

        let undefined =
            Oddball::unchecked_cast(isolate.undefined_value().to_strong_heap_object().unwrap());
        assert_eq!(undefined.kind(heap), Some(OddballKind::Undefined));
        assert!(
            undefined.to_number_raw(heap).is_nan(),
            "Number(undefined) is NaN"
        );
        assert_eq!(
            VxString::unchecked_cast(undefined.type_of(heap).to_strong_heap_object().unwrap())
                .to_rust_string(heap),
            "undefined"
        );

        let null = Oddball::unchecked_cast(isolate.null_value().to_strong_heap_object().unwrap());
        assert_eq!(null.kind(heap), Some(OddballKind::Null));
        assert_eq!(null.to_number_raw(heap), 0.0, "Number(null) is 0");
        assert_eq!(
            VxString::unchecked_cast(null.type_of(heap).to_strong_heap_object().unwrap())
                .to_rust_string(heap),
            "object",
            "the famous typeof null"
        );

        let t = Oddball::unchecked_cast(isolate.true_value().to_strong_heap_object().unwrap());
        assert_eq!(t.to_number_raw(heap), 1.0);
        assert_eq!(
            VxString::unchecked_cast(t.type_of(heap).to_strong_heap_object().unwrap())
                .to_rust_string(heap),
            "boolean"
        );
    }

    #[test]
    fn the_oddballs_are_distinct_objects() {
        let isolate = Isolate::new();
        let values = [
            isolate.undefined_value(),
            isolate.null_value(),
            isolate.true_value(),
            isolate.false_value(),
            isolate.the_hole_value(),
        ];
        for (i, a) in values.iter().enumerate() {
            for b in values.iter().skip(i + 1) {
                assert_ne!(a, b, "oddballs must be distinguishable by identity");
            }
        }
        assert_eq!(isolate.boolean_value(true), isolate.true_value());
        assert_eq!(isolate.boolean_value(false), isolate.false_value());
    }

    #[test]
    fn internalizing_the_same_text_twice_yields_the_same_object() {
        let mut isolate = Isolate::new();
        let a = isolate.internalize_string("length");
        let b = isolate.internalize_string("length");
        let c = isolate.internalize_string("size");

        assert_eq!(
            a, b,
            "that identity is what makes name lookup a pointer compare"
        );
        assert_ne!(a, c);
        assert_eq!(a.to_rust_string(&isolate.heap), "length");
        assert!(a.is_internalized(&isolate.heap));
    }

    #[test]
    fn internalized_strings_pick_their_representation() {
        let mut isolate = Isolate::new();
        let latin1 = isolate.internalize_string("olá");
        let wide = isolate.internalize_string("☃");
        assert!(latin1.is_one_byte(&isolate.heap));
        assert!(!wide.is_one_byte(&isolate.heap));
        assert_eq!(latin1.to_rust_string(&isolate.heap), "olá");
        assert_eq!(wide.to_rust_string(&isolate.heap), "☃");
        assert!(instance_type::is_internalized_string(
            isolate.heap.instance_type(wide.object())
        ));
    }

    #[test]
    fn the_empty_string_is_a_root_and_is_internalized() {
        let mut isolate = Isolate::new();
        let empty = isolate.internalize_string("");
        assert_eq!(empty.tagged(), isolate.roots.empty_string);
        assert_eq!(empty.length(&isolate.heap), 0);
    }

    #[test]
    fn function_maps_are_callable_and_plain_object_maps_are_not() {
        let isolate = Isolate::new();
        let heap = &isolate.heap;
        let function_map = Map::unchecked_cast(
            isolate
                .roots
                .js_function_map
                .to_strong_heap_object()
                .unwrap(),
        );
        let object_map = isolate.roots.js_object_map();

        assert!(function_map.is_callable(heap));
        assert!(function_map.is_constructor(heap));
        assert!(!object_map.is_callable(heap));
        assert!(instance_type::is_callable(function_map.instance_type(heap)));
        assert!(!instance_type::is_callable(object_map.instance_type(heap)));
    }

    #[test]
    fn the_initial_object_map_has_room_for_a_few_properties() {
        let isolate = Isolate::new();
        let map = isolate.roots.js_object_map();
        assert_eq!(
            map.inobject_properties(&isolate.heap),
            Map::K_INITIAL_INOBJECT_SLACK
        );
        assert_eq!(map.number_of_own_descriptors(&isolate.heap), 0);
        assert!(map.owns_descriptors(&isolate.heap));
        assert!(map.is_extensible(&isolate.heap));
        assert_eq!(
            map.instance_descriptors(&isolate.heap),
            isolate.empty_descriptor_array(),
            "a fresh map shares the empty descriptor array"
        );
    }
}

//! Real arrays: a JSArray with a length, a backing store, and an elements
//! kind that only ever generalises.
//!
//! The kind is the point. An array of Smis has a backing store that can be
//! read without a type check, and an array of doubles has one with no boxing
//! at all; a port that made every array HOLEY_ELEMENTS would run every
//! script here and fail every assertion about the kind.

use vx::execution::isolate::Isolate;
use vx::objects::elements::ElementsAccessor;
use vx::objects::elements_kind::ElementsKind::{self, *};
use vx::objects::fixed_primitive_array::FixedDoubleArray;
use vx::objects::js_array::JSArray;
use vx::objects::js_objects::JSObject;

/// Runs a script and returns the isolate plus the completion value as an
/// object.
fn run(source: &str) -> (Isolate, JSObject) {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    let value = match isolate.run_script(source) {
        Ok(value) => value,
        Err(error) => panic!("{source:?} failed: {error}"),
    };
    let object = JSObject::unchecked_cast(
        value
            .to_strong_heap_object()
            .unwrap_or_else(|| panic!("{source:?} produced {value:?}, not an object")),
    );
    (isolate, object)
}

fn kind_of(source: &str) -> ElementsKind {
    let (isolate, object) = run(source);
    object.get_elements_kind(&isolate.heap)
}

fn evaluate(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => isolate.to_string(value),
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

#[test]
fn a_literal_of_smis_gets_the_narrowest_kind() {
    assert_eq!(kind_of("var a = [1, 2, 3]; a"), PackedSmiElements);
    assert_eq!(
        kind_of("var a = []; a"),
        PackedSmiElements,
        "an empty array starts at the bottom of the lattice"
    );
}

#[test]
fn a_double_forces_the_unboxed_kind() {
    assert_eq!(kind_of("var a = [1.5]; a"), PackedDoubleElements);
    assert_eq!(
        kind_of("var a = [1, 2.5, 3]; a"),
        PackedDoubleElements,
        "one double in the literal is enough"
    );
}

#[test]
fn a_store_generalises_the_kind_and_converts_the_backing_store() {
    // The transition that matters: the store was a FixedArray of Smis and
    // has to become a FixedDoubleArray, with what was there carried over.
    let (isolate, object) = run("var a = [1, 2]; a[0] = 1.5; a");
    assert_eq!(
        object.get_elements_kind(&isolate.heap),
        PackedDoubleElements
    );

    let elements = object.elements(&isolate.heap);
    let doubles = FixedDoubleArray::unchecked_cast(elements.to_strong_heap_object().unwrap());
    assert_eq!(doubles.get_scalar(&isolate.heap, 0), 1.5);
    assert_eq!(
        doubles.get_scalar(&isolate.heap, 1),
        2.0,
        "the Smi that was already there is now an unboxed double"
    );
}

#[test]
fn a_non_number_generalises_all_the_way_to_tagged() {
    assert_eq!(kind_of("var a = [1]; a[0] = 'x'; a"), PackedElements);
    assert_eq!(
        kind_of("var a = [1.5]; a[0] = 'x'; a"),
        PackedElements,
        "double to tagged, not back to Smi"
    );
}

#[test]
fn the_lattice_does_not_go_back_down() {
    // Having seen 1.5, the array keeps the double store even though every
    // later value is a Smi. This is what makes the kind worth tracking: a
    // compiler may rely on it not narrowing behind its back.
    assert_eq!(
        kind_of("var a = [1]; a[0] = 1.5; a[0] = 2; a"),
        PackedDoubleElements
    );
    assert_eq!(
        kind_of("var a = [1]; a[0] = 'x'; a[0] = 1; a"),
        PackedElements
    );
}

#[test]
fn a_store_past_the_end_makes_the_array_holey() {
    let (isolate, object) = run("var a = [1]; a[5] = 1; a");
    assert_eq!(object.get_elements_kind(&isolate.heap), HoleySmiElements);
    let array = JSArray::unchecked_cast(object.object());
    assert_eq!(array.length_value(&isolate.heap), 6);

    // The gap really is holes, not zeroes.
    let accessor = ElementsAccessor::for_kind(HoleySmiElements);
    let elements = object.elements(&isolate.heap);
    assert!(accessor.has_entry(&isolate.heap, elements, 0));
    assert!(!accessor.has_entry(&isolate.heap, elements, 1));
    assert!(accessor.has_entry(&isolate.heap, elements, 5));
    assert_eq!(accessor.number_of_elements(&isolate.heap, object), 2);
}

#[test]
fn reading_a_hole_or_past_the_end_gives_undefined() {
    assert_eq!(evaluate("var a = [1]; a[5] = 1; a[2]"), "undefined");
    assert_eq!(evaluate("var a = [1, 2]; a[99]"), "undefined");
}

#[test]
fn length_tracks_the_highest_index_written() {
    assert_eq!(evaluate("var a = []; a[0] = 1; a.length"), "1");
    assert_eq!(evaluate("var a = []; a[9] = 1; a.length"), "10");
    assert_eq!(evaluate("var a = [1, 2, 3]; a.length"), "3");
    assert_eq!(
        evaluate("var a = [1, 2, 3]; a[1] = 9; a.length"),
        "3",
        "writing inside the array does not change its length"
    );
}

#[test]
fn assigning_length_truncates_and_extends() {
    assert_eq!(evaluate("var a = [1, 2, 3]; a.length = 1; a.length"), "1");
    assert_eq!(
        evaluate("var a = [1, 2, 3]; a.length = 1; a[1]"),
        "undefined",
        "truncating drops the elements, it does not just move the length"
    );
    assert_eq!(evaluate("var a = [1]; a.length = 4; a.length"), "4");
    assert_eq!(evaluate("var a = [1]; a.length = 4; a[0]"), "1");
    assert_eq!(evaluate("var a = [1, 2]; a.length = 0; a.length"), "0");

    // Extending makes the array holey, since the new slots hold nothing.
    assert_eq!(kind_of("var a = [1]; a.length = 4; a"), HoleySmiElements);
}

#[test]
fn the_backing_store_grows_geometrically() {
    // NewElementsCapacity: old + 50% + 16. Appending one element at a time
    // must not reallocate on every append.
    let (isolate, object) = run("var a = [];
         var i = 0;
         while (i < 40) { a[i] = i; i = i + 1; }
         a");
    let array = JSArray::unchecked_cast(object.object());
    assert_eq!(array.length_value(&isolate.heap), 40);

    let accessor = ElementsAccessor::for_kind(object.get_elements_kind(&isolate.heap));
    let capacity = accessor.get_capacity(&isolate.heap, object.elements(&isolate.heap));
    assert!(
        capacity >= 40,
        "capacity {capacity} has to cover the length"
    );
    assert!(
        capacity <= JSObject::new_elements_capacity(40),
        "capacity {capacity} grew further than one step past the length"
    );
}

#[test]
fn an_array_survives_a_collection_with_its_elements() {
    // The backing store is a separate heap object, so a scavenge has to
    // update the array's pointer to it. For a FixedDoubleArray it also has
    // to *not* trace the contents: a double whose bits look like a tagged
    // pointer would otherwise be followed.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    let value = isolate
        .run_script("var a = [1.5, 2.5, 3.5]; a")
        .expect("runs");
    let address = value.to_strong_heap_object().unwrap().address();
    isolate.handles.push(value);
    let stats = isolate.collect_garbage();
    assert!(stats.objects_evacuated > 0, "something should have moved");

    let moved = isolate.handles.pop().unwrap();
    assert_ne!(
        moved.to_strong_heap_object().unwrap().address(),
        address,
        "the array should have been evacuated, or the test proves nothing"
    );
    let object = JSObject::unchecked_cast(moved.to_strong_heap_object().unwrap());
    assert_eq!(
        object.get_elements_kind(&isolate.heap),
        PackedDoubleElements
    );
    let elements = object.elements(&isolate.heap);
    let doubles = FixedDoubleArray::unchecked_cast(elements.to_strong_heap_object().unwrap());
    assert_eq!(doubles.get_scalar(&isolate.heap, 0), 1.5);
    assert_eq!(doubles.get_scalar(&isolate.heap, 2), 3.5);
}

#[test]
fn a_sparse_array_goes_to_dictionary_elements() {
    // kMaxGap is 1024: a gap wider than that stops being worth storing as
    // holes. Without this the backing store would be a million slots.
    assert_eq!(
        kind_of("var a = [1]; a[1000000] = 2; a"),
        DictionaryElements
    );
    assert_eq!(
        kind_of("var a = [1]; a[1023] = 2; a"),
        HoleySmiElements,
        "a gap inside kMaxGap stays fast"
    );
}

#[test]
fn a_dictionary_array_still_reads_and_writes() {
    assert_eq!(evaluate("var a = [1]; a[1000000] = 2; a[0]"), "1");
    assert_eq!(evaluate("var a = [1]; a[1000000] = 2; a[1000000]"), "2");
    assert_eq!(evaluate("var a = [1]; a[1000000] = 2; a[500]"), "undefined");
    assert_eq!(evaluate("var a = [1]; a[1000000] = 2; a.length"), "1000001");
    assert_eq!(
        evaluate("var a = [1]; a[1000000] = 2; a[1000000] = 9; a[1000000]"),
        "9",
        "a second write to a key already in the dictionary replaces it"
    );
}

#[test]
fn the_dictionary_holds_only_what_was_written() {
    let (isolate, object) = run("var a = [1]; a[1000000] = 2; a");
    let accessor = ElementsAccessor::for_kind(DictionaryElements);
    assert_eq!(
        accessor.number_of_elements(&isolate.heap, object),
        2,
        "two writes, two entries -- not a million"
    );
    let capacity = accessor.get_capacity(&isolate.heap, object.elements(&isolate.heap));
    assert!(capacity < 64, "capacity {capacity} should be a small table");
}

#[test]
fn filling_a_dictionary_array_in_comes_back_to_fast_elements() {
    // ShouldConvertToFastElements: once the dictionary stops saving 50% of
    // the space a dense store would take, it is worth going back.
    let (isolate, object) = run("var a = [];
         a[2000] = 1;
         var i = 0;
         while (i < 2000) { a[i] = i; i = i + 1; }
         a");
    assert!(
        !vx::objects::elements_kind::is_dictionary_elements_kind(
            object.get_elements_kind(&isolate.heap)
        ),
        "a dense array should not still be a dictionary: {}",
        object.get_elements_kind(&isolate.heap)
    );
    let array = JSArray::unchecked_cast(object.object());
    assert_eq!(array.length_value(&isolate.heap), 2001);
}

#[test]
fn truncating_a_dictionary_array_drops_the_entries_above_the_length() {
    assert_eq!(
        evaluate("var a = []; a[2000] = 7; a.length = 1; a.length"),
        "1"
    );
    assert_eq!(
        evaluate("var a = []; a[2000] = 7; a[0] = 1; a.length = 1; a[2000]"),
        "undefined"
    );
    assert_eq!(
        evaluate("var a = []; a[2000] = 7; a[0] = 1; a.length = 1; a[0]"),
        "1",
        "what is below the new length survives"
    );
}

#[test]
fn a_dictionary_array_survives_a_collection() {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    let value = isolate
        .run_script("var a = [1]; a[1000000] = 2; a")
        .expect("runs");
    isolate.handles.push(value);
    let stats = isolate.collect_garbage();
    assert!(stats.objects_evacuated > 0);
    let moved = isolate.handles.pop().unwrap();
    let object = JSObject::unchecked_cast(moved.to_strong_heap_object().unwrap());
    assert_eq!(object.get_elements_kind(&isolate.heap), DictionaryElements);
    let accessor = ElementsAccessor::for_kind(DictionaryElements);
    assert_eq!(accessor.number_of_elements(&isolate.heap, object), 2);
    assert!(accessor.has_element(&isolate.heap, object, 1000000));
}

#[test]
fn a_double_whose_bits_look_like_a_pointer_is_not_traced() {
    // The reason a FixedDoubleArray needs its own instance type: its
    // contents are raw f64, and the collector must not follow them. A
    // double can hold any bit pattern, including one that reads as a
    // tagged pointer into the arena.
    //
    // 1.5 is not such a pattern -- its low bits read as a Smi, which the
    // collector skips anyway -- so the decoy here is the address of the
    // array's own backing store, stored as a double. If anything traces
    // it, the collector rewrites those bits to the forwarded address and
    // the value changes.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    let value = isolate
        .run_script("var a = [1.5, 2.5, 3.5]; a")
        .expect("runs");
    let object = JSObject::unchecked_cast(value.to_strong_heap_object().unwrap());

    let elements = object.elements(&isolate.heap);
    let decoy = elements.ptr();
    assert_eq!(decoy & 3, 1, "the decoy has to read as a strong pointer");
    FixedDoubleArray::unchecked_cast(elements.to_strong_heap_object().unwrap()).set(
        &mut isolate.heap,
        1,
        f64::from_bits(decoy),
    );

    isolate.handles.push(value);
    let stats = isolate.collect_garbage();
    assert!(stats.objects_evacuated > 0, "something should have moved");
    let moved = isolate.handles.pop().unwrap();
    let object = JSObject::unchecked_cast(moved.to_strong_heap_object().unwrap());
    let elements = object.elements(&isolate.heap);
    assert_ne!(elements.ptr(), decoy, "the store itself did move");

    let doubles = FixedDoubleArray::unchecked_cast(elements.to_strong_heap_object().unwrap());
    assert_eq!(
        doubles.get_representation(&isolate.heap, 1),
        decoy,
        "the double was traced as a pointer and rewritten"
    );
    assert_eq!(doubles.get_scalar(&isolate.heap, 0), 1.5);
    assert_eq!(doubles.get_scalar(&isolate.heap, 2), 3.5);
}

#[test]
fn every_array_of_one_kind_shares_a_map() {
    // The per-kind maps come from the roots, the way V8 keeps them in the
    // native context, so building many arrays does not build many maps.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    isolate
        .run_script(
            "var smis = [1];
             var doubles = [1.5];
             var tagged = ['x'];
             var also_smis = [2, 3];",
        )
        .expect("runs");

    fn map_of(isolate: &mut Isolate, name: &str) -> vx::objects::tagged::Address {
        let global = isolate.global_object();
        let value = isolate.get_property_by_str(global, name);
        let object = JSObject::unchecked_cast(value.to_strong_heap_object().unwrap());
        isolate.heap.map_of(object.object()).address()
    }

    let smis = map_of(&mut isolate, "smis");
    let also_smis = map_of(&mut isolate, "also_smis");
    let doubles = map_of(&mut isolate, "doubles");
    let tagged = map_of(&mut isolate, "tagged");

    assert_eq!(smis, also_smis, "two Smi arrays share one map");
    assert_ne!(smis, doubles);
    assert_ne!(doubles, tagged);
}

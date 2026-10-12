//! Dictionary-mode properties: the slow half of the object model.
//!
//! Hidden classes pay off only while an object's shape is shared. An object
//! whose keys are computed, or that has had a property deleted, has no
//! useful shape, so V8 moves its properties into a `NameDictionary` and
//! gives it a map that says so. These tests assert on which mode the object
//! is in, because every one of them would also pass against a port that
//! never left fast mode.

use vx::execution::isolate::Isolate;
use vx::objects::js_objects::JSObject;

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

fn evaluate(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => isolate.to_string(value),
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

fn is_slow(source: &str) -> bool {
    let (isolate, object) = run(source);
    !object.has_fast_properties(&isolate.heap)
}

#[test]
fn named_stores_never_leave_fast_mode() {
    // V8's TooManyFastProperties only fires for a store that *might* be
    // keyed. Forty properties written as `o.f0 = ...` keep their shape,
    // because code that names its properties is exactly the code hidden
    // classes are for.
    let source = {
        let mut source = String::from("var o = {};");
        for index in 0..40 {
            source.push_str(&format!("o.f{index} = {index};"));
        }
        source.push('o');
        source
    };
    assert!(!is_slow(&source), "named stores should stay fast");
    assert_eq!(
        evaluate(&format!("{}{}", &source[..source.len() - 1], "o.f39")),
        "39"
    );
}

#[test]
fn enough_computed_keys_go_to_dictionary_mode() {
    // A keyed store past the soft limit of 12 external fields does
    // normalize: nothing shares that shape, so the descriptor array and the
    // transition chain are pure overhead.
    assert!(
        is_slow(
            "var o = {};
             var i = 0;
             while (i < 30) { o['k' + i] = i; i = i + 1; }
             o"
        ),
        "thirty computed keys should normalize"
    );
    assert!(
        !is_slow(
            "var o = {};
             var i = 0;
             while (i < 5) { o['k' + i] = i; i = i + 1; }
             o"
        ),
        "five is below the limit"
    );
}

#[test]
fn a_normalized_object_keeps_every_property() {
    // Normalizing copies the fast properties into the dictionary, so a
    // property written before the switch has to survive it.
    assert_eq!(
        evaluate(
            "var o = {};
             var i = 0;
             while (i < 30) { o['k' + i] = i * 2; i = i + 1; }
             o.k0 + o.k15 + o.k29"
        ),
        "88",
        "0 + 30 + 58"
    );
}

#[test]
fn a_dictionary_object_still_takes_new_properties() {
    assert_eq!(
        evaluate(
            "var o = {};
             var i = 0;
             while (i < 30) { o['k' + i] = i; i = i + 1; }
             o.late = 7;
             o.late"
        ),
        "7"
    );
}

#[test]
fn delete_removes_a_property_and_normalizes() {
    assert_eq!(evaluate("var o = { a: 1, b: 2 }; delete o.a"), "true");
    assert_eq!(
        evaluate("var o = { a: 1, b: 2 }; delete o.a; o.a"),
        "undefined"
    );
    assert_eq!(
        evaluate("var o = { a: 1, b: 2 }; delete o.a; o.b"),
        "2",
        "the properties that were not deleted survive"
    );
    assert!(
        is_slow("var o = { a: 1, b: 2 }; delete o.a; o"),
        "a deleted property leaves no useful shape, so the object goes slow"
    );
}

#[test]
fn deleting_what_is_not_there_succeeds_and_changes_nothing() {
    assert_eq!(evaluate("var o = { a: 1 }; delete o.zzz"), "true");
    assert!(
        !is_slow("var o = { a: 1 }; delete o.zzz; o"),
        "deleting a property the object never had must not normalize it"
    );
}

#[test]
fn a_deleted_property_can_be_added_back() {
    assert_eq!(evaluate("var o = { a: 1 }; delete o.a; o.a = 2; o.a"), "2");
}

#[test]
fn delete_on_an_array_punches_a_hole_and_keeps_the_length() {
    assert_eq!(evaluate("var a = [1, 2, 3]; delete a[1]"), "true");
    assert_eq!(
        evaluate("var a = [1, 2, 3]; delete a[1]; a[1]"),
        "undefined"
    );
    assert_eq!(
        evaluate("var a = [1, 2, 3]; delete a[1]; a.length"),
        "3",
        "delete does not shift the elements or shorten the array"
    );
    assert_eq!(evaluate("var a = [1, 2, 3]; delete a[1]; a[2]"), "3");

    // A packed array has to generalise: there is now a hole in it.
    let (isolate, object) = run("var a = [1, 2, 3]; delete a[1]; a");
    assert!(
        vx::objects::elements_kind::is_holey_elements_kind(object.get_elements_kind(&isolate.heap)),
        "should be holey, is {}",
        object.get_elements_kind(&isolate.heap)
    );
}

#[test]
fn a_dictionary_object_survives_a_collection() {
    let mut isolate = Isolate::new();
    isolate.set_allocation_gc_interval(0);
    isolate.setup_global_object();
    let value = isolate
        .run_script(
            "var o = {};
             var i = 0;
             while (i < 30) { o['k' + i] = i; i = i + 1; }
             o",
        )
        .expect("runs");
    isolate.handles.push(value);
    let stats = isolate.collect_garbage();
    assert!(stats.objects_evacuated > 0);

    let moved = isolate.handles.pop().unwrap();
    let object = JSObject::unchecked_cast(moved.to_strong_heap_object().unwrap());
    assert!(!object.has_fast_properties(&isolate.heap));
    let dictionary = object.property_dictionary(&isolate.heap);
    assert_eq!(dictionary.number_of_elements(&isolate.heap), 30);
    let key = isolate.internalize_string("k7").tagged();
    let entry = dictionary.find_entry(&isolate.heap, key);
    assert_ne!(entry, vx::objects::hash_table::K_NOT_FOUND);
    assert_eq!(
        vx::objects::smi::Smi::to_int(dictionary.value_at(&isolate.heap, entry)),
        7
    );
}

#[test]
fn the_enumeration_index_counts_insertions() {
    // V8 keeps insertion order for a dictionary object by stamping each
    // entry with the next enumeration index, which is what `for...in` would
    // sort by.
    let (isolate, object) = run("var o = {};
         var i = 0;
         while (i < 20) { o['k' + i] = i; i = i + 1; }
         o");
    let dictionary = object.property_dictionary(&isolate.heap);
    assert_eq!(
        dictionary.next_enumeration_index(&isolate.heap),
        21,
        "twenty entries, starting from one"
    );

    let mut indices: Vec<u32> = dictionary
        .iterate_entries(&isolate.heap)
        .iter()
        .map(|entry| {
            dictionary
                .details_at(&isolate.heap, *entry)
                .dictionary_index()
        })
        .collect();
    indices.sort();
    assert_eq!(
        indices,
        (1..=20).collect::<Vec<u32>>(),
        "each insertion got a distinct index"
    );
}

fn computed_stores(count: usize) -> String {
    let mut source = String::from("var o = {};");
    for index in 0..count {
        source.push_str(&format!("o['f{index}'] = {index};"));
    }
    source
}

fn dictionary_stores(count: usize) -> String {
    let mut source = String::from("var o = {}; o.pad = 0; delete o.pad;");
    for index in 0..count {
        source.push_str(&format!("o.f{index} = {index};"));
    }
    source
}

fn to_fast(source: &str) -> (Isolate, JSObject) {
    vx::flags::flags::V8_FLAGS.set_allow_natives_syntax(true);
    run(source)
}

fn evaluate_with_natives_syntax(source: &str) -> String {
    vx::flags::flags::V8_FLAGS.set_allow_natives_syntax(true);
    evaluate(source)
}

#[test]
fn to_fast_properties_turns_a_dictionary_back_into_a_hidden_class() {
    let stores = computed_stores(40);
    assert!(is_slow(&format!("{stores}o")));
    let (isolate, object) = to_fast(&format!("{stores}%ToFastProperties(o); o"));
    assert!(object.has_fast_properties(&isolate.heap));
    assert_eq!(
        object
            .map(&isolate.heap)
            .number_of_own_descriptors(&isolate.heap),
        40
    );
    assert_eq!(
        evaluate_with_natives_syntax(&format!(
            "{stores}%ToFastProperties(o); o.f0 + ',' + o.f17 + ',' + o.f39"
        )),
        "0,17,39"
    );
}

#[test]
fn the_migrated_descriptors_are_in_insertion_order() {
    let stores = dictionary_stores(8);
    assert!(is_slow(&format!("{stores}o")));
    assert_eq!(
        evaluate_with_natives_syntax(&format!(
            "{stores}%ToFastProperties(o); Object.keys(o).join(',')"
        )),
        "f0,f1,f2,f3,f4,f5,f6,f7"
    );
}

#[test]
fn a_deleted_entry_gets_no_descriptor() {
    let stores = dictionary_stores(8);
    let (isolate, object) = to_fast(&format!("{stores}delete o.f3; %ToFastProperties(o); o"));
    assert_eq!(
        object
            .map(&isolate.heap)
            .number_of_own_descriptors(&isolate.heap),
        7
    );
    assert_eq!(
        evaluate_with_natives_syntax(&format!(
            "{stores}delete o.f3; %ToFastProperties(o); Object.keys(o).join(',')"
        )),
        "f0,f1,f2,f4,f5,f6,f7"
    );
}

#[test]
fn an_emptied_dictionary_migrates_to_a_shape_with_no_properties() {
    let (isolate, object) = to_fast("var o = {}; o.a = 1; delete o.a; %ToFastProperties(o); o");
    assert!(object.has_fast_properties(&isolate.heap));
    assert_eq!(
        object
            .map(&isolate.heap)
            .number_of_own_descriptors(&isolate.heap),
        0
    );
    assert_eq!(
        evaluate_with_natives_syntax(
            "var o = {}; o.a = 1; delete o.a; %ToFastProperties(o); o.b = 2; o.b"
        ),
        "2"
    );
}

#[test]
fn migrating_a_fast_object_changes_nothing() {
    vx::flags::flags::V8_FLAGS.set_allow_natives_syntax(true);
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    isolate
        .run_script(
            "var o = {}; o.a = 1; o.b = 2; %ToFastProperties(o);
             var p = {}; p.a = 1; p.b = 2;",
        )
        .expect("runs");

    fn map_of(isolate: &mut Isolate, name: &str) -> vx::objects::tagged::Address {
        let global = isolate.global_object();
        let value = isolate.get_property_by_str(global, name);
        let object = JSObject::unchecked_cast(value.to_strong_heap_object().unwrap());
        isolate.heap.map_of(object.object()).address()
    }

    let migrated = map_of(&mut isolate, "o");
    let untouched = map_of(&mut isolate, "p");
    assert_eq!(migrated, untouched, "a fast object keeps the map it shares");
}

#[test]
fn an_accessor_survives_the_migration() {
    let stores = dictionary_stores(8);
    assert!(is_slow(&format!("{stores}o")));
    assert_eq!(
        evaluate_with_natives_syntax(&format!(
            "{stores}
             Object.defineProperty(o, 'g', {{ get: function() {{ return this.f1 + 100; }} }});
             %ToFastProperties(o);
             o.g"
        )),
        "101"
    );
    assert_eq!(
        evaluate_with_natives_syntax(&format!(
            "{stores}
             var seen = 0;
             Object.defineProperty(o, 's', {{ set: function(v) {{ seen = v; }} }});
             %ToFastProperties(o);
             o.s = 9;
             seen"
        )),
        "9"
    );
}

#[test]
fn a_non_enumerable_entry_stays_non_enumerable_after_the_migration() {
    assert_eq!(
        evaluate_with_natives_syntax(
            "var o = {};
             o.a = 1;
             Object.defineProperty(o, 'h', {
                 get: function() { return 7; },
                 enumerable: false
             });
             delete o.a;
             o.b = 2;
             %ToFastProperties(o);
             Object.keys(o).join(',') + '|' + o.h"
        ),
        "b|7"
    );
}

#[test]
fn the_fields_that_do_not_fit_in_the_object_go_to_a_property_array() {
    use vx::objects::fixed_array::PropertyArray;
    let stores = computed_stores(40);
    let (isolate, object) = to_fast(&format!("{stores}%ToFastProperties(o); o"));
    let inobject = object.map(&isolate.heap).inobject_properties(&isolate.heap);
    assert!(inobject > 0 && inobject < 40);
    let array = PropertyArray::unchecked_cast(
        object
            .raw_properties_or_hash(&isolate.heap)
            .to_strong_heap_object()
            .unwrap(),
    );
    assert_eq!(array.length(&isolate.heap), 40 - inobject);
}

#[test]
fn a_migrated_object_can_go_slow_again() {
    let stores = dictionary_stores(8);
    assert!(is_slow(&format!("{stores}o")));
    assert_eq!(
        evaluate_with_natives_syntax(&format!(
            "{stores}%ToFastProperties(o); delete o.f0; Object.keys(o).length + ',' + o.f7"
        )),
        "7,7"
    );
}

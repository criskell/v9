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

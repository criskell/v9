use vx::execution::isolate::Isolate;
use vx::objects::js_objects::JSObject;

fn evaluate(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => isolate.to_string(value),
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

fn output(source: &str) -> Vec<String> {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    isolate
        .run_script(source)
        .unwrap_or_else(|error| panic!("{source:?} failed: {error}"));
    isolate.take_output()
}

#[test]
fn keys_come_out_in_insertion_order() {
    assert_eq!(
        evaluate(
            "var o = { b: 1, a: 2, c: 3 };
             var names = '';
             for (var k in o) { names = names + k; }
             names"
        ),
        "bac",
        "property order follows insertion, not sorting"
    );
}

#[test]
fn the_body_sees_the_value_through_the_key() {
    assert_eq!(
        evaluate(
            "var o = { a: 1, b: 2, c: 3 };
             var total = 0;
             for (var k in o) { total = total + o[k]; }
             total"
        ),
        "6"
    );
}

#[test]
fn array_indices_are_enumerated_as_strings() {
    assert_eq!(
        evaluate(
            "var a = [10, 20, 30];
             var total = 0;
             for (var i in a) { total = total + a[i]; }
             total"
        ),
        "60",
        "the key is the string \"0\", and indexing with it still has to work"
    );
    assert_eq!(
        evaluate(
            "var a = [1, 2];
             var keys = '';
             for (var i in a) { keys = keys + i; }
             keys"
        ),
        "01"
    );
}

#[test]
fn a_hole_is_skipped() {
    assert_eq!(
        evaluate(
            "var a = [1];
             a[3] = 4;
             var keys = '';
             for (var i in a) { keys = keys + i + ','; }
             keys"
        ),
        "0,3,",
        "the holes between them have no keys"
    );
}

#[test]
fn builtin_methods_are_not_enumerable() {
    // Object.prototype.hasOwnProperty and friends are DONT_ENUM, so a loop
    // over a plain object sees only what the program put there.
    assert_eq!(
        evaluate(
            "var o = { own: 1 };
             var names = '';
             for (var k in o) { names = names + k + ','; }
             names"
        ),
        "own,"
    );
    assert_eq!(
        evaluate(
            "var a = [1];
             var names = '';
             for (var k in a) { names = names + k + ','; }
             names"
        ),
        "0,",
        "Array.prototype.push must not show up either"
    );
}

#[test]
fn inherited_enumerable_keys_are_included() {
    assert_eq!(
        evaluate(
            "var base = { shared: 1 };
             var derived = { own: 2 };
             var names = '';
             for (var k in derived) { names = names + k + ','; }
             names"
        ),
        "own,",
        "without a prototype link there is nothing inherited"
    );
}

#[test]
fn nothing_is_enumerated_for_null_or_undefined() {
    assert_eq!(
        evaluate(
            "var count = 0;
             for (var k in null) { count = count + 1; }
             for (var k2 in undefined) { count = count + 1; }
             count"
        ),
        "0"
    );
}

#[test]
fn break_and_continue_work_inside_the_loop() {
    assert_eq!(
        output(
            "var o = { a: 1, b: 2, c: 3, d: 4 };
             for (var k in o) {
               if (k === 'b') { continue; }
               if (k === 'd') { break; }
               console.log(k);
             }"
        ),
        vec!["a", "c"]
    );
}

#[test]
fn a_dictionary_mode_object_enumerates_in_insertion_order_too() {
    assert_eq!(
        evaluate(
            "var o = {};
             var i = 0;
             while (i < 30) { o['k' + i] = i; i = i + 1; }
             var first = '';
             var count = 0;
             for (var k in o) { if (count === 0) { first = k; } count = count + 1; }
             first + ':' + count"
        ),
        "k0:30",
        "the dictionary records the enumeration index, so order survives"
    );
}

#[test]
fn deleting_during_the_loop_skips_the_deleted_key() {
    // The key list is snapshotted, so ForInNext has to re-check that the
    // object still has each key before handing it to the body.
    assert_eq!(
        evaluate(
            "var o = { a: 1, b: 2, c: 3 };
             var names = '';
             for (var k in o) { delete o.b; names = names + k + ','; }
             names"
        ),
        "a,c,"
    );
}

#[test]
fn the_enum_cache_is_filled_and_shared_by_shape() {
    // The cache hangs off the descriptor array, so two objects of the same
    // shape share it.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    isolate
        .run_script(
            "var a = { x: 1, y: 2 };
             var b = { x: 3, y: 4 };
             var n = 0;
             for (var k in a) { n = n + 1; }",
        )
        .expect("runs");

    let global = isolate.global_object();
    let a = JSObject::unchecked_cast(
        isolate
            .get_property_by_str(global, "a")
            .to_strong_heap_object()
            .unwrap(),
    );
    let b = JSObject::unchecked_cast(
        isolate
            .get_property_by_str(global, "b")
            .to_strong_heap_object()
            .unwrap(),
    );
    assert_eq!(
        a.map(&isolate.heap).address(),
        b.map(&isolate.heap).address(),
        "same shape"
    );

    let cache = a
        .map(&isolate.heap)
        .instance_descriptors(&isolate.heap)
        .enum_cache(&isolate.heap);
    let cache = vx::objects::descriptor_array::EnumCache::unchecked_cast(
        cache.to_strong_heap_object().expect("the cache was filled"),
    );
    let keys = vx::objects::fixed_array::FixedArray::unchecked_cast(
        cache.keys(&isolate.heap).to_strong_heap_object().unwrap(),
    );
    assert_eq!(keys.length(&isolate.heap), 2, "x and y");
}

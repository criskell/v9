use vx::execution::isolate::Isolate;

fn evaluate(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => isolate.to_string(value),
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

fn error(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => panic!("{source:?} returned {}", isolate.to_string(value)),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_getter_runs_on_read() {
    assert_eq!(
        evaluate("var o = { get x() { return 42; } }; o.x"),
        "42"
    );
}

#[test]
fn a_setter_runs_on_write() {
    assert_eq!(
        evaluate("var o = { v: 0, set x(value) { this.v = value + 1; } }; o.x = 10; o.v"),
        "11"
    );
}

#[test]
fn the_receiver_is_this_inside_an_accessor() {
    assert_eq!(
        evaluate("var o = { n: 7, get x() { return this.n; } }; o.x"),
        "7"
    );
}

#[test]
fn a_getter_and_a_setter_share_one_pair() {
    assert_eq!(
        evaluate(
            "var o = { _v: 1, get x() { return this._v * 10; }, set x(v) { this._v = v; } };
             var before = o.x;
             o.x = 3;
             before + ':' + o.x"
        ),
        "10:30"
    );
}

#[test]
fn a_setter_declared_before_its_getter_still_pairs() {
    assert_eq!(
        evaluate(
            "var o = { _v: 0, set x(v) { this._v = v; }, get x() { return this._v + 1; } };
             o.x = 5;
             o.x"
        ),
        "6"
    );
}

#[test]
fn reading_a_setter_only_property_is_undefined() {
    assert_eq!(
        evaluate("var o = { set x(v) { this.v = v; } }; o.x"),
        "undefined"
    );
}

#[test]
fn writing_a_getter_only_property_is_dropped() {
    assert_eq!(
        evaluate("var o = { get x() { return 1; } }; o.x = 9; o.x"),
        "1"
    );
}

#[test]
fn an_accessor_is_found_on_the_prototype() {
    assert_eq!(
        evaluate(
            "function Point(n) { this.n = n; }
             Point.prototype = { get doubled() { return this.n * 2; } };
             var p = new Point(21);
             p.doubled"
        ),
        "42"
    );
}

#[test]
fn a_prototype_setter_runs_with_the_instance_as_this() {
    assert_eq!(
        evaluate(
            "function Box() {}
             Box.prototype = { set value(v) { this.stored = v * 2; } };
             var b = new Box();
             b.value = 4;
             b.stored"
        ),
        "8"
    );
}

#[test]
fn a_throwing_getter_propagates() {
    assert_eq!(
        evaluate(
            "var o = { get x() { throw 'boom'; } };
             var caught = 'none';
             try { o.x; } catch (e) { caught = e; }
             caught"
        ),
        "boom"
    );
}

#[test]
fn a_throwing_setter_propagates() {
    assert!(error("var o = { set x(v) { throw 'nope'; } }; o.x = 1;").contains("nope"));
}

#[test]
fn an_accessor_is_enumerable() {
    assert_eq!(
        evaluate(
            "var o = { a: 1, get x() { return 2; }, b: 3 };
             var names = '';
             for (var k in o) { names = names + k; }
             names"
        ),
        "axb"
    );
}

#[test]
fn a_repeated_read_through_the_inline_cache_still_calls_the_getter() {
    assert_eq!(
        evaluate(
            "var o = { n: 0, get next() { this.n = this.n + 1; return this.n; } };
             var total = 0;
             for (var i = 0; i < 5; i = i + 1) { total = total + o.next; }
             total"
        ),
        "15"
    );
}

#[test]
fn deleting_an_accessor_removes_it() {
    assert_eq!(
        evaluate("var o = { get x() { return 1; } }; delete o.x; o.x"),
        "undefined"
    );
}

#[test]
fn a_getter_only_prototype_accessor_blocks_an_own_assignment() {
    assert_eq!(
        evaluate(
            "function Thing() { this.x = 'own'; }
             Thing.prototype = { get x() { return 'inherited'; } };
             var t = new Thing();
             t.x"
        ),
        "inherited"
    );
}

#[test]
fn an_own_data_property_shadows_a_prototype_accessor() {
    assert_eq!(
        evaluate(
            "function Thing() {}
             Thing.prototype = { get x() { return 'inherited'; } };
             var t = new Thing();
             t.y = 'own';
             t.y + ':' + t.x"
        ),
        "own:inherited"
    );
}

#[test]
fn a_prototype_pair_routes_reads_and_writes_to_the_instance() {
    assert_eq!(
        evaluate(
            "function Thing() {}
             Thing.prototype = {
               get x() { return this.stored; },
               set x(v) { this.stored = v * 3; }
             };
             var t = new Thing();
             t.x = 4;
             t.x"
        ),
        "12"
    );
}

#[test]
fn get_and_set_remain_usable_as_property_names() {
    assert_eq!(
        evaluate("var o = { get: 1, set: 2 }; o.get + o.set"),
        "3"
    );
}

#[test]
fn an_accessor_named_get_is_allowed() {
    assert_eq!(
        evaluate("var o = { get get() { return 5; } }; o.get"),
        "5"
    );
}

#[test]
fn accessors_survive_a_collection() {
    assert_eq!(
        evaluate(
            "var o = { _v: 11, get x() { return this._v; }, set x(v) { this._v = v; } };
             var filler = 0;
             for (var i = 0; i < 400; i = i + 1) { filler = { a: i, b: [i, i, i] }; }
             o.x = o.x + 1;
             o.x"
        ),
        "12"
    );
}

#[test]
fn the_last_getter_for_a_key_wins() {
    assert_eq!(
        evaluate("var o = { get x() { return 1; }, get x() { return 2; } }; o.x"),
        "2"
    );
}

#[test]
fn an_accessor_overrides_an_earlier_data_property() {
    assert_eq!(evaluate("var o = { x: 1, get x() { return 2; } }; o.x"), "2");
}

#[test]
fn a_data_property_overrides_an_earlier_accessor() {
    assert_eq!(
        evaluate("var o = { get x() { return 1; }, x: 2 }; o.x"),
        "2"
    );
}

#[test]
fn a_key_defined_twice_is_enumerated_once() {
    assert_eq!(
        evaluate(
            "var o = { get x() { return 1; }, x: 2 };
             var names = '';
             for (var k in o) { names = names + k; }
             names"
        ),
        "x"
    );
}

#[test]
fn a_split_pair_keeps_the_position_of_the_first_half() {
    assert_eq!(
        evaluate(
            "var o = { a: 1, get x() { return 2; }, b: 3, set x(v) { this.a = v; } };
             var names = '';
             for (var k in o) { names = names + k; }
             names"
        ),
        "axb"
    );
}

#[test]
fn define_property_installs_a_getter() {
    assert_eq!(
        evaluate(
            "var o = {};
             Object.defineProperty(o, 'x', { get: function() { return 7; } });
             o.x"
        ),
        "7"
    );
}

#[test]
fn define_property_adds_a_setter_to_an_existing_getter() {
    assert_eq!(
        evaluate(
            "var o = {};
             Object.defineProperty(o, 'x', { get: function() { return this.v; } });
             Object.defineProperty(o, 'x', { set: function(v) { this.v = v * 2; } });
             o.x = 3;
             o.x"
        ),
        "6"
    );
}

#[test]
fn define_property_adds_a_getter_to_an_existing_setter() {
    assert_eq!(
        evaluate(
            "var o = {};
             Object.defineProperty(o, 'x', { set: function(v) { this.v = v + 1; } });
             Object.defineProperty(o, 'x', { get: function() { return this.v; } });
             o.x = 10;
             o.x"
        ),
        "11"
    );
}

#[test]
fn redefining_the_same_getter_keeps_it() {
    assert_eq!(
        evaluate(
            "var o = {};
             var getter = function() { return 4; };
             Object.defineProperty(o, 'x', { get: getter });
             Object.defineProperty(o, 'x', { get: getter });
             o.x"
        ),
        "4"
    );
}

#[test]
fn define_property_defaults_to_not_enumerable() {
    assert_eq!(
        evaluate(
            "var o = {};
             Object.defineProperty(o, 'x', { get: function() { return 1; } });
             var names = '';
             for (var k in o) { names = names + k; }
             '[' + names + ']'"
        ),
        "[]"
    );
}

#[test]
fn define_property_can_ask_for_an_enumerable_accessor() {
    assert_eq!(
        evaluate(
            "var o = {};
             Object.defineProperty(o, 'x', {
               get: function() { return 1; },
               enumerable: true
             });
             var names = '';
             for (var k in o) { names = names + k; }
             names"
        ),
        "x"
    );
}

#[test]
fn define_property_installs_a_data_property() {
    assert_eq!(
        evaluate("var o = {}; Object.defineProperty(o, 'x', { value: 5 }); o.x"),
        "5"
    );
}

#[test]
fn define_property_rejects_a_non_object_descriptor() {
    assert!(error("Object.defineProperty({}, 'x', 1);").contains("must be an object"));
}

#[test]
fn define_property_rejects_a_non_object_target() {
    assert!(error("Object.defineProperty(1, 'x', {});").contains("non-object"));
}

#[test]
fn an_accessor_can_be_defined_on_a_dictionary_object() {
    assert_eq!(
        evaluate(
            "var o = { a: 1 };
             delete o.a;
             Object.defineProperty(o, 'g', { get: function() { return 7; } });
             o.g"
        ),
        "7"
    );
    assert_eq!(
        evaluate(
            "var o = { a: 1 };
             delete o.a;
             var seen = 0;
             Object.defineProperty(o, 's', { set: function(v) { seen = v; } });
             o.s = 4;
             seen"
        ),
        "4"
    );
    assert_eq!(
        evaluate(
            "var o = { a: 1, b: 2 };
             delete o.a;
             Object.defineProperty(o, 'g', { get: function() { return this.b; } });
             o.b = 9;
             o.g + ',' + Object.keys(o).join('')"
        ),
        "9,bg"
    );
}

#[test]
fn normalizing_an_object_keeps_its_accessor() {
    assert_eq!(
        evaluate(
            "var o = { a: 1, get g() { return 7; } };
             delete o.a;
             o.g"
        ),
        "7"
    );
    assert_eq!(
        evaluate(
            "var o = { a: 1 };
             var seen = 0;
             Object.defineProperty(o, 's', { set: function(v) { seen = v; } });
             delete o.a;
             o.s = 3;
             seen"
        ),
        "3"
    );
}

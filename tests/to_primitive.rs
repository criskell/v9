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
fn plus_asks_value_of_first() {
    assert_eq!(
        evaluate("var o = { valueOf: function() { return 5; } }; o + 1"),
        "6"
    );
    assert_eq!(
        evaluate(
            "var o = { valueOf: function() { return 1; } };
             var p = { valueOf: function() { return 2; } };
             o + p"
        ),
        "3"
    );
    assert_eq!(
        evaluate(
            "var o = { valueOf: function() { return 5; }, toString: function() { return 'm'; } };
             '' + o"
        ),
        "5"
    );
}

#[test]
fn plus_falls_back_to_to_string() {
    assert_eq!(
        evaluate("var o = { toString: function() { return 'm'; } }; '' + o"),
        "m"
    );
    assert_eq!(
        evaluate("var o = { toString: function() { return 'm'; } }; o + 1"),
        "m1"
    );
    assert_eq!(evaluate("'' + {}"), "[object Object]");
}

#[test]
fn arithmetic_converts_with_the_number_hint() {
    assert_eq!(
        evaluate("var o = { valueOf: function() { return 5; } }; o * 2"),
        "10"
    );
    assert_eq!(
        evaluate("var o = { valueOf: function() { return 5; } }; o - 2"),
        "3"
    );
    assert_eq!(
        evaluate("var o = { valueOf: function() { return 5; } }; -o"),
        "-5"
    );
    assert_eq!(
        evaluate("var o = { valueOf: function() { return 5; } }; o & 3"),
        "1"
    );
    assert_eq!(
        evaluate("var o = { valueOf: function() { return 5; } }; o << 1"),
        "10"
    );
}

#[test]
fn a_relational_comparison_converts_both_sides() {
    assert_eq!(
        evaluate(
            "var o = { valueOf: function() { return 5; } };
             (o < 6) + ',' + (o > 6) + ',' + (o <= 5)"
        ),
        "true,false,true"
    );
}

#[test]
fn loose_equality_converts_an_object_but_strict_equality_does_not() {
    assert_eq!(
        evaluate(
            "var o = { valueOf: function() { return 5; } };
             (o == 5) + ',' + (o === 5)"
        ),
        "true,false"
    );
    assert_eq!(
        evaluate("var o = { toString: function() { return 'm'; } }; o == 'm'"),
        "true"
    );
    assert_eq!(evaluate("var o = {}; (o == o) + ',' + (o == {})"), "true,false");
}

#[test]
fn a_property_key_is_converted_with_the_string_hint() {
    assert_eq!(
        evaluate(
            "var k = { valueOf: function() { return 5; }, toString: function() { return 'm'; } };
             var o = {};
             o[k] = 1;
             o.m"
        ),
        "1"
    );
    assert_eq!(
        evaluate(
            "var k = { toString: function() { return 'm'; } };
             class A { [k]() { return 1; } }
             (new A()).m()"
        ),
        "1"
    );
}

#[test]
fn a_template_substitution_is_converted_with_the_string_hint() {
    assert_eq!(
        evaluate("var o = { toString: function() { return 'm'; } }; `a${o}b`"),
        "amb"
    );
    assert_eq!(
        evaluate("var o = { valueOf: function() { return 5; } }; `${o}`"),
        "[object Object]"
    );
}

#[test]
fn a_conversion_that_returns_an_object_throws() {
    assert!(error(
        "var o = { valueOf: function() { return {}; }, toString: function() { return {}; } };
         '' + o"
    )
    .contains("Cannot convert object to primitive value"));
}

#[test]
fn a_conversion_that_throws_propagates() {
    assert_eq!(
        evaluate(
            "var o = { toString: function() { throw 'boom'; } };
             var caught = 'no';
             try { '' + o; } catch (e) { caught = e; }
             caught"
        ),
        "boom"
    );
}

#[test]
fn object_prototype_has_value_of_and_to_string() {
    assert_eq!(
        evaluate("var o = {}; (o.valueOf() === o) + ',' + o.toString()"),
        "true,[object Object]"
    );
}

#[test]
fn a_conversion_runs_the_method_with_the_object_as_this() {
    assert_eq!(
        evaluate(
            "var o = { v: 7, valueOf: function() { return this.v; } };
             o + 1"
        ),
        "8"
    );
}

#[test]
fn an_inherited_conversion_method_is_found() {
    assert_eq!(
        evaluate(
            "class A { valueOf() { return 5; } }
             (new A()) + 1"
        ),
        "6"
    );
}

#[test]
fn a_conversion_method_can_be_an_accessor() {
    assert_eq!(
        evaluate(
            "var o = {};
             Object.defineProperty(o, 'valueOf', {
                 get: function() { return function() { return 5; }; }
             });
             o + 1"
        ),
        "6"
    );
}

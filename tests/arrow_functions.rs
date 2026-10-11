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
fn one_unparenthesised_parameter() {
    assert_eq!(evaluate("var f = x => x + 1; f(2)"), "3");
}

#[test]
fn several_parenthesised_parameters() {
    assert_eq!(evaluate("var f = (a, b) => a * b; f(3, 4)"), "12");
}

#[test]
fn no_parameters() {
    assert_eq!(evaluate("var f = () => 7; f()"), "7");
}

#[test]
fn one_parenthesised_parameter() {
    assert_eq!(evaluate("var f = (a) => a; f('p')"), "p");
}

#[test]
fn a_block_body_needs_an_explicit_return() {
    assert_eq!(evaluate("var f = x => { return x * 2; }; f(5)"), "10");
}

#[test]
fn a_block_body_without_a_return_gives_undefined() {
    assert_eq!(evaluate("var f = x => { x * 2; }; typeof f(5)"), "undefined");
}

#[test]
fn an_arrow_is_a_function() {
    assert_eq!(evaluate("typeof (x => x)"), "function");
}

#[test]
fn a_missing_argument_is_undefined() {
    assert_eq!(evaluate("var f = (a, b) => typeof b; f(1)"), "undefined");
}

#[test]
fn an_object_literal_body_needs_parentheses() {
    assert_eq!(evaluate("var f = x => ({ v: x }); f(5).v"), "5");
}

#[test]
fn arrows_curry() {
    assert_eq!(evaluate("var f = x => y => x + y; f(1)(2)"), "3");
}

#[test]
fn an_arrow_closes_over_the_enclosing_scope() {
    assert_eq!(
        evaluate(
            "function outer() { var n = 10; var f = () => n + 1; return f(); }
             outer()"
        ),
        "11"
    );
}

#[test]
fn this_inside_an_arrow_is_the_enclosing_this() {
    assert_eq!(
        evaluate("var o = { v: 10, m: function() { var g = () => this.v; return g(); } }; o.m()"),
        "10"
    );
}

#[test]
fn this_inside_an_arrow_in_a_method_is_the_instance() {
    assert_eq!(
        evaluate(
            "class C {
               constructor() { this.v = 3; }
               m() { var g = () => this.v * 2; return g(); }
             }
             new C().m()"
        ),
        "6"
    );
}

#[test]
fn this_is_the_same_object_inside_and_outside_an_arrow() {
    assert_eq!(
        evaluate("(function() { var self = this; var g = () => self === this; return g(); })()"),
        "true"
    );
}

#[test]
fn super_works_inside_an_arrow() {
    assert_eq!(
        evaluate(
            "class A { m() { return 1; } }
             class B extends A { m() { var g = () => super.m() + 1; return g(); } }
             new B().m()"
        ),
        "2"
    );
}

#[test]
fn an_arrow_nested_in_an_arrow_keeps_the_outer_this() {
    assert_eq!(
        evaluate(
            "var o = { v: 4, m: function() { var g = () => () => this.v; return g()(); } };
             o.m()"
        ),
        "4"
    );
}

#[test]
fn an_arrow_may_be_a_method_value() {
    assert_eq!(evaluate("var o = { m: (x) => x * 3 }; o.m(4)"), "12");
}

#[test]
fn arrows_live_in_arrays() {
    assert_eq!(
        evaluate("var a = [x => x, y => y + 1]; a[0](1) + a[1](1)"),
        "3"
    );
}

#[test]
fn a_parenthesised_expression_is_still_a_parenthesised_expression() {
    assert_eq!(evaluate("var x = (1 + 2) * 3; x"), "9");
}

#[test]
fn a_comma_in_parentheses_is_still_a_sequence() {
    assert_eq!(evaluate("var x = (1, 2); x"), "2");
}

#[test]
fn a_sequence_may_be_an_arrow_body() {
    assert_eq!(evaluate("var g = (a, b) => (a, b); g(1, 2)"), "2");
}

#[test]
fn an_arrow_may_be_parenthesised() {
    assert_eq!(evaluate("var f = ((x) => x); f(4)"), "4");
}

#[test]
fn a_newline_before_the_arrow_is_refused() {
    assert!(error("var f = x\n=> x; f(1);").contains("no newline before `=>`"));
}

#[test]
fn a_newline_after_the_arrow_is_allowed() {
    assert_eq!(evaluate("var f = x =>\n x; f(1)"), "1");
}

#[test]
fn an_arrow_runs_as_a_callback() {
    assert_eq!(
        evaluate(
            "function apply(f, v) { return f(v); }
             apply(x => x * x, 6)"
        ),
        "36"
    );
}

#[test]
fn an_arrow_survives_a_collection() {
    assert_eq!(
        evaluate(
            "var o = { v: 21, m: function() { return () => this.v * 2; } };
             var f = o.m();
             var filler = 0;
             for (var i = 0; i < 400; i = i + 1) { filler = { a: i, b: [i, i, i] }; }
             f()"
        ),
        "42"
    );
}

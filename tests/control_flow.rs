use vx::execution::isolate::Isolate;
use vx::interpreter::bytecode_generator::BytecodeGenerator;

fn evaluate(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => isolate.to_string(value),
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

fn script_listing(source: &str) -> String {
    let program = vx::parsing::parser::Parser::parse_program(source).expect("parses");
    let compiled = BytecodeGenerator::generate(&program).expect("compiles");
    let function = compiled
        .functions
        .iter()
        .find(|function| function.name == "<script>")
        .expect("the script");
    vx::interpreter::bytecode_array_iterator::disassemble(&function.data.bytecodes, &[])
}

#[test]
fn a_negated_condition_takes_the_right_branch() {
    assert_eq!(evaluate("var r = 1; if (!1) { r = 2; } r"), "1");
    assert_eq!(evaluate("var r = 1; if (!0) { r = 2; } r"), "2");
    assert_eq!(evaluate("var r = 1; if (!1) { r = 2; } else { r = 3; } r"), "3");
    assert_eq!(evaluate("var r = 0; if (!!1) { r = 1; } r"), "1");
    assert_eq!(evaluate("var r = !0 ? 5 : 6; r"), "5");
    assert_eq!(evaluate("var r = !1 ? 5 : 6; r"), "6");
}

#[test]
fn a_negated_condition_inverts_the_jump_instead_of_negating_a_value() {
    let listing = script_listing("var a = 1; if (!a) { a = 2; }");
    assert!(
        !listing.contains("LogicalNot"),
        "the test should be inverted, not the value: {listing}"
    );
    assert!(
        listing.contains("JumpIfToBooleanTrue"),
        "a truthy operand must skip the body: {listing}"
    );
}

#[test]
fn a_negated_loop_condition_takes_the_right_branch() {
    assert_eq!(
        evaluate(
            "var a = 1, b = 1, n = 0;
             while (!(a && b)) { n = n + 1; if (n > 3) { break; } }
             n"
        ),
        "0"
    );
    assert_eq!(
        evaluate("var n = 0; while (!(n > 2)) { n = n + 1; } n"),
        "3"
    );
}

#[test]
fn logical_operators_short_circuit_in_a_condition() {
    assert_eq!(
        evaluate(
            "var calls = '';
             function t() { calls = calls + 't'; return true; }
             function f() { calls = calls + 'f'; return false; }
             var r = 'no';
             if (f() && t()) { r = 'yes'; }
             r + ',' + calls"
        ),
        "no,f"
    );
    assert_eq!(
        evaluate(
            "var calls = '';
             function t() { calls = calls + 't'; return true; }
             function f() { calls = calls + 'f'; return false; }
             var r = 'no';
             if (t() || f()) { r = 'yes'; }
             r + ',' + calls"
        ),
        "yes,t"
    );
}

#[test]
fn logical_operators_in_a_condition_pick_the_right_branch() {
    for (source, expected) in [
        ("if (1 && 2) { r = 'a'; } else { r = 'b'; }", "a"),
        ("if (1 && 0) { r = 'a'; } else { r = 'b'; }", "b"),
        ("if (0 && 1) { r = 'a'; } else { r = 'b'; }", "b"),
        ("if (0 || 1) { r = 'a'; } else { r = 'b'; }", "a"),
        ("if (0 || 0) { r = 'a'; } else { r = 'b'; }", "b"),
        ("if (!(0 || 0)) { r = 'a'; } else { r = 'b'; }", "a"),
        ("if (!(1 && 1)) { r = 'a'; } else { r = 'b'; }", "b"),
        ("if (1 && 1 && 0) { r = 'a'; } else { r = 'b'; }", "b"),
        ("if (0 || 0 || 1) { r = 'a'; } else { r = 'b'; }", "a"),
    ] {
        assert_eq!(
            evaluate(&format!("var r = 'none'; {source} r")),
            expected,
            "{source}"
        );
    }
}

#[test]
fn a_comparison_in_a_condition_jumps_without_converting_to_boolean() {
    let listing = script_listing("var a = 1, b = 2; if (a < b) { a = 3; }");
    assert!(
        listing.contains("JumpIfFalse"),
        "a comparison already produces a boolean: {listing}"
    );
    assert!(
        !listing.contains("JumpIfToBooleanFalse"),
        "no conversion is needed: {listing}"
    );
}

#[test]
fn a_logical_value_is_still_a_value_outside_a_condition() {
    assert_eq!(evaluate("var r = 0 || 'x'; r"), "x");
    assert_eq!(evaluate("var r = 1 && 'x'; r"), "x");
    assert_eq!(evaluate("var r = 0 && 'x'; r"), "0");
    assert_eq!(evaluate("var r = !0; r"), "true");
    assert_eq!(evaluate("var r = !1; r"), "false");
    assert_eq!(evaluate("var r = !(1 < 2); r"), "false");
}

#[test]
fn an_expression_visited_for_effect_still_runs_its_calls() {
    assert_eq!(
        evaluate(
            "var calls = '';
             function f() { calls = calls + 'f'; return 1; }
             f();
             (f(), f());
             calls"
        ),
        "fff"
    );
    assert_eq!(
        evaluate(
            "var calls = '';
             function f() { calls = calls + 'f'; return 1; }
             var r = (f(), 7);
             r + ',' + calls"
        ),
        "7,f"
    );
}

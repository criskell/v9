//! Closures: the variable, not a copy of it.
//!
//! Scope analysis decides where each variable lives, so these tests are
//! written as claims about *sharing* -- two closures that see each other's
//! writes, one closure per call that does not -- rather than about the
//! bytecode. A port that allocated a fresh copy of the variable per closure
//! would still run every script in this file; only the sharing tests would
//! fail.
//!
//! Two of them do read the bytecode, for the one thing behaviour cannot
//! show: that a function nothing captures from allocates no context at all,
//! and that a reference two contexts out is reached by depth rather than by
//! search.

use vx::execution::isolate::Isolate;
use vx::interpreter::bytecode_generator::BytecodeGenerator;
use vx::interpreter::constant_array_builder::ConstantEntry;

/// The script's completion value, as a string.
fn evaluate(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => isolate.to_string(value),
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

/// The disassembly of the named function, `<script>` for the top level.
fn listing_of(source: &str, name: &str) -> String {
    let program = vx::parsing::parser::Parser::parse_program(source).expect("parses");
    let compiled = BytecodeGenerator::generate(&program).expect("compiles");
    let function = compiled
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no function {name:?}; there are {:?}",
                compiled
                    .functions
                    .iter()
                    .map(|function| function.name.clone())
                    .collect::<Vec<_>>()
            )
        });
    let constants: Vec<String> = function
        .data
        .constants
        .iter()
        .map(|entry| match entry {
            ConstantEntry::ScopeInfo(names) => format!("<scope {}>", names.join(", ")),
            other => format!("{other:?}"),
        })
        .collect();
    vx::interpreter::bytecode_array_iterator::disassemble(&function.data.bytecodes, &constants)
}

#[test]
fn a_closure_keeps_its_variable_alive_after_the_call_returns() {
    // `n` outlives the frame that declared it, which is the whole reason a
    // captured variable cannot be a register.
    assert_eq!(
        evaluate(
            "function makeCounter() {
               var n = 0;
               return function () { n = n + 1; return n; };
             }
             var next = makeCounter();
             next(); next(); next()"
        ),
        "3"
    );
}

#[test]
fn two_closures_from_one_call_share_one_variable() {
    // The test a per-closure copy would fail: `get` sees what `inc` wrote.
    assert_eq!(
        evaluate(
            "function make() {
               var n = 0;
               return {
                 inc: function () { n = n + 1; return n; },
                 get: function () { return n; }
               };
             }
             var c = make();
             c.inc(); c.inc();
             c.get()"
        ),
        "2"
    );
}

#[test]
fn each_call_captures_its_own_variable() {
    // The other half: one context per *invocation*, not one per function.
    assert_eq!(
        evaluate(
            "function make() {
               var n = 0;
               return function () { n = n + 1; return n; };
             }
             var a = make();
             var b = make();
             a(); a();
             a() + b()"
        ),
        "4",
        "a's third call is 3 and b's first is 1"
    );
}

#[test]
fn the_enclosing_function_sees_what_the_closure_wrote() {
    // Capture is not one-way. `n` is one slot, read and written from both
    // frames.
    assert_eq!(
        evaluate(
            "function f() {
               var n = 1;
               var g = function () { n = 42; };
               g();
               return n;
             }
             f()"
        ),
        "42"
    );
}

#[test]
fn a_parameter_can_be_captured() {
    // A parameter arrives in a frame slot and has to be copied into the
    // context on entry, or the closure would read a slot that was never
    // written.
    assert_eq!(
        evaluate(
            "function adder(x) { return function (y) { return x + y; }; }
             adder(10)(5)"
        ),
        "15"
    );
}

#[test]
fn capture_reaches_through_a_function_that_captures_nothing() {
    // `middle` declares nothing that is captured, so it allocates no
    // context and `inner` finds `a` in the context it was created in.
    assert_eq!(
        evaluate(
            "function outer() {
               var a = 1;
               function middle() { return function inner() { return a + 1; }; }
               return middle()();
             }
             outer()"
        ),
        "2"
    );
}

#[test]
fn two_levels_of_capture_resolve_to_different_depths() {
    assert_eq!(
        evaluate(
            "function a() {
               var x = 1;
               return function b() {
                 var y = 2;
                 return function c() { return x + y; };
               };
             }
             a()()()"
        ),
        "3"
    );
}

#[test]
fn closures_in_a_loop_share_the_var_they_capture() {
    // The classic. With `var` there is one `i`, so every closure reports the
    // value it had when the loop ended -- 3 each. A port that gave each
    // iteration its own slot would answer 0 + 1 + 2.
    assert_eq!(
        evaluate(
            "var fs = [];
             var i;
             for (i = 0; i < 3; i = i + 1) { fs[i] = function () { return i; }; }
             fs[0]() + fs[1]() + fs[2]()"
        ),
        "9"
    );
}

#[test]
fn a_function_nothing_captures_from_allocates_no_context() {
    // Behaviour cannot show this: a context per call would compute the same
    // answers, just slower. Contexts are allocated only where capture
    // forces it, which is `Scope::NeedsContext`.
    let plain = listing_of("function plain() { var n = 0; return n; }", "plain");
    assert!(
        !plain.contains("CreateFunctionContext"),
        "nothing captures n, so it stays in a register:\n{plain}"
    );
    assert!(plain.contains("Star r0"), "{plain}");

    let capturing = listing_of(
        "function capturing() { var n = 0; return function () { return n; }; }",
        "capturing",
    );
    assert!(
        capturing.contains("CreateFunctionContext"),
        "one capture is enough to force a context:\n{capturing}"
    );
    assert!(
        capturing.contains("PushContext"),
        "the context has to become the frame's current one:\n{capturing}"
    );
    assert!(
        capturing.contains("StaCurrentContextSlot 2"),
        "slot 2 is the first after scope_info and previous:\n{capturing}"
    );
}

#[test]
fn a_reference_two_contexts_out_is_reached_by_depth() {
    // `c` reads y from its own context and x from one link further out, so
    // the two loads use different bytecodes -- no search at runtime.
    let inner = listing_of(
        "function a() {
           var x = 1;
           return function b() {
             var y = 2;
             return function c() { return x + y; };
           };
         }",
        "c",
    );
    assert!(
        inner.contains("LdaContextSlot <context>, 2, 1"),
        "x is slot 2 at depth 1:\n{inner}"
    );
    assert!(
        inner.contains("LdaCurrentContextSlot 2"),
        "y is slot 2 at depth 0:\n{inner}"
    );
}

#[test]
fn a_function_declaration_is_visible_from_another_function() {
    assert_eq!(
        evaluate("function f() { return 1; } function g() { return f(); } g()"),
        "1"
    );
    assert_eq!(
        evaluate(
            "function even(n) { if (n === 0) { return true; } return odd(n - 1); }
             function odd(n) { if (n === 0) { return false; } return even(n - 1); }
             even(10) + ',' + even(7)"
        ),
        "true,false"
    );
    assert_eq!(
        evaluate(
            "function f() { return 1; }
             function g() { function h() { return f() + 1; } return h(); }
             g()"
        ),
        "2"
    );
}

#[test]
fn a_captured_function_declaration_goes_to_a_context_slot() {
    let listing = listing_of(
        "function f() { return 1; } function g() { return f(); } g()",
        "<script>",
    );
    assert!(
        listing.contains("StaCurrentContextSlot"),
        "the closure must be stored where the inner function reads it: {listing}"
    );
}

#[test]
fn an_uncaptured_function_declaration_stays_a_global() {
    let listing = listing_of("function f() { return 1; } f()", "<script>");
    assert!(
        listing.contains("StaGlobal"),
        "nothing captures f, so it is a global property: {listing}"
    );
}

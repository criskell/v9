use vx::execution::isolate::Isolate;

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

fn uncaught(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => panic!(
            "{source:?} should have thrown, produced {:?}",
            isolate.to_string(value)
        ),
        Err(error) => error,
    }
}

#[test]
fn a_throw_inside_try_reaches_the_catch_block() {
    assert_eq!(
        output(
            "try { console.log('a'); throw 'boom'; console.log('b'); }
             catch (e) { console.log('caught ' + e); }
             console.log('c');"
        ),
        vec!["a", "caught boom", "c"],
        "the statement after the throw must not run, and the catch must"
    );
}

#[test]
fn the_catch_binding_holds_the_thrown_value_unchanged() {
    assert_eq!(
        evaluate("var r; try { throw 42; } catch (e) { r = e; } r"),
        "42"
    );
    assert_eq!(
        evaluate("var r; try { throw 42; } catch (e) { r = typeof e; } r"),
        "number"
    );
    assert_eq!(
        evaluate("var r; try { throw 'x'; } catch (e) { r = typeof e; } r"),
        "string"
    );
    assert_eq!(
        evaluate("var r; try { throw { code: 7 }; } catch (e) { r = e.code; } r"),
        "7",
        "an object keeps its identity through the unwind"
    );
}

#[test]
fn a_try_that_does_not_throw_skips_its_catch() {
    assert_eq!(
        output("var n = 0; try { n = 1; } catch (e) { n = 2; } console.log(n);"),
        vec!["1"]
    );
}

#[test]
fn a_throw_crosses_a_call_boundary() {
    // The callee has no handler, so the exception leaves its frame and the
    // caller's handler table is what gets consulted.
    assert_eq!(
        evaluate(
            "function f() { throw 'from f'; }
             var r; try { f(); } catch (e) { r = e; } r"
        ),
        "from f"
    );
    assert_eq!(
        evaluate(
            "function deep(n) { if (n === 0) { throw 'bottom'; } return deep(n - 1); }
             var r; try { deep(5); } catch (e) { r = e; } r"
        ),
        "bottom",
        "five frames have to unwind"
    );
}

#[test]
fn nested_handlers_resolve_to_the_innermost_one() {
    assert_eq!(
        output(
            "try {
               try { throw 'inner'; } catch (e) { console.log('inner ' + e); throw 'again'; }
             } catch (e) { console.log('outer ' + e); }"
        ),
        vec!["inner inner", "outer again"],
        "a throw from inside a catch block belongs to the outer handler"
    );
}

#[test]
fn a_runtime_error_is_catchable() {
    // Not just explicit throws: the engine's own errors unwind the same way.
    assert_eq!(
        evaluate("var r; try { missingVariable; } catch (e) { r = typeof e; } r"),
        "string"
    );
    assert_eq!(
        evaluate("var r; var o; try { o.x; } catch (e) { r = e; } r"),
        "Cannot read property 'x' of undefined"
    );
    assert_eq!(
        evaluate("var r; var f = 1; try { f(); } catch (e) { r = typeof e; } r"),
        "string"
    );
}

#[test]
fn return_from_inside_try_still_returns() {
    assert_eq!(
        evaluate("function f() { try { return 'try'; } catch (e) { return 'catch'; } } f()"),
        "try"
    );
    assert_eq!(
        evaluate(
            "function f() { try { throw 1; } catch (e) { return 'catch'; } return 'after'; } f()"
        ),
        "catch"
    );
}

#[test]
fn an_uncaught_throw_still_leaves_the_script() {
    assert_eq!(uncaught("throw 'escaped';"), "escaped");
    assert_eq!(
        uncaught("try { throw 'a'; } catch (e) { throw 'b'; }"),
        "b",
        "a throw from the catch block has no handler left"
    );
    assert_eq!(
        uncaught("function f() { throw 'x'; } try { } catch (e) { } f();"),
        "x",
        "the handler's range must not cover what comes after it"
    );
}

#[test]
fn a_catch_without_a_binding_is_allowed() {
    assert_eq!(
        output("try { throw 'ignored'; } catch { console.log('caught'); }"),
        vec!["caught"]
    );
}

#[test]
fn the_loop_control_flow_around_a_try_still_works() {
    assert_eq!(
        evaluate(
            "var total = 0;
             var i = 0;
             while (i < 5) {
               try { if (i === 2) { throw i; } total = total + 1; }
               catch (e) { total = total + 100; }
               i = i + 1;
             }
             total"
        ),
        "104"
    );
}

#[test]
fn a_catch_block_sees_the_surrounding_scope() {
    assert_eq!(
        evaluate(
            "function f() {
               var n = 10;
               try { throw 5; } catch (e) { n = n + e; }
               return n;
             }
             f()"
        ),
        "15"
    );
}

#[test]
fn the_handler_table_covers_only_the_try_region() {
    use vx::interpreter::bytecode_generator::BytecodeGenerator;
    let program =
        vx::parsing::parser::Parser::parse_program("try { throw 1; } catch (e) { } throw 2;")
            .expect("parses");
    let compiled = BytecodeGenerator::generate(&program).expect("compiles");
    let table = &compiled.functions[0].data.handler_table;
    assert_eq!(table.number_of_range_entries(), 1);

    let start = table.get_range_start(0);
    let end = table.get_range_end(0);
    let handler = table.get_range_handler(0);
    assert!(start < end, "the range has to be non-empty");
    assert!(
        handler >= end,
        "the handler at {handler} must sit outside the range it guards ({start}..{end})"
    );
}

#[test]
fn a_finally_block_runs_on_the_normal_path() {
    assert_eq!(
        output("try { console.log('try'); } finally { console.log('finally'); }"),
        vec!["try", "finally"]
    );
}

#[test]
fn a_finally_block_runs_before_the_exception_leaves() {
    assert_eq!(
        output(
            "function g() { try { throw 'x'; } finally { console.log('finally'); } }
             try { g(); } catch (e) { console.log('caught ' + e); }"
        ),
        vec!["finally", "caught x"],
        "the finally runs first, then the exception continues outward"
    );
}

#[test]
fn a_return_from_the_try_runs_the_finally_first() {
    assert_eq!(
        output(
            "function f() { try { return 'returned'; } finally { console.log('finally'); } }
             console.log(f());"
        ),
        vec!["finally", "returned"],
        "the return value is held in a register while the finally runs"
    );
}

#[test]
fn a_return_in_the_finally_overrides_the_one_in_the_try() {
    assert_eq!(
        evaluate("function f() { try { return 1; } finally { return 2; } } f()"),
        "2"
    );
}

#[test]
fn a_finally_runs_for_catch_too() {
    assert_eq!(
        output(
            "try { throw 'a'; }
             catch (e) { console.log('caught ' + e); }
             finally { console.log('finally'); }"
        ),
        vec!["caught a", "finally"]
    );
    assert_eq!(
        output(
            "try { console.log('ok'); }
             catch (e) { console.log('not reached'); }
             finally { console.log('finally'); }"
        ),
        vec!["ok", "finally"]
    );
}

#[test]
fn nested_finallys_run_from_the_inside_out() {
    assert_eq!(
        output(
            "function f() {
               try {
                 try { return 'r'; } finally { console.log('inner'); }
               } finally { console.log('outer'); }
             }
             console.log(f());"
        ),
        vec!["inner", "outer", "r"]
    );
}

#[test]
fn a_throw_from_the_finally_replaces_the_original() {
    assert_eq!(
        evaluate(
            "var r;
             try {
               try { throw 'first'; } finally { throw 'second'; }
             } catch (e) { r = e; }
             r"
        ),
        "second"
    );
}

#[test]
fn a_transfer_out_of_a_try_with_finally_is_refused() {
    // V8 routes break and continue through the same token dispatch as a
    // return. `vx` only records returns, so it refuses rather than skipping
    // the finally block.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    let error = isolate
        .run_script("while (true) { try { break; } finally { } }")
        .expect_err("should be refused");
    assert!(
        error.contains("`break` across a `finally`"),
        "unexpected error: {error}"
    );

    // A loop wholly inside the try is fine: the break does not cross it.
    assert_eq!(
        evaluate(
            "var n = 0;
             try { while (true) { n = 1; break; } } finally { n = n + 1; }
             n"
        ),
        "2"
    );
}

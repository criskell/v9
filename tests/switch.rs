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
fn a_matching_case_runs() {
    assert_eq!(
        evaluate("var r = ''; switch (1) { case 1: r = 'one'; } r"),
        "one"
    );
}

#[test]
fn a_non_matching_case_is_skipped() {
    assert_eq!(
        evaluate("var r = 'none'; switch (9) { case 1: r = 'one'; } r"),
        "none"
    );
}

#[test]
fn cases_fall_through_without_break() {
    assert_eq!(
        evaluate(
            "var r = '';
             switch (1) { case 1: r = 'a'; case 2: r = r + 'b'; case 3: r = r + 'c'; }
             r"
        ),
        "abc"
    );
}

#[test]
fn break_leaves_the_switch() {
    assert_eq!(
        evaluate(
            "var r = '';
             switch (1) { case 1: r = 'a'; break; case 2: r = r + 'b'; }
             r"
        ),
        "a"
    );
}

#[test]
fn an_empty_case_falls_into_the_next_one() {
    assert_eq!(
        evaluate(
            "function f(n) {
               switch (n) {
                 case 3:
                 case 4: return 'three or four';
                 default: return 'other';
               }
             }
             f(3) + ':' + f(4) + ':' + f(5)"
        ),
        "three or four:three or four:other"
    );
}

#[test]
fn the_default_clause_runs_when_nothing_matches() {
    assert_eq!(
        evaluate("var r = ''; switch (9) { case 1: r = 'one'; default: r = 'many'; } r"),
        "many"
    );
}

#[test]
fn the_default_clause_is_skipped_when_a_case_matches() {
    assert_eq!(
        evaluate(
            "var r = '';
             switch (1) { case 1: r = 'one'; break; default: r = 'many'; }
             r"
        ),
        "one"
    );
}

#[test]
fn a_default_clause_may_come_first() {
    assert_eq!(
        evaluate("var r = ''; switch (2) { default: r = 'd'; break; case 2: r = 'c'; } r"),
        "c"
    );
}

#[test]
fn an_empty_switch_does_nothing() {
    assert_eq!(evaluate("var r = 'x'; switch (9) {} r"), "x");
}

#[test]
fn labels_are_compared_strictly() {
    assert_eq!(
        evaluate(
            "var r = '';
             switch (1) { case '1': r = 'loose'; break; default: r = 'strict'; }
             r"
        ),
        "strict"
    );
}

#[test]
fn a_string_tag_matches_a_string_label() {
    assert_eq!(
        evaluate("var r = ''; switch ('a') { case 'a': r = 'str'; } r"),
        "str"
    );
}

#[test]
fn a_case_label_may_be_an_expression() {
    assert_eq!(
        evaluate("var n = 2; var r = ''; switch (n) { case 1 + 1: r = 'computed'; } r"),
        "computed"
    );
}

#[test]
fn the_tag_is_evaluated_once() {
    assert_eq!(
        evaluate(
            "var calls = 0;
             function tag() { calls = calls + 1; return 1; }
             var r = '';
             switch (tag()) { case 1: r = 'hit'; }
             r + calls"
        ),
        "hit1"
    );
}

#[test]
fn labels_are_evaluated_until_one_matches() {
    assert_eq!(
        evaluate(
            "var seen = '';
             function label(n) { seen = seen + n; return n; }
             switch (2) { case label(1): break; case label(2): break; case label(3): break; }
             seen"
        ),
        "12"
    );
}

#[test]
fn break_inside_a_switch_does_not_leave_the_loop() {
    assert_eq!(
        evaluate(
            "var total = 0;
             for (var i = 0; i < 5; i = i + 1) {
               switch (i) { case 2: break; }
               total = total + i;
             }
             total"
        ),
        "10"
    );
}

#[test]
fn continue_inside_a_switch_reaches_the_loop() {
    assert_eq!(
        evaluate(
            "var total = 0;
             for (var i = 0; i < 5; i = i + 1) {
               switch (i) { case 2: continue; }
               total = total + i;
             }
             total"
        ),
        "8"
    );
}

#[test]
fn a_switch_nested_in_a_switch_breaks_to_its_own_end() {
    assert_eq!(
        evaluate(
            "var r = '';
             switch (1) {
               case 1:
                 switch (2) { case 2: r = r + 'inner'; break; }
                 r = r + '|outer';
                 break;
             }
             r"
        ),
        "inner|outer"
    );
}

#[test]
fn a_var_declared_in_a_case_is_hoisted() {
    assert_eq!(evaluate("switch (1) { case 1: var v = 5; } v"), "5");
}

#[test]
fn a_switch_returns_from_a_function() {
    assert_eq!(
        evaluate(
            "function kind(n) {
               switch (n) {
                 case 0: return 'zero';
                 case 1: return 'one';
                 default: return 'many';
               }
             }
             kind(0) + ':' + kind(1) + ':' + kind(7)"
        ),
        "zero:one:many"
    );
}

#[test]
fn two_default_clauses_are_refused() {
    assert!(
        error("switch (1) { default: break; default: break; }")
            .contains("More than one default clause")
    );
}

#[test]
fn a_switch_survives_a_collection() {
    assert_eq!(
        evaluate(
            "var total = 0;
             for (var i = 0; i < 400; i = i + 1) {
               var filler = { a: i, b: [i, i, i] };
               switch (i % 3) {
                 case 0: total = total + 1; break;
                 case 1: total = total + 10; break;
                 default: total = total + 100;
               }
             }
             total"
        ),
        "14764"
    );
}

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
fn a_template_without_substitutions_is_a_string() {
    assert_eq!(evaluate("`plain`"), "plain");
}

#[test]
fn an_empty_template_is_the_empty_string() {
    assert_eq!(evaluate("`` + 'x'"), "x");
}

#[test]
fn one_substitution_in_the_middle() {
    assert_eq!(evaluate("var n = 2; `n is ${n}`"), "n is 2");
}

#[test]
fn a_substitution_may_be_the_whole_template() {
    assert_eq!(evaluate("`${1}`"), "1");
}

#[test]
fn several_substitutions_interleave_with_the_parts() {
    assert_eq!(evaluate("`a${1}b${2}c`"), "a1b2c");
}

#[test]
fn adjacent_substitutions_need_no_part_between_them() {
    assert_eq!(evaluate("var a = 'x'; `${a}${a}`"), "xx");
}

#[test]
fn adjacent_numeric_substitutions_concatenate_rather_than_add() {
    assert_eq!(evaluate("var a = 1; var b = 2; `${a}${b}`"), "12");
}

#[test]
fn a_leading_numeric_substitution_concatenates_with_the_next_part() {
    assert_eq!(evaluate("var a = 1; `${a}2`"), "12");
}

#[test]
fn a_substitution_may_be_any_expression() {
    assert_eq!(evaluate("`${1 + 2}`"), "3");
}

#[test]
fn a_substitution_may_read_a_property() {
    assert_eq!(evaluate("var o = { v: 5 }; `v=${o.v}`"), "v=5");
}

#[test]
fn a_substitution_may_call_a_function() {
    assert_eq!(
        evaluate("function f(n) { return n * 2; } `${f(4)}`"),
        "8"
    );
}

#[test]
fn a_substitution_may_be_a_conditional() {
    assert_eq!(
        evaluate("var n = 3; `${n > 2 ? 'big' : 'small'}`"),
        "big"
    );
}

#[test]
fn templates_nest() {
    assert_eq!(
        evaluate("`before${ `inner${1}` }after`"),
        "beforeinner1after"
    );
}

#[test]
fn non_strings_are_converted() {
    assert_eq!(
        evaluate("`${true}|${null}|${undefined}|${1.5}`"),
        "true|null|undefined|1.5"
    );
}

#[test]
fn an_object_substitution_uses_to_string() {
    assert_eq!(evaluate("`${{}}`"), "[object Object]");
}

#[test]
fn escapes_are_interpreted() {
    assert_eq!(evaluate("`a\\nb`.length"), "3");
}

#[test]
fn a_literal_newline_is_part_of_the_template() {
    assert_eq!(evaluate("var s = `a\nb`; s.length + ':' + (s === 'a\\nb')"), "3:true");
}

#[test]
fn a_template_concatenates_with_a_string() {
    assert_eq!(evaluate("`a ${1} b` + '!'"), "a 1 b!");
}

#[test]
fn a_template_works_inside_an_arrow() {
    assert_eq!(evaluate("var f = x => `[${x}]`; f(7)"), "[7]");
}

#[test]
fn a_template_works_inside_a_method() {
    assert_eq!(
        evaluate(
            "class C { constructor(n) { this.n = n; } label() { return `C(${this.n})`; } }
             new C(3).label()"
        ),
        "C(3)"
    );
}

#[test]
fn substitutions_are_evaluated_left_to_right() {
    assert_eq!(
        evaluate(
            "var seen = '';
             function mark(n) { seen = seen + n; return n; }
             var s = `${mark(1)}${mark(2)}${mark(3)}`;
             s + ':' + seen"
        ),
        "123:123"
    );
}

#[test]
fn an_unterminated_template_is_refused() {
    assert!(error("var s = `abc;").contains("unterminated template"));
}

#[test]
fn an_unterminated_substitution_is_refused() {
    assert!(error("var s = `a${1 `;").contains("unterminated template expression"));
}

#[test]
fn a_tagged_template_is_refused() {
    assert!(error("var tag = s => s; tag`hello`;").contains("tagged templates"));
}

#[test]
fn a_template_survives_a_collection() {
    assert_eq!(
        evaluate(
            "var out = '';
             for (var i = 0; i < 400; i = i + 1) {
               var filler = { a: i, b: [i, i, i] };
               if (i === 399) { out = `done ${i} of ${400}`; }
             }
             out"
        ),
        "done 399 of 400"
    );
}

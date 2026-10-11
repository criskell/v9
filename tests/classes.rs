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
fn a_class_is_a_function() {
    assert_eq!(evaluate("class C {} typeof C"), "function");
}

#[test]
fn the_constructor_runs_on_new() {
    assert_eq!(
        evaluate("class C { constructor(v) { this.v = v; } } new C(7).v"),
        "7"
    );
}

#[test]
fn a_class_without_a_constructor_still_builds() {
    assert_eq!(evaluate("class C {} typeof new C()"), "object");
}

#[test]
fn a_method_is_callable_on_the_instance() {
    assert_eq!(
        evaluate("class C { m() { return 42; } } new C().m()"),
        "42"
    );
}

#[test]
fn a_method_sees_the_instance_as_this() {
    assert_eq!(
        evaluate(
            "class C { constructor() { this.v = 3; } doubled() { return this.v * 2; } }
             new C().doubled()"
        ),
        "6"
    );
}

#[test]
fn methods_live_on_the_prototype() {
    assert_eq!(
        evaluate(
            "class C { m() { return 1; } }
             var a = new C();
             var b = new C();
             a.m === b.m"
        ),
        "true"
    );
}

#[test]
fn methods_are_not_enumerable() {
    assert_eq!(
        evaluate(
            "class C { m() {} n() {} }
             var names = '';
             for (var k in new C()) { names = names + k; }
             '[' + names + ']'"
        ),
        "[]"
    );
}

#[test]
fn own_fields_assigned_in_the_constructor_are_enumerable() {
    assert_eq!(
        evaluate(
            "class C { constructor() { this.a = 1; this.b = 2; } m() {} }
             var names = '';
             for (var k in new C()) { names = names + k; }
             names"
        ),
        "ab"
    );
}

#[test]
fn constructor_is_not_enumerable_on_the_prototype() {
    assert_eq!(
        evaluate(
            "class C { m() {} }
             var names = '';
             for (var k in C.prototype) { names = names + k; }
             '[' + names + ']'"
        ),
        "[]"
    );
}

#[test]
fn the_prototype_points_back_at_the_class() {
    assert_eq!(
        evaluate("class C {} C.prototype.constructor === C"),
        "true"
    );
}

#[test]
fn an_instance_reaches_its_class_through_constructor() {
    assert_eq!(
        evaluate("class C { m() { return this.constructor === C; } } new C().m()"),
        "true"
    );
}

#[test]
fn a_static_method_hangs_off_the_class() {
    assert_eq!(
        evaluate("class C { static m() { return 5; } } C.m()"),
        "5"
    );
}

#[test]
fn a_static_method_is_not_on_the_instance() {
    assert_eq!(
        evaluate("class C { static m() { return 5; } } typeof new C().m"),
        "undefined"
    );
}

#[test]
fn a_static_method_can_construct_the_class() {
    assert_eq!(
        evaluate(
            "class Point {
               constructor(x, y) { this.x = x; this.y = y; }
               sum() { return this.x + this.y; }
               static origin() { return new Point(0, 0); }
             }
             Point.origin().sum() + ':' + new Point(3, 4).sum()"
        ),
        "0:7"
    );
}

#[test]
fn a_getter_in_a_class_runs_on_read() {
    assert_eq!(
        evaluate(
            "class C { constructor() { this.v = 4; } get doubled() { return this.v * 2; } }
             new C().doubled"
        ),
        "8"
    );
}

#[test]
fn a_setter_in_a_class_runs_on_write() {
    assert_eq!(
        evaluate(
            "class C { set v(value) { this.stored = value + 1; } }
             var c = new C();
             c.v = 9;
             c.stored"
        ),
        "10"
    );
}

#[test]
fn a_getter_and_setter_pair_in_a_class_share_one_slot() {
    assert_eq!(
        evaluate(
            "class C {
               constructor() { this.v = 1; }
               get x() { return this.v * 10; }
               set x(value) { this.v = value; }
             }
             var c = new C();
             var before = c.x;
             c.x = 5;
             before + ':' + c.x"
        ),
        "10:50"
    );
}

#[test]
fn a_class_accessor_is_not_enumerable() {
    assert_eq!(
        evaluate(
            "class C { get x() { return 1; } }
             var names = '';
             for (var k in new C()) { names = names + k; }
             '[' + names + ']'"
        ),
        "[]"
    );
}

#[test]
fn a_static_accessor_hangs_off_the_class() {
    assert_eq!(
        evaluate("class C { static get x() { return 11; } } C.x"),
        "11"
    );
}

#[test]
fn a_class_expression_may_be_anonymous() {
    assert_eq!(
        evaluate("var K = class { m() { return 5; } }; new K().m()"),
        "5"
    );
}

#[test]
fn a_class_expression_may_be_named() {
    assert_eq!(
        evaluate("var K = class Inner { m() { return 6; } }; new K().m()"),
        "6"
    );
}

#[test]
fn a_method_can_be_borrowed_with_call() {
    assert_eq!(
        evaluate(
            "class C { m() { return this.v; } }
             C.prototype.m.call({ v: 'borrowed' })"
        ),
        "borrowed"
    );
}

#[test]
fn two_constructors_are_refused() {
    assert!(error("class C { constructor() {} constructor() {} }").contains("one constructor"));
}

#[test]
fn a_class_declaration_without_a_name_is_refused() {
    assert!(error("class {}").contains("needs a name"));
}

#[test]
fn extends_is_reported_as_unimplemented() {
    assert!(error("class B {} class C extends B {} new C();").contains("extends"));
}

#[test]
fn a_class_survives_a_collection() {
    assert_eq!(
        evaluate(
            "class C { constructor(v) { this.v = v; } doubled() { return this.v * 2; } }
             var c = new C(21);
             var filler = 0;
             for (var i = 0; i < 400; i = i + 1) { filler = { a: i, b: [i, i, i] }; }
             c.doubled()"
        ),
        "42"
    );
}

#[test]
fn many_instances_share_one_map() {
    assert_eq!(
        evaluate(
            "class C { constructor(v) { this.v = v; } }
             var total = 0;
             for (var i = 0; i < 50; i = i + 1) { total = total + new C(i).v; }
             total"
        ),
        "1225"
    );
}

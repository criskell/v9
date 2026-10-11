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

#[test]
fn a_derived_class_inherits_methods() {
    assert_eq!(
        evaluate("class A { m() { return 1; } } class B extends A {} new B().m()"),
        "1"
    );
}

#[test]
fn a_derived_class_may_override_a_method() {
    assert_eq!(
        evaluate(
            "class A { m() { return 1; } }
             class B extends A { m() { return 2; } }
             new B().m()"
        ),
        "2"
    );
}

#[test]
fn super_runs_the_parent_constructor() {
    assert_eq!(
        evaluate(
            "class A { constructor(v) { this.v = v; } }
             class B extends A { constructor(v) { super(v * 2); } }
             new B(5).v"
        ),
        "10"
    );
}

#[test]
fn super_leaves_room_for_the_derived_constructor() {
    assert_eq!(
        evaluate(
            "class A { constructor() { this.a = 1; } }
             class B extends A { constructor() { super(); this.b = 2; } }
             var names = '';
             for (var k in new B()) { names = names + k; }
             names"
        ),
        "ab"
    );
}

#[test]
fn a_default_derived_constructor_forwards_its_arguments() {
    assert_eq!(
        evaluate(
            "class A { constructor(v) { this.v = v; } doubled() { return this.v * 2; } }
             class B extends A {}
             var b = new B(21);
             b.v + ':' + b.doubled()"
        ),
        "21:42"
    );
}

#[test]
fn a_default_derived_constructor_forwards_several_arguments() {
    assert_eq!(
        evaluate(
            "class A { constructor(x, y) { this.x = x; this.y = y; } }
             class B extends A {}
             var b = new B(3, 4);
             b.x + ':' + b.y"
        ),
        "3:4"
    );
}

#[test]
fn an_instance_of_a_derived_class_reaches_the_whole_chain() {
    assert_eq!(
        evaluate(
            "class A { a() { return 'a'; } }
             class B extends A { b() { return 'b'; } }
             class C extends B { c() { return 'c'; } }
             var o = new C();
             o.a() + o.b() + o.c()"
        ),
        "abc"
    );
}

#[test]
fn static_methods_are_inherited() {
    assert_eq!(
        evaluate("class A { static s() { return 's'; } } class B extends A {} B.s()"),
        "s"
    );
}

#[test]
fn an_inherited_accessor_runs_with_the_derived_instance() {
    assert_eq!(
        evaluate(
            "class A { get doubled() { return this.v * 2; } }
             class B extends A { constructor() { super(); this.v = 4; } }
             new B().doubled"
        ),
        "8"
    );
}

#[test]
fn the_derived_prototype_still_points_at_its_own_class() {
    assert_eq!(
        evaluate("class A {} class B extends A {} B.prototype.constructor === B"),
        "true"
    );
}

#[test]
fn inherited_methods_are_not_enumerable() {
    assert_eq!(
        evaluate(
            "class A { m() {} } class B extends A { n() {} }
             var names = '';
             for (var k in new B()) { names = names + k; }
             '[' + names + ']'"
        ),
        "[]"
    );
}

#[test]
fn extending_a_non_constructor_is_refused() {
    assert!(error("class B extends 5 {} new B();").contains("not a constructor"));
}

#[test]
fn extending_a_plain_object_is_refused() {
    assert!(error("var o = {}; class B extends o {} new B();").contains("not a constructor"));
}

#[test]
fn extending_null_gives_an_object_with_no_prototype() {
    assert_eq!(
        evaluate("class B extends null { constructor() { this.v = 1; } } new B().v"),
        "1"
    );
}

#[test]
fn super_outside_a_method_is_refused() {
    assert!(error("function f() { return super.m(); } f();").contains("outside a method"));
}

#[test]
fn a_hierarchy_survives_a_collection() {
    assert_eq!(
        evaluate(
            "class A { constructor(v) { this.v = v; } doubled() { return this.v * 2; } }
             class B extends A { tripled() { return this.v * 3; } }
             var b = new B(7);
             var filler = 0;
             for (var i = 0; i < 400; i = i + 1) { filler = { a: i, b: [i, i, i] }; }
             b.doubled() + ':' + b.tripled()"
        ),
        "14:21"
    );
}

#[test]
fn super_calls_the_overridden_method() {
    assert_eq!(
        evaluate(
            "class A { m() { return 1; } }
             class B extends A { m() { return super.m() + 1; } }
             new B().m()"
        ),
        "2"
    );
}

#[test]
fn a_super_method_sees_the_derived_instance_as_this() {
    assert_eq!(
        evaluate(
            "class A { m() { return this.v; } }
             class B extends A {
               constructor() { super(); this.v = 7; }
               m() { return super.m(); }
             }
             new B().m()"
        ),
        "7"
    );
}

#[test]
fn super_reaches_an_inherited_getter() {
    assert_eq!(
        evaluate(
            "class A { get x() { return this.v * 2; } }
             class B extends A {
               constructor() { super(); this.v = 5; }
               m() { return super.x; }
             }
             new B().m()"
        ),
        "10"
    );
}

#[test]
fn super_chains_through_three_levels() {
    assert_eq!(
        evaluate(
            "class A { m() { return 1; } }
             class B extends A { m() { return super.m(); } }
             class C extends B { m() { return super.m() + 10; } }
             new C().m()"
        ),
        "11"
    );
}

#[test]
fn super_skips_a_class_that_does_not_override() {
    assert_eq!(
        evaluate(
            "class A { m() { return 'a'; } }
             class B extends A {}
             class C extends B { m() { return super.m(); } }
             new C().m()"
        ),
        "a"
    );
}

#[test]
fn a_missing_super_property_is_undefined() {
    assert_eq!(
        evaluate(
            "class A {} class B extends A { m() { return super.missing; } }
             typeof new B().m()"
        ),
        "undefined"
    );
}

#[test]
fn super_in_a_static_method_walks_the_constructor_chain() {
    assert_eq!(
        evaluate(
            "class A { static s() { return 1; } }
             class B extends A { static s() { return super.s() + 1; } }
             B.s()"
        ),
        "2"
    );
}

#[test]
fn a_static_super_does_not_see_instance_methods() {
    assert_eq!(
        evaluate(
            "class A { m() { return 1; } }
             class B extends A { static s() { return super.m; } }
             typeof B.s()"
        ),
        "undefined"
    );
}

#[test]
fn super_works_beside_a_plain_inner_function() {
    assert_eq!(
        evaluate(
            "class A { m() { return 1; } }
             class B extends A {
               m() { var g = function() { return 2; }; return g() + super.m(); }
             }
             new B().m()"
        ),
        "3"
    );
}

#[test]
fn a_keyed_super_access_is_refused() {
    assert!(
        error("class A { m() {} } class B extends A { m() { return super['m'](); } } new B().m();")
            .contains("keyed `super`")
    );
}

#[test]
fn super_survives_a_collection() {
    assert_eq!(
        evaluate(
            "class A { constructor(v) { this.v = v; } doubled() { return this.v * 2; } }
             class B extends A { doubled() { return super.doubled() + 1; } }
             var b = new B(20);
             var filler = 0;
             for (var i = 0; i < 400; i = i + 1) { filler = { a: i, b: [i, i, i] }; }
             b.doubled()"
        ),
        "41"
    );
}

#[test]
fn a_class_without_super_needs_no_context() {
    assert_eq!(
        evaluate("class C { m() { return 1; } } new C().m()"),
        "1"
    );
}

#[test]
fn a_field_with_an_initializer_lands_on_the_instance() {
    assert_eq!(evaluate("class C { x = 1; } new C().x"), "1");
}

#[test]
fn a_field_without_an_initializer_is_undefined() {
    assert_eq!(evaluate("class C { x; } typeof new C().x"), "undefined");
}

#[test]
fn several_fields_initialize_in_order() {
    assert_eq!(
        evaluate("class C { n = 1; m = this.n + 1; } new C().m"),
        "2"
    );
}

#[test]
fn fields_run_before_the_constructor_body() {
    assert_eq!(
        evaluate("class C { x = 1; constructor() { this.y = this.x + 1; } } new C().y"),
        "2"
    );
}

#[test]
fn the_constructor_may_overwrite_a_field() {
    assert_eq!(
        evaluate("class C { x = 1; constructor(v) { this.x = v; } } new C(9).x"),
        "9"
    );
}

#[test]
fn fields_are_enumerable_on_the_instance() {
    assert_eq!(
        evaluate(
            "class C { a = 1; b = 2; m() {} }
             var names = '';
             for (var k in new C()) { names = names + k; }
             names"
        ),
        "ab"
    );
}

#[test]
fn a_field_is_defined_rather_than_set_so_it_shadows_a_prototype_accessor() {
    assert_eq!(
        evaluate("class C { get x() { return 1; } x = 2; } new C().x"),
        "2"
    );
}

#[test]
fn a_field_may_hold_an_arrow_bound_to_the_instance() {
    assert_eq!(
        evaluate("class C { v = 7; f = () => this.v; } new C().f()"),
        "7"
    );
}

#[test]
fn a_static_field_lands_on_the_class() {
    assert_eq!(evaluate("class C { static s = 5; } C.s"), "5");
}

#[test]
fn this_in_a_static_field_initializer_is_the_class() {
    assert_eq!(
        evaluate("class C { static a = 1; static b = this.a + 1; } C.b"),
        "2"
    );
}

#[test]
fn an_instance_field_and_a_static_field_may_share_a_name() {
    assert_eq!(
        evaluate("class C { x = 1; static x = 2; } new C().x + ':' + C.x"),
        "1:2"
    );
}

#[test]
fn a_derived_class_gets_both_sets_of_fields() {
    assert_eq!(
        evaluate(
            "class A { a = 1; } class B extends A { b = 2; }
             var o = new B(); o.a + o.b"
        ),
        "3"
    );
}

#[test]
fn fields_run_after_super_in_a_derived_constructor() {
    assert_eq!(
        evaluate(
            "class A { constructor() { this.v = 1; } }
             class B extends A { b = 2; constructor() { super(); } }
             var o = new B(); o.v + ':' + o.b"
        ),
        "1:2"
    );
}

#[test]
fn a_derived_default_constructor_still_initializes_fields() {
    assert_eq!(
        evaluate("class A { a = 1; } class B extends A {} new B().a"),
        "1"
    );
}

#[test]
fn the_field_initializer_is_not_a_visible_property_of_the_class() {
    assert_eq!(
        evaluate(
            "class C { x = 1; }
             var names = '';
             for (var k in C) { names = names + k; }
             '[' + names + ']:' + Object.keys(C).length"
        ),
        "[]:0"
    );
}

#[test]
fn a_method_sees_a_field() {
    assert_eq!(
        evaluate("class C { x = 1; m() { return this.x; } } new C().m()"),
        "1"
    );
}

#[test]
fn fields_survive_a_collection() {
    assert_eq!(
        evaluate(
            "class C { a = 1; b = [1, 2, 3]; }
             var c = new C();
             var filler = 0;
             for (var i = 0; i < 400; i = i + 1) { filler = { a: i, b: [i, i, i] }; }
             c.a + ':' + c.b.length"
        ),
        "1:3"
    );
}

#[test]
fn a_field_initializer_closes_over_the_enclosing_scope() {
    assert_eq!(
        evaluate(
            "function outer() { var n = 5; class C { x = n + 1; } return new C().x; }
             outer()"
        ),
        "6"
    );
}

#[test]
fn an_arrow_nested_twice_in_a_field_keeps_this() {
    assert_eq!(
        evaluate("class C { v = 2; f = () => () => this.v; } new C().f()()"),
        "2"
    );
}

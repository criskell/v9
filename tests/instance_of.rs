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
fn an_instance_is_an_instance_of_its_constructor() {
    assert_eq!(evaluate("function F() {} new F() instanceof F"), "true");
}

#[test]
fn an_instance_is_an_instance_of_its_class() {
    assert_eq!(evaluate("class C {} new C() instanceof C"), "true");
}

#[test]
fn an_instance_is_an_instance_of_every_class_above_it() {
    assert_eq!(
        evaluate("class A {} class B extends A {} new B() instanceof A"),
        "true"
    );
}

#[test]
fn a_base_instance_is_not_an_instance_of_the_derived_class() {
    assert_eq!(
        evaluate("class A {} class B extends A {} new A() instanceof B"),
        "false"
    );
}

#[test]
fn an_unrelated_object_is_not_an_instance() {
    assert_eq!(evaluate("class C {} ({}) instanceof C"), "false");
}

#[test]
fn a_primitive_is_never_an_instance() {
    assert_eq!(evaluate("class C {} 5 instanceof C"), "false");
}

#[test]
fn a_whole_chain_is_walked() {
    assert_eq!(
        evaluate(
            "class A {} class B extends A {} class C extends B {}
             var o = new C();
             (o instanceof A) + ':' + (o instanceof B) + ':' + (o instanceof C)"
        ),
        "true:true:true"
    );
}

#[test]
fn an_object_made_before_the_prototype_changed_is_no_longer_an_instance() {
    assert_eq!(
        evaluate(
            "function F() {}
             var o = new F();
             F.prototype = {};
             o instanceof F"
        ),
        "false"
    );
}

#[test]
fn an_object_made_after_the_prototype_changed_is_an_instance() {
    assert_eq!(
        evaluate(
            "function F() {}
             F.prototype = {};
             new F() instanceof F"
        ),
        "true"
    );
}

#[test]
fn a_non_object_right_hand_side_is_refused() {
    assert!(error("class C {} new C() instanceof 5").contains("not an object"));
}

#[test]
fn a_non_callable_right_hand_side_is_refused() {
    assert!(error("class C {} new C() instanceof {}").contains("not callable"));
}

#[test]
fn a_primitive_prototype_is_refused() {
    assert!(
        error("function F() {} F.prototype = 5; new F() instanceof F")
            .contains("non-object prototype")
    );
}

#[test]
fn a_primitive_prototype_reads_back_as_itself() {
    assert_eq!(evaluate("function F() {} F.prototype = 5; F.prototype"), "5");
}

#[test]
fn a_primitive_prototype_may_be_replaced_again() {
    assert_eq!(
        evaluate("function F() {} F.prototype = 5; F.prototype = 7; F.prototype"),
        "7"
    );
}

#[test]
fn a_primitive_prototype_still_builds_objects_from_object_prototype() {
    assert_eq!(
        evaluate(
            "function F() { this.v = 1; }
             F.prototype = 5;
             var o = new F();
             typeof o + ':' + o.v"
        ),
        "object:1"
    );
}

#[test]
fn a_primitive_prototype_survives_construction() {
    assert_eq!(
        evaluate("function F() {} F.prototype = 5; new F(); F.prototype"),
        "5"
    );
}

#[test]
fn going_back_to_an_object_prototype_restores_instanceof() {
    assert_eq!(
        evaluate(
            "function F() {}
             F.prototype = 5;
             var p = {};
             F.prototype = p;
             new F() instanceof F"
        ),
        "true"
    );
}

#[test]
fn a_prototype_assigned_after_construction_is_honoured() {
    assert_eq!(
        evaluate(
            "function F() {}
             new F();
             var p = { m: function() { return 2; } };
             F.prototype = p;
             new F().m()"
        ),
        "2"
    );
}

#[test]
fn instanceof_survives_a_collection() {
    assert_eq!(
        evaluate(
            "class A {} class B extends A {}
             var b = new B();
             var filler = 0;
             for (var i = 0; i < 400; i = i + 1) { filler = { a: i, b: [i, i, i] }; }
             (b instanceof A) + ':' + (b instanceof B)"
        ),
        "true:true"
    );
}

use vx::execution::isolate::Isolate;
use vx::objects::js_objects::JSObject;

fn evaluate(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(value) => isolate.to_string(value),
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

fn uncaught(source: &str) -> String {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    match isolate.run_script(source) {
        Ok(_) => panic!("{source:?} should have thrown"),
        Err(error) => error,
    }
}

#[test]
fn new_runs_the_body_with_a_fresh_this() {
    assert_eq!(
        evaluate(
            "function Point(x, y) { this.x = x; this.y = y; }
             var p = new Point(3, 4);
             p.x + p.y"
        ),
        "7"
    );
    assert_eq!(evaluate("function Empty() {} typeof new Empty()"), "object");
}

#[test]
fn each_new_builds_a_separate_object() {
    assert_eq!(
        evaluate(
            "function Box(v) { this.v = v; }
             var a = new Box(1);
             var b = new Box(2);
             a.v + ':' + b.v"
        ),
        "1:2"
    );
}

#[test]
fn instances_of_one_constructor_share_a_shape() {
    // The constructor's initial map is built once and handed to every
    // instance, which is the whole reason `new` is worth a bytecode.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    isolate
        .run_script(
            "function Point(x, y) { this.x = x; this.y = y; }
             var a = new Point(1, 2);
             var b = new Point(3, 4);
             var c = {};
             c.x = 5;
             c.y = 6;",
        )
        .expect("runs");

    let global = isolate.global_object();
    let map_of = |isolate: &mut Isolate, name: &str| {
        let value = isolate.get_property_by_str(global, name);
        JSObject::unchecked_cast(value.to_strong_heap_object().unwrap())
            .map(&isolate.heap)
            .address()
    };
    let a = map_of(&mut isolate, "a");
    let b = map_of(&mut isolate, "b");
    let c = map_of(&mut isolate, "c");
    assert_eq!(a, b, "two instances, one hidden class");
    assert_ne!(
        a, c,
        "an object literal with the same keys has a different prototype, \
         so it cannot share the map"
    );
}

#[test]
fn methods_on_the_prototype_are_inherited() {
    assert_eq!(
        evaluate(
            "function Point(x, y) { this.x = x; this.y = y; }
             Point.prototype.sum = function () { return this.x + this.y; };
             var p = new Point(2, 5);
             p.sum()"
        ),
        "7",
        "the method is found on the prototype and `this` is the instance"
    );
    assert_eq!(
        evaluate(
            "function Point() {}
             Point.prototype.shared = 9;
             var a = new Point();
             var b = new Point();
             a.shared + b.shared"
        ),
        "18",
        "one property, two instances"
    );
}

#[test]
fn an_own_property_shadows_the_prototype() {
    assert_eq!(
        evaluate(
            "function Point() {}
             Point.prototype.v = 1;
             var p = new Point();
             p.v = 2;
             p.v"
        ),
        "2"
    );
    assert_eq!(
        evaluate(
            "function Point() {}
             Point.prototype.v = 1;
             var p = new Point();
             p.v = 2;
             var q = new Point();
             q.v"
        ),
        "1",
        "shadowing one instance must not touch the prototype"
    );
}

#[test]
fn the_prototype_object_is_stable_across_reads() {
    assert_eq!(
        evaluate(
            "function F() {}
             F.prototype.a = 1;
             F.prototype.b = 2;
             F.prototype.a + F.prototype.b"
        ),
        "3",
        "the lazily created prototype is created once, not per read"
    );
}

#[test]
fn assigning_prototype_replaces_it() {
    assert_eq!(
        evaluate(
            "function F() {}
             F.prototype = { kind: 'replaced' };
             var f = new F();
             f.kind"
        ),
        "replaced"
    );
}

#[test]
fn a_returned_object_wins_but_a_returned_primitive_does_not() {
    assert_eq!(
        evaluate("function F() { this.a = 1; return { b: 2 }; } new F().b"),
        "2"
    );
    assert_eq!(
        evaluate("function F() { this.a = 1; return 42; } new F().a"),
        "1",
        "a primitive return is ignored and the instance is the result"
    );
    assert_eq!(
        evaluate("function F() { this.a = 1; return; } new F().a"),
        "1"
    );
}

#[test]
fn new_on_something_that_is_not_a_function_throws() {
    assert_eq!(uncaught("var x = 1; new x();"), "1 is not a constructor");
    assert_eq!(
        uncaught("new undefinedConstructor();"),
        "undefinedConstructor is not defined"
    );
}

#[test]
fn a_constructor_that_throws_propagates() {
    assert_eq!(
        evaluate(
            "function F() { throw 'in constructor'; }
             var r;
             try { new F(); } catch (e) { r = e; }
             r"
        ),
        "in constructor"
    );
}

#[test]
fn new_composes_with_property_access_and_calls() {
    assert_eq!(
        evaluate(
            "function Point(x) { this.x = x; }
             Point.prototype.get = function () { return this.x; };
             new Point(11).get()"
        ),
        "11"
    );
    assert_eq!(
        evaluate("function F() { this.inner = { deep: 7 }; } new F().inner.deep"),
        "7"
    );
}

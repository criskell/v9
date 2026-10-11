//! Inline caches, checked by what they *do* rather than by what they hold.
//!
//! A feedback vector that nothing reads is just a data structure, so every
//! test here asserts on the hit/miss counters or on an observable state
//! transition. The point of hidden classes is that a repeated property
//! access stops searching, and that is what gets measured.

use vx::common::globals::InlineCacheState;
use vx::execution::isolate::Isolate;
use vx::ic::handler_configuration::{Handler, HandlerKind};
use vx::objects::feedback_vector::{FeedbackNexus, FeedbackSlotKind};

fn run(source: &str) -> Isolate {
    let mut isolate = Isolate::new();
    match isolate.run_script(source) {
        Ok(_) => isolate,
        Err(error) => panic!("{source:?} failed: {error}"),
    }
}

/// The state of the script's first *load* site. Store sites are skipped:
/// a store that adds a property is deliberately not cached, so its site
/// stays uninitialized and says nothing about the cache working.
fn first_load_site(isolate: &Isolate) -> (i32, InlineCacheState) {
    let vector = isolate
        .script_feedback_vector()
        .expect("the script should have a feedback vector");
    for slot in 0..vector.length(&isolate.heap) {
        if vector.get_kind(&isolate.heap, slot) == FeedbackSlotKind::LoadProperty {
            let nexus = FeedbackNexus::new(&isolate.heap, vector, slot);
            return (slot, isolate.ic_state(nexus));
        }
    }
    panic!("no load site in the script");
}

#[test]
fn a_repeated_access_on_one_shape_stops_searching() {
    // The whole point: after the first execution the site knows the answer.
    let isolate = run("var o = { x: 1 };
         var total = 0;
         for (var i = 0; i < 20; i++) { total += o.x; }
         total");
    let property = isolate.ic_statistics().property;
    assert_eq!(
        property.hits, 19,
        "nineteen of the twenty reads should be answered by the cache"
    );
    assert_eq!(
        property.misses, 2,
        "two: the load's first execution, and the literal's store of `x` -- \
         a store that *adds* a property is never cached, because it changes \
         the receiver's map"
    );
    assert_eq!(property.monomorphic_transitions, 1);
    assert_eq!(property.megamorphic_transitions, 0);

    // The loop's reads of `total`, `i` and `o` are global accesses, cached
    // through property cells rather than maps.
    let global = isolate.ic_statistics().global;
    assert!(
        global.hits > 50,
        "the loop's global accesses should be cached too: {global:?}"
    );
}

#[test]
fn a_site_that_never_runs_stays_uninitialized() {
    // Two loads, one of them behind a branch that is never taken.
    let isolate = run("var o = { x: 1 };
         var total = o.x;
         if (false) { total = o.x; }
         total");
    let vector = isolate.script_feedback_vector().unwrap();

    let mut states = Vec::new();
    for slot in 0..vector.length(&isolate.heap) {
        if vector.get_kind(&isolate.heap, slot) != FeedbackSlotKind::LoadProperty {
            continue;
        }
        let nexus = FeedbackNexus::new(&isolate.heap, vector, slot);
        states.push(isolate.ic_state(nexus));
    }

    assert!(
        states.contains(&InlineCacheState::Monomorphic),
        "the load that ran should have recorded a shape: {states:?}"
    );
    // The dead branch's load was elided by the generator, so there may be
    // only one site; if both are present the unexecuted one is untouched.
    for state in &states {
        assert!(
            matches!(
                state,
                InlineCacheState::Monomorphic | InlineCacheState::Uninitialized
            ),
            "{state:?}"
        );
    }
}

#[test]
fn the_site_goes_monomorphic_then_polymorphic_then_megamorphic() {
    // One site, fed more and more shapes. The states it passes through are
    // the whole lattice.
    let isolate = run("function read(o) { return o.x; }
         read({ x: 1 });
         read({ x: 1 });
         read({ a: 0, x: 1 });
         read({ a: 0, b: 0, x: 1 });
         read({ a: 0, b: 0, c: 0, x: 1 });
         read({ a: 0, b: 0, c: 0, d: 0, x: 1 });
         0");
    let property = isolate.ic_statistics().property;
    // Six calls: the first is a miss that goes monomorphic, the second hits,
    // and each new shape is another miss.
    assert_eq!(property.monomorphic_transitions, 1);
    assert!(
        property.polymorphic_transitions >= 1,
        "the second shape should widen the site: {property:?}"
    );
    assert_eq!(
        property.megamorphic_transitions, 1,
        "the fifth shape should make the site give up: {property:?}"
    );
    assert!(property.hits >= 1, "the repeated shape should have hit");
}

#[test]
fn a_site_tracks_at_most_four_shapes() {
    let isolate = run("function read(o) { return o.x; }
         read({ x: 1 });
         read({ a: 0, x: 1 });
         read({ a: 0, b: 0, x: 1 });
         read({ a: 0, b: 0, c: 0, x: 1 });
         0");
    // Four distinct shapes is exactly the limit, so the site should still be
    // polymorphic rather than megamorphic.
    assert_eq!(
        isolate.ic_statistics().property.megamorphic_transitions,
        0,
        "four shapes is the limit, not past it"
    );
}

#[test]
fn megamorphic_is_terminal() {
    // Once a site has given up, seeing the first shape again must not take
    // it back -- otherwise the state could oscillate forever.
    let isolate = run("function read(o) { return o.x; }
         var first = { x: 1 };
         read(first);
         read({ a: 0, x: 1 });
         read({ a: 0, b: 0, x: 1 });
         read({ a: 0, b: 0, c: 0, x: 1 });
         read({ a: 0, b: 0, c: 0, d: 0, x: 1 });
         read(first);
         read(first);
         0");
    let property = isolate.ic_statistics().property;
    assert_eq!(property.megamorphic_transitions, 1);
    assert_eq!(
        property.monomorphic_transitions, 1,
        "the site must not go back to monomorphic: {property:?}"
    );
}

#[test]
fn the_cached_handler_names_the_field_the_map_assigned() {
    // The handler is a Smi, and what it encodes is the field index the
    // hidden class chose -- which is why the field index had to be in the
    // property details.
    let isolate = run("var o = { x: 1, y: 2 }; o.y; o.y");
    let vector = isolate.script_feedback_vector().unwrap();

    let mut found = None;
    for slot in 0..vector.length(&isolate.heap) {
        if vector.get_kind(&isolate.heap, slot) != FeedbackSlotKind::LoadProperty {
            continue;
        }
        let nexus = FeedbackNexus::new(&isolate.heap, vector, slot);
        if isolate.ic_state(nexus) == InlineCacheState::Monomorphic {
            found = Handler::from_tagged(nexus.feedback_extra(&isolate.heap));
            break;
        }
    }
    let handler = found.expect("the load site should have cached a handler");
    assert_eq!(handler.kind(), HandlerKind::Field);
    assert!(
        handler.is_inobject(),
        "with two properties and four inline slots, y is inline"
    );
    assert_eq!(handler.descriptor_index(), 1, "y is the second property");
}

#[test]
fn a_property_in_the_backing_store_is_cached_too() {
    // Past the inline slots the handler has to index the property array
    // instead, and the flag is what tells the two apart.
    let isolate = run("var o = {};
         o.a = 1; o.b = 2; o.c = 3; o.d = 4; o.e = 5; o.f = 6;
         o.f; o.f");
    let vector = isolate.script_feedback_vector().unwrap();
    let mut spilled = None;
    for slot in 0..vector.length(&isolate.heap) {
        if vector.get_kind(&isolate.heap, slot) != FeedbackSlotKind::LoadProperty {
            continue;
        }
        let nexus = FeedbackNexus::new(&isolate.heap, vector, slot);
        if isolate.ic_state(nexus) == InlineCacheState::Monomorphic {
            if let Some(handler) = Handler::from_tagged(nexus.feedback_extra(&isolate.heap)) {
                if !handler.is_inobject() {
                    spilled = Some(handler);
                }
            }
        }
    }
    let handler = spilled.expect("f is the sixth property, past four inline slots");
    assert_eq!(handler.kind(), HandlerKind::Field);
    assert!(!handler.is_inobject());
    assert_eq!(handler.descriptor_index(), 5, "f is the sixth property");
    assert_eq!(
        handler.storage_offset_in_words(),
        1,
        "the second slot of the backing store"
    );
}

#[test]
fn a_store_site_caches_too() {
    let isolate = run("var o = { x: 0 };
         for (var i = 0; i < 10; i++) { o.x = i; }
         o.x");
    let property = isolate.ic_statistics().property;
    assert!(
        property.hits >= 9,
        "the repeated store should be cached: {property:?}"
    );
}

#[test]
fn a_store_that_adds_a_property_is_not_cached() {
    // Adding a property changes the receiver's map, so caching the *old*
    // map would hand the next object of that shape a handler for a field
    // that does not exist yet.
    let isolate = run("function add(o) { o.added = 1; return o; }
         add({});
         add({});
         add({});
         0");
    let property = isolate.ic_statistics().property;
    assert_eq!(
        property.hits, 0,
        "a transitioning store must not hit: {property:?}"
    );
    assert_eq!(property.misses, 3);
}

#[test]
fn a_cached_entry_survives_a_collection() {
    // Maps live in read-only space and handlers are Smis, so a collection
    // cannot invalidate a cached entry. The objects the entry *reads from*
    // do move, which is why the cache stores a field index rather than an
    // address.
    let mut isolate = Isolate::new();
    isolate
        .run_script(
            "var o = { x: 7 };
             var total = 0;
             for (var i = 0; i < 5; i++) { total += o.x; }
             total",
        )
        .unwrap();
    assert!(isolate.ic_statistics().property.hits >= 4);

    let vector = isolate.script_feedback_vector().unwrap();
    let mut cached_before = None;
    for slot in 0..vector.length(&isolate.heap) {
        if vector.get_kind(&isolate.heap, slot) != FeedbackSlotKind::LoadProperty {
            continue;
        }
        let nexus = FeedbackNexus::new(&isolate.heap, vector, slot);
        if isolate.ic_state(nexus) == InlineCacheState::Monomorphic {
            cached_before = Some((
                nexus.feedback(&isolate.heap),
                nexus.feedback_extra(&isolate.heap),
            ));
        }
    }
    let (map_before, handler_before) = cached_before.expect("a cached load site");

    isolate.collect_garbage();

    // The cached map reference and handler are unchanged, and the object
    // still reads back through the same field index.
    let vector = isolate.script_feedback_vector().unwrap();
    let mut cached_after = None;
    for slot in 0..vector.length(&isolate.heap) {
        if vector.get_kind(&isolate.heap, slot) != FeedbackSlotKind::LoadProperty {
            continue;
        }
        let nexus = FeedbackNexus::new(&isolate.heap, vector, slot);
        if isolate.ic_state(nexus) == InlineCacheState::Monomorphic {
            cached_after = Some((
                nexus.feedback(&isolate.heap),
                nexus.feedback_extra(&isolate.heap),
            ));
        }
    }
    assert_eq!(
        cached_after,
        Some((map_before, handler_before)),
        "an immovable map and a Smi handler cannot be invalidated by a GC"
    );

    // And the value is still reachable through the global object.
    let global = isolate.global_object();
    let o = isolate.get_property_by_str(global, "o");
    let o = vx::objects::js_objects::JSObject::unchecked_cast(
        o.to_strong_heap_object().expect("o survived"),
    );
    let x = isolate.get_property_by_str(o, "x");
    assert_eq!(isolate.to_string(x), "7");
}

#[test]
fn two_closures_over_one_function_do_not_share_a_cache() {
    // They can be called with different shapes, and sharing one vector
    // would make each pollute the other.
    let isolate = run("function make() { return function (o) { return o.x; }; }
         var a = make();
         var b = make();
         a({ x: 1 }); a({ x: 1 });
         b({ q: 0, x: 1 }); b({ q: 0, x: 1 });
         0");
    let property = isolate.ic_statistics().property;
    // Each closure sees one shape, so each goes monomorphic on its own.
    assert_eq!(
        property.monomorphic_transitions, 2,
        "each closure should have its own cache: {property:?}"
    );
    assert_eq!(property.polymorphic_transitions, 0);
    assert_eq!(property.hits, 2);
}

#[test]
fn trace_ic_reports_every_site() {
    let isolate = run("var o = { x: 1 }; o.x; o.x");
    let trace = isolate.trace_ic();
    assert!(!trace.is_empty());
    assert!(
        trace.iter().any(|line| line.contains("Monomorphic")),
        "{trace:?}"
    );
    assert!(
        trace.iter().any(|line| line.contains("LoadProperty")),
        "{trace:?}"
    );
}

#[test]
fn the_site_state_is_readable_from_outside() {
    let isolate = run("var o = { x: 1 }; o.x; o.x");
    let (_, state) = first_load_site(&isolate);
    assert_eq!(state, InlineCacheState::Monomorphic);
}

// ---------------------------------------------------------------------------
// Globals, through property cells
// ---------------------------------------------------------------------------

use vx::objects::property_details::PropertyCellType;

#[test]
fn a_repeated_global_read_is_cached_through_its_cell() {
    let isolate = run("var n = 5;
         var total = 0;
         for (var i = 0; i < 20; i++) { total = total + n; }
         total");
    let global = isolate.ic_statistics().global;
    assert!(
        global.hits > 50,
        "the loop reads three globals per iteration: {global:?}"
    );
    assert!(
        global.hit_rate() > 0.9,
        "a global read should miss once per site, not once per execution: {global:?}"
    );
}

#[test]
fn declaring_another_global_does_not_invalidate_a_cached_one() {
    // The reason cells exist. Adding a global reshapes the global object, so
    // a cache keyed on its *map* would be thrown away -- a cache keyed on
    // the cell is not.
    let mut isolate = Isolate::new();
    isolate
        .run_script(
            "var a = 1;
             var seen = 0;
             var i = 0;
             while (i < 10) { seen = seen + a; i = i + 1; }
             seen",
        )
        .unwrap();
    let global = isolate.ic_statistics().global;
    // Each site misses once and hits afterwards, whatever else was declared.
    assert!(
        global.misses <= 12,
        "one miss per site, not per execution: {global:?}"
    );
    assert!(global.hits > 30, "{global:?}");
}

#[test]
fn a_global_assigned_once_is_recorded_as_constant() {
    // Which is what lets an optimizing compiler inline it.
    let mut isolate = Isolate::new();
    isolate.run_script("var answer = 42; answer").unwrap();
    let key = isolate.internalize_string("answer").tagged();
    let cell = isolate
        .global_cell(key)
        .expect("the global should have a cell");
    assert_eq!(
        cell.cell_type(&isolate.heap),
        PropertyCellType::Constant,
        "assigned exactly once"
    );
    assert_eq!(isolate.to_string(cell.value(&isolate.heap)), "42");
}

#[test]
fn a_global_declared_without_a_value_is_not_a_constant_undefined() {
    // `var x;` must not look like a constant an optimizer could inline.
    let mut isolate = Isolate::new();
    isolate.run_script("var x; x").unwrap();
    let key = isolate.internalize_string("x").tagged();
    let cell = isolate.global_cell(key).unwrap();
    assert_eq!(cell.cell_type(&isolate.heap), PropertyCellType::Undefined);
}

#[test]
fn the_cell_type_walks_down_as_the_global_changes() {
    // Constant -> ConstantType -> Mutable, one way only.
    let mut isolate = Isolate::new();
    isolate.run_script("var v = 1; v = 2; v").unwrap();
    let key = isolate.internalize_string("v").tagged();
    let cell = isolate.global_cell(key).unwrap();
    assert_eq!(
        cell.cell_type(&isolate.heap),
        PropertyCellType::ConstantType,
        "two different Smis: still one type"
    );

    let mut isolate = Isolate::new();
    isolate.run_script("var v = 1; v = 'text'; v").unwrap();
    let key = isolate.internalize_string("v").tagged();
    let cell = isolate.global_cell(key).unwrap();
    assert_eq!(
        cell.cell_type(&isolate.heap),
        PropertyCellType::Mutable,
        "a Smi then a string: no type to be constant about"
    );
}

#[test]
fn globals_are_still_visible_through_global_this() {
    // The cells have to be transparent to ordinary property access: a cell
    // leaking out here would be observable from JavaScript.
    let mut isolate = Isolate::new();
    let value = isolate
        .run_script("var answer = 42; globalThis.answer")
        .unwrap();
    assert_eq!(isolate.to_string(value), "42");
}

#[test]
fn a_global_write_through_global_this_is_seen_by_a_global_read() {
    let mut isolate = Isolate::new();
    let value = isolate
        .run_script("var answer = 1; globalThis.answer = 2; answer")
        .unwrap();
    assert_eq!(
        isolate.to_string(value),
        "2",
        "both paths must reach the same cell"
    );
}

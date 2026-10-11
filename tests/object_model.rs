//! End-to-end exercises of the engine's public API.
//!
//! The unit tests inside each module check layouts and edge cases; these
//! check that the pieces add up to the behaviour V8 is built to deliver.

use vx::execution::isolate::Isolate;
use vx::heap::heap::{Heap, HeapConfig};
use vx::objects::fixed_array::FixedArray;
use vx::objects::heap_number::number_value;
use vx::objects::js_objects::JSObject;
use vx::objects::map::Map;
use vx::objects::smi::Smi;
use vx::objects::transitions::TransitionsAccessor;

#[test]
fn a_constructor_run_many_times_produces_one_shape() {
    let mut isolate = Isolate::new();

    // The shape of `function Point(x, y) { this.x = x; this.y = y; }`.
    let mut maps = std::collections::HashSet::new();
    for i in 0..500 {
        let point = isolate.new_js_object();
        isolate.set_property_by_str(point, "x", Smi::from_int(i));
        isolate.set_property_by_str(point, "y", Smi::from_int(i * 2));
        maps.insert(point.map(&isolate.heap));
    }

    assert_eq!(maps.len(), 1, "500 instances, one hidden class");
    let map = *maps.iter().next().unwrap();
    assert_eq!(map.number_of_own_descriptors(&isolate.heap), 2);

    // Both fields are inline, at fixed offsets -- which is what makes `p.x`
    // a map check plus one load.
    let x = isolate.internalize_string("x").tagged();
    let y = isolate.internalize_string("y").tagged();
    assert_eq!(
        map.find_property_details(&isolate.heap, x)
            .unwrap()
            .field_index(),
        0
    );
    assert_eq!(
        map.find_property_details(&isolate.heap, y)
            .unwrap()
            .field_index(),
        1
    );
    assert!(map.inobject_properties(&isolate.heap) >= 2);
}

#[test]
fn the_shape_graph_is_a_tree_rooted_at_the_empty_object() {
    let mut isolate = Isolate::new();
    let root = isolate.roots.js_object_map();

    // Three shapes that share a prefix.
    for properties in [
        vec!["a"],
        vec!["a", "b"],
        vec!["a", "b", "c"],
        vec!["a", "z"],
    ] {
        let object = isolate.new_js_object();
        for name in &properties {
            isolate.set_property_by_str(object, name, Smi::zero());
        }
    }

    // One edge out of the root: everything started with `a`.
    assert_eq!(
        TransitionsAccessor::number_of_transitions(&isolate.heap, root),
        1
    );
    let a = isolate.internalize_string("a").tagged();
    let map_a = TransitionsAccessor::search_transition(&isolate.heap, root, a).unwrap();

    // Two edges out of {a}: one to {a,b} and one to {a,z}.
    assert_eq!(
        TransitionsAccessor::number_of_transitions(&isolate.heap, map_a),
        2
    );

    // Every map in the graph walks back to the root.
    let mut frontier = vec![map_a];
    let mut visited = 0;
    while let Some(map) = frontier.pop() {
        visited += 1;
        assert!(visited < 100, "the graph should be small and acyclic");
        let mut current = map;
        while let Some(parent) = current.back_pointer(&isolate.heap) {
            current = parent;
        }
        assert_eq!(current, root);
        for (_, target) in TransitionsAccessor::transitions(&isolate.heap, map) {
            frontier.push(target);
        }
    }
    assert_eq!(visited, 4, "{{a}}, {{a,b}}, {{a,b,c}}, {{a,z}}");
}

#[test]
fn an_object_graph_survives_repeated_collections_with_a_tiny_nursery() {
    let mut isolate = Isolate::with_heap(Heap::with_config(HeapConfig {
        read_only_space_size: 256 * 1024,
        semi_space_size: 4 * 1024,
        initial_old_space_size: 256 * 1024,
        max_old_space_size: 16 * 1024 * 1024,
    }));

    // A linked list, held only by its head, which is rooted in the handle
    // scope. Every collection has to trace the whole chain.
    const LENGTH: i32 = 40;
    let head = isolate.new_fixed_array(2);
    isolate.handles.push(head.tagged());
    let mut tail = head;
    for i in 0..LENGTH {
        let value = isolate.new_heap_number(i as f64 + 0.5).tagged();
        let next = isolate.new_fixed_array(2);
        tail.set(&mut isolate.heap, 0, value);
        tail.set(&mut isolate.heap, 1, next.tagged());

        // Churn, so a collection is worth doing.
        for j in 0..20 {
            isolate.new_heap_number(j as f64);
        }
        isolate.collect_garbage();

        // `tail` and `next` are Rust locals and are now stale: the
        // collection may have moved what they point at. Walking back from
        // the root is the discipline a moving collector requires, and it
        // doubles as a check that every link was updated.
        let mut node =
            FixedArray::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
        for step in 0..=i {
            let value = node.get(&isolate.heap, 0);
            assert_eq!(
                number_value(&isolate.heap, value),
                Some(step as f64 + 0.5),
                "node {step} lost after {} collections",
                i + 1
            );
            node = FixedArray::unchecked_cast(
                node.get(&isolate.heap, 1).to_strong_heap_object().unwrap(),
            );
        }
        tail = node;
    }

    assert!(isolate.heap.gc_count >= LENGTH as usize);
}

#[test]
fn inherited_and_own_properties_resolve_in_the_right_order() {
    let mut isolate = Isolate::new();

    let base = isolate.new_js_object();
    isolate.set_property_by_str(base, "kind", Smi::from_int(1));
    isolate.set_property_by_str(base, "shared", Smi::from_int(2));

    let derived = isolate.new_js_object();
    isolate.set_property_by_str(derived, "own", Smi::from_int(3));
    derived
        .map(&isolate.heap)
        .set_prototype(&mut isolate.heap, base.tagged());

    assert_eq!(Smi::to_int(isolate.get_property_by_str(derived, "own")), 3);
    assert_eq!(Smi::to_int(isolate.get_property_by_str(derived, "kind")), 1);

    // Shadowing creates an own property and leaves the prototype alone.
    isolate.set_property_by_str(derived, "kind", Smi::from_int(9));
    assert_eq!(Smi::to_int(isolate.get_property_by_str(derived, "kind")), 9);
    assert_eq!(Smi::to_int(isolate.get_property_by_str(base, "kind")), 1);

    // Still inherited.
    assert_eq!(
        Smi::to_int(isolate.get_property_by_str(derived, "shared")),
        2
    );
    // Absent everywhere.
    assert_eq!(
        isolate.get_property_by_str(derived, "missing"),
        isolate.undefined_value()
    );
}

#[test]
fn numbers_choose_between_immediate_and_boxed_representations() {
    let mut isolate = Isolate::new();
    let object = isolate.new_js_object();

    let cases: [(f64, bool); 6] = [
        (0.0, true),
        (42.0, true),
        (i32::MAX as f64, true),
        (i32::MAX as f64 + 1.0, false),
        (0.5, false),
        (-0.0, false),
    ];

    for (value, expect_smi) in cases {
        let tagged = isolate.new_number(value);
        assert_eq!(
            tagged.is_smi(),
            expect_smi,
            "{value} should {}be a Smi",
            if expect_smi { "" } else { "not " }
        );
        isolate.set_property_by_str(object, "n", tagged);
        let read_back = isolate.get_property_by_str(object, "n");
        assert_eq!(
            number_value(&isolate.heap, read_back).map(f64::to_bits),
            Some(value.to_bits()),
            "{value} did not round-trip through a property"
        );
    }
}

#[test]
fn maps_are_immovable_so_shapes_outlive_collections() {
    let mut isolate = Isolate::new();
    let object = isolate.new_js_object();
    isolate.set_property_by_str(object, "x", Smi::from_int(1));
    let map_before = object.map(&isolate.heap);
    let descriptors_before = map_before.instance_descriptors(&isolate.heap);

    isolate.handles.push(object.tagged());
    for _ in 0..5 {
        for i in 0..50 {
            isolate.new_heap_number(i as f64);
        }
        isolate.collect_garbage();
    }

    let moved = JSObject::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    assert_eq!(moved.map(&isolate.heap), map_before);
    assert_eq!(
        map_before.instance_descriptors(&isolate.heap),
        descriptors_before,
        "a descriptor array in old space is not evacuated either"
    );
    assert_eq!(Smi::to_int(isolate.get_property_by_str(moved, "x")), 1);

    // And transitions still lead to it.
    let fresh = isolate.new_js_object();
    isolate.set_property_by_str(fresh, "x", Smi::from_int(2));
    assert_eq!(fresh.map(&isolate.heap), map_before);
}

#[test]
fn the_initial_map_is_shared_and_never_mutated() {
    let mut isolate = Isolate::new();
    let initial = isolate.roots.js_object_map();
    let descriptors_before = initial.number_of_own_descriptors(&isolate.heap);

    for i in 0..10 {
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, &format!("f{i}"), Smi::zero());
        assert_ne!(object.map(&isolate.heap), initial);
    }

    assert_eq!(
        initial.number_of_own_descriptors(&isolate.heap),
        descriptors_before,
        "adding a property must never mutate the shape it started from"
    );
    assert_eq!(descriptors_before, 0);
    // Ten distinct first properties means ten edges out of the initial map.
    assert_eq!(
        TransitionsAccessor::number_of_transitions(&isolate.heap, initial),
        10
    );
    assert_eq!(
        isolate.new_js_object().map(&isolate.heap),
        initial,
        "a fresh object still starts at the shared initial map"
    );
}

#[test]
fn a_bad_address_panics_rather_than_corrupting_the_heap() {
    // The payoff of addressing the heap by offset: an address that does not
    // name a slot cannot be read at all.
    let heap = Heap::new();
    let result = std::panic::catch_unwind(move || heap.read_u64(0xdead_beef_0000));
    assert!(result.is_err(), "an out-of-arena read must not be silent");
}

#[test]
fn a_fresh_isolate_is_a_working_world() {
    let isolate = Isolate::new();
    // The bootstrap's fixed point.
    let meta = isolate.roots.meta_map();
    assert_eq!(isolate.heap.map(meta.object()), meta.object());
    assert_eq!(Map::K_SIZE, 56);
    // Nothing allocated outside read-only space yet.
    assert_eq!(isolate.heap.new_space().size(), 0);
    assert_eq!(isolate.heap.old_space().size(), 0);
    assert!(isolate.heap.read_only_space().size() > 0);
}

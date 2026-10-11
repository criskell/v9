//! Major collection: mark, clear the weak references, compact.
//!
//! The scavenger only ever touched the nursery, so old space grew and was
//! never reclaimed. These tests assert on what a major GC is *for*: that
//! dead old-space objects give their bytes back, that everything still
//! reachable still reads correctly after being moved, and that a weak
//! reference to something dead comes back cleared rather than dangling.

use vx::execution::isolate::Isolate;
use vx::objects::fixed_array::FixedArray;
use vx::objects::js_objects::JSObject;
use vx::objects::smi::Smi;
use vx::objects::tagged::Tagged;

/// Allocates `count` tenured arrays, keeping every `keep_every`-th one
/// reachable from a handle.
fn tenured_arrays(isolate: &mut Isolate, count: i32, keep_every: i32) -> Vec<Tagged> {
    let mut kept = Vec::new();
    for index in 0..count {
        let array = isolate.new_fixed_array_tenured(4);
        array.set(&mut isolate.heap, 0, Smi::from_int(index));
        if index % keep_every == 0 {
            kept.push(array.tagged());
        }
    }
    for value in &kept {
        isolate.handles.push(*value);
    }
    kept
}

#[test]
fn dead_old_space_objects_give_their_bytes_back() {
    let mut isolate = Isolate::new();
    let kept = tenured_arrays(&mut isolate, 100, 10);
    let before = isolate.heap.old_space().size();

    let stats = isolate.collect_all_garbage();

    assert_eq!(stats.marked_objects > 0, true, "something must be live");
    assert!(
        stats.swept_objects >= 90,
        "ninety of the hundred arrays are unreachable, swept {}",
        stats.swept_objects
    );
    assert!(
        isolate.heap.old_space().size() < before,
        "old space should shrink: {} -> {}",
        before,
        isolate.heap.old_space().size()
    );
    assert_eq!(kept.len(), 10);
}

#[test]
fn a_surviving_object_still_reads_correctly_after_moving() {
    let mut isolate = Isolate::new();
    tenured_arrays(&mut isolate, 100, 10);
    let addresses_before: Vec<u64> = isolate
        .handles
        .iter()
        .map(|value| value.to_strong_heap_object().unwrap().address())
        .collect();

    let stats = isolate.collect_all_garbage();
    assert!(
        stats.moved_objects > 0,
        "compaction should have moved something"
    );

    let handles = isolate.handles.clone();
    let mut moved = 0;
    for (position, value) in handles.iter().enumerate() {
        let object = value.to_strong_heap_object().unwrap();
        if object.address() != addresses_before[position] {
            moved += 1;
        }
        let array = FixedArray::unchecked_cast(object);
        assert_eq!(
            Smi::to_int(array.get(&isolate.heap, 0)),
            position as i32 * 10,
            "the {position}th survivor lost its contents"
        );
    }
    assert!(
        moved > 0,
        "handles should have been updated to new addresses"
    );
}

#[test]
fn references_between_survivors_are_updated() {
    // The hard part of compaction: a pointer held *inside* a moved object
    // to another moved object.
    let mut isolate = Isolate::new();

    // Garbage *before* the inner array as well, or it would sit at the
    // bottom of old space and never move -- and then the test would pass
    // without the update pass running at all.
    for _ in 0..50 {
        isolate.new_fixed_array_tenured(8);
    }
    let inner = isolate.new_fixed_array_tenured(2);
    inner.set(&mut isolate.heap, 0, Smi::from_int(1234));
    let inner_before = inner.object().address();

    for _ in 0..50 {
        isolate.new_fixed_array_tenured(8);
    }

    let outer = isolate.new_fixed_array_tenured(2);
    outer.set(&mut isolate.heap, 0, inner.tagged());
    isolate.handles.push(outer.tagged());

    isolate.collect_all_garbage();

    let outer = FixedArray::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    let inner = FixedArray::unchecked_cast(
        outer
            .get(&isolate.heap, 0)
            .to_strong_heap_object()
            .expect("the inner array should still be a live pointer"),
    );
    assert_ne!(
        inner.object().address(),
        inner_before,
        "the inner array has to move, or this test proves nothing"
    );
    assert_eq!(
        Smi::to_int(inner.get(&isolate.heap, 0)),
        1234,
        "the reference was not updated and now points at other bytes"
    );
}

#[test]
fn a_weak_reference_to_something_dead_comes_back_cleared() {
    // The scavenger treats weak references as strong, so this is the first
    // collection that can actually clear one.
    let mut isolate = Isolate::new();
    let target = isolate.new_fixed_array_tenured(2);
    let holder = isolate.new_fixed_array_tenured(2);
    holder.set(&mut isolate.heap, 0, target.object().tagged_weak());
    isolate.handles.push(holder.tagged());

    assert!(holder.get(&isolate.heap, 0).is_weak());

    let stats = isolate.collect_all_garbage();
    assert_eq!(
        stats.cleared_weak_references, 1,
        "nothing else holds the target, so the weak slot must be cleared"
    );

    let holder = FixedArray::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    assert!(
        holder.get(&isolate.heap, 0).is_cleared(),
        "a weak slot whose target died reads as cleared, not as a pointer"
    );
}

#[test]
fn a_weak_reference_to_something_alive_survives_and_is_updated() {
    let mut isolate = Isolate::new();
    let target = isolate.new_fixed_array_tenured(2);
    target.set(&mut isolate.heap, 0, Smi::from_int(99));
    for _ in 0..50 {
        isolate.new_fixed_array_tenured(8);
    }
    let holder = isolate.new_fixed_array_tenured(2);
    holder.set(&mut isolate.heap, 0, target.object().tagged_weak());
    // Both are reachable strongly, so the weak slot must be kept.
    isolate.handles.push(holder.tagged());
    isolate.handles.push(target.tagged());

    let stats = isolate.collect_all_garbage();
    assert_eq!(stats.cleared_weak_references, 0);

    let holder = FixedArray::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    let slot = holder.get(&isolate.heap, 0);
    assert!(
        slot.is_weak(),
        "it must still be weak, not promoted to strong"
    );
    let target = FixedArray::unchecked_cast(isolate.handles[1].to_strong_heap_object().unwrap());
    assert_eq!(
        slot.ptr() & !3,
        target.tagged().ptr() & !3,
        "the weak reference should point at where the target moved to"
    );
    assert_eq!(Smi::to_int(target.get(&isolate.heap, 0)), 99);
}

#[test]
fn a_whole_engine_heap_survives_a_major_collection() {
    // Everything the engine itself needs -- maps, internalized strings, the
    // global object, feedback vectors, closures -- goes through the same
    // update pass. Reading the globals back afterwards exercises all of it.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    isolate
        .run_script(
            "var o = { a: 1, b: 2 };
             var slow = {};
             var i = 0;
             while (i < 30) { slow['k' + i] = i; i = i + 1; }
             var sparse = [1];
             sparse[100000] = 5;
             var counter = (function () { var n = 7; return function () { return n; }; })();
             o.c = counter();",
        )
        .expect("runs");

    let stats = isolate.collect_all_garbage();
    assert!(stats.marked_objects > 0);

    let global = isolate.global_object();
    let o = JSObject::unchecked_cast(
        isolate
            .get_property_by_str(global, "o")
            .to_strong_heap_object()
            .expect("o survived"),
    );
    assert_eq!(Smi::to_int(isolate.get_property_by_str(o, "a")), 1);
    assert_eq!(Smi::to_int(isolate.get_property_by_str(o, "b")), 2);
    assert_eq!(
        Smi::to_int(isolate.get_property_by_str(o, "c")),
        7,
        "the closure's context slot survived"
    );

    let slow = JSObject::unchecked_cast(
        isolate
            .get_property_by_str(global, "slow")
            .to_strong_heap_object()
            .unwrap(),
    );
    assert!(!slow.has_fast_properties(&isolate.heap));
    assert_eq!(Smi::to_int(isolate.get_property_by_str(slow, "k29")), 29);
}

#[test]
fn collecting_twice_in_a_row_is_stable() {
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    isolate
        .run_script("var a = [1, 2, 3]; var o = { x: a };")
        .expect("runs");

    let first = isolate.collect_all_garbage();
    let second = isolate.collect_all_garbage();
    assert_eq!(
        second.swept_objects, 0,
        "the first collection should have taken all the garbage there was"
    );
    assert_eq!(
        second.moved_objects, 0,
        "a compacted heap has nothing left to slide"
    );
    assert!(second.marked_objects <= first.marked_objects);

    let global = isolate.global_object();
    let o = JSObject::unchecked_cast(
        isolate
            .get_property_by_str(global, "o")
            .to_strong_heap_object()
            .unwrap(),
    );
    let array = isolate.get_property_by_str(o, "x");
    let array = JSObject::unchecked_cast(array.to_strong_heap_object().unwrap());
    let accessor =
        vx::objects::elements::ElementsAccessor::for_kind(array.get_elements_kind(&isolate.heap));
    let value = accessor.get(&mut isolate, array, 2).expect("a[2]");
    assert_eq!(Smi::to_int(value), 3);
}

#[test]
fn a_transitioned_map_is_reached_through_the_object_that_wears_it() {
    // An object's own map is a reference like any other, and it is the only
    // one to a map built by a property transition. The scavenger can skip
    // the map word because maps never move; a major collection cannot, or
    // the map's descriptor array is swept out from under the object and the
    // next property read walks off the end of it.
    let mut isolate = Isolate::new();
    let object = isolate.new_js_object();
    isolate.handles.push(object.tagged());
    isolate.set_property_by_str(object, "alpha", Smi::from_int(11));
    isolate.set_property_by_str(object, "beta", Smi::from_int(22));

    // Tenure the descriptor array, so that it is in the space being swept.
    isolate.collect_garbage();
    isolate.collect_garbage();
    let object = JSObject::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    let descriptors = isolate
        .heap
        .map_of(object.object())
        .instance_descriptors(&isolate.heap);
    assert!(
        !isolate.heap.in_young_generation(descriptors.object()),
        "the descriptor array has to be out of the nursery for this test to bite"
    );

    isolate.collect_all_garbage();

    let object = JSObject::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    assert_eq!(
        Smi::to_int(isolate.get_property_by_str(object, "alpha")),
        11
    );
    assert_eq!(Smi::to_int(isolate.get_property_by_str(object, "beta")), 22);
    assert_eq!(
        isolate
            .heap
            .map_of(object.object())
            .number_of_own_descriptors(&isolate.heap),
        2
    );
}

#[test]
fn read_only_objects_are_never_moved() {
    // Maps live in read-only space precisely so that nothing has to update
    // a reference to them.
    let mut isolate = Isolate::new();
    isolate.setup_global_object();
    let before = isolate.roots.js_object_map;
    let object = isolate.new_js_object();
    isolate.handles.push(object.tagged());

    isolate.collect_all_garbage();

    assert_eq!(
        isolate.roots.js_object_map, before,
        "a read-only map must keep its address"
    );
    let object = JSObject::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    assert_eq!(
        isolate.heap.map_of(object.object()).tagged(),
        before,
        "and the object's map word must still point at it"
    );
}

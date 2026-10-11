//! `vx` command line.
//!
//! There is no parser yet, so this cannot take a `.js` file. What it does is
//! drive the engine through its API and show what the heap and the object
//! model are actually doing -- the equivalent of V8's `--trace-maps`,
//! `--trace-gc` and `--print-heap` output, on a worked example.

use vx::ast::prettyprinter::AstPrinter;
use vx::execution::isolate::Isolate;
use vx::heap::heap::Heap;
use vx::objects::instance_type;
use vx::objects::js_objects::JSObject;
use vx::objects::map::Map;
use vx::objects::smi::Smi;
use vx::objects::string::String as VxString;
use vx::objects::tagged::Tagged;
use vx::objects::transitions::TransitionsAccessor;

fn main() {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo".to_string());
    match command.as_str() {
        "demo" => {
            trace_maps();
            println!();
            trace_gc();
            println!();
            print_heap();
        }
        "maps" | "trace-maps" => trace_maps(),
        "gc" | "trace-gc" => trace_gc(),
        "heap" | "print-heap" => print_heap(),
        "roots" => print_roots(),
        "ast" | "parse" => match std::env::args().nth(2) {
            Some(path) => print_ast(&path),
            None => {
                eprintln!("vx: usage: vx ast <file.js>");
                std::process::exit(2);
            }
        },
        other => {
            eprintln!("vx: unknown command `{other}`");
            eprintln!();
            eprintln!("usage: vx [demo|maps|gc|heap|roots]");
            eprintln!();
            eprintln!("  demo    all of the below");
            eprintln!("  maps    hidden class transitions as properties are added");
            eprintln!("  gc      a scavenge, with what survived and what moved");
            eprintln!("  heap    space occupancy");
            eprintln!("  roots   the root table");
            eprintln!("  ast <file.js>");
            eprintln!("          parse a file and print its syntax tree");
            eprintln!();
            eprintln!("vx can parse JavaScript but not yet run it: the bytecode");
            eprintln!("pipeline is not built. See PORTING.md.");
            std::process::exit(2);
        }
    }
}

/// `--print-ast`: parse a file and print the tree.
fn print_ast(path: &str) {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("vx: cannot read {path}: {error}");
            std::process::exit(1);
        }
    };
    match vx::parsing::parser::Parser::parse_program(&source) {
        Ok(program) => print!("{}", AstPrinter::print_program(&program)),
        Err(error) => {
            eprintln!("{}", error.display(&source));
            std::process::exit(1);
        }
    }
}

fn name_of(isolate: &Isolate, value: Tagged) -> String {
    match VxString::try_cast(&isolate.heap, value) {
        Some(string) => string.to_rust_string(&isolate.heap),
        None => format!("{value:?}"),
    }
}

fn describe_map(isolate: &Isolate, map: Map) -> String {
    let properties = map
        .own_property_names(&isolate.heap)
        .iter()
        .map(|name| name_of(isolate, *name))
        .collect::<Vec<_>>();
    format!(
        "Map@{:x} {{{}}} size={} inline={} descs={}",
        map.address(),
        properties.join(", "),
        map.instance_size(&isolate.heap),
        map.inobject_properties(&isolate.heap),
        map.number_of_own_descriptors(&isolate.heap),
    )
}

/// `--trace-maps`: watch an object change shape.
fn trace_maps() {
    let mut isolate = Isolate::new();

    println!("== hidden classes ==");
    println!();
    println!("Building two objects the same way, and one the other way round.");
    println!();

    let a = isolate.new_js_object();
    println!("  var a = {{}}");
    println!("    {}", describe_map(&isolate, a.map(&isolate.heap)));

    for (name, value) in [("x", 1), ("y", 2)] {
        isolate.set_property_by_str(a, name, Smi::from_int(value));
        println!("  a.{name} = {value}");
        println!("    {}", describe_map(&isolate, a.map(&isolate.heap)));
    }

    let b = isolate.new_js_object();
    isolate.set_property_by_str(b, "x", Smi::from_int(10));
    isolate.set_property_by_str(b, "y", Smi::from_int(20));
    println!();
    println!("  var b = {{}}; b.x = 10; b.y = 20");
    println!("    {}", describe_map(&isolate, b.map(&isolate.heap)));
    println!(
        "    a and b share a map: {}",
        a.map(&isolate.heap) == b.map(&isolate.heap)
    );

    let c = isolate.new_js_object();
    isolate.set_property_by_str(c, "y", Smi::from_int(20));
    isolate.set_property_by_str(c, "x", Smi::from_int(10));
    println!();
    println!("  var c = {{}}; c.y = 20; c.x = 10      // other order");
    println!("    {}", describe_map(&isolate, c.map(&isolate.heap)));
    println!(
        "    a and c share a map: {}   <- property order is part of the shape",
        a.map(&isolate.heap) == c.map(&isolate.heap)
    );

    // Walk the transition graph out of the empty-object map.
    println!();
    println!("Transition graph from the initial object map:");
    print_transitions(&isolate, isolate.roots.js_object_map(), 1);

    // Spilling out of the inline slots.
    println!();
    let wide = isolate.new_js_object();
    let inline = wide.map(&isolate.heap).inobject_properties(&isolate.heap);
    for i in 0..inline + 2 {
        isolate.set_property_by_str(wide, &format!("p{i}"), Smi::from_int(i));
    }
    println!(
        "An object with {} properties and {inline} inline slots:",
        inline + 2
    );
    println!("  {}", describe_map(&isolate, wide.map(&isolate.heap)));
    println!(
        "  backing store: PropertyArray with {} slots",
        wide.property_array(&isolate.heap).length(&isolate.heap)
    );
    for i in 0..inline + 2 {
        let name = format!("p{i}");
        let key = isolate.internalize_string(&name).tagged();
        let details = wide
            .map(&isolate.heap)
            .find_property_details(&isolate.heap, key)
            .unwrap();
        let index = details.field_index() as i32;
        let where_ = if index < inline {
            format!("inline slot {index}")
        } else {
            format!("backing store slot {}", index - inline)
        };
        println!("    {name} -> field {index} ({where_})");
    }
}

fn print_transitions(isolate: &Isolate, map: Map, depth: usize) {
    let indent = "  ".repeat(depth);
    for (key, target) in TransitionsAccessor::transitions(&isolate.heap, map) {
        println!(
            "{indent}--{}--> {}",
            name_of(isolate, key),
            describe_map(isolate, target)
        );
        if depth < 4 {
            print_transitions(isolate, target, depth + 1);
        }
    }
}

/// `--trace-gc`: a scavenge, with the survivors.
fn trace_gc() {
    let mut isolate = Isolate::with_heap(Heap::with_config(vx::heap::heap::HeapConfig {
        read_only_space_size: 512 * 1024,
        // A small nursery, so the example is legible.
        semi_space_size: 16 * 1024,
        initial_old_space_size: 1024 * 1024,
        max_old_space_size: 64 * 1024 * 1024,
    }));

    println!("== garbage collection ==");
    println!();

    // One object graph we keep, parked in the handle scope so the collector
    // treats it as a root.
    let kept = isolate.new_js_object();
    let inner = isolate.new_js_object();
    isolate.set_property_by_str(inner, "deep", Smi::from_int(42));
    isolate.set_property_by_str(kept, "inner", inner.tagged());
    isolate.handles.push(kept.tagged());

    // ...and a pile we do not.
    for i in 0..200 {
        isolate.new_heap_number(i as f64 + 0.5);
    }

    println!("  kept:    one object holding another holding 42");
    println!("  garbage: 200 unreferenced HeapNumbers");
    println!(
        "  new space before: {} bytes",
        isolate.heap.new_space().size()
    );
    let mut address_before = isolate.handles[0].address_unchecked();

    for round in 1..=3 {
        let stats = isolate.collect_garbage();
        let address_after = isolate.handles[0].address_unchecked();
        let object = JSObject::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
        let inner_after = isolate.get_property_by_str(object, "inner");
        let inner_object = JSObject::unchecked_cast(inner_after.to_strong_heap_object().unwrap());
        let deep = isolate.get_property_by_str(inner_object, "deep");

        println!();
        println!("  scavenge #{round}");
        println!(
            "    evacuated {} objects ({} bytes), promoted {} objects ({} bytes)",
            stats.objects_evacuated,
            stats.bytes_evacuated,
            stats.objects_promoted,
            stats.bytes_promoted
        );
        println!(
            "    new space {} -> {} bytes",
            stats.new_space_before, stats.new_space_after
        );
        let generation = if isolate.heap.in_young_generation(object.object()) {
            "still young"
        } else {
            "tenured"
        };
        if address_after == address_before {
            println!("    kept object stayed at 0x{address_after:x} ({generation})");
        } else {
            println!(
                "    kept object moved 0x{address_before:x} -> 0x{address_after:x} ({generation})"
            );
        }
        println!(
            "    its graph is intact: inner.deep == {}",
            Smi::to_int(deep)
        );

        address_before = address_after;

        // More garbage for the next round.
        for i in 0..50 {
            isolate.new_heap_number(i as f64);
        }
    }
}

fn print_heap() {
    let mut isolate = Isolate::new();
    // Put something in it first, so the numbers mean something.
    for i in 0..100 {
        let object = isolate.new_js_object();
        isolate.set_property_by_str(object, "x", Smi::from_int(i));
        isolate.set_property_by_str(object, "y", Smi::from_int(i));
        isolate.new_string(&format!("string number {i}"));
    }

    let stats = isolate.heap.stats();
    println!("== heap ==");
    println!();
    println!("  read-only space  {:>9} bytes used", stats.read_only_size);
    println!(
        "  new space        {:>9} / {} bytes",
        stats.new_space_size, stats.new_space_capacity
    );
    println!(
        "  old space        {:>9} / {} bytes",
        stats.old_space_size, stats.old_space_capacity
    );
    println!("  arena            {:>9} bytes reserved", stats.arena_size);
    println!("  collections      {:>9}", stats.gc_count);
    println!(
        "  string table     {:>9} internalized names",
        isolate.string_table_size()
    );
}

fn print_roots() {
    let isolate = Isolate::new();
    println!("== roots ==");
    println!();
    for (name, value) in isolate.roots.named() {
        let description = match value.to_strong_heap_object() {
            None => "<unset>".to_string(),
            Some(object) => {
                let instance_type = isolate.heap.instance_type(object);
                let mut description = format!(
                    "0x{:<8x} {}",
                    object.address(),
                    instance_type::instance_type_name(instance_type)
                );
                if instance_type::is_string(instance_type) {
                    description.push_str(&format!(
                        " {:?}",
                        VxString::unchecked_cast(object).to_rust_string(&isolate.heap)
                    ));
                } else if instance_type::is_map(instance_type) {
                    let map = Map::unchecked_cast(object);
                    description.push_str(&format!(
                        " of {}",
                        instance_type::instance_type_name(map.instance_type(&isolate.heap))
                    ));
                }
                description
            }
        };
        println!("  {name:<34} {description}");
    }
}

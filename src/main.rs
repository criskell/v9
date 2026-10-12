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
        "ast" | "parse" => with_file("ast", print_ast),
        "run" => with_file("run", run_file),
        "bytecode" | "print-bytecode" => with_file("bytecode", print_bytecode),
        "ic" | "trace-ic" => with_file("ic", trace_ic),
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
            eprintln!("  run <file.js>");
            eprintln!("          run a script");
            eprintln!("  ast <file.js>");
            eprintln!("          parse a file and print its syntax tree");
            eprintln!("  bytecode <file.js>");
            eprintln!("          compile a file and disassemble it");
            eprintln!("  ic <file.js>");
            eprintln!("          run a file and report its inline caches");
            eprintln!();
            std::process::exit(2);
        }
    }
}

fn with_file(command: &str, action: fn(&str, &str)) {
    let Some(path) = std::env::args().nth(2) else {
        eprintln!("vx: usage: vx {command} <file.js>");
        std::process::exit(2);
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("vx: cannot read {path}: {error}");
            std::process::exit(1);
        }
    };
    action(&path, &source);
}

fn print_ast(_path: &str, source: &str) {
    match vx::parsing::parser::Parser::parse_program(source) {
        Ok(program) => print!("{}", AstPrinter::print_program(&program)),
        Err(error) => {
            eprintln!("{}", error.display(source));
            std::process::exit(1);
        }
    }
}

fn run_file(_path: &str, source: &str) {
    let mut isolate = Isolate::new();
    let result = isolate.run_script(source);
    for line in isolate.take_output() {
        println!("{line}");
    }
    match result {
        Ok(value) => {
            if value != isolate.undefined_value() {
                println!("{}", isolate.to_string(value));
            }
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

fn trace_ic(_path: &str, source: &str) {
    let mut isolate = Isolate::new();
    isolate.trace_ic_enabled = true;
    let result = isolate.run_script(source);
    for line in isolate.take_output() {
        println!("{line}");
    }
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }

    println!("== transitions ==");
    println!();
    for line in isolate.ic_transition_log() {
        println!("  {line}");
    }

    println!();
    println!("== the script's own sites ==");
    println!();
    for line in isolate.trace_ic() {
        println!("  {line}");
    }
    let statistics = isolate.ic_statistics();
    println!();
    println!("== statistics ==");
    println!();
    for (label, site) in [
        ("property (map + field)", statistics.property),
        ("global   (cell)       ", statistics.global),
    ] {
        println!(
            "  {label}  {:>7} site hits  {:>5} shared hits  {:>5} misses  \
             {:>5.1}% answered without a lookup",
            site.hits,
            site.shared_hits,
            site.misses,
            site.hit_rate() * 100.0
        );
    }
    println!(
        "  property transitions: {} -> monomorphic, {} -> polymorphic, {} -> megamorphic",
        statistics.property.monomorphic_transitions,
        statistics.property.polymorphic_transitions,
        statistics.property.megamorphic_transitions
    );

    let (primary, secondary, misses, updates) = isolate.stub_cache_statistics();
    let (primary_used, secondary_used) = isolate.stub_cache_occupancy();
    println!(
        "  stub cache: {} primary hits, {} secondary hits, {} misses, {} updates",
        primary, secondary, misses, updates
    );
    println!("              {primary_used}/4096 primary slots, {secondary_used}/1024 secondary");
}

fn print_bytecode(_path: &str, source: &str) {
    use vx::interpreter::bytecode_array_iterator::disassemble;
    use vx::interpreter::bytecode_generator::BytecodeGenerator;
    use vx::interpreter::constant_array_builder::ConstantEntry;

    let program = match vx::parsing::parser::Parser::parse_program(source) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("{}", error.display(source));
            std::process::exit(1);
        }
    };
    let compiled = match BytecodeGenerator::generate(&program) {
        Ok(compiled) => compiled,
        Err(error) => {
            eprintln!("vx: {}", error.message);
            std::process::exit(1);
        }
    };

    for function in &compiled.functions {
        let name = if function.name.is_empty() {
            "<anonymous>".to_string()
        } else {
            function.name.clone()
        };
        println!(
            "=== [{}] parameters: {}, registers: {}, feedback slots: {} ===",
            name,
            function.declared_parameter_count,
            function.data.register_count,
            function.data.feedback_slot_count
        );
        let constants: Vec<String> = function
            .data
            .constants
            .iter()
            .map(|entry| match entry {
                ConstantEntry::Smi(value) => value.to_string(),
                ConstantEntry::Number(value) => value.to_string(),
                ConstantEntry::String(value) => format!("{value:?}"),
                ConstantEntry::Name(value) => format!("#{value}"),
                ConstantEntry::ClassFieldsSymbol => "<class fields symbol>".to_string(),
                ConstantEntry::ClassBoilerplate(members) => {
                    let names: Vec<&str> = members
                        .iter()
                        .map(|(_, name)| match name {
                            Some(name) => name.as_str(),
                            None => "[]",
                        })
                        .collect();
                    format!("<class {{{}}}>", names.join(", "))
                }
                ConstantEntry::FunctionLiteral(index) => format!("<function {index}>"),
                ConstantEntry::ScopeInfo(names) => format!("<scope {}>", names.join(", ")),
                ConstantEntry::TheHole => "<the_hole>".to_string(),
            })
            .collect();
        print!("{}", disassemble(&function.data.bytecodes, &constants));
        if !constants.is_empty() {
            println!("constant pool: [{}]", constants.join(", "));
        }
        println!();
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

    println!();
    println!("Transition graph from the initial object map:");
    print_transitions(&isolate, isolate.roots.js_object_map(), 1);

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

fn trace_gc() {
    let mut isolate = Isolate::with_heap(Heap::with_config(vx::heap::heap::HeapConfig {
        read_only_space_size: 512 * 1024,
        semi_space_size: 16 * 1024,
        initial_old_space_size: 1024 * 1024,
        max_old_space_size: 64 * 1024 * 1024,
    }));

    println!("== garbage collection ==");
    println!();

    let kept = isolate.new_js_object();
    let inner = isolate.new_js_object();
    isolate.set_property_by_str(inner, "deep", Smi::from_int(42));
    isolate.set_property_by_str(kept, "inner", inner.tagged());
    isolate.handles.push(kept.tagged());

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

        for i in 0..50 {
            isolate.new_heap_number(i as f64);
        }
    }

    for i in 0..200 {
        isolate.new_fixed_array_tenured(4);
        isolate.new_heap_number_tenured(i as f64);
    }
    let old_before = isolate.heap.old_space().size();
    let stats = isolate.collect_all_garbage();
    println!();
    println!("  mark-compact");
    println!(
        "    marked {} objects ({} bytes), swept {} objects ({} bytes)",
        stats.marked_objects, stats.marked_bytes, stats.swept_objects, stats.swept_bytes
    );
    println!(
        "    compacted {} objects, cleared {} weak references",
        stats.moved_objects, stats.cleared_weak_references
    );
    println!(
        "    old space {} -> {} bytes",
        old_before,
        isolate.heap.old_space().size()
    );
    let object = JSObject::unchecked_cast(isolate.handles[0].to_strong_heap_object().unwrap());
    let inner = isolate.get_property_by_str(object, "inner");
    let inner = JSObject::unchecked_cast(inner.to_strong_heap_object().unwrap());
    println!(
        "    its graph is intact: inner.deep == {}",
        Smi::to_int(isolate.get_property_by_str(inner, "deep"))
    );
}

fn print_heap() {
    let mut isolate = Isolate::new();
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

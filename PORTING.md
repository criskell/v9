# Porting ledger

`vx` is V8 ported to Rust, module by module. This file is the ledger: what is
ported, what is stubbed, and where `vx` deliberately differs.

Reference checkout: `~/devel/v8` at `a74948bb81a5` (2026-10-10).

## Ground rules

1. **V8 is the source of truth for structure.** Module tree, type names,
   layouts, bit encodings and algorithms follow V8. Every module names the
   file(s) it came from.
2. **Not a transcription.** Where C++ shape and Rust shape disagree, Rust
   wins — `Option` instead of out-parameters, enums instead of tag fields,
   methods instead of `AllStatic` classes where it reads better.
3. **No `unsafe`.** V8 dereferences tagged pointers directly; `vx` makes an
   address an offset into a heap arena, reachable only through the `Heap` that
   owns it. Every field access is bounds and alignment checked. A stale
   address is a panic with a message, never undefined behaviour.
4. **Comments in `src/` are V8's comments.** Nothing in the Rust source is
   commentary written for `vx`: a comment is kept only where V8 has one, and
   kept verbatim. The two exceptions are mechanical, not prose — a doc line
   naming the V8 symbol a Rust item was ported from (`/// \`Map::AsElementsKind\``),
   and nothing else.
5. **A deviation is documented here,** in this file, not in the source. Rule
   4 leaves nowhere else for it to go, and the list below is the whole
   record.

`tools/check-comments.py <v8-checkout>` enforces rule 4: it reports every
comment line in `src/` that does not appear in V8's source, matching on word
shingles so that a comment reflowed to different line breaks still counts as
V8's. It currently reports zero.

`tools/strip-comments.py <file>...` is its other half, for when a sweep is
needed: it deletes what the checker reports and never writes a comment. It
decides per contiguous comment block rather than per line, because a layout
diagram judged line by line keeps its frame and loses its content.

## Configuration

`vx` targets one V8 build configuration and only that one:

| Flag | Setting |
| --- | --- |
| `V8_COMPRESS_POINTERS` | off — a tagged word is 8 bytes, no cage |
| `V8_EXTERNAL_CODE_SPACE` | off |
| `V8_ENABLE_SANDBOX` | off |
| `V8_MAP_PACKING` | off |
| Smi | `SmiTagging<8>`: full `i32`, in the upper half of the word |

## Layout

Every `vx` source file maps to a V8 source file **at the same path**, with
`snake_case` for V8's `kebab-case`. `tools/check-layout.sh <v8-checkout>`
enforces it:

```
src/objects/map.rs          <- src/objects/{map.h, map.cc, map.tq}
src/heap/scavenger.rs       <- src/heap/{scavenger.h, scavenger.cc}
src/parsing/token.rs        <- src/parsing/{token.h, token.cc}
```

Function and method names follow suit: `Map::TransitionToDataProperty`
becomes `transition_to_data_property`, `Scavenger::ShouldBePromoted` becomes
`should_be_promoted`. Where V8 decomposes a job into several methods, `vx`
keeps the same decomposition even when one Rust function would do, because
the decomposition is how the original is navigated.

### Splits `vx` deliberately does not copy

Three kinds of file boundary in V8 exist because of C++, not because of the
engine's structure, and reproducing them would be cargo cult:

* **`*-inl.h`** — inline definitions split out of the header so other
  headers can include the declarations without the bodies. Rust has no
  separate declaration, so these fold into the one module.
* **`*.tq`** — Torque sources, which *generate* C++ layout accessors. `vx`
  reads them for the layouts and writes the accessors by hand, so they fold
  into the corresponding module.
* **`parser-base.h` vs `parser.cc`** — `ParserBase` is a CRTP template so
  that `Parser` and `PreParser` can share the grammar while instantiating
  different node types. `vx` has no preparser, so there is nothing to share
  and the split would be two files with one implementation. If lazy parsing
  is ever added, this is where it reappears.

Where a C++ class lives in a header `vx` does not have (`Scavenger` and
`ScavengerCollector` in `scavenger.h`, `StringTable` in `string-table.h`),
its methods hang off `Isolate` in the matching module. That is because they
all need the heap, the roots and the factory at once; V8 passes the isolate
into the equivalent static methods for exactly the same reason.

## Fidelity

"Ported" does not mean the same thing in every row below, and the difference
matters when judging whether `vx` can be trusted to behave like V8. Three
levels, marked in the status table:

**[T] transcribed** — the constants, bit encodings, layouts and orderings are
V8's values, read out of the V8 source. These are not reconstructions: the
numbers *are* the specification, and inventing them produces something that
is merely plausible. Reading the source rather than working from memory is
what caught `kHeapObjectTagSize` being 2 bits rather than 1, and a forwarding
address being tag `00` rather than `11`.

**[A] algorithm ported** — V8's method was read and its logic reproduced, with
V8's method names, but not line by line. Signatures gain a heap parameter,
out-parameters become `Option`, errors become `Result`. Behaviour should
match; the code does not.

**[R] reconstructed** — written against the language specification and V8's
*structure* (method names, call chain) rather than its code, with only
targeted parts of the original read. These are the rows to distrust: V8's
corner cases are not in them unless they happen to coincide. `parser.rs` is
the clearest case — `parser-base.h` is 7,000 lines and most of that length
*is* corner cases.

A reconstruction is not automatically wrong, but it is reliably worse at the
edges. Writing `ParseLogicalExpression` from the grammar gave a version that
parsed the head at precedence 4 and then checked explicitly for an illegal
`??` mixture. V8 parses the head at precedence 6, and the mixture then
becomes unparseable on its own — no check, and correct in cases the explicit
check got wrong. Reading the original replaced worse code with better code,
which is the argument for reading it.

## Status

| V8 | `vx` | State |
| --- | --- | --- |
| `include/v8-internal.h`, `src/common/globals.h` | `src/common/globals.rs` | [T] tagging, alignment, sizes |
| `src/objects/tagged-impl.h`, `tagged.h`, `heap-object.h` | `src/objects/tagged.rs` | [T] `Tagged`, `HeapObject`, strong/weak predicates |
| `src/objects/smi.h` | `src/objects/smi.rs` | [T] done |
| `src/objects/map-word.h`, `map-word-inl.h` | `src/objects/map_word.rs` | [T] done |
| `src/objects/instance-type.h` | `src/objects/instance_type.rs` | [T] string bits exact; non-string types renumbered |
| `src/objects/property-details.h` | `src/objects/property_details.rs` | [T] bit layout exact; `Representation` tracked, not exploited |
| `src/objects/fixed-array.h` | `src/objects/fixed_array.rs` | [T] `FixedArray`, `PropertyArray`, `WeakFixedArray` |
| `src/objects/descriptor-array.h` | `src/objects/descriptor_array.rs` | [T] layout; [R] linear search, no enum cache |
| `src/objects/map.h`, `map.tq`, `map-inl.h` | `src/objects/map.rs` | [T] layout + `bit_field`/`bit_field3` |
| `src/objects/transitions.h`, `transitions.cc` | `src/objects/transitions.rs` | [A] `Uninitialized` / `WeakRef` / `FullTransitionArray` |
| `src/objects/name.h`, `string.h`, `string-hasher-inl.h` | `src/objects/string.rs` | [T] layout + hasher; sequential strings only |
| `src/objects/heap-number.h` | `src/objects/heap_number.rs` | [T] done |
| `src/objects/oddball.h` | `src/objects/oddball.rs` | [T] done |
| `src/objects/js-objects.h` | `src/objects/js_objects.rs` | [T] `JSObject` layout; `JSFunction` is a placeholder |
| `src/objects/lookup.cc`, transition half of `map.cc` | `src/objects/lookup.rs` | [A] data properties on the prototype chain |
| `src/objects/js-function.h` | `src/objects/js_function.rs` | [T] layout: shared, context, feedback vector |
| `src/objects/contexts.h` | `src/objects/contexts.rs` | [T] slot indices; function contexts only |
| `src/objects/scope-info.h` | `src/objects/scope_info.rs` | [A] the context slot names, nothing else |
| `src/roots/roots.h` | `src/roots/roots.rs` | [A] the roots `vx` needs |
| `src/execution/isolate.h` | `src/execution/isolate.rs` | [A] heap + roots + string table + bootstrap |
| `src/heap/spaces.h`, `new-spaces.h`, `paged-spaces.h` | `src/heap/spaces.rs` | [A] bump allocation, no pages |
| `src/heap/heap.h`, `heap.cc` | `src/heap/heap.rs` | [A] arena, spaces, raw access |
| `src/heap/factory.h`, `factory.cc` | `src/heap/factory.rs` | [A] the objects `vx` can build |
| `src/heap/heap-visitor.h` | `src/heap/heap_visitor.rs` | [A] switches on instance type, not `visitor_id` |
| `src/heap/scavenger.h`, `scavenger.cc` | `src/heap/scavenger.rs` | [A] Cheney copying collector with `age_mark` promotion |
| `src/numbers/conversions.cc` | `src/numbers/conversions.rs` | [A] `Number::toString` per ECMA-262 §6.1.6.1.20 |
| `src/objects/objects.h`, `objects.cc` | `src/objects/objects.rs` | [A] the abstract operations: `ToNumber`, `Add`, the comparisons |
| `src/objects/bytecode-array.h` | `src/objects/bytecode_array.rs` | [T] layout; no handler table |
| `src/objects/shared-function-info.h` | `src/objects/shared_function_info.rs` | [T] layout; no `ScopeInfo`, no sandbox split |
| `src/interpreter/bytecode-operands.h` | `src/interpreter/bytecode_operands.rs` | [T] operand types, scales, accumulator use |
| `src/interpreter/bytecode-register.h` | `src/interpreter/bytecode_register.rs` | [T] `Register`, `RegisterList` |
| `src/interpreter/bytecodes.h`, `.cc` | `src/interpreter/bytecodes.rs` | [T] `BYTECODE_LIST` for the ported subset |
| `src/interpreter/constant-array-builder.cc` | `src/interpreter/constant_array_builder.rs` | [A] three slices, reservations |
| `src/interpreter/bytecode-node.h` | `src/interpreter/bytecode_node.rs` | [A] |
| `src/interpreter/bytecode-label.h` | `src/interpreter/bytecode_label.rs` | [A] |
| `src/interpreter/bytecode-array-writer.cc` | `src/interpreter/bytecode_array_writer.rs` | [A] encoding, jump patching, dead-code elision |
| `src/interpreter/bytecode-register-allocator.h` | `src/interpreter/bytecode_register_allocator.rs` | [A] |
| `src/interpreter/bytecode-array-builder.cc` | `src/interpreter/bytecode_array_builder.rs` | [A] the generator's interface |
| `src/interpreter/bytecode-array-iterator.cc` | `src/interpreter/bytecode_array_iterator.rs` | [A] decoding, plus `--print-bytecode` |
| `src/interpreter/control-flow-builders.cc` | `src/interpreter/control_flow_builders.rs` | [A] break/continue label bookkeeping |
| `src/interpreter/bytecode-generator.cc` | `src/interpreter/bytecode_generator.rs` | [A] AST to bytecode, with V8's result scopes |
| `src/interpreter/interpreter-generator.cc` | `src/interpreter/interpreter.rs` | [A] CSA handlers become a dispatch loop |
| `src/builtins/builtins.h` | `src/builtins/builtins.rs` | [R] a handful, in Rust |
| `src/init/bootstrapper.cc` | `src/init/bootstrapper.rs` | [R] the global object and the builtins |
| `src/objects/feedback-vector.h`, `.cc` | `src/objects/feedback_vector.rs` | [T] slot kinds and widths; [A] the nexus and the state encoding |
| `src/ic/handler-configuration.h` | `src/ic/handler_configuration.rs` | [T] the Smi handler bit layout |
| `src/ic/ic.h`, `ic.cc` | `src/ic/ic.rs` | [A] the monomorphic/polymorphic/megamorphic lattice, plus the global ICs |
| `src/objects/property-cell.h` | `src/objects/property_cell.rs` | [T] layout; [A] the cell-type lattice |
| `src/ic/stub-cache.h`, `.cc` | `src/ic/stub_cache.rs` | [T] table sizes and both hash functions; [A] the victim-cache eviction |
| `src/parsing/token.h` | `src/parsing/token.rs` | [T] full token list, precedences and enum order |
| `src/parsing/scanner.h`, `scanner.cc` | `src/parsing/scanner.rs` | [A] `c0_`, two-token lookahead, ASI line tracking |
| `src/ast/ast.h` | `src/ast/ast.rs` | [R] node kinds follow `AST_NODE_LIST`; enums replace the class hierarchy |
| `src/ast/scopes.h`, `scopes.cc` | `src/ast/scopes.rs` | [A] declaration, resolution, `AllocateVariables` |
| `src/ast/prettyprinter.cc` | `src/ast/prettyprinter.rs` | [R] `--print-ast` |
| `src/parsing/parser-base.h`, `parser.cc` | `src/parsing/parser.rs` | **[R]** method chain and names are V8's; the bodies are written from the grammar |

### Not started

| V8 | Would become | Notes |
| --- | --- | --- |
| `src/objects/scope-info.h` (the rest) | `src/objects/scope_info.rs` | `with`, `catch`, block and module scopes |
| `src/heap/mark-compact.cc` | `src/heap/` | old-generation collection |
| `src/objects/js-array.h`, `elements.cc` | `src/objects/` | real arrays with `length` and elements kinds |
| `src/objects/dictionary.h` | `src/objects/` | dictionary-mode objects |
| `src/compiler/`, `src/maglev/`, `src/baseline/` | — | optimizing tiers; not planned |

## Deviations

### Memory model

V8 dereferences a tagged pointer. `vx` treats an address as an offset into one
contiguous arena, so accessors take `&Heap` / `&mut Heap`:

```cpp
int Map::instance_size() const;                     // V8
```
```rust
fn instance_size(self, heap: &Heap) -> i32;         // vx
```

Costs one subtraction and one bounds check per field access. Buys: no
`unsafe`, a panic instead of corruption on a bad address, and addresses that
survive the arena being reallocated — which is why growing old space is a
`Vec::resize` rather than a reservation game.

### Garbage collection

* **No pages.** A space is one run of bytes, not a list of `MemoryChunk`s.
  Pages are what buy V8 incremental sweeping, per-page remembered sets and
  growth without reservation tricks. Needed when mark-compact arrives.
* **No write barrier, no remembered set.** A scavenge sweeps old and
  read-only space linearly for references into from-space. Correct, but
  O(old space) per collection instead of O(recorded slots).
* **Weak references are treated as strong during a scavenge.** V8 clears the
  ones whose target died in a separate phase. `vx` keeps them one cycle
  longer and preserves weakness in the updated slot. Nothing is affected yet:
  the only weak slots are transition targets, and maps are immovable.
* **No old-generation collection.** Old space grows and is never swept.
* **Collection is explicit.** `Isolate::collect_garbage` is called at a
  safepoint; a failed allocation does *not* trigger a GC and retry. Making it
  do so needs the handle discipline that V8's `Handle<T>` enforces — the
  `Isolate::handles` scope is the beginning of it, but every allocation site
  would have to use it.
* **Maps and internalized strings live in read-only space.** V8 puts them in
  old space and relies on the write barrier. Keeping them immovable is what
  makes the previous two points sound: an inline cache's cached map, and a
  descriptor array's keys, never need updating.
* **Immediate promotion on second survival.** Implemented with the heap's
  `age_mark`, which is V8's own mechanism, but V8 tracks the intermediate
  generation per page.

### Object model

* **Non-string instance types are renumbered.** V8 generates them from
  Torque. The string half of the encoding is bit-exact because those bits are
  load-bearing; the rest preserves only the ordering the range checks need.
* **`Map` omits `dependent_code` and `prototype_validity_cell`.** Both exist
  to invalidate optimized code when a shape assumption breaks, and there is
  no optimizing tier to invalidate. Omitted rather than stubbed, so adding a
  JIT has to confront them.
* **Descriptor arrays and transition arrays are searched linearly.** V8 keeps
  both sorted by name hash and binary-searches above a threshold. A linear
  miss is the cost; a hit on a hot property never reaches here once inline
  caches exist.
* **No dictionary-mode objects.** `Map::is_dictionary_map` exists and is
  never set, so an object with thousands of properties keeps growing its
  shape chain instead of degrading to a hash table.
* **Field representation is widened in place.** V8 builds a new map and
  deprecates the old one so specialised code is invalidated; `vx` edits the
  descriptor and marks the map unstable. Sound only while nothing specialises
  on representation.
* **Only sequential strings.** No `ConsString`, `SlicedString`, `ThinString`
  or external strings. The instance-type bits for all of them are present and
  the read paths check `IsIndirectString`, so adding them is implementing
  representations rather than reshaping the model.
* **Lookup handles data properties only.** No accessors, proxies,
  interceptors or indexed elements.
* **No `Context`.** A `JSFunction` has a real `SharedFunctionInfo` but no
  captured scope, so a function can reach its own parameters and locals and
  the globals, and nothing else. A closure over an enclosing local does not
  work, and `let`/`const` are compiled as function-scoped, so their block
  scoping and temporal dead zone are not enforced.
* **An array is a plain object.** `CreateEmptyArrayLiteral` makes a
  `JSObject`, and keyed access converts its key to a string, so `[1,2,3]`
  becomes an object with `"0"`, `"1"`, `"2"` properties and no `length`.
  Real arrays need `JSArray` and elements kinds.
* **Inline caches cover named property access only.** A load or store of a
  known name is cached: one map comparison, then a Smi handler decoded into
  a field offset. What is *not* cached:
  * **A property found on a prototype.** That needs V8's object-form
    handler, which carries the holder and a validity cell to invalidate the
    entry if the prototype chain changes.
  * **`globalThis.x`.** A property load whose receiver is the global object
    finds a `PropertyCell`, not a value, so V8 caches it with a `kGlobal`
    handler -- which is an object-form handler carrying the cell. `vx` has
    only the Smi form, so this path is left uncached rather than cached
    wrongly. (`x` on its own, which compiles to `LdaGlobal`, *is* cached.)
  * **A store that adds a property**, because that changes the receiver's
    map. V8 caches it with a transition handler that performs the map change.
  * **Keyed access, calls, binary operations.** The slots are allocated and
    the embedded feedback byte is emitted, but nothing reads them.
* **Globals are cached through property cells.** A global's value lives in
  a `PropertyCell`, and a global access caches a weak reference to the cell
  -- so declaring another global cannot invalidate it. V8 keeps the cells in
  a `GlobalDictionary` on `JSGlobalObject`, so adding a global does not
  reshape it at all; `vx` stores each cell as an ordinary fast property,
  which does reshape the global object. That costs nothing here precisely
  *because* the caches hold cells rather than maps, which is the point the
  indirection exists to make.
* **`PropertyCell::dependent_code` is always empty.** It exists so that
  assigning to a global that an optimizing compiler had inlined as a
  constant can throw that code away. There is no such code.
* **The stub cache holds only Smi handlers**, like the per-site caches, so
  the pairs it cannot describe are the same ones: a prototype load, a
  transitioning store. V8's table holds any handler.
* **The stub cache is never cleared.** V8 clears it when maps move; in `vx`
  a map and an internalized name both live in read-only space and a handler
  is a Smi, so no entry can be invalidated by a collection and the table
  needs neither tracing nor clearing. The assertion in
  `update_megamorphic_cache` states that, where V8 asserts only the weaker
  "not in the young generation".

### Per-file deviations

Rule 4 leaves no room for these in the source, so they live here. Each entry
is what the Rust file leaves out of, or does differently from, the V8 file it
maps to.

| File | Deviation |
| --- | --- |
| `ast/ast.rs` | No classes, generators, async functions, destructuring, template literals, `try`/`catch`, `switch`, `with`, labels or regular expressions. |
| `ast/scopes.rs` | Function scopes only. V8 also has block, eval, with, catch, class and module scopes, which is what makes `let` block-scoped and gives it a temporal dead zone; `vx` gives `let`/`const` function scope, so neither is enforced. No `eval`, so no `VariableLocation::LOOKUP`. |
| `heap/paged_spaces.rs` | V8 grows a paged space by committing another `MemoryChunk`. `vx` has no pages, so growth raises the space's limit and the arena extends — sound only because an address is an offset, so extending cannot invalidate one. |
| `heap/read_only_spaces.rs` | V8 serialises this space into a snapshot and maps it read-only at startup, which is most of why it is a separate space. `vx` builds it every time. |
| `heap/scavenger.rs` | No remembered set: V8's write barrier records old-to-young references so a scavenge touches only those, while `vx` walks old and read-only space linearly — correct, but O(old space) per collection. Single threaded. Weak references are treated as strong, so they survive one cycle longer. No old-generation collection. |
| `heap/setup_heap_internal.rs` | V8 skips almost all of this by deserialising a snapshot. |
| `heap/spaces.rs` | No pages: a space is one run of bytes, not a list of `MemoryChunk`s. Pages buy incremental sweeping, per-page remembered sets and growth without reservation games, none of which is needed before mark-compact. |
| `ic/handler_configuration.rs` | Only the Smi handler form. V8 also has `DataHandler` objects for what a Smi cannot describe — accessors, proxies, a load from a prototype needing the holder and a validity cell — so a property found on a prototype is not cached. |
| `ic/ic.rs` | No `RECOMPUTE_HANDLER` (it exists for prototype-chain invalidation via validity cells) and no MEGADOM. A property found on a prototype is not cached, because that needs the object-form handler. |
| `init/bootstrapper.rs` | V8 installs the standard library through a `NativeContext` holding the realm's prototypes, constructors and intrinsics. `vx` has a global object with a handful of builtins, so realms and per-realm prototypes do not exist. |
| `interpreter/bytecode_register.rs` | V8 derives the fixed slot indices from `InterpreterFrameConstants`, because a `Register` is an offset into a machine frame that generated code also reads. `vx` interprets in a `Vec<Tagged>` and defines the layout here, keeping V8's ordering, which is what the encoding depends on. |
| `interpreter/bytecodes.rs` | 103 of V8's 196 bytecodes. No debug-break bytecodes, no `*Constant` jump variants, no generators, no `for...in`, no class or template literals, no runtime calls or intrinsics. |
| `interpreter/control_flow_builders.rs` | V8's builders hold a pointer to the `BytecodeArrayBuilder` and bind their targets in a destructor. `vx`'s hold only labels and take the builder as a parameter: two mutable borrows cannot coexist, and an explicit `finish` is clearer than a `Drop` that needs an argument it cannot have. |
| `interpreter/interpreter.rs` | A call recurses into `execute` rather than pushing a machine frame, so JavaScript recursion is bounded by Rust's stack and is checked explicitly. Keyed property access converts its key to a string name, so an array is an object with `"0"`, `"1"` … properties and has no `length`. |
| `objects/bytecode_array.rs` | No handler table (no `try`/`catch`), no `incoming_new_target_or_generator_register`, no sandbox wrapper. V8 also puts this in trusted space so corrupting it cannot forge bytecode; `vx` has one heap. |
| `objects/contexts.rs` | Function contexts only. V8 also has block, catch, with, eval, module and native contexts — the last holding the realm's intrinsics, which in `vx` hang off the global object. No `ContextCell`. |
| `objects/feedback_vector.rs` | V8's sentinels are `Symbol`s; `vx` has no symbols and uses oddballs. V8 packs slot kinds into five bits each in `FeedbackMetadata`; `vx` uses a byte apiece. No `invocation_count_before_stable`, no OSR state, no closure feedback cell array. |
| `objects/lookup.rs` | The data-property path only: no accessors, proxies, interceptors or indexed elements. Each is a branch in V8's state machine. |
| `objects/map.rs` | No `dependent_code`, no `prototype_validity_cell`. Both exist to invalidate optimized code when a shape assumption breaks; omitted rather than stubbed, so adding a JIT has to confront them. |
| `objects/map_updater.rs` | `vx` widens a descriptor **in place** and marks the map unstable, rather than building a replacement and deprecating the old one. Sound only because nothing specialises on representation yet; with an optimizing tier this becomes a correctness bug, since in-place widening leaves optimized code trusting a representation that no longer holds. |
| `objects/objects.rs` | `ToPrimitive` does not call `valueOf` or `toString`, so an object converts to `NaN` and to `"[object Object]"`. No `Symbol`, no `BigInt`. |
| `objects/property_cell.rs` | `dependent_code` is present for layout fidelity but always empty. V8 keeps the cells in a `GlobalDictionary` on `JSGlobalObject`, so declaring a global does not reshape it; `vx` stores each cell as an ordinary fast property, which does reshape the global object — and costs nothing precisely because the caches hold cells rather than maps. |
| `objects/scope_info.rs` | V8's `ScopeInfo` carries the scope type, language mode, function kind, parameter count, flags for every special binding and a positions table. `vx` keeps the context-slot names and the count. |
| `objects/shared_function_info.rs` | V8 splits `function_data` into trusted and untrusted halves for the sandbox and overloads it to hold a `Code` object, a lazy-compilation stub or a builtin id. `vx` compiles eagerly and has one tier, so it holds a `BytecodeArray` or `undefined`. No `Script`. |
| `objects/string.rs` | Sequential representations only. V8 also has `ConsString`, `SlicedString`, `ThinString` and external strings; their instance-type bits are present and the read paths check `IsIndirectString`, so adding them is implementing representations rather than reshaping the object model. |
| `objects/string_table.rs` | V8's table is an open-addressed hash table in the heap, keyed by `StringTableKey` so a lookup needs no allocation. `vx` uses a Rust `HashMap` beside the heap: one object per name is preserved, but the table is not a heap object and is not collected. Internalized strings go in read-only space and are copied, where V8 uses old space with a write barrier and `ThinString` forwarding. |
| `parsing/parser.rs` | No preparser: V8 scans function bodies once with a `PreParser` that records only scope information and parses them fully on first call, while `vx` parses everything eagerly. `let`/`const` get function scope, so block scoping and the temporal dead zone are not enforced. No classes, generators, async functions, destructuring, spread, template literals, `for...in`/`for...of`, `switch`, `try`, `with`, labels, `new`, getters or setters. |
| `parsing/scanner.rs` | Scans Rust `char`, not UTF-16 code units; the two differ only for lone surrogates, which cannot appear in a `&str`. String literals are still converted to UTF-16 when they become heap strings, so `length` stays correct. No template literals, no regular expression literals, no `\u` escapes in identifiers, no numeric separators, no BigInt. Identifier start/continue use Rust's Unicode alphabetic classes rather than the exact `ID_Start`/`ID_Continue` tables. |
| `objects/elements_kind.rs` | The six fast kinds keep V8's numbering, since the predicates are range checks on it. No nonextensible, sealed, frozen, typed-array, arguments, string-wrapper, shared-array or wasm kinds, so `DICTIONARY_ELEMENTS` and `NO_ELEMENTS` are renumbered to 6 and 7. `DICTIONARY_ELEMENTS` is declared but not produced. |

### What an inline cache buys, measured

`tools/../cargo run -- ic f.js` reports it. For a thousand reads of one
shape:

```
  LoadProperty slot 0: Uninitialized -> Monomorphic (map 0xd80, 1 tracked)
  LoadProperty slot 0: Monomorphic -> Polymorphic (map 0xdf0, 2 tracked)
  LoadProperty slot 0: Polymorphic -> Megamorphic (map 0xf40, 0 tracked)

  property   999 site hits      0 shared hits    22 misses
  global    6994 site hits      0 shared hits    17 misses
```

Three tiers, counted apart, because each says something different:

* a **site hit** means that site had seen this shape before;
* a **shared hit** means the site had given up, but some *other* site had
  already resolved the same `(map, name)` pair -- that is the stub cache;
* a **miss** means nothing knew.

Summing them would hide which tier is working. On a site deliberately fed
six shapes and then hammered, the split is the whole story:

```
  property     0 site hits    200 shared hits   724 misses
  stub cache: 200 primary hits, 0 secondary hits, 1 misses, 6 updates
```

Zero site hits, because the site is megamorphic and holds nothing. Two
hundred shared hits, because the pairs were all in the table. The 724 misses
are the property *stores* that build the objects, which are uncacheable by
construction.

### Contexts and closures

A `Context` is a `FixedArray` with a fixed header -- `[scope_info, previous,
var0, ...]`, `MIN_CONTEXT_SLOTS = 2` -- and the `previous` links are the
scope chain at runtime. `vx` has function contexts only: no `with`, `catch`,
block, module or native contexts, and no `ScriptContext` table.

Which variables go there is decided while parsing, in `src/ast/scopes.rs`.
Closing a scope hands every name it could not resolve to the enclosing
scope, and a name that resolves **across a function boundary** sets
`force_context_allocation` on the variable it found. `AllocateVariables`
then gives each variable a context slot, a register, or neither (a global, at
script level). Three consequences worth stating, because each one is a place
a plausible port goes wrong:

* **One context per invocation, not per closure.** Two closures made in one
  call share the context, so one sees the other's writes; two calls to the
  same function do not. This is what `tests/closures.rs` asserts on, since
  both a shared slot and a per-closure copy would compute the same answers
  for a counter that is only ever read once.
* **A scope that captures nothing allocates no context**, and so is skipped
  when counting depth. A closure created in it simply keeps using the
  context it was created in. Behaviour cannot show this -- a context per
  call would give the same answers -- so it is asserted on the bytecode.
* **A captured parameter keeps its frame slot and gains a context slot.**
  Parameters arrive in the frame, so the prologue copies each captured one
  into the context with `Ldar a_i; StaCurrentContextSlot`.

The generator resolves a reference to a `(depth, slot)` pair, which is what
makes the runtime load a pointer walk of known length rather than a search:
`LdaCurrentContextSlot` at depth 0, `LdaContextSlot` beyond it. A closure
captures by storing the context it was created in -- `JSFunction::context`,
the one field that makes a closure close over anything at all.

The interpreter frame grew the fixed slots this needs. V8's
`InterpreterFrameConstants` puts the context and the closure at negative
register indices below the parameters, so `Register::current_context()` and
`Register::function_closure()` are ordinary registers as far as the
bytecode is concerned, and `register_slot` maps the three index ranges --
registers, parameters, fixed slots -- onto one `Vec<Tagged>`.

### What the offsets do and do not catch

Addressing the heap by offset makes a *stale or out-of-arena* address a panic.
It does **not** make an out-of-*object* write a panic: writing past the end of
one object lands inside the next one, which is still inside the arena.

That bit during this port. `JSFunction`'s map was created with a plain
`JSObject`'s instance size, so storing its `SharedFunctionInfo` wrote four
words past the end of the object. Nothing complained; the symptom was a
builtin whose `is_native` flag read back false. The fix was to size the map
correctly, and the lesson is that bounds checks are not type checks.

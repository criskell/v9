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
4. **A deviation is documented where it lives,** in the module's docs, and
   listed here.

## Configuration

`vx` targets one V8 build configuration and only that one:

| Flag | Setting |
| --- | --- |
| `V8_COMPRESS_POINTERS` | off — a tagged word is 8 bytes, no cage |
| `V8_EXTERNAL_CODE_SPACE` | off |
| `V8_ENABLE_SANDBOX` | off |
| `V8_MAP_PACKING` | off |
| Smi | `SmiTagging<8>`: full `i32`, in the upper half of the word |

## Status

| V8 | `vx` | State |
| --- | --- | --- |
| `include/v8-internal.h`, `src/common/globals.h` | `src/common/globals.rs` | tagging, alignment, sizes |
| `src/objects/tagged-impl.h`, `tagged.h`, `heap-object.h` | `src/objects/tagged.rs` | `Tagged`, `HeapObject`, strong/weak predicates |
| `src/objects/smi.h` | `src/objects/smi.rs` | done |
| `src/objects/map-word.h`, `map-word-inl.h` | `src/objects/map_word.rs` | done |
| `src/objects/instance-type.h` | `src/objects/instance_type.rs` | string bits exact; non-string types renumbered |
| `src/objects/property-details.h` | `src/objects/property_details.rs` | bit layout exact; `Representation` tracked, not exploited |
| `src/objects/fixed-array.h` | `src/objects/fixed_array.rs` | `FixedArray`, `PropertyArray`, `WeakFixedArray` |
| `src/objects/descriptor-array.h` | `src/objects/descriptor_array.rs` | linear search, no enum cache |
| `src/objects/map.h`, `map.tq`, `map-inl.h` | `src/objects/map.rs` | layout + `bit_field`/`bit_field3` |
| `src/objects/transitions.h`, `transitions.cc` | `src/objects/transitions.rs` | `Uninitialized` / `WeakRef` / `FullTransitionArray` |
| `src/objects/name.h`, `string.h`, `string-hasher-inl.h` | `src/objects/string.rs` | sequential one/two-byte only |
| `src/objects/heap-number.h` | `src/objects/heap_number.rs` | done |
| `src/objects/oddball.h` | `src/objects/oddball.rs` | done |
| `src/objects/js-objects.h` | `src/objects/js_objects.rs` | `JSObject`; `JSFunction` is a placeholder |
| `src/objects/lookup.cc`, transition half of `map.cc` | `src/objects/lookup.rs` | data properties on the prototype chain |
| `src/roots/roots.h` | `src/roots/roots.rs` | the roots `vx` needs |
| `src/execution/isolate.h` | `src/execution/isolate.rs` | heap + roots + string table + bootstrap |
| `src/heap/spaces.h`, `new-spaces.h`, `paged-spaces.h` | `src/heap/spaces.rs` | bump allocation, no pages |
| `src/heap/heap.h`, `heap.cc` | `src/heap/heap.rs` | arena, spaces, raw access |
| `src/heap/factory.h`, `factory.cc` | `src/heap/factory.rs` | the objects `vx` can build |
| `src/heap/heap-visitor.h` | `src/heap/visitor.rs` | switches on instance type, not `visitor_id` |
| `src/heap/scavenger.h`, `scavenger.cc` | `src/heap/scavenger.rs` | Cheney copying collector with `age_mark` promotion |

### Not started

| V8 | Would become | Notes |
| --- | --- | --- |
| `src/parsing/token.h`, `scanner.cc` | `src/parsing/` | the next step |
| `src/ast/ast.h`, `scopes.cc` | `src/ast/` | |
| `src/parsing/parser-base.h`, `parser.cc` | `src/parsing/parser.rs` | 11k lines in V8; the single biggest chunk |
| `src/interpreter/bytecodes.h` | `src/interpreter/bytecodes.rs` | the opcode table |
| `src/interpreter/bytecode-array-builder.cc` | `src/interpreter/` | register allocation, jump patching |
| `src/interpreter/bytecode-generator.cc` | `src/interpreter/` | 9k lines: AST → bytecode |
| `src/interpreter/interpreter-generator.cc` | `src/interpreter/interpreter.rs` | CSA in V8; becomes a Rust dispatch loop |
| `src/ic/*`, `src/objects/feedback-vector.h` | `src/ic/` | inline caches — the `Map` + field index are already what a load needs |
| `src/heap/mark-compact.cc` | `src/heap/` | old-generation collection |
| `src/compiler/`, `src/maglev/` | — | optimizing tiers; not planned |

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
* **`JSFunction` is a placeholder.** No `SharedFunctionInfo`, no `Context`.
  Those arrive with the bytecode pipeline.

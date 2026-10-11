# vx

Testing the power of LLMs.

---

A JavaScript engine in Rust, built by porting V8, same module tree, same
method names, same algorithms, written as safe Rust. An experiment.

```
$ cargo run -- run demo.js
factorials: 873
sqrt(total) floor = 29

$ cargo test        # 488 tests
```

No dependencies: `std` only. No `unsafe`.

## It runs JavaScript

```js
function factorial(n) {
  if (n <= 1) return 1;
  return n * factorial(n - 1);
}
var total = 0;
for (var i = 1; i <= 6; i++) {
  total += factorial(i);
}
console.log({ total: total, label: "factorials" }.label + ":", total);
```

The whole V8 pipeline is there: **scanner → parser → AST → Ignition bytecode →
register-machine interpreter**, over a real heap with hidden classes and a
copying garbage collector.

`cargo run -- bytecode f.js` disassembles, the way `--print-bytecode` does:

```
=== [add] parameters: 2, registers: 0, feedback slots: 0 ===
    0 : 02 08                Ldar a1
    2 : 1a 07 00             Add a0, 0
    5 : 5b                   Return
```

`cargo run -- ic f.js` reports what the inline caches learned, the way
`--trace-ic` does:

```
  LoadProperty slot 0: Uninitialized -> Monomorphic  (map 0xd48, 1 tracked)
  LoadProperty slot 0: Monomorphic   -> Polymorphic  (map 0xdb8, 2 tracked)
  LoadProperty slot 0: Polymorphic   -> Megamorphic  (map 0xf08, 0 tracked)

  property   999 site hits     0 shared hits    22 misses
  global    6994 site hits     0 shared hits    17 misses
```

A *site hit* means that site had seen the shape; a *shared hit* means it had
given up but another site had already resolved the same `(map, name)` pair,
from the stub cache.

Commands: `run`, `bytecode`, `ast`, `ic`, `maps`, `gc`, `heap`, `roots`.

## What works

* **Pointer tagging** exactly as V8 encodes it — Smis immediate, strong and
  weak references distinguished, forwarding addresses in the map word.
* **Hidden classes.** Adding a property moves an object to a new shape along
  a cached transition, so a thousand objects built the same way share one
  `Map` and one field layout.
* **A copying garbage collector.** Cheney over two semi-spaces, with
  promotion out of the nursery on second survival via the heap's `age_mark`.
* **Ignition.** The bytecode is V8's, including the operand-scaling prefixes
  and the constant-pool reservation scheme that makes forward jumps
  encodable.
* **Inline caches.** A named property access records the shape it saw and
  the field it found, so the next execution is one map comparison and one
  indexed read. Sites climb V8's lattice -- monomorphic, polymorphic up to
  four shapes, then megamorphic -- and the handler is a Smi, exactly as in
  V8.
* **Property cells.** A global's value lives in a cell, and a global access
  caches the cell rather than the global object's map -- so declaring another
  global cannot invalidate it. A cell also records whether the global has
  been assigned once (`Constant`), several values of one type
  (`ConstantType`) or anything else (`Mutable`), which is what would let an
  optimizing compiler inline it.
* **A stub cache.** The isolate-wide `(map, name) -> handler` table a
  megamorphic site consults before giving up: two tables, no probe sequence,
  and an evicted entry demoted to the secondary table rather than dropped.
  Going megamorphic promotes everything the site had learned into it, so the
  knowledge survives the site forgetting it.
* **Closures.** A variable an inner function references is moved out of the
  frame and into a `Context`, so it outlives the call that declared it. One
  context per *invocation*: two closures made in the same call share the
  variable and see each other's writes, two calls do not. A scope that
  captures nothing allocates no context at all, and a reference is compiled
  to a `(depth, slot)` pair -- a pointer walk of known length, never a
  search.
* **The language:** numbers and strings, `var`/`let`/`const`, operators with
  JavaScript's coercions, `if`/`while`/`do`/`for`, `break`/`continue`,
  functions, recursion, closures, objects, property access, method calls, `typeof`.

## What does not

No classes, no `try`/`catch`, no generators or `async`, no real arrays, no
`for...in`. Contexts are function contexts only: no `with`, no `catch`
scope, no per-iteration `let` binding in a loop. Inline caches cover named property access and globals; prototype loads,
transitioning stores and `globalThis.x` still do the full lookup.

[`PORTING.md`](PORTING.md) is the ledger — what is ported, at what fidelity,
and every deviation. `tools/check-layout.sh` checks that every source file
still maps to a V8 file at the same path.

## Why offsets instead of pointers

V8 is built on dereferencing tagged pointers. `vx` keeps the tagging and the
layouts but makes an address an offset into a heap arena, so every field
access goes through the `Heap` that owns it and is bounds checked. A stale
address panics with a message instead of corrupting memory.

What that does *not* buy is type safety — see the end of `PORTING.md` for a
bug this port hit where a too-small instance size wrote past an object and
nothing complained.

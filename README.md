# vx

A JavaScript engine in Rust, built by porting V8 — structure, object model and
algorithms from V8, written as safe Rust. An experiment.

```
cargo test        # 162 tests
cargo run -- demo
```

No dependencies: `std` only.

## What works

The heap and the object model — which is to say, the part of V8 that makes
JavaScript fast:

* **Pointer tagging** exactly as V8 encodes it: Smis immediate, strong and
  weak references distinguished, forwarding addresses in the map word.
* **Hidden classes.** `Map`, `DescriptorArray`, `TransitionArray`. Adding a
  property moves an object to a new shape along a cached transition, so a
  thousand objects built the same way share one `Map` and one field layout.
* **Property storage** split between inline object slots and a
  `PropertyArray`, decided by the map.
* **A copying garbage collector.** Cheney over two semi-spaces, forwarding
  addresses, promotion out of the nursery on second survival via the heap's
  `age_mark`.
* **Strings** one and two byte, internalized through a string table so
  property names compare by address.

## What does not work yet

There is no parser, no bytecode and no interpreter, so `vx` cannot run
JavaScript source. The engine is driven through its API. See
[`PORTING.md`](PORTING.md) for the ledger and the roadmap.

## Why offsets instead of pointers

V8 is built on dereferencing tagged pointers. `vx` keeps the tagging and the
layouts but makes an address an offset into a heap arena, so every field
access goes through the `Heap` that owns it and is bounds checked. A stale
address panics with a message instead of corrupting memory. Nothing in `vx`
needs `unsafe`.

The trade is documented in [`PORTING.md`](PORTING.md), along with every other
place `vx` departs from V8.

//! `vx` -- a JavaScript engine in Rust, built by porting V8.
//!
//! The module tree mirrors V8's `src/` directory, and each module names the
//! file(s) it was ported from. Where `vx` deviates, the deviation is called
//! out in the module docs; `PORTING.md` tracks what is ported, what is
//! stubbed, and what is deliberately different.
//!
//! The one global deviation: V8 dereferences tagged pointers directly, while
//! `vx` keeps the same tagging and the same object layouts but makes an
//! address an offset into a heap arena, reachable only through the `Heap`
//! that owns it. Nothing in `vx` needs `unsafe`.

pub mod ast;
pub mod common;
pub mod execution;
pub mod heap;
pub mod interpreter;
pub mod numbers;
pub mod objects;
pub mod parsing;
pub mod roots;
pub mod strings;

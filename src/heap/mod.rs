//! Mirrors `src/heap/` in V8: the arena, the spaces carved out of it, the
//! factory that creates objects in them, and the garbage collector.

pub mod factory;
pub mod heap;
pub mod scavenger;
pub mod spaces;
pub mod visitor;

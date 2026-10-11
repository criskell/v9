//! Mirrors `src/heap/` in V8: the arena, the spaces carved out of it, the
//! factory that creates objects in them, and the garbage collector.

pub mod factory;
pub mod heap;
pub mod heap_visitor;
pub mod new_spaces;
pub mod paged_spaces;
pub mod read_only_spaces;
pub mod scavenger;
pub mod setup_heap_internal;
pub mod spaces;

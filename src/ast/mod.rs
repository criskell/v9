//! Mirrors `src/ast/` in V8: the syntax tree the parser builds, the scope
//! analysis that decides where each variable lives, and the printer.

pub mod ast;
pub mod prettyprinter;
pub mod scopes;

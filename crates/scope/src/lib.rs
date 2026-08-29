//! Domain-object scope resolution shared by the analyzer and renderer:
//! qualifying a definition's cross-reference key and resolving a reference
//! against the same enclosing scope, so both phases always agree.
//!
//! `scope` holds [`Scope`], the bundle every traversal threads; `python`,
//! `c` and `program` are the one-per-domain scopes inside it, each with
//! its own qualification rules — see [`Scope`]'s doc comment for why they
//! stay separate rather than being unified.

mod c;
mod program;
mod python;
mod scope;

pub use c::{CQualification, CScope};
pub use program::ProgramScope;
pub use python::{PythonScope, Qualification};
pub use scope::Scope;

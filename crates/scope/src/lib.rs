//! Domain-object scope shared by the analyzer and renderer: qualifying a
//! definition's cross-reference key and resolving a reference against the
//! same enclosing scope, so both phases always agree.
//!
//! `scope` holds [`Scope`], the bundle of the one-per-domain scopes `python`,
//! `c` and `program`, each with its own qualification rules — see [`Scope`]'s
//! doc comment for why they stay separate rather than being unified.
//! `definition` holds the rules for how a definition moves the scope, and
//! `document_scopes` applies them once over a whole document, so no phase
//! keeps a scope of its own while it walks one.

mod c;
mod definition;
mod document_scopes;
mod program;
mod python;
mod scope;

pub use c::{CQualification, CScope};
pub use definition::DefinitionNames;
pub use document_scopes::DocumentScopes;
pub use program::ProgramScope;
pub use python::{PythonScope, Qualification};
pub use scope::Scope;

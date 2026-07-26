//! Domain-object scope resolution shared by the analyzer and renderer:
//! qualifying a definition's cross-reference key and resolving a reference
//! against the same enclosing scope, so both phases always agree.

mod python_scope;

pub use python_scope::{PythonScope, Qualification};

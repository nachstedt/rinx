//! Domain-object scope resolution shared by the analyzer and renderer:
//! qualifying a definition's cross-reference key and resolving a reference
//! against the same enclosing scope, so both phases always agree.

mod c_scope;
mod python_scope;
mod scope;

pub use c_scope::{CQualification, CScope};
pub use python_scope::{PythonScope, Qualification};
pub use scope::Scope;

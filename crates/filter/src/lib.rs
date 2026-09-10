//! The filter expression language a listing directive selects entities with.
//!
//! # Why a language at all
//!
//! Sphinx-needs evaluates `:filter:` strings as Python expressions. This build
//! has no interpreter and will not embed one (ADR-009 decision 9), but every
//! real `.. needtable::` in the wild carries a filter, so a listing directive
//! without one would render an unfiltered table and call it support.
//!
//! So this crate implements the subset that real filters actually use, with
//! Python's own spelling — `and`, `or`, `not`, `in`, `is not None` — and
//! diagnoses everything outside it *by name*, with a position. A filter that
//! this language cannot evaluate is a reported error, never a silently empty
//! table.
//!
//! # Why it is its own crate
//!
//! Two phases need it and neither may depend on the other: the **parser**
//! parses a filter (so the syntax error lands on the option line the author
//! wrote) and the **renderer** evaluates one. It therefore depends on nothing
//! of ours at all — not on `rusty_sphinx_ast`, and not on the index whose
//! records it filters. What it knows about the thing being filtered arrives
//! through [`FilterSubject`], the same injected-trait seam
//! `rusty_sphinx_parser`'s file loader uses.

mod error;
mod eval;
mod expr;
mod field_name;
mod parse;
mod token;
mod value;

pub use error::{FilterError, FilterErrorKind};
pub use eval::FilterSubject;
pub use expr::{CompareOp, Expr, Operand};
pub use field_name::{FieldName, FieldNameError};
pub use parse::parse_filter;
pub use value::{FieldValue, Literal};

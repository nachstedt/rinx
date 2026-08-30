//! Directive parsing: everything introduced by a `.. name::` marker.
//!
//! [`dispatch`] recognizes the marker and routes by name; [`body`] collects
//! the lines belonging to it. Each directive family then owns its own
//! module — [`domains`] for the domain objects (`.. py:function::`,
//! `.. c:struct::`, `.. option::`), [`data_table`] for the two table
//! directives that share an AST node, and one apiece for the rest.

mod admonitions;
// `pub(crate)` rather than private: `blocks::comment` reaches
// `collect_directive_body`, since an RST comment is the same `.. `
// explicit-markup construct and its body ends by exactly the same rule.
pub(crate) mod body;
mod data_table;
mod dispatch;
mod doctest;
mod domains;
mod glossary;
mod index_directive;
mod scope;
mod toctree;

pub(super) use dispatch::try_parse_directive;

//! Directive parsing: everything introduced by a `.. name::` marker.
//!
//! [`dispatch`] recognizes the marker and routes by name; [`body`] collects
//! the lines belonging to it. Each directive family then owns its own
//! module — [`domains`] for the domain objects (`.. py:function::`,
//! `.. c:struct::`, `.. option::`), [`data_table`] for the two table
//! directives that share an AST node, and one apiece for the rest.
//! [`table_options`] and [`table_widths`] sit flat here rather than under
//! [`data_table`], despite [`data_table`] being their only caller today:
//! their five options (`widths`, `width`, `align`, `class`, `name`) are
//! shared by every option-bearing table directive, not just the two that
//! spell their rows out as data, and the upcoming `.. table::` directive
//! (which produces no `Directive::DataTable` at all) will need them too.

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
mod table_options;
mod table_widths;
mod toctree;

pub(super) use dispatch::try_parse_directive;

//! Directive parsing: everything introduced by a `.. name::` marker.
//!
//! [`dispatch`] recognizes the marker and routes by name; [`body`] collects
//! the lines belonging to it. Each directive family then owns its own
//! module — [`domains`] for the domain objects (`.. py:function::`,
//! `.. c:struct::`, `.. option::`), [`data_table`] for the two table
//! directives that share an AST node, [`table`] for `.. table::` (which
//! wraps a grid/simple table rather than being one itself), and one apiece
//! for the rest. [`table_options`] and [`table_widths`] sit flat here rather
//! than under [`data_table`]: their five options (`widths`, `width`,
//! `align`, `class`, `name`) are shared by every option-bearing table
//! directive, not just the two that spell their rows out as data — [`table`]
//! needs them too, and produces no `Directive::DataTable` at all.
//!
//! [`options`] sits one level more general still: scanning a body's leading
//! `:name: value` run and diagnosing unclaimed options is common to *every*
//! directive, tables included, so it is named for what it does rather than
//! for the first construct that needed it ([`math`] reads its
//! `:label:`/`:nowrap:`/`:class:` off the same scan).

mod admonitions;
// `pub(crate)` rather than private: `blocks::comment` reaches
// `collect_directive_body`, since an RST comment is the same `.. `
// explicit-markup construct and its body ends by exactly the same rule.
pub(crate) mod body;
mod code_block;
mod contents;
mod data_table;
mod dispatch;
mod doctest;
mod domains;
mod glossary;
mod image;
mod include;
mod index_directive;
mod math;
mod options;
mod scope;
mod substitution;
mod table;
mod table_options;
mod table_widths;
mod toctree;

pub(super) use dispatch::try_parse_directive;

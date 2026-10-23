//! Directive parsing: everything introduced by a `.. name::` marker.
//!
//! [`dispatch`] recognizes the marker and routes by name; [`body`] collects
//! the lines belonging to it. [`spec`] is the table of every directive this
//! build implements by a fixed name — each one's spec lives in the module that
//! parses it — and what [`dispatch`] matches a name's kind against, so a
//! directive cannot be named without being dispatched, nor dispatched without
//! being named; [`input`] is what every one of them is interpreted from, so
//! one signature serves all. Each directive family then owns its own
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
//! [`filter_option`] is shared in the same spirit but for a narrower reason:
//! the two directives that ask the entity graph a question — [`entity_table`]
//! and [`entity_flow`] — must select the same entities from the same
//! expression and refuse the same ones at the same column, so there is one
//! reader for both and only the diagnostic code differs.
//!
//! [`include`], [`if_builder`] and [`needimport`] are the three *splicing*
//! directives: alone among the body-bearing ones they contribute several nodes
//! to the enclosing block instead of wrapping them in one, so a heading,
//! target or `.. toctree::` written inside one belongs to the document itself
//! — and an entity [`needimport`] reads out of a `needs.json` is an entity of
//! the document, not something nested inside a directive. Every directive is
//! interpreted into a `Vec<Node>`, so they need no dispatch of their own —
//! only the [`input`]'s `splice_ctx`, which keeps the titles allowed where the
//! directive stands.
//!
//! [`error_node`] is the other shared-on-purpose module: it owns the two nodes
//! a directive degrades to when it cannot become itself — an unrecognized name
//! and a recognized name with content this build cannot accept — so that every
//! such failure reports its diagnostic and keeps its source in one place.
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
mod button_link;
mod chart_options;
mod classes;
mod code_block;
mod contents;
mod data_table;
mod default_role;
mod dispatch;
mod doctest;
mod domains;
mod dropdown;
mod entity;
mod entity_bar;
mod entity_fields;
mod entity_flow;
mod entity_pie;
mod entity_section;
mod entity_sequence;
mod entity_spec;
mod entity_table;
mod entity_update;
mod error_node;
mod filter_option;
mod glossary;
mod grid;
mod if_builder;
mod image;
mod include;
pub(crate) mod index_directive;
mod invocation;
mod math;
mod needimport;
mod needservice;
mod option_value;
mod options;
mod reader;
mod role;
mod scope;
mod sectnum;
mod spec;
mod substitution;
mod table;
mod table_options;
mod table_widths;
mod toctree;
mod uml;

pub(super) use dispatch::try_parse_directive;
pub use entity_spec::EntitySpec;
pub use spec::{
    Arguments, DirectiveSpec, OptionSet, OptionSpec, OptionSupport, OptionValue, directive_specs,
    domain_directive_specs, is_builtin_directive_name,
};

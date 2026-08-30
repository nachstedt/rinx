//! Body rendering: the node dispatcher that turns a document's
//! [`rusty_sphinx_ast::Node`]s into HTML, and one module per block construct.
//!
//! [`dispatch`] owns the traversal and the entry points the crate root calls;
//! every other module here is something it delegates to — [`admonitions`],
//! [`doctest`], [`glossary`], [`list_table`], [`scope_directives`],
//! [`tables`], the toctree sidebar markup in [`nav`], and the domain-object
//! directives in [`domain_object`]. Inline markup inside these constructs goes
//! to [`crate::inline`].

mod admonitions;
mod dispatch;
mod doctest;
mod domain_object;
mod glossary;
mod list_table;
mod nav;
mod scope_directives;
mod tables;

pub(crate) use dispatch::{collect_anonymous_targets, render_nodes};

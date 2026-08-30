//! Body rendering: the node dispatcher that turns a document's
//! [`rusty_sphinx_ast::Node`]s into HTML, and one module per block construct.
//!
//! [`dispatch`] owns the traversal and the entry points the crate root calls;
//! every other module here is something it delegates to — [`admonitions`],
//! [`data_table`], [`table_directive`] (`.. table::`, sharing
//! [`table_shell`]'s presentation-shell helpers with `data_table`),
//! [`doctest`], [`glossary`], [`scope_directives`], [`tables`], the toctree
//! sidebar markup in [`nav`], and the domain-object directives in
//! [`domain_object`]. Inline markup inside these constructs goes to
//! [`crate::inline`]. [`table_test_support`] holds fixtures `data_table`'s
//! and `table_directive`'s tests share.

mod admonitions;
mod data_table;
mod dispatch;
mod doctest;
mod domain_object;
mod glossary;
mod nav;
mod scope_directives;
mod table_directive;
mod table_shell;
#[cfg(test)]
mod table_test_support;
mod tables;

pub(crate) use dispatch::{collect_anonymous_targets, render_nodes};

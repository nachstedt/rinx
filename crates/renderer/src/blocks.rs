//! Body rendering: the node dispatcher that turns a document's
//! [`rusty_sphinx_ast::Node`]s into HTML, and one module per block construct.
//!
//! [`dispatch`] owns the traversal and the entry points the crate root calls;
//! every other module here is something it delegates to — [`admonitions`],
//! [`block_quote`], [`data_table`], [`table_directive`] (`.. table::`, sharing
//! [`table_shell`]'s presentation-shell helpers with `data_table`),
//! [`doctest`], [`glossary`], [`image`], [`figure`], [`line_block`],
//! [`option_list`], [`scope_directives`], [`tables`], [`contents`] (the
//! local, single-document table of contents `.. contents::` renders — not to
//! be confused with the cross-document toctree sidebar markup in `crate::nav`),
//! and the domain-object directives in [`domain_object`].
//! Inline markup inside these constructs goes to [`crate::inline`].
//! [`table_test_support`] holds fixtures `data_table`'s and
//! `table_directive`'s tests share, and [`asset_href`] the `_images/` path
//! arithmetic every picture on the page shares — an authored image and a
//! compiled `.. plantuml::` diagram alike.

mod admonitions;
mod asset_href;
mod block_quote;
mod code_block;
mod contents;
mod data_table;
mod dispatch;
mod doctest;
mod domain_object;
mod dropdown;
mod entity;
mod entity_table;
mod figure;
mod glossary;
mod image;
mod line_block;
mod math;
mod option_list;
#[cfg(test)]
mod render_test_support;
mod scope_directives;
mod table_directive;
mod table_shell;
#[cfg(test)]
mod table_test_support;
mod tables;
mod uml;

pub(crate) use dispatch::{collect_anonymous_targets, render_nodes};
// Reused by `crate::inline::image` for an `InlineImage` node, whose `<img>`
// element is built exactly the same way as a standalone `.. image::`'s.
pub use entity::EntityTemplates;
pub(crate) use entity::entity_anchor;
pub(crate) use image::render_linked_image;
pub(crate) use math::equation_anchor_id;

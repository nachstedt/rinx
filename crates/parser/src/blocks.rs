//! Block-level parsing: the dispatch chain that turns a document's lines
//! into [`rusty_sphinx_ast::Node`]s, and one module per block construct.
//!
//! [`dispatch`] owns the loop and the `parse` entry points; each construct
//! module is tried in turn and either claims some lines or declines.

mod bullet_list;
mod comment;
mod definition_list;
mod dispatch;
mod docinfo;
mod doctest_block;
mod enumerated_list;
mod index_ids;
mod literal_block;
mod simple_table;
mod table;
mod target;
mod transition;

#[cfg(test)]
mod pipeline_tests;

pub(crate) use dispatch::parse_blocks;
pub use dispatch::{parse, parse_with_ctx, parse_with_domain};

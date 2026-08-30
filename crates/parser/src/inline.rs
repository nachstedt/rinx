//! Inline markup parsing: everything that turns a paragraph's raw text into
//! [`rusty_sphinx_ast::InlineNode`]s.
//!
//! [`text`] drives the scan and owns the entry point; it consults
//! [`regexes`] for what to look for, [`markup`] for emphasis/strong/literal
//! spans, and [`dispatch`] to build a node once something matches — which in
//! turn hands the domain roles to [`roles`]. [`escapes`], [`punctuation`] and
//! [`typography`] are the text-level primitives the rest is phrased in.

mod dispatch;
mod escapes;
mod markup;
mod punctuation;
mod regexes;
mod roles;
mod text;
mod typography;

#[cfg(test)]
mod pipeline_tests;

pub(super) use text::parse_inline_text;

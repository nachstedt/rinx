//! Inline markup parsing: everything that turns a paragraph's raw text into
//! [`rinx_ast::InlineNode`]s.
//!
//! [`text`] drives the scan and owns the entry point; it consults
//! [`regexes`] for what to look for, [`markup`] for emphasis/strong/literal
//! spans, and [`dispatch`] to build a node once something matches — which in
//! turn hands the domain roles to [`roles`]. [`escapes`], [`punctuation`] and
//! [`typography`] are the text-level primitives the rest is phrased in.
//! [`source_map`] is what lets a matched role report where it was written:
//! block-level parsing reflows text before the scan sees it, so the mapping
//! back to the `.rst` has to be recorded while that reflow happens. The two
//! role-name predicates are re-exported for `.. role::`, which must refuse a
//! name this scan could never match, or would match as something else.

mod dispatch;
mod escapes;
mod markup;
mod punctuation;
mod regexes;
mod roles;
mod source_map;
mod text;
mod typography;

#[cfg(test)]
mod code_pipeline_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod registry_pipeline_tests;

pub(crate) use regexes::{is_fixed_role_name, is_writable_role_name};
pub(super) use source_map::SourceMap;
pub(super) use text::parse_inline_text_mapped;

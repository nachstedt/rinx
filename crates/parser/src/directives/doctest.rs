//! Parsing for the `sphinx.ext.doctest` directive family.
//!
//! All five directives share one option sub-syntax, so they share one scanner
//! ([`options::scan_options`]); what differs is which options each accepts,
//! which is stated once in [`kind::DocTestDirectiveKind::accepts`] and
//! enforced there. [`block`] then assembles the [`rusty_sphinx_ast::DocTestBlock`]
//! variant for the directive's kind.
//!
//! # Unrecognized options do not degrade the directive
//!
//! An unknown or misplaced option produces a diagnostic and the block is still
//! built. Falling back to [`rusty_sphinx_ast::Directive::Unknown`] would make
//! "not implemented" indistinguishable from "implemented, with a gap" in the
//! benchmark's unsupported-directive tally, and would drop code the author
//! wrote. Only genuinely unusable input (an empty body) degrades, and then to
//! [`rusty_sphinx_ast::Directive::Malformed`], which says the name was fine.

mod block;
mod kind;
mod options;

pub(super) use block::parse_doctest_directive;
pub(super) use kind::DocTestDirectiveKind;

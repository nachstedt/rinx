//! Code blocks: the directives that present a run of text verbatim, with
//! presentation options around it.
//!
//! Five modules, split by the question each answers about a block:
//!
//! - [`block`] — the two directives that write one inline, `.. code-block::`
//!   and `.. code::`, and the lowering they share.
//! - [`options`] — the option vocabulary all of them draw on, and the
//!   `CodeBlockOptions` a parse accumulates into.
//! - [`dedent`] — `:dedent:`, a source transform applied while parsing.
//! - [`emphasize`] — `:emphasize-lines:`, validated against the block's own
//!   line count.
//! - [`selection`] — which part of a named file `.. include::` and
//!   `.. literalinclude::` splice in.
//! - [`highlight`] — `.. highlight::`, which sets what a block with no
//!   language of its own inherits.
//!
//! [`test_support`] holds the fixtures all five test modules drive the parsers
//! through.

mod block;
mod dedent;
mod diff;
mod emphasize;
mod highlight;
mod literal_include;
mod options;
mod selection;
#[cfg(test)]
mod test_support;

pub(in crate::directives) use block::parse_code_block;
pub(in crate::directives) use highlight::parse_highlight;
pub(in crate::directives) use literal_include::parse_literal_include;
pub(in crate::directives) use selection::{Selection, check_encoding, expand_tabs};

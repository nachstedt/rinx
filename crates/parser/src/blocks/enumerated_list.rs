//! Enumerated (ordered) list parsing.
//!
//! This is a port of docutils' `states.py` — `Body.parse_enumerator`,
//! `Body.is_enumerated_list_item`, `Body.make_enumerator` and the
//! `EnumeratedList` specialized state — rather than a simplified "line starts
//! with a digit" detector, because the spec's rules are load-bearing:
//!
//! * An enumerator is ambiguous. `v.` is the 22nd *letter*, not roman five; only
//!   a bare `i.`/`I.` seeds a roman list. Once a list's sequence is established,
//!   later items are read in that sequence first, which is what keeps
//!   `A. B. … H. I.` counting letters instead of flipping to roman at `I`.
//! * A line that looks like an enumerator is only an item if the *next* line
//!   agrees — blank, more-indented, or carrying the next enumerator in the
//!   sequence. This is what keeps `A. Einstein said this.` / `He was smart.` a
//!   paragraph. Note it only bites in that exact shape: with a blank line, an
//!   indented continuation, or at end of input, `A. Einstein` really does
//!   become a one-item list, in docutils too. The spec's advice there is to
//!   escape the period.
//! * Any break in format, sequence or ordering starts a *new* list rather than
//!   continuing the current one, so an `EnumeratedList` node never has to
//!   record per-item enumerators.
//!
//! Diagnostics go beyond docutils'. Its two messages are reproduced verbatim
//! (the not-ordinal-1 info and the unexpected-unindent warning), and
//! [`diagnose_unrecognised_list`] adds one docutils lacks: when two adjacent
//! lines both carry an enumerator but the continuation rule rejects them, the
//! entire block silently becomes a paragraph, which is close to impossible to
//! diagnose from the rendered output. That message names the cause. It stays
//! silent across every file of the benchmark corpus (see `docs/benchmark.rst`),
//! so it is precise enough not to need narrowing.
//!
//! One deliberate behavioural difference from [`super::bullet_list`]: when a
//! list is interrupted mid-item by an unindented line, docutils drops the
//! already-scanned item back into the input rather than keeping it, so
//! `1. a` / `2. b` / `Paragraph.` yields a *one*-item list followed by the
//! paragraph `2. b\nParagraph.`. Bullet lists keep both items. That asymmetry
//! is docutils', and [`try_parse_enumerated_list`] reproduces it by testing
//! each item before consuming it.

mod diagnostics;
mod format;
mod list;

#[cfg(test)]
mod pipeline_tests;

pub(super) use list::try_parse_enumerated_list;

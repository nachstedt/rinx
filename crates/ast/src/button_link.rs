//! The vocabulary `.. button-link::` is written in.
//!
//! Three modules, split by the question each answers about a button:
//!
//! - [`block`] — the node itself: its target, its inline-markup label and its
//!   nine options.
//! - [`flag`] — the four of those options that carry no value, as one set
//!   rather than four fields.
//! - [`target`] — *where* the button points, which is the one thing its
//!   sibling `.. button-ref::` will spell differently (see that module).
//! - [`text_align`] — the four alignments `:align:` places the button with.
//!
//! `:color:` is read by [`crate::SemanticColor`], which `.. dropdown::` writes
//! its own `:color:` in too: the eleven names are sphinx-design's shared
//! `SEMANTIC_COLORS`, not either directive's own vocabulary.
//!
//! Like [`crate::Dropdown`] and [`crate::Grid`], the construct these describe
//! is not docutils' or Sphinx's: `.. button-link::` comes from sphinx-design,
//! and `badges_buttons.py`'s `_ButtonDirective`/`ButtonLinkDirective` are the
//! authority for every name, value and rendered class here.

mod block;
mod flag;
mod target;
mod text_align;

pub use block::ButtonLink;
pub use flag::ButtonFlag;
pub use target::ButtonTarget;
pub use text_align::TextAlign;

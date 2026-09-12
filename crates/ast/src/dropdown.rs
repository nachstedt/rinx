//! The vocabulary `.. dropdown::` is written in.
//!
//! Six modules, split by the question each answers about a collapsible block:
//!
//! - [`block`] — the node itself: its inline-markup title, its ten options
//!   and its body.
//! - [`color`] — the eleven semantic colours `:color:` paints the summary
//!   with.
//! - [`chevron`] — which state marker `:chevron:` draws, and the octicon that
//!   marker is.
//! - [`animation`] — the two ways `:animate:` reveals the body.
//!
//! `:margin:` is read by the shared [`crate::Spacing`], which `.. grid::`
//! writes its `:margin:` and `:padding:` in too.
//! - [`octicon`] — the *name* `:icon:` carries, deliberately unvalidated
//!   against any icon set (see that module for why).
//!
//! Unlike everything else in this crate, the construct these describe is not
//! docutils' or Sphinx's: `.. dropdown::` comes from sphinx-design, and its
//! `DropdownDirective.option_spec` is the authority for every name and value
//! here.

mod animation;
mod block;
mod chevron;
mod color;
mod octicon;

pub use animation::Animation;
pub use block::Dropdown;
pub use chevron::Chevron;
pub use color::SemanticColor;
pub use octicon::{InvalidOcticonName, OcticonName};

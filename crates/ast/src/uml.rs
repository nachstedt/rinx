//! The vocabulary every `PlantUML` diagram directive is written in.
//!
//! Two modules:
//!
//! - [`block`] — the node itself: the template it draws from, and the options
//!   deciding how the finished picture sits on the page.
//! - [`source`] — which of the six spellings was written, and the two
//!   questions that separate the three constructs behind them.
//!
//! One node serves all three because they differ only in what fills the
//! template, never in what happens afterwards: every one of them ends as
//! `PlantUML` text, hashed into an `_images/<hash>.svg` the renderer links to.
//! Keeping them apart would mean two hash populations, and the set of diagrams
//! that gets *compiled* would be free to drift from the set that gets
//! *validated* — which is the one thing the worker's `diagrams` command exists
//! to prevent.
//!
//! Unlike most of this crate, two of these constructs are neither docutils' nor
//! Sphinx's: `.. plantuml::`/`.. uml::` come from sphinxcontrib-plantuml and
//! the templated pair from sphinx-needs.

mod block;
mod source;

pub use block::Uml;
pub use source::UmlSource;

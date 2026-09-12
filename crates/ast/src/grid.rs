//! The vocabulary `.. grid::` and `.. grid-item::` are written in.
//!
//! Five modules, split by the question each answers about a responsive row:
//!
//! - [`block`] — the two nodes themselves, their options and their bodies.
//! - [`media_spec`] — the one-or-four *xs sm md lg* value shape every grid
//!   option shares, and the five classes it becomes.
//! - [`column_spec`] — a count of twelve columns, in the two class families a
//!   row and an item render it as.
//! - [`gutter`] — the space between items, the one media option that admits a
//!   zero and refuses `auto`.
//! - [`child_layout`] — how an item stacks and aligns its own content.
//!
//! `:margin:` and `:padding:` are read by the shared [`crate::Spacing`], which
//! `.. dropdown::` writes its `:margin:` in too.
//!
//! Like [`crate::Dropdown`], the construct these describe is not docutils' or
//! Sphinx's: it comes from sphinx-design, and `GridDirective` /
//! `GridItemDirective` are the authority for every name, value and emitted
//! class here.

mod block;
mod child_layout;
mod column_spec;
mod gutter;
mod media_spec;

pub use block::{Grid, GridItem};
pub use child_layout::{ChildAlign, ChildDirection};
pub use column_spec::{ColumnPrefix, ColumnSpec};
pub use gutter::Gutter;
pub use media_spec::{InvalidMediaSpec, MediaDomain, MediaSpec, MediaValue};

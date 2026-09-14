//! The vocabulary a pie chart over the entity graph is written in.
//!
//! Three modules:
//!
//! - [`pie`] — the node itself: what it counts and how the chart looks.
//! - [`slice`] — one wedge, which is either a filter to count or a number
//!   written outright.
//! - [`source`] — which of the directive's two names was written.
//!
//! Like [`crate::entity_table`] and [`crate::entity_flow`], the node carries a
//! *question* and nothing an author wrote: the entities a slice counts are
//! declared in documents the one holding the directive has never heard of, so
//! only the project index can answer it.
//!
//! Unlike a flowchart, though, the answer never reaches `PlantUML`. A pie is
//! drawn as SVG while the page is rendered, so it needs no `.puml` file, no
//! compile action, and no `diagrams = True` on its library — see
//! `docs/decisions/017-entity-pie.md`.

mod pie;
mod slice;
mod source;

pub use pie::EntityPie;
pub use slice::{PieSlice, SliceSource};
pub use source::EntityPieSource;

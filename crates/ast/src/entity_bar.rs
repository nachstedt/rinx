//! The vocabulary a bar chart over the entity graph is written in.
//!
//! Four modules:
//!
//! - [`bar`] — the node itself: what it counts and how the chart looks.
//! - [`arrangement`] — the layout choices sphinx-needs spells as flags, as
//!   types naming both sides of each.
//! - [`grid`] — the two-dimensional body: one row per series, one column per
//!   category, each cell a [`ChartValue`](crate::ChartValue) — the very type a
//!   pie's wedge holds, since the two charts differ only in presentation.
//! - [`source`] — which of the directive's two names was written.
//!
//! Like [`crate::entity_pie`], the answer never reaches `PlantUML`: a bar
//! chart is drawn as SVG while the page is rendered — see
//! `docs/decisions/021-entity-bar.md`.

mod arrangement;
mod bar;
mod grid;
mod source;

pub use arrangement::{BarArrangement, BarOrientation, BarValueLabels};
pub use bar::EntityBar;
pub use grid::{BarGrid, RaggedGrid};
pub use source::EntityBarSource;

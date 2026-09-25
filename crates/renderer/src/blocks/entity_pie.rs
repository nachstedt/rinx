//! Rendering `.. entity-pie::` / `.. needpie::` — a chart of entity counts.
//!
//! Two modules:
//!
//! - [`series`] — pairing each wedge with its count. The counting itself is
//!   [`super::chart_counts`], shared with `.. entity-bar::`: a bar chart asks
//!   the same question with a second dimension, so it wants counting, not pie
//!   geometry.
//! - [`html`] — drawing the chart into the figure every chart shares
//!   ([`super::chart_figure`]), and the empty-result report.
//!
//! The drawing itself is [`crate::chart`], flat at the crate root, because
//! it is where the charting *backend* is contained — the same place
//! `syntect`, `math-core` and `octicons-pack` are each contained.
//!
//! What separates this from every other picture here: nothing is compiled. The
//! SVG goes straight into the page, so a library holding a chart needs no
//! `diagrams = True` and starts no JVM.

mod html;
mod series;

pub(super) use html::render_entity_pie;

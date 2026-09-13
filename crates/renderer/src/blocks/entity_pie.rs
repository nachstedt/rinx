//! Rendering `.. entity-pie::` / `.. needpie::` — a chart of entity counts.
//!
//! Two modules:
//!
//! - [`series`] — turning the wedges' filters into counts, against the project
//!   index. This is the layer worth generalising, and the only one a later
//!   `.. entity-bar::` would share: a bar chart asks the same question with a
//!   second dimension, so it wants counting, not pie geometry.
//! - [`html`] — the figure the chart sits in, and the empty-result report.
//!
//! The drawing itself is [`crate::pie_chart`], flat at the crate root, because
//! it is where the charting *backend* is contained — the same place
//! `syntect`, `math-core` and `octicons-pack` are each contained.
//!
//! What separates this from every other picture here: nothing is compiled. The
//! SVG goes straight into the page, so a library holding a chart needs no
//! `diagrams = True` and starts no JVM.

mod html;
mod series;

pub(super) use html::render_entity_pie;

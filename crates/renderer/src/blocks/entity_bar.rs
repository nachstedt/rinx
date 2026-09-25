//! Rendering `.. entity-bar::` / `.. needbar::` — a bar chart of entity counts.
//!
//! Two modules, the pie's shape:
//!
//! - [`series`] — turning the grid's cells into counts, through the
//!   [`super::chart_counts`] a pie's wedges go through too, and resolving
//!   every row and column label.
//! - [`html`] — drawing the chart into the figure every chart shares
//!   ([`super::chart_figure`]), and the empty-result report.
//!
//! The drawing itself is [`crate::chart`]'s. Nothing is compiled: the SVG goes
//! straight into the page, so a library holding a bar chart needs no
//! `diagrams = True` and starts no JVM.

mod html;
mod series;

pub(super) use html::render_entity_bar;

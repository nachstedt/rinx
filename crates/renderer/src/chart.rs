//! The one place a chart is drawn.
//!
//! Four modules: [`pie`] and [`bar`] each draw one kind of chart, [`style`]
//! holds what they share — the vendored font, the palette and the colour
//! conversions — so the two cannot drift apart, and [`placed_text`] writes the
//! text `plotters` cannot turn by an arbitrary angle.
//!
//! `plotters` is contained here the way `syntect` is in [`crate::highlight`],
//! `math-core` in [`crate::math`] and `octicons-pack` in [`crate::octicon`],
//! and for the same reason: it is a *backend*, its vocabulary changes when the
//! dependency is upgraded, and it must never reach a `.ast` file. What crosses
//! this boundary is a spec — [`PieSpec`] or [`BarSpec`], holding labels,
//! counts and colours — and an SVG string.
//!
//! The SVG is emitted **inline** into the page rather than written as a file
//! and pointed at. That is what makes a chart cost a project nothing: no
//! `.puml` directory, no compile action, no `PlantUML`, and so no
//! `diagrams = True` on its library. The renderer still performs no I/O —
//! `SVGBackend::with_string` draws into a `String`.
//!
//! Two properties this module has to keep, because a rendered page is a build
//! artefact cached on its inputs:
//!
//! - **Determinism.** The same counts must produce the same bytes, on every
//!   machine. That is why the font is vendored (see `fonts/README.md`) rather
//!   than resolved from the host, and why the palette is a fixed table.
//! - **Totality.** Drawing must not panic on any input a document can hold,
//!   so every failure is a `None` the caller reports.

mod bar;
mod pie;
mod placed_text;
mod style;

pub(crate) use bar::{BarSpec, render_bar_svg};
pub(crate) use pie::{PieSpec, PieWedge, render_pie_svg};

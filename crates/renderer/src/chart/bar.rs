//! Drawing a bar chart.
//!
//! Two modules:
//!
//! - [`layout`] — where bars and ticks go in the chart's own units: the
//!   grouping sphinx-needs' `needbar` uses, and a value axis counted in whole
//!   steps. Pure arithmetic, testable without drawing anything.
//! - [`draw`] — fitting the plot between its labels, and drawing it. What
//!   crosses into it is [`BarSpec`] and what leaves is an SVG string.
//!
//! What it shares with the pie — font, palette, colour conversion — is
//! [`super::style`]'s; what it adds is text at any angle, which is
//! [`super::placed_text`]'s.

mod draw;
mod layout;

pub(crate) use draw::{BarSpec, render_bar_svg};

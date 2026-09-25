//! Drawing a pie chart.
//!
//! What crosses into this module is [`PieSpec`] — labels, counts and colours —
//! and what leaves it is an SVG string; everything shared with the bar chart
//! (the font, the palette, the colour conversion) is [`super::style`]'s.

use plotters::prelude::*;
use rinx_ast::ChartColor;

use super::style::{as_size, ensure_font_registered, series_color, to_rgb};

/// The size the chart is drawn at, in SVG user units.
///
/// A fixed pair rather than a computed one: the SVG scales to whatever
/// `:width:` the page gives it, so this decides the *proportions* and the
/// relative size of the text, not how large the chart appears.
const CHART_WIDTH: u32 = 520;
const CHART_HEIGHT: u32 = 340;

/// Where the pie sits and how large it is, leaving room for the legend.
const PIE_CENTER: (i32, i32) = (170, 170);
const PIE_RADIUS: f64 = 130.0;

/// One wedge, as the drawing side sees it.
pub(crate) struct PieWedge {
    /// What the wedge is called, already resolved from `:labels:`.
    pub label: String,
    /// How many entities it counts.
    pub count: u64,
}

/// Everything a chart is drawn from.
///
/// Borrowed where it can be, because every field already lives on the node or
/// the counted rows the caller holds.
pub(crate) struct PieSpec<'a> {
    /// The wedges, in the order their content lines were written.
    pub wedges: &'a [PieWedge],
    /// `:colors:`, or empty for the built-in palette.
    pub colors: &'a [ChartColor],
    /// `:text_color:` for the percentages drawn on the wedges.
    pub text_color: Option<ChartColor>,
    /// `:legend:` — draw a key naming each wedge beside the chart.
    pub legend: bool,
}

/// Draws `spec` as a standalone SVG element.
///
/// `None` when there is nothing to draw — no wedges, or every wedge zero. A
/// pie of nothing has no geometry at all, so the caller reports it rather than
/// emitting an empty picture.
pub(crate) fn render_pie_svg(spec: &PieSpec<'_>) -> Option<String> {
    let total: u64 = spec.wedges.iter().map(|wedge| wedge.count).sum();
    if total == 0 {
        return None;
    }

    ensure_font_registered();

    let mut svg = String::new();
    {
        let root =
            SVGBackend::with_string(&mut svg, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
        draw_pie(&root, spec).ok()?;
        if spec.legend {
            draw_legend(&root, spec, total).ok()?;
        }
        root.present().ok()?;
    }
    Some(svg)
}

/// Draws the wedges themselves.
fn draw_pie<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    spec: &PieSpec<'_>,
) -> Result<(), ()> {
    let sizes: Vec<f64> = spec
        .wedges
        .iter()
        .map(|wedge| as_size(wedge.count))
        .collect();
    let colors: Vec<RGBColor> = (0..spec.wedges.len())
        .map(|at| series_color(spec.colors, at))
        .collect();
    let labels: Vec<&str> = spec
        .wedges
        .iter()
        .map(|wedge| wedge.label.as_str())
        .collect();

    let mut pie = Pie::new(&PIE_CENTER, &PIE_RADIUS, &sizes, &colors, &labels);
    // Twelve o'clock, so the first wedge starts where a reader looks first.
    pie.start_angle(-90.0);
    pie.label_style(("sans-serif", 13).into_text_style(root));
    let percentage_color = spec.text_color.map_or(WHITE, to_rgb);
    pie.percentages(
        ("sans-serif", 11)
            .into_text_style(root)
            .color(&percentage_color),
    );
    root.draw(&pie).map_err(|_| ())
}

/// Draws the key naming each wedge, down the right-hand side.
///
/// Built from primitives rather than from a `ChartContext`'s own legend: there
/// is no chart context here — a pie is drawn straight onto the area — and the
/// count beside each name is the part a reader actually wants.
fn draw_legend<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    spec: &PieSpec<'_>,
    total: u64,
) -> Result<(), ()> {
    let left = 340;
    let swatch = 12;
    for (at, wedge) in spec.wedges.iter().enumerate() {
        let top = 40 + i32::try_from(at).unwrap_or(0) * 22;
        root.draw(&Rectangle::new(
            [(left, top), (left + swatch, top + swatch)],
            series_color(spec.colors, at).filled(),
        ))
        .map_err(|_| ())?;
        let share = 100.0 * as_size(wedge.count) / as_size(total);
        root.draw(&Text::new(
            format!("{} — {} ({share:.0}%)", wedge.label, wedge.count),
            (left + swatch + 8, top + 1),
            ("sans-serif", 12).into_text_style(root),
        ))
        .map_err(|_| ())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

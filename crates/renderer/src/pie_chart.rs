//! The one place a chart is drawn.
//!
//! `plotters` is contained here the way `syntect` is in [`crate::highlight`],
//! `math-core` in [`crate::math`] and `octicons-pack` in [`crate::octicon`],
//! and for the same reason: it is a *backend*, its vocabulary changes when the
//! dependency is upgraded, and it must never reach a `.ast` file. What crosses
//! this boundary is [`PieSpec`] — labels, counts and colours — and an SVG
//! string.
//!
//! The SVG is emitted **inline** into the page rather than written as a file
//! and pointed at. That is what makes a pie chart cost a project nothing: no
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

use std::sync::OnceLock;

use plotters::prelude::*;
use plotters::style::register_font;
use rusty_sphinx_ast::ChartColor;

/// The vendored font, registered once before anything is drawn.
///
/// `ab_glyph` ships no font of its own, so without this every draw fails with
/// `FontError(FontUnavailable)`.
static FONT: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

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

/// The colours a chart uses when `:colors:` does not say.
///
/// A fixed table, in this order, so an unchanged chart draws identically on
/// the next build. Eight because a pie with more wedges than that is already
/// unreadable, and repeating is better than inventing colours by formula —
/// which would have to be deterministic anyway.
const PALETTE: [(u8, u8, u8); 8] = [
    (0x4c, 0x72, 0xb0),
    (0xdd, 0x85, 0x2c),
    (0x55, 0xa8, 0x68),
    (0xc4, 0x4e, 0x52),
    (0x81, 0x72, 0xb2),
    (0x93, 0x78, 0x60),
    (0xda, 0x8b, 0xc3),
    (0x8c, 0x8c, 0x8c),
];

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

/// Registers the vendored font, once per process.
///
/// `plotters`' registry is global, so this must not run twice — and it is
/// reached from every chart on every page of a render action.
fn ensure_font_registered() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        // A failure here means the vendored file is not a font, which is a
        // build-time fact rather than a document's fault. Drawing then fails
        // per chart and is reported there, so there is nothing to do with the
        // error but decline to register.
        let _ = register_font("sans-serif", FontStyle::Normal, FONT);
    });
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
        .map(|at| color_at(spec, at))
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
            color_at(spec, at).filled(),
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

/// A count as the number the drawing backend measures wedges in.
///
/// Saturating through `u32` rather than casting from `u64` directly: past
/// 2**53 that cast is lossy, and a chart of four billion entities has stopped
/// meaning anything long before then. Converting from `u32` is exact, so every
/// count a project can really hold is drawn at exactly its size.
fn as_size(count: u64) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// The colour of the wedge at `at`.
///
/// `:colors:` first, then the built-in palette, each wrapping round — so a
/// list shorter than the wedges repeats rather than leaving wedges uncoloured,
/// and the two cases need no separate handling.
fn color_at(spec: &PieSpec<'_>, at: usize) -> RGBColor {
    if spec.colors.is_empty() {
        let (red, green, blue) = PALETTE[at % PALETTE.len()];
        return RGBColor(red, green, blue);
    }
    to_rgb(spec.colors[at % spec.colors.len()])
}

/// One of our colours as one of the backend's.
fn to_rgb(color: ChartColor) -> RGBColor {
    let (red, green, blue) = color.rgb();
    RGBColor(red, green, blue)
}

#[cfg(test)]
mod tests;

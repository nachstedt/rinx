//! What every chart drawn here shares: the font, the palette, and the
//! conversions into `plotters`' own vocabulary.
//!
//! Split out of the pie once the bar chart needed the same things, so the two
//! cannot disagree about which colour comes third or how a count is measured.
//! The module doc of [`super`] says why `plotters` is contained in this tree.

use std::sync::OnceLock;

use plotters::prelude::*;
use plotters::style::register_font;
use rinx_ast::ChartColor;

/// The vendored font, registered once before anything is drawn.
///
/// `ab_glyph` ships no font of its own, so without this every draw fails with
/// `FontError(FontUnavailable)`.
static FONT: &[u8] = include_bytes!("../../fonts/DejaVuSans.ttf");

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

/// Registers the vendored font, once per process.
///
/// `plotters`' registry is global, so this must not run twice — and it is
/// reached from every chart on every page of a render action.
pub(super) fn ensure_font_registered() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        // A failure here means the vendored file is not a font, which is a
        // build-time fact rather than a document's fault. Drawing then fails
        // per chart and is reported there, so there is nothing to do with the
        // error but decline to register.
        let _ = register_font("sans-serif", FontStyle::Normal, FONT);
    });
}

/// A count as the number the drawing backend measures wedges and bars in.
///
/// Saturating through `u32` rather than casting from `u64` directly: past
/// 2**53 that cast is lossy, and a chart of four billion entities has stopped
/// meaning anything long before then. Converting from `u32` is exact, so every
/// count a project can really hold is drawn at exactly its size.
pub(super) fn as_size(count: u64) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// The colour of the series or wedge at `at`.
///
/// `:colors:` first, then the built-in palette, each wrapping round — so a
/// list shorter than the wedges or series repeats rather than leaving one
/// uncoloured, and the two cases need no separate handling.
pub(super) fn series_color(colors: &[ChartColor], at: usize) -> RGBColor {
    if colors.is_empty() {
        let (red, green, blue) = PALETTE[at % PALETTE.len()];
        return RGBColor(red, green, blue);
    }
    to_rgb(colors[at % colors.len()])
}

/// The colour of the bar-chart series at `at`: the written `:colors:`, then
/// the built-in palette after them, the whole list wrapping round.
///
/// Deliberately not [`series_color`]'s rule, because the two sphinx-needs
/// directives differ here and each chart follows its own: `needpie` hands
/// matplotlib the written list alone, which cycles it, while `needbar`
/// appends matplotlib's default cycle to it — so a bar chart given two
/// colours for three series draws the third from the palette, where a pie
/// would repeat the first.
pub(super) fn extended_series_color(colors: &[ChartColor], at: usize) -> RGBColor {
    let at = at % (colors.len() + PALETTE.len());
    colors.get(at).map_or_else(
        || {
            let (red, green, blue) = PALETTE[at - colors.len()];
            RGBColor(red, green, blue)
        },
        |written| to_rgb(*written),
    )
}

/// One of our colours as one of the backend's.
pub(super) fn to_rgb(color: ChartColor) -> RGBColor {
    let (red, green, blue) = color.rgb();
    RGBColor(red, green, blue)
}

/// A position or length in the drawing's units, rounded to a whole pixel.
///
/// Through the decimal text of the rounded value rather than an `as` cast, so
/// the conversion is exact where it can be and saturates — rather than
/// wrapping — where it cannot: a NaN or an absurd measurement lands on the
/// canvas edge instead of somewhere arbitrary.
pub(super) fn to_pixel(measure: f64) -> i32 {
    let bounded = measure
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    format!("{bounded:.0}").parse().unwrap_or(0)
}

#[cfg(test)]
mod tests;

//! Text a chart writes itself, rather than through `plotters`.
//!
//! `plotters`' SVG backend can turn text only by a quarter turn —
//! `FontTransform` has no arbitrary angle — while sphinx-needs'
//! `:xlabels_rotation:`, `:ylabels_rotation:` and `:sum_rotation:` take any
//! whole number of degrees, and 45 is the one people write. Snapping 45 to 90
//! would be a wrong picture drawn convincingly, so a bar chart's text is
//! collected as [`PlacedText`] while drawing and spliced into the finished SVG
//! as ordinary `<text>` elements with a `rotate()` transform.
//!
//! The element's shape copies the one `plotters` writes — the same
//! `font-family`, and the same `size / 1.24` it scales a font size by — so text
//! written here and text written by `plotters` in a pie read at the same size.
//!
//! Where text sits relative to its anchor is [`placement`]'s job: a label
//! turned by any angle must still grow *away* from the thing it names, which
//! decides the `text-anchor` and `dominant-baseline` for that angle.

use std::fmt::Write as _;

use plotters::style::RGBColor;

use super::style::to_pixel;

/// The family every chart's text is drawn in: the vendored font is registered
/// under this name (see [`super::style`]).
const FONT_FAMILY: &str = "sans-serif";

/// The factor `plotters` divides a font size by when writing SVG, copied so
/// that the two agree.
const PLOTTERS_FONT_SCALE: f64 = 1.24;

/// Which end of the text sits on the anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Anchor {
    Start,
    Middle,
    End,
}

/// Which line through the text sits on the anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Baseline {
    /// The glyphs sit above the anchor.
    Alphabetic,
    /// The glyphs are centred on the anchor.
    Middle,
    /// The glyphs hang below the anchor.
    Hanging,
}

/// Which side of its anchor a piece of text should grow towards, before any
/// rotation — below an x-axis tick, left of a y-axis tick, and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Below,
    Left,
    Above,
    Right,
    /// Centred on the anchor, whatever the angle — a value in a bar's middle.
    Centre,
}

/// One piece of text, with where and how it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PlacedText {
    pub text: String,
    pub x: i32,
    pub y: i32,
    /// Counter-clockwise degrees, matplotlib's sense — what sphinx-needs'
    /// options mean.
    pub angle: u16,
    pub anchor: Anchor,
    pub baseline: Baseline,
    /// The size in `plotters`' units, so [`super::style`]'s measurement and the
    /// written element agree.
    pub size: u32,
    pub color: RGBColor,
}

impl PlacedText {
    /// Text at `(x, y)` growing `towards` a side once turned by `angle`.
    pub(super) fn new(
        text: String,
        (x, y): (i32, i32),
        angle: u16,
        towards: Side,
        size: u32,
        color: RGBColor,
    ) -> Self {
        let (anchor, baseline) = placement(angle, towards);
        Self {
            text,
            x,
            y,
            angle,
            anchor,
            baseline,
            size,
            color,
        }
    }

    /// This text as one SVG `<text>` element.
    pub(super) fn to_svg(&self) -> String {
        let RGBColor(red, green, blue) = self.color;
        let mut element = format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" dominant-baseline=\"{}\" \
             font-family=\"{FONT_FAMILY}\" font-size=\"{:.2}\" fill=\"#{red:02X}{green:02X}{blue:02X}\"",
            self.x,
            self.y,
            match self.anchor {
                Anchor::Start => "start",
                Anchor::Middle => "middle",
                Anchor::End => "end",
            },
            match self.baseline {
                Baseline::Alphabetic => "alphabetic",
                Baseline::Middle => "central",
                Baseline::Hanging => "hanging",
            },
            f64::from(self.size) / PLOTTERS_FONT_SCALE,
        );
        if self.angle != 0 {
            // SVG turns clockwise and matplotlib counter-clockwise, hence the
            // sign.
            let _ = write!(
                element,
                " transform=\"rotate(-{}, {}, {})\"",
                self.angle, self.x, self.y
            );
        }
        let _ = write!(element, ">{}</text>", html_escape::encode_text(&self.text));
        element
    }
}

/// How text turned by `angle` must be anchored to grow towards `side`.
///
/// The rule is one question per side: along which direction does the turned
/// text run, and so which end must sit on the anchor for its body to land on
/// the right side? When the text runs *across* that side — a label below an
/// axis, turned by 0° or 180° — no end can do it and the baseline must.
///
/// Exact integer comparisons on the angle rather than trigonometry, so the
/// cases where the text runs exactly across a side cannot be missed by
/// rounding.
pub(super) fn placement(angle: u16, side: Side) -> (Anchor, Baseline) {
    let angle = angle % 360;
    // Screen directions of the text's run: `up` when it rises, `right` when it
    // runs rightwards.
    let rises = angle > 0 && angle < 180;
    let falls = angle > 180;
    let runs_right = !(90..=270).contains(&angle);
    let runs_left = angle > 90 && angle < 270;
    match side {
        Side::Centre => (Anchor::Middle, Baseline::Middle),
        Side::Below => match (rises, falls) {
            (true, _) => (Anchor::End, Baseline::Middle),
            (_, true) => (Anchor::Start, Baseline::Middle),
            _ if angle == 0 => (Anchor::Middle, Baseline::Hanging),
            _ => (Anchor::Middle, Baseline::Alphabetic),
        },
        Side::Above => match (rises, falls) {
            (true, _) => (Anchor::Start, Baseline::Middle),
            (_, true) => (Anchor::End, Baseline::Middle),
            _ if angle == 0 => (Anchor::Middle, Baseline::Alphabetic),
            _ => (Anchor::Middle, Baseline::Hanging),
        },
        Side::Left => match (runs_right, runs_left) {
            (true, _) => (Anchor::End, Baseline::Middle),
            (_, true) => (Anchor::Start, Baseline::Middle),
            _ if angle == 90 => (Anchor::Middle, Baseline::Alphabetic),
            _ => (Anchor::Middle, Baseline::Hanging),
        },
        Side::Right => match (runs_right, runs_left) {
            (true, _) => (Anchor::Start, Baseline::Middle),
            (_, true) => (Anchor::End, Baseline::Middle),
            _ if angle == 90 => (Anchor::Middle, Baseline::Hanging),
            _ => (Anchor::Middle, Baseline::Alphabetic),
        },
    }
}

/// How far text of `(width, height)` turned by `angle` reaches across and
/// along the axes — the box a layout has to leave room for.
pub(super) fn rotated_extent((width, height): (u32, u32), angle: u16) -> (u32, u32) {
    let radians = f64::from(angle).to_radians();
    let (sin, cos) = (radians.sin().abs(), radians.cos().abs());
    let (width, height) = (f64::from(width), f64::from(height));
    // `cos 90°` is a rounding residue rather than zero, so a quarter turn
    // would otherwise gain a pixel; nothing measured here is that precise.
    let pixels = |measure: f64| u32::try_from(to_pixel((measure - 1e-6).ceil())).unwrap_or(0);
    (
        pixels(width.mul_add(cos, height * sin)),
        pixels(width.mul_add(sin, height * cos)),
    )
}

/// Adds `texts` to a finished SVG document, just before it closes, so they
/// draw over everything `plotters` drew.
///
/// `None` if the document has no closing tag, which would mean `plotters`
/// wrote something this module does not understand.
pub(super) fn splice_into(svg: &str, texts: &[PlacedText]) -> Option<String> {
    let close = svg.rfind("</svg>")?;
    let mut spliced = String::with_capacity(svg.len() + texts.len() * 160);
    spliced.push_str(&svg[..close]);
    for text in texts {
        spliced.push_str(&text.to_svg());
        spliced.push('\n');
    }
    spliced.push_str(&svg[close..]);
    Some(spliced)
}

#[cfg(test)]
mod tests;

//! Drawing a bar chart: measuring its text, fitting the plot between it, and
//! putting bars, frame, ticks and labels where [`super::layout`] says.
//!
//! `plotters` draws the shapes — bars, the frame, tick marks, legend swatches
//! — and every piece of text is a [`PlacedText`], spliced in afterwards, since
//! `plotters` cannot turn text by the angles sphinx-needs' rotation options
//! take (see [`super::super::placed_text`]).

use plotters::prelude::*;
use rinx_ast::{BarArrangement, BarOrientation, BarValueLabels, ChartColor};

use super::super::placed_text::{PlacedText, Side, rotated_extent, splice_into};
use super::super::style::{
    as_size, ensure_font_registered, extended_series_color, to_pixel, to_rgb,
};
use super::layout::{BAR_WIDTH, GridShape, ValueAxis, value_extent};

/// The size the chart is drawn at, in SVG user units — wider than a pie, since
/// a bar chart's categories run along its width.
const CHART_WIDTH: u32 = 640;
const CHART_HEIGHT: u32 = 400;

/// The space between the canvas edge and anything drawn.
const MARGIN: i32 = 12;
/// How far a tick mark reaches out of the frame.
const TICK: i32 = 4;
/// The space between a tick mark and its label, or a bar and its value.
const GAP: i32 = 4;
/// A legend swatch's side, and the height of one legend row.
const SWATCH: i32 = 12;
const LEGEND_ROW: i32 = 20;

/// Font sizes, in `plotters`' units (see [`PlacedText::size`]).
const TICK_LABEL_SIZE: u32 = 12;
const AXIS_TITLE_SIZE: u32 = 13;
const VALUE_LABEL_SIZE: u32 = 11;
const LEGEND_SIZE: u32 = 12;

/// The largest share of the canvas a label area may take, so a very long
/// label crowds the plot rather than erasing it.
const MAX_LABEL_SHARE: i32 = 40;

/// The colour of text and frame when `:text_color:` does not say.
const INK: RGBColor = RGBColor(0, 0, 0);

/// Everything a bar chart is drawn from.
///
/// Borrowed where it can be, because every field already lives on the node or
/// the counted grid the caller holds.
pub(crate) struct BarSpec<'a> {
    /// One label per series (row), already resolved.
    pub series: &'a [String],
    /// One label per category (column), already resolved.
    pub categories: &'a [String],
    /// The counts, one row per series, every row one value per category.
    pub values: &'a [Vec<u64>],
    pub arrangement: BarArrangement,
    pub orientation: BarOrientation,
    pub value_labels: BarValueLabels,
    /// `:legend:` — a key naming each series, right of the plot.
    pub legend: bool,
    /// `:colors:`, continued by the built-in palette.
    pub colors: &'a [ChartColor],
    /// `:text_color:` for every piece of text.
    pub text_color: Option<ChartColor>,
    /// `:x_axis_title:` — under the horizontal axis, whichever way the bars
    /// run, as matplotlib's `set_xlabel` is.
    pub x_axis_title: Option<&'a str>,
    /// `:y_axis_title:` — beside the vertical axis.
    pub y_axis_title: Option<&'a str>,
    /// Counter-clockwise degrees for the horizontal axis' tick labels.
    pub xlabels_rotation: u16,
    /// Counter-clockwise degrees for the vertical axis' tick labels.
    pub ylabels_rotation: u16,
    /// Counter-clockwise degrees for the values written onto bars.
    pub sum_rotation: u16,
}

/// Draws `spec` as a standalone SVG element.
///
/// `None` when there is nothing to draw — no series, no categories, or every
/// value zero — since a chart of nothing has no scale; the caller reports it.
pub(crate) fn render_bar_svg(spec: &BarSpec<'_>) -> Option<String> {
    let shape = GridShape {
        series: spec.values.len(),
        categories: spec.values.first().map_or(0, Vec::len),
        stacked: spec.arrangement == BarArrangement::Stacked,
    };
    if shape.series == 0 || shape.categories == 0 {
        return None;
    }
    let axis = ValueAxis::fitting(value_extent(spec.values, shape.stacked))?;

    ensure_font_registered();

    let mut svg = String::new();
    let texts = {
        let root =
            SVGBackend::with_string(&mut svg, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
        let chart = Chart {
            spec,
            shape,
            axis,
            frame: Frame::fit(&root, spec, axis)?,
            ink: spec.text_color.map_or(INK, to_rgb),
        };
        let texts = chart.draw(&root)?;
        root.present().ok()?;
        texts
    };
    splice_into(&svg, &texts)
}

/// The plot's rectangle on the canvas, inside every label area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frame {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Frame {
    /// Leaves room around the plot for every label the chart will write.
    ///
    /// Measured with the vendored font's own metrics, so a turned label gets
    /// exactly the room its turned box needs. `None` only if the font cannot
    /// measure at all, which the caller reports as a chart it could not draw.
    fn fit<DB: DrawingBackend>(
        root: &DrawingArea<DB, plotters::coord::Shift>,
        spec: &BarSpec<'_>,
        axis: ValueAxis,
    ) -> Option<Self> {
        let width = i32::try_from(CHART_WIDTH).ok()?;
        let height = i32::try_from(CHART_HEIGHT).ok()?;
        let (bottom_labels, left_labels) = axis_labels(spec, axis);

        let bottom_area = widest(root, &bottom_labels, TICK_LABEL_SIZE, |text| {
            rotated_extent(text, spec.xlabels_rotation).1
        })?;
        let left_area = widest(root, &left_labels, TICK_LABEL_SIZE, |text| {
            rotated_extent(text, spec.ylabels_rotation).0
        })?;
        let x_title = title_room(root, spec.x_axis_title)?;
        let y_title = title_room(root, spec.y_axis_title)?;
        let legend = if spec.legend {
            let names = widest(root, spec.series, LEGEND_SIZE, |(w, _)| w)?;
            SWATCH + GAP + names + MARGIN
        } else {
            0
        };

        let cap = |area: i32, of: i32| area.min(of * MAX_LABEL_SHARE / 100);
        Some(Self {
            left: MARGIN + y_title + cap(left_area, width) + GAP + TICK,
            top: MARGIN,
            right: width - MARGIN - cap(legend, width),
            bottom: height - MARGIN - x_title - cap(bottom_area, height) - GAP - TICK,
        })
    }
}

/// A chart being drawn: its data, its scale and where it goes.
struct Chart<'s, 'a> {
    spec: &'s BarSpec<'a>,
    shape: GridShape,
    axis: ValueAxis,
    frame: Frame,
    /// The colour of text and of the frame.
    ink: RGBColor,
}

impl Chart<'_, '_> {
    /// Draws the shapes onto `root` and returns the text to splice in.
    fn draw<DB: DrawingBackend>(
        &self,
        root: &DrawingArea<DB, plotters::coord::Shift>,
    ) -> Option<Vec<PlacedText>> {
        let mut texts = Vec::new();
        self.draw_bars(root, &mut texts)?;
        self.draw_frame(root)?;
        self.draw_category_ticks(root, &mut texts)?;
        self.draw_value_ticks(root, &mut texts)?;
        self.write_axis_titles(&mut texts);
        if self.spec.legend {
            self.draw_legend(root, &mut texts)?;
        }
        Some(texts)
    }

    /// Draws every bar, and the values written onto them.
    fn draw_bars<DB: DrawingBackend>(
        &self,
        root: &DrawingArea<DB, plotters::coord::Shift>,
        texts: &mut Vec<PlacedText>,
    ) -> Option<()> {
        let half = BAR_WIDTH / 2.0;
        for category in 0..self.shape.categories {
            let mut stacked_to = 0.0;
            for (series, row) in self.spec.values.iter().enumerate() {
                let value = as_size(row.get(category).copied().unwrap_or(0));
                let position = self.shape.bar_position(series, category);
                let base = if self.shape.stacked { stacked_to } else { 0.0 };
                let end = base + value;
                if value > 0.0 {
                    let corners = [
                        self.point(position - half, base),
                        self.point(position + half, end),
                    ];
                    let color = extended_series_color(self.spec.colors, series);
                    root.draw(&Rectangle::new(corners, color.filled())).ok()?;
                    if self.spec.value_labels.inside {
                        texts.push(self.value_label(
                            value,
                            self.point(position, base + value / 2.0),
                            Side::Centre,
                        ));
                    }
                }
                let last_of_stack = series + 1 == self.shape.series;
                if self.spec.value_labels.at_end && (!self.shape.stacked || last_of_stack) {
                    texts.push(self.end_label(end, position));
                }
                stacked_to = end;
            }
        }
        Some(())
    }

    /// The value written past a bar's end: its own value side by side, the
    /// stack's total stacked — what matplotlib's edge `bar_label` writes.
    fn end_label(&self, end: f64, position: f64) -> PlacedText {
        let (x, y) = self.point(position, end);
        let (anchor, side) = match self.spec.orientation {
            BarOrientation::Vertical => ((x, y - GAP), Side::Above),
            BarOrientation::Horizontal => ((x + GAP, y), Side::Right),
        };
        self.value_label(end, anchor, side)
    }

    /// One value written onto the chart.
    fn value_label(&self, value: f64, at: (i32, i32), side: Side) -> PlacedText {
        PlacedText::new(
            to_pixel(value).to_string(),
            at,
            self.spec.sum_rotation,
            side,
            VALUE_LABEL_SIZE,
            self.ink,
        )
    }

    /// Draws the frame round the plot — matplotlib's four spines.
    fn draw_frame<DB: DrawingBackend>(
        &self,
        root: &DrawingArea<DB, plotters::coord::Shift>,
    ) -> Option<()> {
        let Frame {
            left,
            top,
            right,
            bottom,
        } = self.frame;
        root.draw(&Rectangle::new(
            [(left, top), (right, bottom)],
            self.ink.stroke_width(1),
        ))
        .ok()
    }

    /// Draws a tick and a label at every category's centre.
    fn draw_category_ticks<DB: DrawingBackend>(
        &self,
        root: &DrawingArea<DB, plotters::coord::Shift>,
        texts: &mut Vec<PlacedText>,
    ) -> Option<()> {
        for (category, label) in self.spec.categories.iter().enumerate() {
            let centre = self.shape.category_centre(category);
            let at = match self.spec.orientation {
                BarOrientation::Vertical => self.point(centre, 0.0).0,
                BarOrientation::Horizontal => self.point(centre, 0.0).1,
            };
            self.draw_tick(root, texts, self.category_side(), at, label.clone())?;
        }
        Some(())
    }

    /// Draws a tick and a label at every step of the value axis.
    fn draw_value_ticks<DB: DrawingBackend>(
        &self,
        root: &DrawingArea<DB, plotters::coord::Shift>,
        texts: &mut Vec<PlacedText>,
    ) -> Option<()> {
        let side = match self.category_side() {
            Side::Below => Side::Left,
            _ => Side::Below,
        };
        for tick in self.axis.ticks() {
            let (x, y) = self.point(self.shape.category_range().0, as_size(tick));
            let at = if side == Side::Left { y } else { x };
            self.draw_tick(root, texts, side, at, tick.to_string())?;
        }
        Some(())
    }

    /// Which side of the frame the categories are written on.
    const fn category_side(&self) -> Side {
        match self.spec.orientation {
            BarOrientation::Vertical => Side::Below,
            BarOrientation::Horizontal => Side::Left,
        }
    }

    /// One tick mark out of the frame's bottom or left edge, and its label.
    fn draw_tick<DB: DrawingBackend>(
        &self,
        root: &DrawingArea<DB, plotters::coord::Shift>,
        texts: &mut Vec<PlacedText>,
        side: Side,
        at: i32,
        label: String,
    ) -> Option<()> {
        let (mark, label_at, angle) = if side == Side::Below {
            let edge = self.frame.bottom;
            (
                [(at, edge), (at, edge + TICK)],
                (at, edge + TICK + GAP),
                self.spec.xlabels_rotation,
            )
        } else {
            let edge = self.frame.left;
            (
                [(edge, at), (edge - TICK, at)],
                (edge - TICK - GAP, at),
                self.spec.ylabels_rotation,
            )
        };
        root.draw(&PathElement::new(mark, self.ink.stroke_width(1)))
            .ok()?;
        texts.push(PlacedText::new(
            label,
            label_at,
            angle,
            side,
            TICK_LABEL_SIZE,
            self.ink,
        ));
        Some(())
    }

    /// Writes `:x_axis_title:` along the bottom and `:y_axis_title:` up the
    /// left edge, each centred on the plot.
    fn write_axis_titles(&self, texts: &mut Vec<PlacedText>) {
        let Frame {
            left,
            top,
            right,
            bottom,
        } = self.frame;
        let bottom_edge = i32::try_from(CHART_HEIGHT).unwrap_or(i32::MAX) - MARGIN;
        if let Some(title) = self.spec.x_axis_title {
            texts.push(PlacedText::new(
                title.to_string(),
                (left + (right - left) / 2, bottom_edge),
                0,
                Side::Above,
                AXIS_TITLE_SIZE,
                self.ink,
            ));
        }
        if let Some(title) = self.spec.y_axis_title {
            texts.push(PlacedText::new(
                title.to_string(),
                (MARGIN, top + (bottom - top) / 2),
                90,
                Side::Right,
                AXIS_TITLE_SIZE,
                self.ink,
            ));
        }
    }

    /// Draws the key naming each series, right of the plot.
    fn draw_legend<DB: DrawingBackend>(
        &self,
        root: &DrawingArea<DB, plotters::coord::Shift>,
        texts: &mut Vec<PlacedText>,
    ) -> Option<()> {
        let left = self.frame.right + MARGIN;
        for (series, name) in self.spec.series.iter().enumerate() {
            let top = self.frame.top + i32::try_from(series).ok()? * LEGEND_ROW;
            root.draw(&Rectangle::new(
                [(left, top), (left + SWATCH, top + SWATCH)],
                extended_series_color(self.spec.colors, series).filled(),
            ))
            .ok()?;
            texts.push(PlacedText::new(
                name.clone(),
                (left + SWATCH + GAP, top + SWATCH / 2),
                0,
                Side::Right,
                LEGEND_SIZE,
                self.ink,
            ));
        }
        Some(())
    }

    /// The canvas point of a category-axis position and a value.
    fn point(&self, position: f64, value: f64) -> (i32, i32) {
        let Frame {
            left,
            top,
            right,
            bottom,
        } = self.frame;
        let (low, high) = self.shape.category_range();
        let along = (position - low) / (high - low);
        let up = value / as_size(self.axis.top);
        let span = |from: i32, to: i32, share: f64| {
            to_pixel(f64::from(to - from).mul_add(share, f64::from(from)))
        };
        match self.spec.orientation {
            BarOrientation::Vertical => (span(left, right, along), span(bottom, top, up)),
            BarOrientation::Horizontal => (span(left, right, up), span(top, bottom, along)),
        }
    }
}

/// The labels written along the bottom and the left edge, in that order.
fn axis_labels(spec: &BarSpec<'_>, axis: ValueAxis) -> (Vec<String>, Vec<String>) {
    let values: Vec<String> = axis.ticks().map(|tick| tick.to_string()).collect();
    match spec.orientation {
        BarOrientation::Vertical => (spec.categories.to_vec(), values),
        BarOrientation::Horizontal => (values, spec.categories.to_vec()),
    }
}

/// The room a label area needs: its widest label as `reach` measures it, plus
/// nothing for no labels at all.
fn widest<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    labels: &[String],
    size: u32,
    reach: impl Fn((u32, u32)) -> u32,
) -> Option<i32> {
    let mut widest = 0;
    for label in labels {
        widest = widest.max(reach(measure(root, label, size)?));
    }
    i32::try_from(widest).ok()
}

/// The room an axis title needs across its axis: its height and a gap, or
/// nothing when there is no title.
fn title_room<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    title: Option<&str>,
) -> Option<i32> {
    let Some(title) = title else {
        return Some(0);
    };
    let (_, height) = measure(root, title, AXIS_TITLE_SIZE)?;
    Some(i32::try_from(height).ok()? + GAP)
}

/// How large `text` is drawn at `size`, from the vendored font's metrics.
fn measure<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    text: &str,
    size: u32,
) -> Option<(u32, u32)> {
    let style = ("sans-serif", size).into_text_style(root);
    root.estimate_text_size(text, &style).ok()
}

#[cfg(test)]
mod tests;

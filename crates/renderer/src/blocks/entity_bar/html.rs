//! Rendering a bar chart into the page: counting, drawing, and placing it in
//! the figure every chart shares ([`crate::blocks::chart_figure`]).

use rusty_sphinx_ast::{EntityBar, LabelRotation};

use crate::RenderCtx;
use crate::blocks::chart_figure::{ChartFigure, write_chart_figure};
use crate::chart::{BarSpec, render_bar_svg};
use crate::empty_listing_error::EmptyListingError;

use super::series::count_grid;

/// The class every bar chart carries, whichever name it was written under.
const CHART_CLASS: &str = "entity-bar";

/// Renders an `.. entity-bar::` / `.. needbar::`.
pub(in crate::blocks) fn render_entity_bar(
    html: &mut String,
    bar: &EntityBar,
    ctx: &mut RenderCtx<'_>,
) {
    let counted = count_grid(bar, ctx.index, ctx.schema);
    let Some(svg) = render_bar_svg(&BarSpec {
        series: &counted.series,
        categories: &counted.categories,
        values: &counted.values,
        arrangement: bar.arrangement,
        orientation: bar.orientation,
        value_labels: bar.value_labels,
        legend: bar.legend,
        colors: &bar.colors,
        text_color: bar.text_color,
        x_axis_title: bar.x_axis_title.as_deref(),
        y_axis_title: bar.y_axis_title.as_deref(),
        xlabels_rotation: degrees(bar.xlabels_rotation),
        ylabels_rotation: degrees(bar.ylabels_rotation),
        sum_rotation: degrees(bar.sum_rotation),
    }) else {
        // Every cell counted zero, so there is no scale to draw bars against —
        // the pie's reason for drawing nothing, reported the pie's way.
        ctx.empty_listing_errors
            .push(EmptyListingError::bar(bar.source.as_str(), bar.span));
        return;
    };

    write_chart_figure(
        html,
        &ChartFigure {
            class: CHART_CLASS,
            title: bar.title.as_deref(),
            caption: bar.caption.as_deref(),
            align: bar.align,
            classes: &bar.classes,
            name: bar.name.as_ref(),
            width: bar.rendered_width(),
        },
        &svg,
    );
}

/// A rotation option as the degrees the drawing takes, unrotated when unset.
fn degrees(rotation: Option<LabelRotation>) -> u16 {
    rotation.map_or(0, LabelRotation::degrees)
}

#[cfg(test)]
mod tests;

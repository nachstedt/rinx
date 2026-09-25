//! Rendering a pie chart into the page: counting, drawing, and placing it in
//! the figure every chart shares ([`crate::blocks::chart_figure`]).

use rusty_sphinx_ast::EntityPie;

use crate::RenderCtx;
use crate::blocks::chart_figure::{ChartFigure, write_chart_figure};
use crate::chart::{PieSpec, render_pie_svg};
use crate::empty_listing_error::EmptyListingError;

use super::series::count_wedges;

/// The class every pie chart carries, whichever name it was written under.
///
/// One class for both spellings deliberately: two documents using different
/// names for one directive should not need two stylesheets.
const CHART_CLASS: &str = "entity-pie";

/// Renders an `.. entity-pie::` / `.. needpie::`.
pub(in crate::blocks) fn render_entity_pie(
    html: &mut String,
    pie: &EntityPie,
    ctx: &mut RenderCtx<'_>,
) {
    let wedges = count_wedges(pie, ctx.index, ctx.schema);
    let Some(svg) = render_pie_svg(&PieSpec {
        wedges: &wedges,
        colors: &pie.colors,
        text_color: pie.text_color,
        legend: pie.legend,
    }) else {
        // A chart of nothing has no geometry, so there is no picture to place
        // — the same reason a flowchart that drew nothing renders no `<img>`.
        ctx.empty_listing_errors
            .push(EmptyListingError::pie(pie.source.as_str(), pie.span));
        return;
    };

    write_chart_figure(
        html,
        &ChartFigure {
            class: CHART_CLASS,
            title: pie.title.as_deref(),
            caption: pie.caption.as_deref(),
            align: pie.align,
            classes: &pie.classes,
            name: pie.name.as_ref(),
            width: pie.rendered_width(),
        },
        &svg,
    );
}

#[cfg(test)]
mod tests;

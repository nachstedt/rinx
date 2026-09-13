//! The figure a pie chart sits in.
//!
//! Deliberately *not* [`crate::blocks::diagram_figure`], whose whole job is an
//! `<img>` pointing at a hash-named SVG a build action compiled. A chart has no
//! compile step and no hash: its SVG is written straight into the page. The
//! wrapper's shape is copied from that module all the same, so a page already
//! styled for a diagram needs no new rule for a chart, and a reader sees the
//! two placed the same way.

use std::fmt::Write as _;

use rusty_sphinx_ast::{EntityPie, ImageAlign};

use crate::RenderCtx;
use crate::empty_listing_error::EmptyListingError;
use crate::pie_chart::{PieSpec, render_pie_svg};

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

    let classes = wrapper_classes(pie);
    let classes = html_escape::encode_double_quoted_attribute(&classes);
    let _ = write!(html, "<div class=\"{classes}\"");
    if let Some(name) = &pie.name {
        let id_attr = html_escape::encode_double_quoted_attribute(name.as_str());
        let _ = write!(html, " id=\"{id_attr}\"");
    }
    let _ = writeln!(html, ">");

    if let Some(title) = &pie.title {
        let text = html_escape::encode_text(title);
        let _ = writeln!(html, "  <p class=\"entity-pie-title\">{text}</p>");
    }

    let _ = write!(html, "  <div class=\"entity-pie-figure\"");
    if let Some(width) = pie.rendered_width() {
        let style = format!("width: {width}");
        let style = html_escape::encode_double_quoted_attribute(&style);
        let _ = write!(html, " style=\"{style}\"");
    }
    let _ = writeln!(html, ">{svg}</div>");

    if let Some(caption) = &pie.caption {
        let text = html_escape::encode_text(caption);
        let _ = writeln!(html, "  <p class=\"caption\">{text}</p>");
    }

    let _ = writeln!(html, "</div>");
}

/// The classes on the wrapper: the chart's own marker, its `:align:` and
/// whatever `:class:` added.
fn wrapper_classes(pie: &EntityPie) -> String {
    let mut classes = vec![CHART_CLASS.to_string()];
    classes.extend(pie.align.map(ImageAlign::css_class));
    classes.extend(pie.classes.iter().cloned());
    classes.join(" ")
}

#[cfg(test)]
mod tests;

//! Rendering `.. entity-flow::` / `.. needflow::` — a generated flowchart.
//!
//! The directive carries a question, so the picture cannot exist until the
//! project index does. Generating it is `rusty_sphinx_uml::build_flow`, which
//! this module calls exactly once per directive: the resulting hash names the
//! compiled SVG the page points at, and the text itself goes back on the render
//! output for the same process to write as `<hash>.puml`. The page and the file
//! compiled for it therefore come from one call and agree by construction —
//! the property the whole diagram pipeline is built around.
//!
//! Everything visible on the page is [`super::diagram_figure`], shared with the
//! written diagrams: a flowchart is a diagram, and should look like one.

use rusty_sphinx_ast::{EntityFlow, HashedContent};
use rusty_sphinx_uml::{UmlContext, build_flow};

use crate::RenderCtx;
use crate::uml_error::DiagramError;

use super::diagram_figure::{DiagramFigure, record_diagram_source, render_diagram_figure};

/// Draws the flowchart, recording a failure against the directive.
///
/// The two things that can still fail here are the two that need the whole
/// project: a filter that matched nothing, and a `:config:` naming a preamble
/// the site does not declare. Everything else was settled while parsing.
fn drawn_content(flow: &EntityFlow, ctx: &mut RenderCtx) -> Option<HashedContent> {
    let uml_ctx =
        UmlContext::new(ctx.index, ctx.schema, ctx.original_doc_path).with_configs(ctx.uml_configs);
    match build_flow(flow, &uml_ctx) {
        Ok(content) => {
            record_diagram_source(ctx, &content);
            Some(content)
        }
        Err(error) => {
            ctx.diagram_errors.push(DiagramError {
                directive: flow.source.as_str().to_string(),
                error: error.into(),
                span: flow.span,
            });
            None
        }
    }
}

/// Renders a flowchart directive.
pub(super) fn render_entity_flow(html: &mut String, flow: &EntityFlow, ctx: &mut RenderCtx) {
    // A flowchart that drew nothing was never compiled, so there is no picture
    // to point at — the same reason a failed template renders no `<img>`.
    let Some(content) = drawn_content(flow, ctx) else {
        return;
    };

    render_diagram_figure(
        html,
        &DiagramFigure {
            content: &content,
            classes: &flow.classes,
            align: flow.align,
            width: flow.rendered_width(),
            caption: flow.caption.as_deref(),
            name: flow.name.as_ref(),
            debug: flow.debug,
        },
        ctx,
    );
}

#[cfg(test)]
mod tests;

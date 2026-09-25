//! Rendering `.. entity-sequence::` / `.. needsequence::` — a generated
//! sequence diagram.
//!
//! The flowchart's shape exactly: `rinx_uml::build_sequence` is called
//! once per directive, its hash names the compiled SVG the page points at, and
//! its text goes back on the render output for the same process to write as
//! `<hash>.puml`. Everything visible is [`super::diagram_figure`], so a
//! generated sequence diagram cannot be told apart from a written one.
//!
//! The one addition is the truncation notice. A walk `:max-items:` cut short
//! still draws, so the page says under the picture how much it left out —
//! sphinx-needs does the same — while the build log carries the matching
//! `entity-sequence.truncated` warning for whoever never opens the page.

use std::fmt::Write;

use rinx_ast::EntitySequence;
use rinx_uml::{SequenceDrawing, UmlContext, build_sequence};

use crate::RenderCtx;
use crate::uml_error::DiagramError;

use super::diagram_figure::{DiagramFigure, record_diagram_source, render_diagram_figure};

/// Walks the diagram, recording every problem against the directive.
fn walked(sequence: &EntitySequence, ctx: &mut RenderCtx) -> SequenceDrawing {
    let uml_ctx =
        UmlContext::new(ctx.index, ctx.schema, ctx.original_doc_path).with_configs(ctx.uml_configs);
    let drawing = build_sequence(sequence, &uml_ctx);
    for problem in &drawing.problems {
        ctx.diagram_errors.push(DiagramError {
            directive: sequence.source.as_str().to_string(),
            error: problem.clone().into(),
            span: sequence.span,
        });
    }
    if let Some(content) = &drawing.content {
        record_diagram_source(ctx, content);
    }
    drawing
}

/// Renders a sequence diagram directive.
pub(super) fn render_entity_sequence(
    html: &mut String,
    sequence: &EntitySequence,
    ctx: &mut RenderCtx,
) {
    let drawing = walked(sequence, ctx);
    // A walk that drew nothing was never compiled, so there is no picture to
    // point at — the same reason a failed flowchart renders no `<img>`.
    let Some(content) = &drawing.content else {
        return;
    };

    render_diagram_figure(
        html,
        &DiagramFigure {
            content,
            classes: &sequence.classes,
            align: sequence.align,
            width: sequence.rendered_width(),
            caption: sequence.caption.as_deref(),
            name: sequence.name.as_ref(),
            debug: sequence.debug,
        },
        ctx,
    );
    if let Some((shown, total)) = drawing.truncation() {
        render_truncation_notice(html, shown, total);
    }
}

/// The notice under a picture `:max-items:` cut short, worded after
/// sphinx-needs' own so a migrating reader recognizes it.
fn render_truncation_notice(html: &mut String, shown: usize, total: usize) {
    let _ = writeln!(
        html,
        "<p class=\"entity-sequence-truncated\">Showing the first {shown} of {total} messages; \
         raise :max-items: (0 for all) to draw more.</p>"
    );
}

#[cfg(test)]
mod tests;

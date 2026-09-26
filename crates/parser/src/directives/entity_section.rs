//! Parsing a named prose section inside an entity.
//!
//! A section is recognised only where its type declares it, which is what
//! makes `.. verification-criteria::` a real construct inside a `.. req::` and
//! a diagnosed mistake anywhere else. The alternative — recognising the name
//! everywhere — would let a typo'd section silently render as nothing, which
//! is the degradation this whole diagnostic set exists to catch.

use rinx_ast::{Diagnostic, DiagnosticCode, Directive, Node, Span};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

/// Parses a section sub-directive, if `name` is one here.
///
/// Returns `None` when the name is not a section at all, letting the ordinary
/// directive chain continue. Returns a node — and a diagnostic — when the name
/// *is* a declared section of some type but was written in the wrong place,
/// because a section outside an entity would otherwise become an unknown
/// directive and render as nothing at all.
pub(in crate::directives) struct EntitySectionSite<'a> {
    /// Where the sub-directive marker was written.
    pub span: Option<Span>,
    /// Its body, indentation intact.
    pub body_lines: &'a [&'a str],
}

pub(in crate::directives) fn try_parse_entity_section(
    name: &str,
    argument: &str,
    site: &EntitySectionSite<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Node> {
    let &EntitySectionSite {
        span: directive_span,
        body_lines,
    } = site;
    match ctx.enclosing_entity {
        Some(entity_type) if entity_type.section(name).is_some() => {}
        Some(entity_type) => {
            // Inside an entity, but this type declares no such section. Worth
            // saying so precisely: the author is likelier to have mistyped a
            // name than to have meant a directive that does not exist.
            if declares_section_anywhere(name, ctx) {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::EntityUnknownSection,
                    format!(
                        "`.. {name}::` is not a section of `.. {}::`",
                        entity_type.name
                    ),
                    directive_span,
                ));
                return Some(section_node(
                    name,
                    body_lines,
                    directive_span,
                    adornment_order,
                    diagnostics,
                    ctx,
                ));
            }
            return None;
        }
        None => {
            if !declares_section_anywhere(name, ctx) {
                return None;
            }
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntitySectionOutsideEntity,
                format!(
                    "`.. {name}::` is a section of an entity type, and means nothing outside one"
                ),
                directive_span,
            ));
            return Some(section_node(
                name,
                body_lines,
                directive_span,
                adornment_order,
                diagnostics,
                ctx,
            ));
        }
    }

    // An argument on a section is a mistake worth naming: a section takes its
    // content from its body, so text on the marker line would vanish.
    if !argument.trim().is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityMalformedArgument,
            format!(
                "`.. {name}::` takes no argument, but {:?} was given",
                argument.trim()
            ),
            directive_span,
        ));
    }

    Some(section_node(
        name,
        body_lines,
        directive_span,
        adornment_order,
        diagnostics,
        ctx,
    ))
}

/// Builds the section node, parsing its body outside any enclosing entity.
fn section_node(
    name: &str,
    body_lines: &[&str],
    span: Option<Span>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Node {
    // Cleared, so a section written inside a section is not recognised as one:
    // sections are one level deep, and a nested one has no rendering that
    // means anything.
    let body_ctx = ctx.outside_entity();
    let unindented = unindent_body_lines(body_lines);
    let lines: Vec<&str> = unindented.iter().map(String::as_str).collect();
    let body = parse_blocks(&lines, adornment_order, diagnostics, &body_ctx);
    Node::Directive(Directive::EntitySection {
        name: name.to_string(),
        body,
        span,
    })
}

/// Reports whether any type in the schema declares a section called `name`.
///
/// The test that separates "a section in the wrong place", which is worth a
/// diagnostic, from "a directive this build does not implement", which is not
/// this module's business.
fn declares_section_anywhere(name: &str, ctx: &ParseCtx<'_>) -> bool {
    ctx.schema
        .types()
        .iter()
        .any(|entity_type| entity_type.section(name).is_some())
}

#[cfg(test)]
mod tests;

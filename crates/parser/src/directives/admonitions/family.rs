//! Recognizing the admonition family's names and choosing, for each, what of
//! its marker line is content.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::{DirectiveBody, MarkerLine, MarkerText, directive_content};
use crate::headings::Adornment;
use rinx_ast::{AdmonitionKind, Directive, VersionChangeKind};

use super::admonition::{generic_title, parse_admonition};
use super::seealso::parse_seealso;
use super::version_change::parse_version_change;

/// Parses `name` when it is one of the admonition family, or returns `None`.
///
/// Dispatched ahead of the directive chain rather than inside it, because
/// only here are the marker line and the body both still in reach: the chain
/// receives a context already positioned below the marker, and the text after
/// the `::` sits above it.
///
/// That text is the content's first line for every member but two. The
/// generic `.. admonition::` reads it as its title; a version change reads its
/// first word as the version, and only the rest — Sphinx's optional second
/// argument — as content.
pub(in crate::directives) fn try_parse_admonition_family(
    name: &str,
    marker: &MarkerLine<'_>,
    body: &DirectiveBody<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    if let Ok(kind) = name.parse::<VersionChangeKind>() {
        let (version, explanation) = marker.argument.split_first_word();
        let content = directive_content(&explanation, marker.index, body, ctx);
        return Some(parse_version_change(
            kind,
            version,
            marker.span,
            &content,
            adornment_order,
            diagnostics,
        ));
    }
    if name == "seealso" {
        let content = directive_content(&marker.argument, marker.index, body, ctx);
        return Some(parse_seealso(content, adornment_order, diagnostics));
    }
    let kind = name.parse::<AdmonitionKind>().ok()?;
    let (title, first_line) = if kind == AdmonitionKind::Admonition {
        let title = generic_title(marker.argument.text, marker.span, diagnostics);
        (Some(title), MarkerText::NONE)
    } else {
        (None, marker.argument)
    };
    let content = directive_content(&first_line, marker.index, body, ctx);
    Some(parse_admonition(
        kind,
        title,
        content,
        adornment_order,
        diagnostics,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::body::collect_directive_body;
    use rinx_ast::{Domain, Node};

    fn parse_family(name: &str, lines: &[&str]) -> Option<Directive> {
        let body = collect_directive_body(lines, 1, 0);
        let marker = MarkerLine {
            index: 0,
            span: None,
            argument: MarkerText::after_marker(lines[0]),
        };
        try_parse_admonition_family(
            name,
            &marker,
            &body,
            &mut Vec::new(),
            &mut Diagnostics::default(),
            &ParseCtx::with_domain(Domain::Py),
        )
    }

    #[test]
    fn test_try_parse_admonition_family_ignores_other_names() {
        // Given / When
        let directive = parse_family("toctree", &[".. toctree::"]);

        // Then
        assert!(directive.is_none());
    }

    #[test]
    fn test_try_parse_admonition_family_reads_seealso_marker_text_as_content() {
        // Given / When
        let directive = parse_family("seealso", &[".. seealso:: Plain text"]);

        // Then
        let Some(Directive::SeeAlso { body }) = directive else {
            panic!("Expected SeeAlso, got {directive:?}");
        };
        assert!(matches!(body[..], [Node::Paragraph(_)]));
    }

    #[test]
    fn test_try_parse_admonition_family_reads_a_note_marker_text_as_content() {
        // Given / When
        let directive = parse_family("note", &[".. note:: Plain text"]);

        // Then
        let Some(Directive::Admonition { title, body, .. }) = directive else {
            panic!("Expected Admonition, got {directive:?}");
        };
        assert_eq!(title, None);
        assert!(matches!(body[..], [Node::Paragraph(_)]));
    }

    #[test]
    fn test_try_parse_admonition_family_reads_a_generic_marker_text_as_title() {
        // Given / When
        let directive = parse_family("admonition", &[".. admonition:: My Title", "   Body"]);

        // Then
        let Some(Directive::Admonition { title, body, .. }) = directive else {
            panic!("Expected Admonition, got {directive:?}");
        };
        assert_eq!(title.as_deref(), Some("My Title"));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_try_parse_admonition_family_splits_a_version_from_its_explanation() {
        // Given / When
        let directive = parse_family("versionchanged", &[".. versionchanged:: 3.1 Some text"]);

        // Then
        let Some(Directive::VersionChange { version, body, .. }) = directive else {
            panic!("Expected VersionChange, got {directive:?}");
        };
        assert_eq!(version, "3.1");
        assert!(matches!(body[..], [Node::Paragraph(_)]));
    }
}

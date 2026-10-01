//! `.. versionadded::`, `.. versionchanged::` and `.. deprecated::`.

use crate::diagnostics::Diagnostics;
use crate::directives::body::DirectiveContent;
use crate::headings::Adornment;
use rinx_ast::{Diagnostic, DiagnosticCode, Directive, Span, VersionChangeKind};

/// Parses a version change whose marker line named `version`, with the rest of
/// that line, if any, already the first line of `content`.
///
/// Reads no options, as Sphinx declares none: a field-looking line here is
/// content.
pub(super) fn parse_version_change(
    kind: VersionChangeKind,
    version: &str,
    marker_span: Option<Span>,
    content: &DirectiveContent<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let version = if version.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveVersionArgumentMissing,
            format!("'{}' requires a version argument.", kind.as_str()),
            marker_span,
        ));
        "unknown".to_string()
    } else {
        version.to_string()
    };
    Directive::VersionChange {
        kind,
        version,
        body: content.parse(adornment_order, diagnostics),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{InlineNode, Node};

    fn only_version_change(input: &str) -> (VersionChangeKind, String, Vec<Node>) {
        let doc = parse("test.rst", input);
        let [
            Node::Directive(Directive::VersionChange {
                kind,
                version,
                body,
            }),
        ] = &doc.nodes[..]
        else {
            panic!("Expected one VersionChange, got {:?}", doc.nodes);
        };
        (*kind, version.clone(), body.clone())
    }

    #[test]
    fn test_parse_versionchanged_creates_directive() {
        // Given
        let input = ".. versionchanged:: 2.3\n\n   Added async support.";

        // When
        let (kind, version, body) = only_version_change(input);

        // Then
        assert_eq!(kind, VersionChangeKind::Changed);
        assert_eq!(version, "2.3");
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_versionadded_creates_directive() {
        // Given
        let input = ".. versionadded:: 1.0\n\n   Initial release.";

        // When
        let (kind, version, body) = only_version_change(input);

        // Then
        assert_eq!(kind, VersionChangeKind::Added);
        assert_eq!(version, "1.0");
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_deprecated_creates_directive() {
        // Given
        let input = ".. deprecated:: 3.0\n\n   Use new API.";

        // When
        let (kind, version, body) = only_version_change(input);

        // Then
        assert_eq!(kind, VersionChangeKind::Deprecated);
        assert_eq!(version, "3.0");
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_version_change_emits_diagnostic_for_missing_version() {
        // Given
        let input = ".. versionchanged::\n\n   Missing version.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.diagnostics.len(), 1);
        assert_eq!(
            doc.diagnostics[0].message,
            "'versionchanged' requires a version argument."
        );
        assert_eq!(doc.diagnostics[0].span.expect("positioned").start.line, 1);
        let (_, version, body) = only_version_change(input);
        assert_eq!(version, "unknown");
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_version_change_keeps_text_after_the_version() {
        // Given
        let input = ".. versionchanged:: 3.1 Some text\n";

        // When
        let (_, version, body) = only_version_change(input);

        // Then
        assert_eq!(version, "3.1");
        assert_eq!(
            body,
            [Node::Paragraph(vec![InlineNode::Text(
                "Some text".to_string()
            )])]
        );
    }

    #[test]
    fn test_parse_version_change_joins_text_after_the_version_with_its_continuation() {
        // Given
        let input = ".. versionchanged:: 3.1 Some text\n   continued.\n\n   Body.\n";

        // When
        let (_, version, body) = only_version_change(input);

        // Then
        assert_eq!(version, "3.1");
        assert_eq!(
            body,
            [
                Node::Paragraph(vec![InlineNode::Text("Some text\ncontinued.".to_string())]),
                Node::Paragraph(vec![InlineNode::Text("Body.".to_string())]),
            ]
        );
    }

    #[test]
    fn test_parse_version_change_reads_a_field_line_as_content() {
        // Given
        let input = ".. versionadded:: 2.0\n   :class: wide\n";

        // When
        let (_, _, body) = only_version_change(input);

        // Then
        assert_eq!(body.len(), 1);
    }
}

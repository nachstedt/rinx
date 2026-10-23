//! `.. versionadded::`, `.. versionchanged::` and `.. deprecated::`.

use crate::diagnostics::Diagnostics;
use crate::directives::body::{DirectiveContent, MarkerText};
use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::spec::{Arguments, DirectiveSpec, OptionSet};
use crate::headings::Adornment;
use rinx_ast::{Directive, VersionChangeKind};

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const VERSION_CHANGE_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["versionadded", "versionchanged", "deprecated"],
    doc: "A note on the version that added, changed or deprecated something.",
    arguments: Arguments {
        required: 1,
        optional: 1,
        final_whitespace: true,
    },
    takes_content: true,
    content_hint: None,
    options: OptionSet::NONE,
    accepts_further: false,
};

/// Interprets a version change: the first word after the `::` is the version,
/// and only the rest — Sphinx's optional second argument — is content.
pub(in crate::directives) fn interpret_version_change(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(kind) = input.name.parse::<rinx_ast::VersionChangeKind>() else {
        return unknown(input, state);
    };
    single(parse_version_change(
        kind,
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses a version change whose marker line named `version`, with the rest of
/// that line, if any, already the first line of `content`.
///
/// Reads no options, as Sphinx declares none: a field-looking line here is
/// content.
pub(super) fn parse_version_change(
    kind: VersionChangeKind,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    // A missing version is the reader's to report.
    let version = match invocation.argument(0) {
        "" => "unknown".to_string(),
        version => version.to_string(),
    };
    Directive::VersionChange {
        kind,
        version,
        body: explanation_and_content(invocation).parse(adornment_order, diagnostics),
    }
}

/// The version change's second argument — the explanation Sphinx lets follow
/// the version — and its content, as one positioned slice.
///
/// Sphinx parses the explanation as a paragraph of its own above the content,
/// and the explanation is the rest of the argument block, so it is exactly
/// the block's lines with the version cut off the first: that keeps every
/// line where it was written, the version's line included.
fn explanation_and_content<'c>(invocation: &DirectiveInvocation<'_, 'c>) -> DirectiveContent<'c> {
    let marker_text = invocation.lines.first().map_or("", String::as_str);
    if marker_text.is_empty() {
        return invocation.content();
    }
    let (_, explanation) = MarkerText {
        text: marker_text,
        column: 0,
    }
    .split_first_word();
    let mut lines = invocation.lines.clone();
    lines[0] = explanation.text.to_string();
    DirectiveContent {
        lines,
        ctx: invocation.ctx.hanging(0, explanation.column, 0),
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
            "versionchanged: 1 argument(s) required, 0 supplied"
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

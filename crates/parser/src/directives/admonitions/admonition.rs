//! `.. note::`, `.. warning::` and the other specific admonitions, and the
//! generic `.. admonition::`.

use crate::diagnostics::Diagnostics;
use crate::directives::body::DirectiveContent;
use crate::directives::options::take_option_block;
use crate::headings::Adornment;
use rinx_ast::{AdmonitionKind, Diagnostic, DiagnosticCode, Directive, Span};

/// The title a generic `.. admonition::` shows: its argument, or — reported
/// as missing — a placeholder.
pub(super) fn generic_title(
    argument: &str,
    marker_span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> String {
    if argument.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveTitleArgumentMissing,
            "Generic 'admonition' directive requires a title argument.",
            marker_span,
        ));
        "Admonition".to_string()
    } else {
        argument.to_string()
    }
}

/// Parses an admonition of `kind` from its content, which for every kind but
/// the generic one may have begun on the marker line.
pub(super) fn parse_admonition(
    kind: AdmonitionKind,
    title: Option<String>,
    mut content: DirectiveContent<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    // Only `:collapsible:` means anything here; every other option is
    // skipped silently, as it always has been. `:collapsible:` alone or
    // `:collapsible: close` starts closed.
    let collapsible = take_option_block(&mut content.lines)
        .iter()
        .rfind(|option| option.name == "collapsible")
        .map(|option| option.value == "open");
    Directive::Admonition {
        kind,
        title,
        collapsible,
        body: content.parse(adornment_order, diagnostics),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParseCtx;
    use crate::parse;
    use rinx_ast::{Domain, InlineNode, Node, Position};

    fn content(lines: &[&str]) -> DirectiveContent<'static> {
        DirectiveContent {
            lines: lines.iter().map(ToString::to_string).collect(),
            ctx: ParseCtx::with_domain(Domain::Py),
        }
    }

    fn only_admonition(input: &str) -> (Option<String>, Option<bool>, Vec<Node>) {
        let doc = parse("test.rst", input);
        let [
            Node::Directive(Directive::Admonition {
                title,
                collapsible,
                body,
                ..
            }),
        ] = &doc.nodes[..]
        else {
            panic!("Expected one Admonition, got {:?}", doc.nodes);
        };
        (title.clone(), *collapsible, body.clone())
    }

    #[test]
    fn test_parse_admonition_basic() {
        // Given
        let kind = AdmonitionKind::Note;

        // When
        let directive = parse_admonition(
            kind,
            None,
            content(&["Body line"]),
            &mut Vec::new(),
            &mut Diagnostics::default(),
        );

        // Then
        let Directive::Admonition {
            kind, title, body, ..
        } = directive
        else {
            panic!("Expected Admonition directive");
        };
        assert_eq!(kind, AdmonitionKind::Note);
        assert_eq!(title, None);
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_admonition_keeps_the_title_it_is_given() {
        // Given
        let title = Some("Custom Title".to_string());

        // When
        let directive = parse_admonition(
            AdmonitionKind::Admonition,
            title,
            content(&["Body line"]),
            &mut Vec::new(),
            &mut Diagnostics::default(),
        );

        // Then
        let Directive::Admonition { title, .. } = directive else {
            panic!("Expected Admonition directive");
        };
        assert_eq!(title.as_deref(), Some("Custom Title"));
    }

    #[test]
    fn test_parse_admonition_collapsible() {
        // Given
        let lines = content(&[":collapsible: open", "", "Content"]);

        // When
        let directive = parse_admonition(
            AdmonitionKind::Warning,
            None,
            lines,
            &mut Vec::new(),
            &mut Diagnostics::default(),
        );

        // Then
        let Directive::Admonition {
            collapsible, body, ..
        } = directive
        else {
            panic!("Expected Admonition directive");
        };
        assert_eq!(collapsible, Some(true));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_generic_title_is_the_argument() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let title = generic_title("Custom Title", None, &mut diagnostics);

        // Then
        assert_eq!(title, "Custom Title");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_generic_title_reports_a_missing_argument() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let title = generic_title("", None, &mut diagnostics);

        // Then
        assert_eq!(title, "Admonition");
        assert!(diagnostics[0].message.contains("requires a title argument"));
    }

    #[test]
    fn test_parse_reports_a_missing_generic_title_on_the_marker_line() {
        // Given
        let input = "Intro.\n\n.. admonition::\n\n   Body.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        let diagnostic = doc
            .diagnostics
            .iter()
            .find(|d| d.code == DiagnosticCode::DirectiveTitleArgumentMissing)
            .expect("reported");
        assert_eq!(diagnostic.span.expect("positioned").start.line, 3);
    }

    #[test]
    fn test_parse_admonition_places_a_diagnostic_below_its_options_on_its_own_line() {
        // Given — the unknown directive sits on line 4, below an option and a
        // blank line
        let input = ".. note::\n   :collapsible:\n\n   .. bogus::\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        let lines: Vec<u32> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code == DiagnosticCode::DirectiveUnknown)
            .map(|d| d.span.expect("a directive has a position").start.line)
            .collect();
        assert_eq!(lines, [4]);
    }

    #[test]
    fn test_parse_note_keeps_content_written_on_its_marker_line() {
        // Given
        let input = ".. note:: Plain text\n";

        // When
        let (title, _, body) = only_admonition(input);

        // Then
        assert_eq!(title, None);
        assert_eq!(
            body,
            [Node::Paragraph(vec![InlineNode::Text(
                "Plain text".to_string()
            )])]
        );
    }

    #[test]
    fn test_parse_warning_keeps_content_written_on_its_marker_line() {
        // Given
        let input = ".. warning:: Careful\n\n   More.\n";

        // When
        let (_, _, body) = only_admonition(input);

        // Then
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn test_parse_note_reads_an_option_below_marker_line_content() {
        // Given
        let input = ".. note:: Some text\n   :collapsible: open\n\n   Body.\n";

        // When
        let (_, collapsible, body) = only_admonition(input);

        // Then
        assert_eq!(collapsible, Some(true));
        assert_eq!(
            body,
            [
                Node::Paragraph(vec![InlineNode::Text("Some text".to_string())]),
                Node::Paragraph(vec![InlineNode::Text("Body.".to_string())]),
            ]
        );
    }

    #[test]
    fn test_parse_note_reads_an_option_below_content_starting_the_body() {
        // Given — docutils reads the first block's field lines as options
        // wherever the content above them began
        let input = ".. note::\n   Some text\n   :collapsible:\n";

        // When
        let (_, collapsible, body) = only_admonition(input);

        // Then
        assert_eq!(collapsible, Some(false));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_note_keeps_a_role_opening_its_body_as_content() {
        // Given
        let input = ".. note::\n   :ref:`label` explains it.\n";

        // When
        let (_, collapsible, body) = only_admonition(input);

        // Then
        assert_eq!(collapsible, None);
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_note_places_a_marker_line_diagnostic_at_its_column() {
        // Given an invalid PEP number written after the marker's `::`, and
        // another on the line continuing it
        let input = ".. note:: See :pep:`eight`.\n   And :pep:`nine`.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        let starts: Vec<Position> = doc
            .diagnostics
            .iter()
            .filter_map(|d| d.span.map(|span| span.start))
            .collect();
        assert_eq!(starts, [Position::new(1, 15), Position::new(2, 8)]);
    }
}

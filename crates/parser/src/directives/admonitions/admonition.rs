//! `.. note::`, `.. warning::` and the other specific admonitions, and the
//! generic `.. admonition::`.

use crate::diagnostics::Diagnostics;
use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::options::{DirectiveOptions, classify_options, directive_options};
use crate::directives::spec::{Arguments, DirectiveSpec};
use crate::headings::Adornment;
use rinx_ast::{AdmonitionKind, Directive};

/// The title a generic `.. admonition::` shows: its argument, or a
/// placeholder when the argument is missing — which the reader has reported.
pub(super) fn generic_title(argument: &str) -> String {
    if argument.is_empty() {
        "Admonition".to_string()
    } else {
        argument.to_string()
    }
}

/// The advice for the options this build does not draw on a callout.
const NOT_DRAWN: &str = "a callout is drawn with its kind's class and no label";

directive_options! {
    /// The options every callout takes from docutils, neither drawn here.
    pub(super) enum CalloutOption {
        _ => (["class"], Classes, "Classes for the callout.", NOT_DRAWN),
        _ => (["name"], Text, "A label to reference the callout by.", NOT_DRAWN),
    }
}

directive_options! {
    /// The options an admonition takes: docutils' two and Sphinx's
    /// `:collapsible:`.
    enum AdmonitionOption {
        /// `:collapsible:`.
        Collapsible => (["collapsible"], Choice(|| vec!["open", "closed"]), "Draws the callout folded; `open` starts it unfolded.");
        Callout(CalloutOption),
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const ADMONITION_SPEC: DirectiveSpec = DirectiveSpec {
    names: &[
        "attention",
        "caution",
        "danger",
        "error",
        "hint",
        "important",
        "note",
        "tip",
        "warning",
    ],
    doc: "A callout of one of docutils' nine kinds, titled by its kind.",
    arguments: Arguments::NONE,
    takes_content: true,
    content_hint: None,
    options: <AdmonitionOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const GENERIC_ADMONITION_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["admonition"],
    doc: "A callout titled by its argument.",
    arguments: Arguments::ONE_TEXT,
    takes_content: true,
    content_hint: None,
    options: <AdmonitionOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. note::` and the other eight titled by their kind: the text
/// after the `::` is the first line of the content.
pub(in crate::directives) fn interpret_admonition(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(kind) = input.name.parse::<rinx_ast::AdmonitionKind>() else {
        return unknown(input, state);
    };
    single(parse_admonition(
        kind,
        None,
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Interprets `.. admonition::`, whose marker text is its title rather than
/// content.
pub(in crate::directives) fn interpret_generic_admonition(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_admonition(
        rinx_ast::AdmonitionKind::Admonition,
        Some(generic_title(input.argument(0))),
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses an admonition of `kind` from its content, which for every kind but
/// the generic one may have begun on the marker line.
pub(super) fn parse_admonition(
    kind: AdmonitionKind,
    title: Option<String>,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    // `:collapsible:` alone or `:collapsible: closed` starts closed.
    let mut collapsible = None;
    for (option, line) in classify_options::<AdmonitionOption>(
        &invocation.options,
        invocation.name,
        diagnostics,
        &invocation.ctx,
    ) {
        match option {
            AdmonitionOption::Collapsible => collapsible = Some(line.value == "open"),
            AdmonitionOption::Callout(option) => match option {},
        }
    }
    Directive::Admonition {
        kind,
        title,
        collapsible,
        body: invocation.content().parse(adornment_order, diagnostics),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParseCtx;
    use crate::parse_document;
    use rinx_ast::{DiagnosticCode, Domain, InlineNode, Node, Position};

    /// Parses an admonition of `kind` titled `title`, with `body` below its
    /// marker, as the reader hands it over.
    fn admonition(kind: AdmonitionKind, title: Option<String>, body: &[&str]) -> Directive {
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "note",
            &ADMONITION_SPEC,
            "",
            body,
            &mut diagnostics,
            &ctx,
        );
        parse_admonition(kind, title, &invocation, &mut Vec::new(), &mut diagnostics)
    }

    fn only_admonition(input: &str) -> (Option<String>, Option<bool>, Vec<Node>) {
        let doc = parse_document("test.rst", input);
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
        let directive = admonition(kind, None, &["Body line"]);

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
        let directive = admonition(AdmonitionKind::Admonition, title, &["Body line"]);

        // Then
        let Directive::Admonition { title, .. } = directive else {
            panic!("Expected Admonition directive");
        };
        assert_eq!(title.as_deref(), Some("Custom Title"));
    }

    #[test]
    fn test_parse_admonition_collapsible() {
        // Given
        let lines = [":collapsible: open", "", "Content"];

        // When
        let directive = admonition(AdmonitionKind::Warning, None, &lines);

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
        // Given / When / Then
        assert_eq!(generic_title("Custom Title"), "Custom Title");
    }

    #[test]
    fn test_generic_title_falls_back_to_a_placeholder() {
        // Given / When / Then — the missing argument is the reader's to report
        assert_eq!(generic_title(""), "Admonition");
    }

    #[test]
    fn test_parse_reports_a_missing_generic_title_on_the_marker_line() {
        // Given
        let input = "Intro.\n\n.. admonition::\n\n   Body.\n";

        // When
        let doc = parse_document("test.rst", input);

        // Then
        let diagnostic = doc
            .diagnostics
            .iter()
            .find(|d| d.code == DiagnosticCode::DirectiveMissingArgument)
            .expect("reported");
        assert_eq!(diagnostic.span.expect("positioned").start.line, 3);
    }

    #[test]
    fn test_parse_admonition_places_a_diagnostic_below_its_options_on_its_own_line() {
        // Given — the unknown directive sits on line 4, below an option and a
        // blank line
        let input = ".. note::\n   :collapsible:\n\n   .. bogus::\n";

        // When
        let doc = parse_document("test.rst", input);

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
        let doc = parse_document("test.rst", input);

        // Then
        let starts: Vec<Position> = doc
            .diagnostics
            .iter()
            .filter_map(|d| d.span.map(|span| span.start))
            .collect();
        assert_eq!(starts, [Position::new(1, 15), Position::new(2, 8)]);
    }

    #[test]
    fn test_interpret_admonition_refuses_a_name_no_admonition_kind_spells() {
        // Given — `seealso` is a directive of its own, no admonition kind
        let ctx = ParseCtx::with_domain(Domain::Py);
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let input = crate::directives::reader::test_invocation(
            "seealso",
            &ADMONITION_SPEC,
            "",
            &[],
            &mut diagnostics,
            &ctx,
        );

        // When
        let nodes = interpret_admonition(
            &input,
            &mut ParseState {
                adornment_order: &mut adornment_order,
                diagnostics: &mut diagnostics,
            },
        );

        // Then
        assert!(matches!(
            nodes[..],
            [Node::Directive(Directive::Unknown { .. })]
        ));
    }
}

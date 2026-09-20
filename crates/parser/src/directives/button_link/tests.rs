use rusty_sphinx_ast::{
    ButtonFlag, ButtonLink, ButtonTarget, Diagnostic, DiagnosticCode, Directive, InlineNode, Node,
    SemanticColor, TextAlign, walk_nodes,
};

use crate::parse;

/// Parses a whole document and returns its single `.. button-link::` with the
/// document's diagnostics — the dispatcher is what positions the parse
/// context, so spans are only meaningful this way.
///
/// Walks the whole tree rather than the top level, so a button written inside
/// a container is found by the same helper.
fn parse_document(input: &str) -> (ButtonLink, Vec<Diagnostic>) {
    let doc = parse("test.rst", input);
    let mut found = None;
    walk_nodes(&doc.nodes, &mut |node| {
        if let Node::Directive(Directive::ButtonLink(button)) = node {
            found = Some((**button).clone());
        }
    });
    let button = found.expect("document should contain a button-link directive");
    (button, doc.diagnostics)
}

/// The codes reported, in order, for asserting about diagnostics without
/// pinning their wording.
fn codes(diagnostics: &[Diagnostic]) -> Vec<DiagnosticCode> {
    diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn test_parses_a_button_with_a_url_and_a_label() {
    // Given
    let input = ".. button-link:: https://example.com\n\n   Read the docs\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        button.target,
        ButtonTarget::Url("https://example.com".to_string())
    );
    assert_eq!(
        button.label,
        vec![InlineNode::Text("Read the docs".to_string())]
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_button_with_no_content_keeps_an_empty_label() {
    // Given — the target is shown instead, which the renderer decides
    let input = ".. button-link:: https://example.com\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert!(button.label.is_empty());
    assert!(!button.has_label());
    assert!(diagnostics.is_empty());
}

#[test]
fn test_whitespace_inside_an_argument_is_removed() {
    // Given — docutils' `directives.uri` removes every whitespace character
    // rather than trimming the ends
    let input = ".. button-link::   https://example.com/a b \n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        button.target,
        ButtonTarget::Url("https://example.com/ab".to_string())
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_an_argument_does_not_continue_onto_the_next_line() {
    // Given — docutils would fold this line into the argument, since the
    // directive declares `final_argument_whitespace`. This build continues no
    // directive argument across lines except a domain object's signatures, so
    // the line is read as the label instead — visible to the author rather
    // than silently dropped. See `spec_gaps.md`.
    let input = ".. button-link:: https://example.com/a/very/\n   long/path\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        button.target,
        ButtonTarget::Url("https://example.com/a/very/".to_string())
    );
    assert_eq!(
        button.label,
        vec![InlineNode::Text("long/path".to_string())]
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_label_written_over_several_lines_is_joined() {
    // Given
    let input = ".. button-link:: https://example.com\n\n   Open the\n   online editor\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        button.label,
        vec![InlineNode::Text("Open the\nonline editor".to_string())]
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_label_is_parsed_as_inline_markup() {
    // Given
    let input = ".. button-link:: https://example.com\n\n   Read ``conf.py``\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        button.label,
        vec![
            InlineNode::Text("Read ".to_string()),
            InlineNode::Literal("conf.py".to_string()),
        ]
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_reads_every_flag_option() {
    // Given
    let input = ".. button-link:: https://example.com\n   :outline:\n   :color: primary\n   \
                 :expand:\n   :click-parent:\n   :shadow:\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        button.flags.iter().copied().collect::<Vec<_>>(),
        ButtonFlag::ALL
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_reads_the_color_align_tooltip_and_class_options() {
    // Given
    let input = ".. button-link:: https://example.com\n   :color: success\n   :align: center\n   \
                 :tooltip: Opens in the browser\n   :class: my-button wide\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(button.color, Some(SemanticColor::Success));
    assert_eq!(button.align, Some(TextAlign::Center));
    assert_eq!(button.tooltip, Some("Opens in the browser".to_string()));
    assert_eq!(button.class, vec!["my-button", "wide"]);
    assert!(diagnostics.is_empty());
}

#[test]
fn test_the_corpus_shape_parses_without_a_diagnostic() {
    // Given — every `.. button-link::` in the sphinx-needs demo has this shape
    let input = ".. button-link:: https://gitpod.io/#https://github.com/useblocks/demo\n   \
                 :color: primary\n   :shadow:\n\n   Open this playground in Gitpod!\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(button.color, Some(SemanticColor::Primary));
    assert!(button.has(ButtonFlag::Shadow));
    assert_eq!(
        button.label,
        vec![InlineNode::Text(
            "Open this playground in Gitpod!".to_string()
        )]
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_missing_argument_becomes_a_malformed_directive() {
    // Given
    let input = ".. button-link::\n\n   Nowhere to go\n";

    // When
    let doc = parse("test.rst", input);

    // Then — the name is recognized, so the content is refused rather than
    // swallowed as an unknown directive
    assert!(matches!(
        doc.nodes.first(),
        Some(Node::Directive(Directive::Malformed { .. }))
    ));
    assert_eq!(
        codes(&doc.diagnostics),
        vec![DiagnosticCode::ButtonLinkMissingTarget]
    );
}

#[test]
fn test_an_unreadable_color_is_reported_and_the_button_survives() {
    // Given
    let input = ".. button-link:: https://example.com\n   :color: puce\n\n   Open\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(button.color, None);
    assert_eq!(button.label, vec![InlineNode::Text("Open".to_string())]);
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::ButtonLinkInvalidColor]
    );
}

#[test]
fn test_an_unreadable_align_is_reported_and_the_button_survives() {
    // Given — docutils' image `:align:` has `middle`, this option does not
    let input = ".. button-link:: https://example.com\n   :align: middle\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(button.align, None);
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::ButtonLinkInvalidAlign]
    );
}

#[test]
fn test_a_tooltip_without_a_value_is_reported() {
    // Given
    let input = ".. button-link:: https://example.com\n   :tooltip:\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(button.tooltip, None);
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::ButtonLinkEmptyOptionValue]
    );
}

#[test]
fn test_an_unknown_option_is_reported_and_the_button_survives() {
    // Given
    let input = ".. button-link:: https://example.com\n   :colour: primary\n\n   Open\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(button.color, None);
    assert!(button.has_label());
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::DirectiveButtonLinkUnknownOption]
    );
}

#[test]
fn test_ref_type_is_refused_by_name() {
    // Given — sphinx-design accepts it here only because both button
    // directives share one option spec
    let input = ".. button-link:: https://example.com\n   :ref-type: myst\n";

    // When
    let (_button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::ButtonLinkUnsupportedOption]
    );
    assert!(
        diagnostics[0].message.contains("button-ref"),
        "the message should say where the option belongs: {}",
        diagnostics[0].message
    );
}

#[test]
fn test_an_outline_without_a_color_is_reported() {
    // Given
    let input = ".. button-link:: https://example.com\n   :outline:\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then — the flag is still recorded; it is the *pair* that draws nothing
    assert!(button.has(ButtonFlag::Outline));
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::ButtonLinkUnusableOutline]
    );
}

#[test]
fn test_an_outline_with_a_color_is_not_reported() {
    // Given
    let input = ".. button-link:: https://example.com\n   :outline:\n   :color: danger\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert!(button.has(ButtonFlag::Outline));
    assert_eq!(button.color, Some(SemanticColor::Danger));
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_reference_in_the_label_is_reported_and_kept() {
    // Given
    let input = ".. button-link:: https://example.com\n\n   See :ref:`the guide`\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then — the reference survives in the node; the renderer flattens it
    assert!(matches!(
        button.label.last(),
        Some(InlineNode::Reference { .. })
    ));
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::ButtonLinkNestedReference]
    );
}

#[test]
fn test_plain_markup_in_the_label_is_not_reported_as_a_nested_link() {
    // Given
    let input = ".. button-link:: https://example.com\n\n   *Now* with ``markup``\n";

    // When
    let (_button, diagnostics) = parse_document(input);

    // Then
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_diagnostic_points_at_the_option_line_that_caused_it() {
    // Given — the bad option is the document's third line
    let input = ".. button-link:: https://example.com\n   :shadow:\n   :color: puce\n";

    // When
    let (_button, diagnostics) = parse_document(input);

    // Then
    let span = diagnostics[0]
        .span
        .expect("the diagnostic should have a span");
    assert_eq!(span.start.line, 3);
}

#[test]
fn test_a_nested_reference_points_at_the_label_line_it_was_written_on() {
    // Given — the role is on the document's fifth line
    let input = ".. button-link:: https://example.com\n   :color: primary\n\n   Read on\n   \
                 and see :ref:`the guide`\n";

    // When
    let (_button, diagnostics) = parse_document(input);

    // Then
    let span = diagnostics[0]
        .span
        .expect("the diagnostic should have a span");
    assert_eq!(span.start.line, 5);
}

#[test]
fn test_a_button_written_inside_a_grid_item_is_parsed() {
    // Given — the shape every corpus use has: a button nested two containers
    // deep, which reaches the parser only because both of them parse a body
    let input = ".. grid::\n\n   .. grid-item::\n\n      .. button-link:: https://example.com\n \
                 \n         Open\n";

    // When
    let (button, diagnostics) = parse_document(input);

    // Then
    assert_eq!(
        button.target,
        ButtonTarget::Url("https://example.com".to_string())
    );
    assert_eq!(button.label, vec![InlineNode::Text("Open".to_string())]);
    assert!(diagnostics.is_empty());
}

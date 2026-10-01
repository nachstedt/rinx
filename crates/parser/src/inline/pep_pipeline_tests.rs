//! End-to-end `parse()` pipeline tests for the `:pep:` role and docutils'
//! `:pep-reference:` beside it.
//!
//! What only the whole pipeline can show: the position the scan records, the
//! anchor ids minted once the document is parsed — shared with `.. index::`
//! and given afresh to each use of a substitution — and a refusal reported
//! at the role and lowered to its source text.

use crate::parse;
use rinx_ast::{
    DiagnosticCode, Directive, DocutilsPepNumber, InlineNode, Node, PepTarget, Position, Span,
};

fn peps(nodes: &[Node]) -> Vec<&InlineNode> {
    let mut found = Vec::new();
    for node in nodes {
        if let Node::Paragraph(inlines) = node {
            found.extend(
                inlines
                    .iter()
                    .filter(|inline| matches!(inline, InlineNode::PepReference { .. })),
            );
        }
    }
    found
}

#[test]
fn test_parse_creates_a_pep_reference_with_its_span_and_anchor() {
    // Given
    let input = "See :pep:`8` here.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("See ".to_string()),
            InlineNode::PepReference {
                target: PepTarget::parse("8").unwrap(),
                display: None,
                index_id: "index-0".to_string(),
                span: Some(Span::new(Position::new(1, 5), Position::new(1, 13))),
            },
            InlineNode::Text(" here.".to_string()),
        ])]
    );
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn test_parse_numbers_pep_anchors_after_index_directives() {
    // Given
    let input = ":pep:`8`\n\n.. index:: single: execution\n\n:pep:`20`\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Directive(Directive::Index { id, .. }) = &doc.nodes[1] else {
        panic!("expected an index directive, got {:?}", doc.nodes[1]);
    };
    assert_eq!(id, "index-0");
    let ids: Vec<&str> = peps(&doc.nodes)
        .into_iter()
        .map(|node| match node {
            InlineNode::PepReference { index_id, .. } => index_id.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(ids, vec!["index-1", "index-2"]);
}

#[test]
fn test_parse_gives_each_substituted_pep_its_own_anchor() {
    // Given
    let input = "|style| and |style|\n\n.. |style| replace:: :pep:`8`\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    let ids: Vec<&str> = inlines
        .iter()
        .filter_map(|node| match node {
            InlineNode::PepReference { index_id, .. } => Some(index_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec!["index-0", "index-1"]);
}

#[test]
fn test_parse_unescapes_the_target_before_reading_the_number() {
    // Given — an escaped digit is still a digit
    let input = r"See :pep:`\8`.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(peps(&doc.nodes).len(), 1, "{:?}", doc.nodes);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn test_parse_reports_an_invalid_pep_number_at_the_role() {
    // Given
    let input = "See :pep:`eight` here.";

    // When
    let doc = parse("test.rst", input);

    // Then — shown as written, reported where it was written
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("See ".to_string()),
            InlineNode::Text(":pep:`eight`".to_string()),
            InlineNode::Text(" here.".to_string()),
        ])]
    );
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].code, DiagnosticCode::PepInvalidNumber);
    assert_eq!(
        doc.diagnostics[0].span,
        Some(Span::new(Position::new(1, 5), Position::new(1, 17)))
    );
}

#[test]
fn test_parse_refuses_a_custom_role_named_pep() {
    // Given
    let input = ".. role:: pep(code)\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(
        doc.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RoleBuiltinName),
        "{:?}",
        doc.diagnostics
    );
}

#[test]
fn test_parse_creates_a_docutils_pep_reference_with_its_span() {
    // Given
    let input = "See :pep-reference:`8` here.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("See ".to_string()),
            InlineNode::DocutilsPepReference {
                number: DocutilsPepNumber::parse("8").unwrap(),
                span: Some(Span::new(Position::new(1, 5), Position::new(1, 23))),
            },
            InlineNode::Text(" here.".to_string()),
        ])]
    );
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn test_parse_mints_no_anchor_for_a_pep_reference_role() {
    // Given — a `:pep-reference:` between two `:pep:`s takes no number
    let input = ":pep:`8` :pep-reference:`8` :pep:`20`";

    // When
    let doc = parse("test.rst", input);

    // Then
    let ids: Vec<&str> = peps(&doc.nodes)
        .into_iter()
        .map(|node| match node {
            InlineNode::PepReference { index_id, .. } => index_id.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(ids, vec!["index-0", "index-1"]);
}

#[test]
fn test_parse_reports_an_invalid_pep_reference_number_at_the_role() {
    // Given
    let input = "See :pep-reference:`8#x` here.";

    // When
    let doc = parse("test.rst", input);

    // Then — shown as written, reported where it was written
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("See ".to_string()),
            InlineNode::Text(":pep-reference:`8#x`".to_string()),
            InlineNode::Text(" here.".to_string()),
        ])]
    );
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(
        doc.diagnostics[0].code,
        DiagnosticCode::PepReferenceInvalidNumber
    );
    assert_eq!(
        doc.diagnostics[0].span,
        Some(Span::new(Position::new(1, 5), Position::new(1, 25)))
    );
}

#[test]
fn test_parse_refuses_a_custom_role_named_pep_reference() {
    // Given
    let input = ".. role:: pep-reference(code)\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(
        doc.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RoleBuiltinName),
        "{:?}",
        doc.diagnostics
    );
}

#[test]
fn test_parse_unescapes_a_pep_reference_number_before_reading_it() {
    // Given — an escaped digit is still a digit
    let input = r"See :pep-reference:`\8`.";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    assert!(
        inlines
            .iter()
            .any(|node| matches!(node, InlineNode::DocutilsPepReference { .. })),
        "{inlines:?}"
    );
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

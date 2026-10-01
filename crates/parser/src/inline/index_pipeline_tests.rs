//! End-to-end `parse()` pipeline tests for the `:index:` role.
//!
//! What only the whole pipeline can show: the span the scan records, the
//! anchor ids minted once the document is parsed — continuing the
//! `.. index::` counter, shared with the registry roles, and given afresh to
//! each use of a substitution — escapes, and a malformed entry reported
//! under the role's own code while its text stays on the page.

use crate::parse;
use rinx_ast::{DiagnosticCode, Directive, IndexEntry, InlineNode, Node, Position, Span};

/// Every `:index:` role in the document's top-level paragraphs.
fn index_references(nodes: &[Node]) -> Vec<&InlineNode> {
    nodes
        .iter()
        .filter_map(|node| match node {
            Node::Paragraph(inlines) => Some(inlines),
            _ => None,
        })
        .flatten()
        .filter(|inline| matches!(inline, InlineNode::IndexReference { .. }))
        .collect()
}

fn index_ids(nodes: &[Node]) -> Vec<&str> {
    index_references(nodes)
        .into_iter()
        .map(|node| match node {
            InlineNode::IndexReference { index_id, .. } => index_id.as_str(),
            _ => unreachable!(),
        })
        .collect()
}

#[test]
fn test_parse_creates_an_index_reference_with_its_span_and_anchor() {
    // Given
    let input = "The :index:`execution` model.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("The ".to_string()),
            InlineNode::IndexReference {
                title: "execution".to_string(),
                entries: vec![IndexEntry::Term {
                    primary: "execution".to_string(),
                    subentry: None,
                    main: false,
                }],
                index_id: "index-0".to_string(),
                span: Some(Span::new(Position::new(1, 5), Position::new(1, 23))),
            },
            InlineNode::Text(" model.".to_string()),
        ])]
    );
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn test_parse_numbers_index_role_anchors_after_index_directives_and_among_registry_roles() {
    // Given
    let input = ":index:`a`\n\n.. index:: single: b\n\n:pep:`8` and :index:`c`\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Directive(Directive::Index { id, .. }) = &doc.nodes[1] else {
        panic!("expected an index directive, got {:?}", doc.nodes[1]);
    };
    assert_eq!(id, "index-0");
    assert_eq!(index_ids(&doc.nodes), vec!["index-1", "index-3"]);
}

#[test]
fn test_parse_gives_each_substituted_index_role_its_own_anchor() {
    // Given
    let input = "|term| and |term|\n\n.. |term| replace:: :index:`execution`\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(index_ids(&doc.nodes), vec!["index-0", "index-1"]);
}

#[test]
fn test_parse_unescapes_the_title_and_the_entry() {
    // Given
    let input = r"A :index:`\*args` parameter.";

    // When
    let doc = parse("test.rst", input);

    // Then
    let found = index_references(&doc.nodes);
    let [InlineNode::IndexReference { title, entries, .. }] = found.as_slice() else {
        panic!("expected one index reference, got {:?}", doc.nodes);
    };
    assert_eq!(title, "*args");
    assert_eq!(
        entries,
        &vec![IndexEntry::Term {
            primary: "*args".to_string(),
            subentry: None,
            main: false,
        }]
    );
}

#[test]
fn test_parse_reports_a_malformed_entry_under_the_role_code_and_keeps_the_text() {
    // Given
    let input = "See :index:`loops <pair: loop>` here.";

    // When
    let doc = parse("test.rst", input);

    // Then — the text and anchor stay, with no entries
    assert_eq!(
        index_references(&doc.nodes),
        vec![&InlineNode::IndexReference {
            title: "loops".to_string(),
            entries: Vec::new(),
            index_id: "index-0".to_string(),
            span: Some(Span::new(Position::new(1, 5), Position::new(1, 32))),
        }]
    );
    assert_eq!(doc.diagnostics.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(
        doc.diagnostics[0].code,
        DiagnosticCode::IndexRoleInvalidPair
    );
    assert_eq!(
        doc.diagnostics[0].span,
        Some(Span::new(Position::new(1, 5), Position::new(1, 32)))
    );
}

#[test]
fn test_parse_reports_the_directive_family_for_a_malformed_index_directive() {
    // Given — the same entry, written as a directive
    let input = ".. index:: pair: loop\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.diagnostics.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(doc.diagnostics[0].code, DiagnosticCode::IndexInvalidPair);
}

#[test]
fn test_parse_suppresses_an_index_role_diagnostic_with_its_own_code() {
    // Given
    let input = ".. noqa: index-role.invalid-pair\n\nSee :index:`loops <pair: loop>` here.\n";

    // When
    let doc = parse("test.rst", input);

    // Then — the suppression is recorded against the role's code
    assert_eq!(doc.diagnostics.len(), 1);
    assert!(
        doc.suppressions
            .iter()
            .any(|suppression| suppression
                .suppresses(doc.diagnostics[0].code, doc.diagnostics[0].span)),
        "{:?}",
        doc.suppressions
    );
}

#[test]
fn test_parse_leaves_an_index_role_in_a_heading_in_the_heading() {
    // Given
    let input = "The :index:`loop` statement\n===========================\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Heading { text, .. } = &doc.nodes[0] else {
        panic!("expected a heading, got {:?}", doc.nodes);
    };
    assert_eq!(rinx_ast::inline_plain_text(text), "The loop statement");
}

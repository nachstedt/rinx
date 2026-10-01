//! End-to-end `parse()` pipeline tests for the registry roles — `:pep:`,
//! `:rfc:`, `:cve:`, `:cwe:` — and docutils' `:pep-reference:` and
//! `:rfc-reference:` beside them.
//!
//! What only the whole pipeline can show: the position the scan records, the
//! anchor ids minted once the document is parsed — shared with `.. index::`
//! and given afresh to each use of a substitution — and a refusal reported
//! at the role and lowered to its source text.

use crate::parse;
use rinx_ast::{
    DiagnosticCode, Directive, DocutilsPepNumber, DocutilsRfcNumber, InlineNode, Node, Position,
    Registry, RegistryTarget, Span,
};

fn registry_references(nodes: &[Node]) -> Vec<&InlineNode> {
    let mut found = Vec::new();
    for node in nodes {
        if let Node::Paragraph(inlines) = node {
            found.extend(
                inlines
                    .iter()
                    .filter(|inline| matches!(inline, InlineNode::RegistryReference { .. })),
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
            InlineNode::RegistryReference {
                target: RegistryTarget::parse(Registry::Pep, "8").unwrap(),
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
    let ids: Vec<&str> = registry_references(&doc.nodes)
        .into_iter()
        .map(|node| match node {
            InlineNode::RegistryReference { index_id, .. } => index_id.as_str(),
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
            InlineNode::RegistryReference { index_id, .. } => Some(index_id.as_str()),
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
    assert_eq!(registry_references(&doc.nodes).len(), 1, "{:?}", doc.nodes);
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
    let ids: Vec<&str> = registry_references(&doc.nodes)
        .into_iter()
        .map(|node| match node {
            InlineNode::RegistryReference { index_id, .. } => index_id.as_str(),
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

#[test]
fn test_parse_creates_a_reference_for_each_registry() {
    // Given
    let input = ":rfc:`2324#section-2` :cve:`2024-3094` :cwe:`Overflow <787>`";

    // When
    let doc = parse("test.rst", input);

    // Then — one anchor sequence across all registries
    let found: Vec<(&RegistryTarget, Option<&str>, &str)> = registry_references(&doc.nodes)
        .into_iter()
        .map(|node| match node {
            InlineNode::RegistryReference {
                target,
                display,
                index_id,
                ..
            } => (target, display.as_deref(), index_id.as_str()),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        found,
        vec![
            (
                &RegistryTarget::parse(Registry::Rfc, "2324#section-2").unwrap(),
                None,
                "index-0"
            ),
            (
                &RegistryTarget::parse(Registry::Cve, "2024-3094").unwrap(),
                None,
                "index-1"
            ),
            (
                &RegistryTarget::parse(Registry::Cwe, "787").unwrap(),
                Some("Overflow"),
                "index-2"
            ),
        ]
    );
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn test_parse_reports_each_registry_refusal_under_its_own_code() {
    // Given
    let input = ":rfc:`HTTP` :cve:`CVE-2024-3094` :cwe:`CWE-787`";

    // When
    let doc = parse("test.rst", input);

    // Then — each shown as written
    let codes: Vec<DiagnosticCode> = doc.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![
            DiagnosticCode::RfcInvalidNumber,
            DiagnosticCode::CveInvalidId,
            DiagnosticCode::CweInvalidNumber,
        ]
    );
    assert!(
        registry_references(&doc.nodes).is_empty(),
        "{:?}",
        doc.nodes
    );
    assert_eq!(
        doc.diagnostics[1].span,
        Some(Span::new(Position::new(1, 13), Position::new(1, 33)))
    );
}

#[test]
fn test_parse_refuses_custom_roles_named_after_the_registries() {
    // Given / When / Then
    for name in ["rfc", "cve", "cwe", "rfc-reference"] {
        let doc = parse("test.rst", &format!(".. role:: {name}(code)\n"));
        assert!(
            doc.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::RoleBuiltinName),
            "{name}: {:?}",
            doc.diagnostics
        );
    }
}

#[test]
fn test_parse_creates_a_docutils_rfc_reference_without_an_anchor() {
    // Given
    let input = ":rfc:`1` :rfc-reference:`2822#section-3` :rfc:`2`";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    assert_eq!(
        inlines[2],
        InlineNode::DocutilsRfcReference {
            number: DocutilsRfcNumber::parse("2822#section-3").unwrap(),
            span: Some(Span::new(Position::new(1, 10), Position::new(1, 41))),
        }
    );
    let ids: Vec<&str> = registry_references(&doc.nodes)
        .into_iter()
        .map(|node| match node {
            InlineNode::RegistryReference { index_id, .. } => index_id.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(ids, vec!["index-0", "index-1"]);
}

#[test]
fn test_parse_reports_an_invalid_rfc_reference_number_at_the_role() {
    // Given
    let input = "See :rfc-reference:`0` here.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("See ".to_string()),
            InlineNode::Text(":rfc-reference:`0`".to_string()),
            InlineNode::Text(" here.".to_string()),
        ])]
    );
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(
        doc.diagnostics[0].code,
        DiagnosticCode::RfcReferenceInvalidNumber
    );
}

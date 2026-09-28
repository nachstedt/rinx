//! End-to-end `parse()` pipeline tests for `:code:` and the roles a
//! `.. role:: name(code)` derives from it.
//!
//! A sibling of `pipeline_tests.rs` rather than part of it, which had already
//! outgrown the size a file is split at. What only the whole pipeline can
//! show lives here: that a role applies from its definition onwards, how the
//! escape rewrite treats code, and the positions the scan records.

use crate::parse;
use rinx_ast::{DiagnosticCode, InlineNode, Node, Position, ResolvedLanguage, Span};

fn code(text: &str, language: ResolvedLanguage, classes: &[&str], span: Span) -> InlineNode {
    InlineNode::Code {
        text: text.to_string(),
        language,
        classes: classes.iter().map(ToString::to_string).collect(),
        span: Some(span),
    }
}

fn at(line: u32, start: u32, end: u32) -> Span {
    Span::new(Position::new(line, start), Position::new(line, end))
}

fn python() -> ResolvedLanguage {
    ResolvedLanguage::parse("python").unwrap()
}

#[test]
fn test_parse_creates_a_code_node_for_the_code_role() {
    // Given
    let input = "Write :code:`x = 1` here.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("Write ".to_string()),
            code("x = 1", ResolvedLanguage::None, &[], at(1, 7, 20)),
            InlineNode::Text(" here.".to_string()),
        ])]
    );
}

#[test]
fn test_parse_treats_backslashes_in_code_as_escapes() {
    // Given — measured against Sphinx: `\*` shows `*`, `\\` one backslash
    let input = r"A :code:`a\*b \\ c` d.";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    assert!(
        matches!(&inlines[1], InlineNode::Code { text, .. } if text == r"a*b \ c"),
        "{inlines:?}"
    );
}

#[test]
fn test_parse_does_not_apply_smart_typography_inside_code() {
    // Given
    let input = r#"A :code:`"quoted" -- x` d."#;

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    assert!(
        matches!(&inlines[1], InlineNode::Code { text, .. } if text == r#""quoted" -- x"#),
        "{inlines:?}"
    );
}

#[test]
fn test_parse_applies_a_defined_role_after_its_definition() {
    // Given
    let input = "\
.. role:: py(code)
   :language: python

Call :py:`print(1)`.
";

    // When
    let doc = parse("test.rst", input);

    // Then — the definition leaves no node; the use becomes highlighted code
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("Call ".to_string()),
            code("print(1)", python(), &["py"], at(4, 6, 20)),
            InlineNode::Text(".".to_string()),
        ])]
    );
}

#[test]
fn test_parse_leaves_a_role_used_before_its_definition_as_text() {
    // Given — docutils applies a role only from its definition onwards
    let input = "\
Early :py:`x`.

.. role:: py(code)

Late :py:`y`.
";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![
            InlineNode::Text("Early ".to_string()),
            InlineNode::Text(":py:`x`".to_string()),
            InlineNode::Text(".".to_string()),
        ])
    );
    let Node::Paragraph(late) = &doc.nodes[1] else {
        panic!("expected a paragraph");
    };
    assert!(matches!(&late[1], InlineNode::Code { .. }), "{late:?}");
}

#[test]
fn test_parse_applies_a_role_in_a_later_nested_block() {
    // Given a role used inside a list and a heading after its definition
    let input = "\
.. role:: py(code)

Title :py:`t`
=============

* item :py:`i`
";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Heading { text, .. } = &doc.nodes[0] else {
        panic!("expected a heading, got {:?}", doc.nodes[0]);
    };
    assert!(matches!(&text[1], InlineNode::Code { .. }), "{text:?}");
    let rendered = format!("{:?}", doc.nodes[1]);
    assert!(rendered.contains("Code"), "{rendered}");
}

#[test]
fn test_parse_role_names_are_case_insensitive() {
    // Given
    let input = "\
.. role:: Py(code)

Use :PY:`x`.
";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    assert!(
        matches!(&inlines[1], InlineNode::Code { .. }),
        "{inlines:?}"
    );
}

#[test]
fn test_parse_does_not_carry_a_role_into_another_document() {
    // Given one document defining a role, and another using it
    let first = parse("a.rst", ".. role:: py(code)\n\nIn :py:`a`.\n");

    // When
    let second = parse("b.rst", "In :py:`b`.\n");

    // Then — Sphinx forgets a role when its document ends
    let rendered = format!("{:?}", first.nodes);
    assert!(rendered.contains("Code"), "{rendered}");
    assert_eq!(
        second.nodes,
        vec![Node::Paragraph(vec![
            InlineNode::Text("In ".to_string()),
            InlineNode::Text(":py:`b`".to_string()),
            InlineNode::Text(".".to_string()),
        ])]
    );
}

#[test]
fn test_parse_reports_a_refused_role_on_its_directive_line() {
    // Given
    let input = "Intro.\n\n.. role:: red\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].code, DiagnosticCode::RoleUnsupportedBase);
    assert_eq!(doc.diagnostics[0].span.map(|span| span.start.line), Some(3));
}

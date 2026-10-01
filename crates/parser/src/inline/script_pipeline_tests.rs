//! End-to-end `parse()` pipeline tests for `:sub:`/`:subscript:`,
//! `:sup:`/`:superscript:` and the roles a `.. role:: name(sub)` derives from
//! them.
//!
//! What only the whole pipeline can show lives here: that the escaped-space
//! idiom joins a script to the word around it, how the escape rewrite treats
//! its text, and that a heading or a substitution carries one through.

use crate::parse;
use rinx_ast::{DiagnosticCode, InlineNode, Node, ScriptPosition};

fn script(position: ScriptPosition, text: &str, classes: &[&str]) -> InlineNode {
    InlineNode::Script {
        position,
        text: text.to_string(),
        classes: classes.iter().map(ToString::to_string).collect(),
    }
}

fn text(text: &str) -> InlineNode {
    InlineNode::Text(text.to_string())
}

/// The inline content of the document's only paragraph.
fn paragraph(input: &str) -> Vec<InlineNode> {
    let doc = parse("test.rst", input);
    let paragraphs: Vec<_> = doc
        .nodes
        .into_iter()
        .filter_map(|node| match node {
            Node::Paragraph(inlines) => Some(inlines),
            _ => None,
        })
        .collect();
    assert_eq!(paragraphs.len(), 1, "{paragraphs:?}");
    paragraphs.into_iter().next().unwrap()
}

#[test]
fn test_parse_joins_a_subscript_written_with_escaped_spaces() {
    // Given — docutils' idiom for markup inside a word
    let input = r"Water is H\ :sub:`2`\ O.";

    // When
    let inlines = paragraph(input);

    // Then — the escaped spaces disappear
    assert_eq!(
        inlines,
        vec![
            text("Water is H"),
            script(ScriptPosition::Subscript, "2", &[]),
            text("O."),
        ]
    );
}

#[test]
fn test_parse_reads_a_superscript_at_the_end_of_a_word() {
    // Given
    let input = r"E = mc\ :superscript:`2`";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(
        inlines,
        vec![
            text("E = mc"),
            script(ScriptPosition::Superscript, "2", &[]),
        ]
    );
}

#[test]
fn test_parse_unescapes_the_text_of_a_script() {
    // Given — an escaped asterisk, which would otherwise be markup elsewhere
    let input = r"x\ :sup:`\*` here";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(inlines[1], script(ScriptPosition::Superscript, "*", &[]));
}

#[test]
fn test_parse_leaves_a_script_role_in_other_case_as_text() {
    // Given — role names are case-sensitive
    let input = "A :Sub:`2` here.";

    // When
    let inlines = paragraph(input);

    // Then
    assert!(
        inlines
            .iter()
            .all(|node| !matches!(node, InlineNode::Script { .. })),
        "{inlines:?}"
    );
}

#[test]
fn test_parse_keeps_a_script_in_a_heading() {
    // Given
    let input = "CO\\ :sub:`2` levels\n==================\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![
                text("CO"),
                script(ScriptPosition::Subscript, "2", &[]),
                text(" levels"),
            ],
        }
    );
}

#[test]
fn test_parse_substitutes_a_script() {
    // Given
    let input = "Square |m2|.\n\n.. |m2| replace:: m\\ :sup:`2`\n";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(
        inlines,
        vec![
            text("Square "),
            text("m"),
            script(ScriptPosition::Superscript, "2", &[]),
            text("."),
        ]
    );
}

#[test]
fn test_parse_builds_a_role_derived_from_sub() {
    // Given
    let input = ".. role:: chem(sub)\n\nH\\ :chem:`2`\\ O\n";

    // When
    let doc = parse("test.rst", input);

    // Then — classed by its own name
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            text("H"),
            script(ScriptPosition::Subscript, "2", &["chem"]),
            text("O"),
        ])]
    );
}

#[test]
fn test_parse_builds_a_role_derived_from_superscript_with_its_class() {
    // Given
    let input = ".. role:: power(Superscript)\n   :class: exponent\n\n10\\ :power:`3`\n";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(
        inlines,
        vec![
            text("10"),
            script(ScriptPosition::Superscript, "3", &["exponent"]),
        ]
    );
}

#[test]
fn test_parse_refuses_a_role_named_like_a_script_role() {
    // Given
    let input = ".. role:: sup(code)\n\nx :sup:`2`\n";

    // When
    let doc = parse("test.rst", input);

    // Then — refused, and `:sup:` stays the built-in role
    let codes: Vec<_> = doc.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::RoleBuiltinName]);
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            text("x "),
            script(ScriptPosition::Superscript, "2", &[]),
        ])]
    );
}

//! End-to-end `parse()` tests for the block level: headings, paragraphs,
//! targets, literal blocks, doctest blocks and transitions, and how they
//! interact. Kept separate from [`super::dispatch`]'s own unit tests purely
//! for file size.

use crate::parse;
use rusty_sphinx_ast::TargetName;
use rusty_sphinx_ast::{CodeLanguage, InlineNode, Node};

#[test]
fn test_parse_returns_empty_document_for_empty_input() {
    // Given
    let input = "";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 0);
}

#[test]
fn test_parse_creates_paragraph_node() {
    // Given
    let input = "Just some\ntext";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text("Just some\ntext".to_string())])
    );
}

#[test]
fn test_parse_creates_mixed_nodes_for_heading_and_paragraph() {
    // Given
    let input = "Title\n=====\n\nText.";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Title".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Paragraph(vec![InlineNode::Text("Text.".to_string())])
    );
}

#[test]
fn test_parse_creates_paragraph_for_shorter_underline() {
    // Given
    let input = "Long Heading\n===";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text("Long Heading\n===".to_string())])
    );
}

#[test]
fn test_parse_ignores_surrounding_whitespace_for_heading() {
    // Given
    let input = "Heading  \n  =======  \n\nNext";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Heading".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Paragraph(vec![InlineNode::Text("Next".to_string())])
    );
}

#[test]
fn test_parse_creates_multiple_paragraphs_ignoring_blank_lines() {
    // Given
    let input = "Para 1\n\n\nPara 2\n\nPara 3";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 3);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text("Para 1".to_string())])
    );
    assert_eq!(
        doc.nodes[1],
        Node::Paragraph(vec![InlineNode::Text("Para 2".to_string())])
    );
    assert_eq!(
        doc.nodes[2],
        Node::Paragraph(vec![InlineNode::Text("Para 3".to_string())])
    );
}

#[test]
fn test_parse_handles_carriage_returns_gracefully() {
    // Given
    let input = "Heading\r\n=======\r\n\r\nPara\r\nline 2";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Heading".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Paragraph(vec![InlineNode::Text("Para\nline 2".to_string())])
    );
}

#[test]
fn test_parse_creates_target_node_for_explicit_target() {
    // Given
    let input = ".. _my-target:\n\nSome text.";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Target {
            name: TargetName::new("my-target"),
            uri: None
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Paragraph(vec![InlineNode::Text("Some text.".to_string())])
    );
}

#[test]
fn test_parse_creates_external_target_node() {
    // Given
    let input = ".. _my-link: https://example.com\n\nSome text.";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Target {
            name: TargetName::new("my-link"),
            uri: Some("https://example.com".to_string())
        }
    );
}

#[test]
fn test_parse_creates_indented_external_target_node() {
    // Given
    let input = ".. _my-link:\n   https://example.com\n\nSome text.";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Target {
            name: TargetName::new("my-link"),
            uri: Some("https://example.com".to_string())
        }
    );
}

#[test]
fn test_parse_paragraph_breaks_at_overline() {
    // Given — an adornment of fewer than four characters, so the paragraph
    // line above it cannot itself be read as a too-short-underlined heading
    // (see `headings.rs`) and the overlined form below is the only heading.
    let input = "Para text.\n###\nHi\n###";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text("Para text.".to_string())])
    );
    assert_eq!(
        doc.nodes[1],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Hi".to_string())]
        }
    );
}

#[test]
fn test_parse_keeps_a_literal_block_containing_prompts_literal() {
    // Given — THE case this feature must not break. A `::`-introduced
    // block is a literal block in docutils, never a doctest block, and
    // Sphinx does not execute it. In the CPython corpus 1711 nodes have
    // this shape against 448 real doctest blocks, so a detector that fired
    // here would make a great deal of illustrative code suddenly runnable.
    let input = "Example::\n\n    >>> 1 + 1\n    2\n";

    // When
    let doc = parse("test.rst", input);

    // Then — paragraph plus LiteralBlock; no DoctestBlock anywhere.
    assert_eq!(doc.nodes.len(), 2);
    assert!(matches!(doc.nodes[1], Node::LiteralBlock { .. }));
    assert!(!doc.nodes.iter().any(|n| matches!(n, Node::DoctestBlock(_))));
}

#[test]
fn test_parse_creates_a_doctest_block_after_a_single_colon() {
    // Given — the real-world shape: one colon, so a block quote rather
    // than a literal block, and the indented run is a doctest block.
    let input = "Use search rather than match:\n\n   >>> import re\n   >>> re.search('a', 'ba')\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 2);
    match &doc.nodes[1] {
        Node::BlockQuote {
            content,
            attribution: None,
        } => {
            assert_eq!(content.len(), 1);
            match &content[0] {
                Node::DoctestBlock(inner) => {
                    assert_eq!(inner.body(), ">>> import re\n>>> re.search('a', 'ba')");
                }
                other => panic!("expected a doctest block, got {other:?}"),
            }
        }
        other => panic!("expected a block quote, got {other:?}"),
    }
}

#[test]
fn test_parse_creates_a_doctest_block_at_the_left_margin() {
    // Given
    let input = ">>> 1 + 1\n2\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert!(matches!(doc.nodes[0], Node::DoctestBlock(_)));
}

#[test]
fn test_parse_leaves_a_mid_paragraph_prompt_as_prose() {
    // Given — a doctest block has to *start* a text block, in docutils and
    // here alike; 3 CPython paragraphs rely on this.
    let input = "Some prose.\n>>> f()\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert!(matches!(doc.nodes[0], Node::Paragraph(_)));
}

#[test]
fn test_parse_keeps_a_transition_a_transition() {
    // Given — `>>>>` is four repeated punctuation characters.
    let input = "Before.\n\n>>>>\n\nAfter.\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(doc.nodes.iter().any(|n| matches!(n, Node::Transition)));
    assert!(!doc.nodes.iter().any(|n| matches!(n, Node::DoctestBlock(_))));
}

#[test]
fn test_parse_finds_a_doctest_block_nested_in_an_admonition() {
    // Given — directive bodies are parsed recursively, so bare blocks
    // inside them are found too (and `walk_nodes` reaches them later).
    let input = ".. note::\n\n   Try it:\n\n   >>> 1 + 1\n   2\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let mut found = false;
    rusty_sphinx_ast::walk_nodes(&doc.nodes, &mut |node| {
        if matches!(node, Node::DoctestBlock(_)) {
            found = true;
        }
    });
    assert!(found, "expected a doctest block inside the admonition");
}

#[test]
fn test_parse_separates_two_doctest_blocks_by_a_blank_line() {
    // Given
    let input = ">>> a = 1\n\n>>> b = 2\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert!(doc.nodes.iter().all(|n| matches!(n, Node::DoctestBlock(_))));
}

#[test]
fn test_parse_double_colon_paragraph_emits_literal_block() {
    // Given: a paragraph ending with :: followed by an indented block
    let input = "Here is some code::\n\n    def hello():\n        pass\n";
    // When
    let doc = parse("test.rst", input);
    // Then: two nodes — paragraph (with :: reduced to :) and LiteralBlock
    assert_eq!(doc.nodes.len(), 2);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let text = match &inlines[0] {
            InlineNode::Text(t) => t.as_str(),
            other => panic!("Expected Text inline, got {other:?}"),
        };
        assert_eq!(text, "Here is some code:");
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
    if let Node::LiteralBlock { language, content } = &doc.nodes[1] {
        assert_eq!(language, &CodeLanguage::Inherit);
        assert_eq!(content, "def hello():\n    pass");
    } else {
        panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
    }
}

#[test]
fn test_parse_standalone_double_colon_suppresses_paragraph() {
    // Given: a line of only "::" introduces a literal block with no visible paragraph
    let input = "::\n\n    verbatim content\n";
    // When
    let doc = parse("test.rst", input);
    // Then: only the LiteralBlock is emitted (no paragraph)
    assert_eq!(doc.nodes.len(), 1);
    if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
        assert_eq!(language, &CodeLanguage::Inherit);
        assert_eq!(content, "verbatim content");
    } else {
        panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
    }
}

#[test]
fn test_parse_double_colon_strips_to_single_colon() {
    // Given: text followed by "::" — the "::" becomes ":"
    let input = "Example::\n\n    content\n";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let text = match &inlines[0] {
            InlineNode::Text(t) => t.as_str(),
            other => panic!("Expected Text, got {other:?}"),
        };
        assert_eq!(text, "Example:");
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}

#[test]
fn test_parse_literal_block_preserves_internal_blank_lines() {
    // Given: blank lines inside the block must be kept
    let input = "Example::\n\n    line one\n\n    line three\n";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 2);
    if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
        assert_eq!(content, "line one\n\nline three");
    } else {
        panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
    }
}

#[test]
fn test_parse_literal_block_strips_common_indentation() {
    // Given: all lines indented 4 spaces; inner block adds 4 more
    let input = "Example::\n\n    outer\n        inner\n    outer again\n";
    // When
    let doc = parse("test.rst", input);
    // Then: 4 spaces stripped from all lines; inner keeps its extra 4
    assert_eq!(doc.nodes.len(), 2);
    if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
        assert_eq!(content, "outer\n    inner\nouter again");
    } else {
        panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
    }
}

#[test]
fn test_parse_bodyless_index_directive_inside_glossary_does_not_swallow_following_paragraph() {
    // Given — mirrors real CPython usage (Doc/glossary.rst): a bodyless
    // `.. index::` directive nested inside a glossary term's definition,
    // immediately followed by a plain paragraph at the SAME indentation
    // (not a deeper one). Before the indentation-depth fix, the
    // paragraph was swallowed into the directive's body and mis-parsed
    // as bogus index entries.
    let input = "\
.. glossary::

   magic method
      .. index:: pair: magic; method

      An informal synonym for something.
";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(doc.nodes.len(), 1);
    let Node::Directive(rusty_sphinx_ast::Directive::Glossary { entries, .. }) = &doc.nodes[0]
    else {
        panic!("Expected Glossary directive, got {:?}", doc.nodes[0]);
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].terms, vec!["magic method".to_string()]);
    // The definition must contain the Index directive AND the paragraph
    // as two separate sibling nodes — not one node with the paragraph's
    // text corrupted into bogus index entries.
    assert_eq!(entries[0].definition.len(), 2);
    assert!(matches!(
        entries[0].definition[0],
        Node::Directive(rusty_sphinx_ast::Directive::Index { .. })
    ));
    assert_eq!(
        entries[0].definition[1],
        Node::Paragraph(vec![InlineNode::Text(
            "An informal synonym for something.".to_string()
        )])
    );
    // No diagnostics about invalid/unknown index entries should be emitted.
    assert!(
        !doc.diagnostics
            .iter()
            .any(|d| d.message.contains(".. index::") || d.message.contains("index:"))
    );
}

#[test]
fn test_parse_reads_a_math_directive_with_a_label() {
    // Given a labeled display equation
    let input = ".. math::\n   :label: euler\n\n   e^{i\\pi} + 1 = 0\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let rusty_sphinx_ast::Node::Directive(rusty_sphinx_ast::Directive::Math {
        parts,
        label,
        nowrap,
        ..
    }) = &doc.nodes[0]
    else {
        panic!("expected a math directive, got {:?}", doc.nodes[0]);
    };
    assert_eq!(parts, &vec!["e^{i\\pi} + 1 = 0".to_string()]);
    assert_eq!(label.as_ref(), Some(&TargetName::new("euler")));
    assert!(!nowrap);
}

#[test]
fn test_parse_reads_an_inline_math_role_keeping_its_backslashes() {
    // Given inline math whose LaTeX is nothing but escapes — the case where
    // the escape-marker round trip would silently eat the content
    let input = "The angle :math:`\\alpha \\\\ \\beta` matters.\n";

    // When
    let doc = parse("test.rst", input);

    // Then the backslashes survive verbatim, as they do in an inline literal
    let Node::Paragraph(nodes) = &doc.nodes[0] else {
        panic!("expected a paragraph, got {:?}", doc.nodes[0]);
    };
    assert!(
        nodes.iter().any(|node| matches!(
            node,
            InlineNode::Math { latex, .. } if latex == "\\alpha \\\\ \\beta"
        )),
        "{nodes:?}"
    );
}

#[test]
fn test_parse_reads_an_eq_role() {
    // Given a reference to a labeled equation
    let input = "See :eq:`euler` above.\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let Node::Paragraph(nodes) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    assert!(
        nodes.iter().any(|node| matches!(
            node,
            InlineNode::EquationReference { label, .. } if label == "euler"
        )),
        "{nodes:?}"
    );
}

#[test]
fn test_parse_does_not_recognize_a_math_role_inside_an_inline_literal() {
    // Given the role spelled out inside an inline literal
    let input = "Write ``:math:`x``` to show the markup.\n";

    // When
    let doc = parse("test.rst", input);

    // Then it stays literal text, not an equation
    let Node::Paragraph(nodes) = &doc.nodes[0] else {
        panic!("expected a paragraph");
    };
    assert!(
        !nodes
            .iter()
            .any(|node| matches!(node, InlineNode::Math { .. })),
        "{nodes:?}"
    );
}

#[test]
fn test_parse_places_a_math_directive_nested_in_an_admonition() {
    // Given an equation inside a note, so the body was dedented before parsing
    let input = ".. note::\n\n   .. math::\n\n      a = b\n";

    // When
    let doc = parse("test.rst", input);

    // Then the span points at the nested directive's real line (3), not line 1
    let rusty_sphinx_ast::Node::Directive(rusty_sphinx_ast::Directive::Admonition { body, .. }) =
        &doc.nodes[0]
    else {
        panic!("expected an admonition, got {:?}", doc.nodes[0]);
    };
    let rusty_sphinx_ast::Node::Directive(rusty_sphinx_ast::Directive::Math { span, .. }) =
        &body[0]
    else {
        panic!("expected a nested math directive, got {:?}", body[0]);
    };
    assert_eq!(
        span.expect("a nested math directive should carry a span")
            .start
            .line,
        5
    );
}

#[test]
fn test_parse_points_a_math_directive_span_past_its_option_lines() {
    // Given a labeled equation: the body opens with `:label:` on line 5, and
    // the equation itself is on line 7
    let input = "T\n=\n\n.. math::\n   :label: euler\n\n   a = b\n";

    // When
    let doc = parse("test.rst", input);

    // Then the span names the equation's line, not the option line above it —
    // a LaTeX error is about the LaTeX
    let rusty_sphinx_ast::Node::Directive(rusty_sphinx_ast::Directive::Math { span, .. }) =
        &doc.nodes[1]
    else {
        panic!("expected a math directive, got {:?}", doc.nodes[1]);
    };
    let span = span.expect("a math directive should carry a span");
    assert_eq!(span.start.line, 7);
    assert_eq!(span.start.column, 4);
    // ... and ends at the end of `a = b`, not one indent width further
    assert_eq!(span.end.column, 9);
}

#[test]
fn test_parse_points_a_math_directive_span_at_the_first_of_several_equations() {
    // Given two equations after an option line
    let input = ".. math::\n   :label: pair\n\n   a = b\n\n   c = d\n";

    // When
    let doc = parse("test.rst", input);

    // Then the span names the first equation's line
    let rusty_sphinx_ast::Node::Directive(rusty_sphinx_ast::Directive::Math { span, .. }) =
        &doc.nodes[0]
    else {
        panic!("expected a math directive");
    };
    assert_eq!(
        span.expect("a math directive should carry a span")
            .start
            .line,
        4
    );
}

#[test]
fn test_parse_recognizes_a_substitution_definition_and_reference_end_to_end() {
    // Given — the definition follows its (only) use, the common real-world
    // shape, and what the benchmark corpus mis-tallied as an unsupported
    // `|release| replace` directive before this feature existed.
    let input = "Version |release| is current.\n\n.. |release| replace:: 3.13.0\n";

    // When
    let doc = parse("test.rst", input);

    // Then — the reference is spliced with the definition's resolved
    // content, and the definition itself produces no separate visible node.
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![
            InlineNode::Text("Version ".to_string()),
            InlineNode::Text("3.13.0".to_string()),
            InlineNode::Text(" is current.".to_string()),
        ])
    );
    assert!(doc.diagnostics.is_empty());
}

#[test]
fn test_parse_reports_an_undefined_substitution_reference() {
    // Given
    let input = "No |such-thing| here.\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![rusty_sphinx_ast::DiagnosticCode::SubstitutionUndefined]
    );
}

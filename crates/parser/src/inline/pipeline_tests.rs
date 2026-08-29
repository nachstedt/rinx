//! End-to-end `parse()` pipeline tests for general inline markup —
//! emphasis/strong/literal, hyperlinks, typography, and the non-domain-object
//! roles (`:ref:`/`:term:`/`:option:`/`:program:`). Kept as its own file
//! purely for `super::inline`'s line count: every test here exercises the
//! whole pipeline, not any one function split out of it, so none of the
//! split-out sibling modules is a better home than this one.

use crate::parse;
use rusty_sphinx_ast::{InlineNode, Node};

#[test]
fn test_parse_creates_inline_text_and_reference_nodes_for_paragraph() {
    let input = "Here is a :ref:`my-target` link.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![
            InlineNode::Text("Here is a ".to_string()),
            InlineNode::Reference {
                display: "my-target".to_string(),
                target: "my-target".to_string(),
            },
            InlineNode::Text(" link.".to_string()),
        ])
    );
}
#[test]
fn test_parse_creates_phrased_hyperlink_node() {
    let input = "Check the `Python Guide`_ for more.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![
            InlineNode::Text("Check the ".to_string()),
            InlineNode::Hyperlink {
                text: "Python Guide".to_string(),
                target: "Python Guide".to_string()
            },
            InlineNode::Text(" for more.".to_string()),
        ])
    );
}
#[test]
fn test_parse_creates_embedded_uri_hyperlink_node() {
    let input = "Check `Google <https://google.com>`_ now.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![
            InlineNode::Text("Check ".to_string()),
            InlineNode::Hyperlink {
                text: "Google".to_string(),
                target: "https://google.com".to_string()
            },
            InlineNode::Text(" now.".to_string()),
        ])
    );
}
#[test]
fn test_parse_creates_simple_link_node() {
    let input = "Refer to target_ for details.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![
            InlineNode::Text("Refer to ".to_string()),
            InlineNode::Hyperlink {
                text: "target".to_string(),
                target: "target".to_string()
            },
            InlineNode::Text(" for details.".to_string()),
        ])
    );
}
#[test]
fn test_parse_does_not_treat_snake_case_word_as_hyperlink() {
    // Given: a plain identifier with underscores but no trailing one
    let input = "Use my_variable_name in code.";
    // When
    let doc = parse("test.rst", input);
    // Then: the whole sentence stays a single Text node, no Hyperlink
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text(
            "Use my_variable_name in code.".to_string()
        )])
    );
}
#[test]
fn test_parse_does_not_treat_multi_underscore_word_as_hyperlink() {
    // Given: several internal underscores but no trailing underscore
    let input = "call foo_bar_baz here";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text("call foo_bar_baz here".to_string())])
    );
}
#[test]
fn test_parse_creates_simple_link_node_for_target_name_containing_underscore() {
    // Given: the target name itself contains an underscore, and ends
    // with the triggering trailing underscore
    let input = "See my_target_ here.";
    // When
    let doc = parse("test.rst", input);
    // Then: still recognized as a reference to "my_target"
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![
            InlineNode::Text("See ".to_string()),
            InlineNode::Hyperlink {
                text: "my_target".to_string(),
                target: "my_target".to_string()
            },
            InlineNode::Text(" here.".to_string()),
        ])
    );
}
#[test]
fn test_parse_does_not_treat_word_with_underscores_as_anonymous_reference() {
    // Given: no trailing double underscore
    let input = "word_with_underscores stays text";
    // When
    let doc = parse("test.rst", input);
    // Then
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text(
            "word_with_underscores stays text".to_string()
        )])
    );
}
#[test]
fn test_parse_creates_anonymous_target_node() {
    let input = ".. __: https://example.com\n\nText";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::AnonymousTarget {
            uri: "https://example.com".to_string()
        }
    );
}
#[test]
fn test_parse_creates_anonymous_reference() {
    let input = "See `Example`__ and link__";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 4);
        assert_eq!(
            inlines[1],
            InlineNode::AnonymousReference("Example".to_string())
        );
        assert_eq!(
            inlines[3],
            InlineNode::AnonymousReference("link".to_string())
        );
    } else {
        panic!("Expected paragraph");
    }
}
#[test]
fn test_parse_creates_anonymous_hyperlink_with_embedded_uri() {
    let input = "See `Google <https://google.com>`__";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 2);
        assert_eq!(
            inlines[1],
            InlineNode::AnonymousHyperlink {
                text: "Google".to_string(),
                target: "https://google.com".to_string()
            }
        );
    } else {
        panic!("Expected paragraph");
    }
}
#[test]
fn test_parse_paragraph_with_emphasis() {
    let input = "*emphasized* text";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 2);
        assert_eq!(inlines[0], InlineNode::Emphasis("emphasized".to_string()));
        assert_eq!(inlines[1], InlineNode::Text(" text".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_with_strong_emphasis() {
    let input = "some **strong** text";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 3);
        assert_eq!(inlines[0], InlineNode::Text("some ".to_string()));
        assert_eq!(inlines[1], InlineNode::Strong("strong".to_string()));
        assert_eq!(inlines[2], InlineNode::Text(" text".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_with_inline_literal() {
    let input = "some ``venv`` text";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 3);
        assert_eq!(inlines[0], InlineNode::Text("some ".to_string()));
        assert_eq!(inlines[1], InlineNode::Literal("venv".to_string()));
        assert_eq!(inlines[2], InlineNode::Text(" text".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_inline_literal_with_backslash() {
    let input = "``some\\path``";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0], InlineNode::Literal("some\\path".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_inline_literal_ignoring_inner_markup() {
    let input = "``**bold**``";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0], InlineNode::Literal("**bold**".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_with_mixed_markup() {
    let input = "Go to *emphasis* or **strong** link.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 5);
        assert_eq!(inlines[0], InlineNode::Text("Go to ".to_string()));
        assert_eq!(inlines[1], InlineNode::Emphasis("emphasis".to_string()));
        assert_eq!(inlines[2], InlineNode::Text(" or ".to_string()));
        assert_eq!(inlines[3], InlineNode::Strong("strong".to_string()));
        assert_eq!(inlines[4], InlineNode::Text(" link.".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_escaped_space_joins_markup_to_neighbouring_text() {
    // Given the RST idiom for attaching markup to adjacent text: the
    // escaped spaces are separators for the parser and vanish from output
    let doc = parse("test.rst", r"Join foo\ *bar*\ baz tightly.");

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    };
    assert_eq!(inlines[0], InlineNode::Text("Join foo".to_string()));
    assert_eq!(inlines[1], InlineNode::Emphasis("bar".to_string()));
    assert_eq!(inlines[2], InlineNode::Text("baz tightly.".to_string()));
}
#[test]
fn test_parse_escaped_dashes_are_not_turned_into_a_typographic_dash() {
    // Given an escaped and an unescaped pair. Smart typography runs on the
    // escaped form, so only the unescaped pair converts — matching
    // docutils' smartquotes transform.
    let doc = parse("test.rst", r"escaped \-\- and real -- here");

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    };
    assert_eq!(
        inlines[0],
        InlineNode::Text("escaped -- and real \u{2013} here".to_string())
    );
}
#[test]
fn test_parse_role_content_is_unescaped_through_the_full_pipeline() {
    // Given a role whose content carries an escape. The unit tests for
    // `parse_domain_object_target` hand it pre-escaped text directly, so
    // this covers the one thing they cannot: that the escaping actually
    // reaches them from `parse_inline_text`.
    let doc = parse("test.rst", r":func:`spawn\* <spawnl>`");

    // Then
    let Node::Paragraph(inlines) = &doc.nodes[0] else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    };
    let InlineNode::DomainObjectReference { name, display, .. } = &inlines[0] else {
        panic!("Expected DomainObjectReference, got {:?}", inlines[0]);
    };
    assert_eq!(name, "spawnl");
    assert_eq!(display, "spawn*");
}
#[test]
fn test_no_escape_marker_survives_into_the_parsed_document() {
    // Given an escape inside every construct that carries text out of the
    // inline parser
    let input = concat!(
        r"Text \*with\* escapes, ``a\literal``, *em\*phasis*, **str\*ong**,",
        "\n",
        r":func:`spawn\* <spawnl>`, :ref:`see\* <somewhere>`,",
        "\n",
        r":term:`a\*term`, :option:`--flag\*`, `a link\* <https://example.com>`_",
    );

    // When
    let doc = parse("test.rst", input);

    // Then — a marker reaching the AST would be rendered into the HTML as
    // a stray NUL, so assert on the whole tree rather than field by field.
    // This is the guarantee `unescape_node`'s exhaustive match exists for.
    let rendered = format!("{:?}", doc.nodes);
    assert!(
        !rendered.contains('\u{0}'),
        "an escape marker leaked into the AST: {rendered}"
    );
}
#[test]
fn test_parse_paragraph_with_escaped_markup() {
    // The backslashes suppress the emphasis *and* are removed from the
    // output, as docutils does — they are markup, not content.
    let input = r"Keep \*stars\* as is and **strong** text.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 3);
        assert_eq!(
            inlines[0],
            InlineNode::Text("Keep *stars* as is and ".to_string())
        );
        assert_eq!(inlines[1], InlineNode::Strong("strong".to_string()));
        assert_eq!(inlines[2], InlineNode::Text(" text.".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_rejects_invalid_boundary_markup() {
    let input = "a*text* *text*b";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0], InlineNode::Text("a*text* *text*b".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_with_many_unclosed_emphasis_markers_stays_fast() {
    // Given: many isolated, never-validly-closed single-star tokens (each
    // "*" is followed by a space before the next one, so none can close
    // any other) — the exact pathological shape from rust_review.md
    // finding #3, which previously made find_inline_markup/
    // try_match_inline rescan the remaining text from every failed
    // candidate, causing O(n^2) parse time.
    let input = "*word ".repeat(20_000);

    // When
    let start = std::time::Instant::now();
    let doc = parse("test.rst", &input);
    let elapsed = start.elapsed();

    // Then: stays well under a second (the O(n^2) implementation took
    // several seconds at this size); still parses as a single
    // unmatched-markup Text node.
    assert!(
        elapsed.as_secs() < 3,
        "parsing took too long: {elapsed:?} (quadratic regression?)"
    );
    assert_eq!(doc.nodes.len(), 1);
}
#[test]
fn test_parse_paragraph_with_punctuation_boundaries() {
    let input = "(*emphasis*), [**strong**];";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 5);
        assert_eq!(inlines[0], InlineNode::Text("(".to_string()));
        assert_eq!(inlines[1], InlineNode::Emphasis("emphasis".to_string()));
        assert_eq!(inlines[2], InlineNode::Text("), [".to_string()));
        assert_eq!(inlines[3], InlineNode::Strong("strong".to_string()));
        assert_eq!(inlines[4], InlineNode::Text("];".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_with_emphasis_multibyte() {
    let input = "*\u{03c0}* text";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 2);
        assert_eq!(inlines[0], InlineNode::Emphasis("\u{03c0}".to_string()));
        assert_eq!(inlines[1], InlineNode::Text(" text".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_with_program_role() {
    let input = "Run :program:`curl` to download files.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 3);
        assert_eq!(inlines[0], InlineNode::Text("Run ".to_string()));
        assert_eq!(inlines[1], InlineNode::Program("curl".to_string()));
        assert_eq!(
            inlines[2],
            InlineNode::Text(" to download files.".to_string())
        );
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_paragraph_with_multiple_program_roles_and_punctuation() {
    let input = "Use :program:`git` or :program:`hg` to manage code.";
    let doc = parse("test.rst", input);
    assert_eq!(doc.nodes.len(), 1);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(inlines.len(), 5);
        assert_eq!(inlines[0], InlineNode::Text("Use ".to_string()));
        assert_eq!(inlines[1], InlineNode::Program("git".to_string()));
        assert_eq!(inlines[2], InlineNode::Text(" or ".to_string()));
        assert_eq!(inlines[3], InlineNode::Program("hg".to_string()));
        assert_eq!(inlines[4], InlineNode::Text(" to manage code.".to_string()));
    } else {
        panic!("Expected Paragraph node");
    }
}
#[test]
fn test_parse_term_role_basic() {
    let input = "See :term:`environment` for details.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let term_ref = inlines
            .iter()
            .find(|n| matches!(n, InlineNode::TermReference { .. }));
        assert!(term_ref.is_some(), "Expected TermReference in paragraph");
        if let Some(InlineNode::TermReference { display, term }) = term_ref {
            assert_eq!(display, "environment");
            assert_eq!(term, "environment");
        }
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_term_role_with_display_text() {
    let input = "See :term:`the env <environment>` here.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let term_ref = inlines
            .iter()
            .find(|n| matches!(n, InlineNode::TermReference { .. }));
        assert!(term_ref.is_some());
        if let Some(InlineNode::TermReference { display, term }) = term_ref {
            assert_eq!(display, "the env");
            assert_eq!(term, "environment");
        }
    } else {
        panic!("Expected Paragraph");
    }
}
#[test]
fn test_parse_option_role_basic() {
    let input = "See :option:`-h` for details.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let option_ref = inlines
            .iter()
            .find(|n| matches!(n, InlineNode::OptionReference { .. }));
        assert!(
            option_ref.is_some(),
            "Expected OptionReference in paragraph"
        );
        if let Some(InlineNode::OptionReference { display, target }) = option_ref {
            assert_eq!(display, "-h");
            assert_eq!(target, "-h");
        }
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_option_role_with_display_text() {
    let input = "See :option:`-W default <-W>` here.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let option_ref = inlines
            .iter()
            .find(|n| matches!(n, InlineNode::OptionReference { .. }));
        assert!(option_ref.is_some());
        if let Some(InlineNode::OptionReference { display, target }) = option_ref {
            assert_eq!(display, "-W default");
            assert_eq!(target, "-W");
        }
    } else {
        panic!("Expected Paragraph");
    }
}
#[test]
fn test_parse_option_role_with_embedded_program_in_target() {
    // Given — the whole target is carried through verbatim; splitting it
    // into program + optname happens at render time (see
    // `rusty_sphinx_renderer::option_resolution`).
    let input = "See :option:`-O <dis --show-offsets>` here.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let option_ref = inlines
            .iter()
            .find(|n| matches!(n, InlineNode::OptionReference { .. }));
        assert!(option_ref.is_some());
        if let Some(InlineNode::OptionReference { display, target }) = option_ref {
            assert_eq!(display, "-O");
            assert_eq!(target, "dis --show-offsets");
        }
    } else {
        panic!("Expected Paragraph");
    }
}
#[test]
fn test_parse_option_role_mixed_with_surrounding_text() {
    let input = "Before :option:`-x` after.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(inlines.len() >= 3, "Expected text + option + text");
        assert!(matches!(&inlines[0], InlineNode::Text(t) if t == "Before "));
        assert!(
            matches!(&inlines[1], InlineNode::OptionReference { target, .. } if target == "-x")
        );
    } else {
        panic!("Expected Paragraph");
    }
}
#[test]
fn test_parse_ref_role_with_display_text() {
    let input = "See :ref:`GenericAlias <types-genericalias>` here.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        let reference = inlines
            .iter()
            .find(|n| matches!(n, InlineNode::Reference { .. }));
        assert!(reference.is_some());
        if let Some(InlineNode::Reference { display, target }) = reference {
            assert_eq!(display, "GenericAlias");
            assert_eq!(target, "types-genericalias");
        }
    } else {
        panic!("Expected Paragraph");
    }
}
#[test]
fn test_parse_term_role_mixed_with_surrounding_text() {
    let input = "Before :term:`foo` after.\n";
    let doc = parse("test.rst", input);
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(inlines.len() >= 3, "Expected text + term + text");
        assert!(matches!(&inlines[0], InlineNode::Text(t) if t == "Before "));
        assert!(matches!(&inlines[1], InlineNode::TermReference { term, .. } if term == "foo"));
    } else {
        panic!("Expected Paragraph");
    }
}
#[test]
fn test_parse_paragraph_converts_triple_hyphen_to_em_dash() {
    // Given
    let input = "wait---no, that's wrong.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text(
            "wait\u{2014}no, that's wrong.".to_string()
        )])
    );
}
#[test]
fn test_parse_paragraph_converts_double_hyphen_to_en_dash() {
    // Given
    let input = "See pages 10--20.";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text(
            "See pages 10\u{2013}20.".to_string()
        )])
    );
}
#[test]
fn test_parse_paragraph_converts_triple_dot_to_ellipsis() {
    // Given
    let input = "Wait... what happened?";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes[0],
        Node::Paragraph(vec![InlineNode::Text(
            "Wait\u{2026} what happened?".to_string()
        )])
    );
}
#[test]
fn test_parse_paragraph_applies_smart_typography_inside_strong_emphasis() {
    // Given
    let input = "This is **really---important**.";

    // When
    let doc = parse("test.rst", input);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(inlines.contains(&InlineNode::Strong("really\u{2014}important".to_string())));
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_paragraph_does_not_apply_smart_typography_inside_inline_literal() {
    // Given — literal/code content must not be transformed
    let input = "Run ``git log a---b``.";

    // When
    let doc = parse("test.rst", input);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(inlines.contains(&InlineNode::Literal("git log a---b".to_string())));
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}

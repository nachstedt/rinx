//! End-to-end `parse()` pipeline tests for interpreted text: the
//! `:title-reference:` roles, text written between single backquotes with no
//! role at all, a role written after the text instead of before it, and what
//! `.. default-role::` makes of the bare form.
//!
//! What only the whole pipeline can show lives here: how the bare form
//! competes with every other construct a backquote opens, and that the
//! default role is state the parse carries from one paragraph to the next.

use crate::parse;
use rinx_ast::{DiagnosticCode, InlineNode, Node, Position, ScriptPosition, Span};

fn text(text: &str) -> InlineNode {
    InlineNode::Text(text.to_string())
}

fn title(text: &str) -> InlineNode {
    InlineNode::TitleReference(text.to_string())
}

/// The inline content of every paragraph in the document, in order.
fn paragraphs(input: &str) -> Vec<Vec<InlineNode>> {
    parse("test.rst", input)
        .nodes
        .into_iter()
        .filter_map(|node| match node {
            Node::Paragraph(inlines) => Some(inlines),
            _ => None,
        })
        .collect()
}

/// The inline content of the document's only paragraph, spans dropped.
fn paragraph(input: &str) -> Vec<InlineNode> {
    let paragraphs = paragraphs(input);
    assert_eq!(paragraphs.len(), 1, "{paragraphs:?}");
    paragraphs.into_iter().next().unwrap()
}

#[test]
fn test_parse_reads_every_spelling_of_the_title_reference_role() {
    // Given
    for input in [
        "Read :title-reference:`Dune` now.",
        "Read :title:`Dune` now.",
        "Read :t:`Dune` now.",
    ] {
        // When
        let inlines = paragraph(input);

        // Then
        assert_eq!(
            inlines,
            vec![text("Read "), title("Dune"), text(" now.")],
            "{input}"
        );
    }
}

#[test]
fn test_parse_keeps_a_title_reference_plain_and_typographic() {
    // Given — markup and an explicit-title shape inside, and a dash
    let input = "See :t:`*Not* emphasis -- <x>`.";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(
        inlines,
        vec![
            text("See "),
            title("*Not* emphasis \u{2013} <x>"),
            text("."),
        ]
    );
}

#[test]
fn test_parse_reads_bare_interpreted_text_as_a_title() {
    // Given
    let input = "Read `Dune` and `The Left Hand of Darkness`.";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(
        inlines,
        vec![
            text("Read "),
            title("Dune"),
            text(" and "),
            title("The Left Hand of Darkness"),
            text("."),
        ]
    );
}

#[test]
fn test_parse_leaves_backquotes_that_open_no_markup() {
    // Given — inside a word, followed by a space, and escaped
    for input in ["don`t `stop", "a ` b ` c", r"an \`escaped\` one", "a`b`c"] {
        // When
        let inlines = paragraph(input);

        // Then
        assert!(
            inlines
                .iter()
                .all(|node| matches!(node, InlineNode::Text(_))),
            "{input}: {inlines:?}"
        );
    }
}

#[test]
fn test_parse_prefers_every_other_backquote_construct() {
    // Given / When / Then — a literal, both link forms and a role
    assert_eq!(
        paragraph("A ``literal`` here."),
        vec![
            text("A "),
            InlineNode::Literal("literal".to_string()),
            text(" here."),
        ]
    );
    assert!(matches!(
        paragraph("A `link`_ here.")[1],
        InlineNode::Hyperlink { .. }
    ));
    assert!(matches!(
        paragraph("A `link`__ here.")[1],
        InlineNode::AnonymousReference { .. }
    ));
    assert!(matches!(
        paragraph("A :ref:`label` here.")[1],
        InlineNode::Reference { .. }
    ));
}

#[test]
fn test_parse_does_not_run_a_title_on_into_a_later_link() {
    // Given — the first closing backquote ends a link, not the title
    let input = "See `docs`_ and `Dune`.";

    // When
    let inlines = paragraph(input);

    // Then
    assert!(
        matches!(inlines[1], InlineNode::Hyperlink { .. }),
        "{inlines:?}"
    );
    assert_eq!(inlines[3], title("Dune"));
}

#[test]
fn test_parse_leaves_an_unknown_role_before_the_text_as_written() {
    // Given — a role name only docutils' wider name shape allows
    let input = "A :not.known:`x` here.";

    // When
    let inlines = paragraph(input);

    // Then
    assert!(
        inlines
            .iter()
            .all(|node| matches!(node, InlineNode::Text(_))),
        "{inlines:?}"
    );
}

#[test]
fn test_parse_keeps_bare_interpreted_text_plain() {
    // Given — emphasis markers and an escape inside
    let input = r"A `*not* \*emphasis` here.";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(
        inlines,
        vec![text("A "), title("*not* *emphasis"), text(" here.")]
    );
}

#[test]
fn test_parse_reads_a_role_written_after_the_text() {
    // Given — docutils' suffix form, inside a word via escaped spaces
    let input = r"Water is H\ `2`:sub:\ O.";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(
        inlines,
        vec![
            text("Water is H"),
            InlineNode::Script {
                position: ScriptPosition::Subscript,
                text: "2".to_string(),
                classes: Vec::new(),
            },
            text("O."),
        ]
    );
}

#[test]
fn test_parse_reads_a_domain_role_written_after_the_text() {
    // Given
    let input = "Call `spam`:py:func: now.";

    // When
    let inlines = paragraph(input);

    // Then
    assert!(
        matches!(&inlines[1], InlineNode::DomainObjectReference { name, .. } if name == "spam"),
        "{inlines:?}"
    );
}

#[test]
fn test_parse_leaves_an_unknown_role_after_the_text_as_written() {
    // Given
    let input = "A `x`:nope: here.";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(inlines, vec![text("A "), text("`x`:nope:"), text(" here.")]);
}

#[test]
fn test_parse_reads_a_suffix_that_is_not_a_whole_role_as_text() {
    // Given — `:sub:` is followed by a letter, so it is no role
    let input = "A `x`:sub:y here.";

    // When
    let inlines = paragraph(input);

    // Then
    assert_eq!(inlines, vec![text("A "), title("x"), text(":sub:y here.")]);
}

#[test]
fn test_parse_reports_a_role_before_and_after_the_same_text() {
    // Given
    let input = "A :sub:`2`:sup: here.";

    // When
    let doc = parse("test.rst", input);

    // Then — shown as written, and reported where it was written
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![
            text("A "),
            text(":sub:`2`:sup:"),
            text(" here."),
        ])]
    );
    assert_eq!(doc.diagnostics.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(
        doc.diagnostics[0].code,
        DiagnosticCode::InterpretedMultipleRoles
    );
    assert_eq!(
        doc.diagnostics[0].span,
        Some(Span::new(Position::new(1, 3), Position::new(1, 16)))
    );
}

#[test]
fn test_parse_reports_a_role_with_a_reference_suffix() {
    // Given — before the text, and after it
    for (input, source) in [
        ("A :ref:`x`_ here.", ":ref:`x`_"),
        ("A `x`:sub:__ here.", "`x`:sub:__"),
    ] {
        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Paragraph(vec![
                text("A "),
                text(source),
                text(" here.")
            ])],
            "{input}"
        );
        assert_eq!(doc.diagnostics.len(), 1, "{input}: {:?}", doc.diagnostics);
        assert_eq!(
            doc.diagnostics[0].code,
            DiagnosticCode::InterpretedRoleAndReference,
            "{input}"
        );
    }
}

#[test]
fn test_parse_records_where_interpreted_text_was_written() {
    // Given — a default role whose node carries a span
    let input = ".. default-role:: ref\n\nSee `label` now.\n";

    // When
    let paragraphs = paragraphs(input);

    // Then
    assert!(
        matches!(
            &paragraphs[0][1],
            InlineNode::Reference { span: Some(span), .. }
                if *span == Span::new(Position::new(3, 5), Position::new(3, 12))
        ),
        "{paragraphs:?}"
    );
}

#[test]
fn test_parse_applies_a_default_role_from_where_it_is_written() {
    // Given
    let input = "`a` first.\n\n.. default-role:: sup\n\n`b` then.\n";

    // When
    let paragraphs = paragraphs(input);

    // Then
    assert_eq!(paragraphs[0][0], title("a"));
    assert_eq!(
        paragraphs[1][0],
        InlineNode::Script {
            position: ScriptPosition::Superscript,
            text: "b".to_string(),
            classes: Vec::new(),
        }
    );
}

#[test]
fn test_parse_restores_title_reference_after_an_empty_default_role() {
    // Given
    let input = ".. default-role:: sup\n\n`a`\n\n.. default-role::\n\n`b`\n";

    // When
    let paragraphs = paragraphs(input);

    // Then
    assert!(matches!(paragraphs[0][0], InlineNode::Script { .. }));
    assert_eq!(paragraphs[1][0], title("b"));
}

#[test]
fn test_parse_keeps_the_default_role_when_a_directive_names_an_unknown_one() {
    // Given
    let input = ".. default-role:: sup\n\n.. default-role:: nope\n\n`a`\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(
        matches!(&doc.nodes[0], Node::Paragraph(inlines) if matches!(inlines[0], InlineNode::Script { .. })),
        "{:?}",
        doc.nodes
    );
    assert_eq!(
        doc.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![DiagnosticCode::DefaultRoleUnknownRole]
    );
}

#[test]
fn test_parse_reads_bare_text_with_a_cross_reference_default_role() {
    // Given — an explicit title, as the role itself takes one
    let input = ".. default-role:: any\n\nSee `the guide <guide>` and `api`.\n";

    // When
    let paragraphs = paragraphs(input);

    // Then
    assert!(
        matches!(
            &paragraphs[0][1],
            InlineNode::AnyReference { display: Some(display), target, .. }
                if display == "the guide" && target == "guide"
        ),
        "{paragraphs:?}"
    );
    assert!(
        matches!(&paragraphs[0][3], InlineNode::AnyReference { target, .. } if target == "api"),
        "{paragraphs:?}"
    );
}

#[test]
fn test_parse_resolves_a_bare_domain_default_role_in_the_default_domain() {
    // Given
    let input = ".. default-role:: func\n\nCall `spam`.\n";

    // When
    let doc = crate::parse_with_domain("test.rst", input, rinx_ast::Domain::C);

    // Then
    assert!(
        matches!(
            &doc.nodes[0],
            Node::Paragraph(inlines) if matches!(
                &inlines[1],
                InlineNode::DomainObjectReference { object_type: rinx_ast::ObjectType::C(_), .. }
            )
        ),
        "{:?}",
        doc.nodes
    );
}

#[test]
fn test_parse_reads_bare_text_with_a_role_the_document_defined() {
    // Given
    let input = ".. role:: py(code)\n   :language: python\n\n.. default-role:: py\n\nCall `f()`.\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert!(
        matches!(
            &doc.nodes[0],
            Node::Paragraph(inlines) if matches!(&inlines[1], InlineNode::Code { text, .. } if text == "f()")
        ),
        "{:?}",
        doc.nodes
    );
}

#[test]
fn test_parse_refuses_a_default_role_defined_only_later() {
    // Given — a role applies only after its definition
    let input = ".. default-role:: py\n\n.. role:: py(code)\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![DiagnosticCode::DefaultRoleUnknownRole]
    );
}

#[test]
fn test_parse_reads_bare_text_with_an_entity_role() {
    // Given
    let schema = rinx_entity::load_schema(
        "[[entity_type]]\nname = \"req\"\n\n[[role]]\nname = \"req\"\n",
        &rinx_entity::NoReservedNames,
    )
    .unwrap();
    let ctx = crate::ParseCtx::with_domain(rinx_ast::Domain::Py).with_schema(&schema);

    // When
    let doc = crate::parse_with_ctx("test.rst", ".. default-role:: req\n\nSee `R_1`.\n", &ctx);

    // Then
    assert!(
        matches!(
            &doc.nodes[0],
            Node::Paragraph(inlines) if matches!(&inlines[1], InlineNode::EntityReference { target, .. } if target == "R_1")
        ),
        "{:?}",
        doc.nodes
    );
}

#[test]
fn test_parse_reads_bare_text_with_the_configured_default_role() {
    // Given
    let base = crate::ParseCtx::with_domain(rinx_ast::Domain::Py);
    let role = crate::DefaultRole::parse("sub", &base).unwrap();
    let ctx = base.with_default_role(&role);

    // When
    let doc = crate::parse_with_ctx("test.rst", "H\\ `2`\\ O\n", &ctx);

    // Then
    assert!(
        matches!(&doc.nodes[0], Node::Paragraph(inlines) if matches!(inlines[1], InlineNode::Script { .. })),
        "{:?}",
        doc.nodes
    );
}

#[test]
fn test_parse_restores_title_reference_rather_than_the_configured_default() {
    // Given — as Sphinx's directive unregisters the role, leaving docutils'
    let base = crate::ParseCtx::with_domain(rinx_ast::Domain::Py);
    let role = crate::DefaultRole::parse("sub", &base).unwrap();
    let ctx = base.with_default_role(&role);

    // When
    let doc = crate::parse_with_ctx("test.rst", ".. default-role::\n\n`a`\n", &ctx);

    // Then
    assert_eq!(doc.nodes, vec![Node::Paragraph(vec![title("a")])]);
}

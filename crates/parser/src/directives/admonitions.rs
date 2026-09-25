use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rinx_ast::{Diagnostic, DiagnosticCode, Directive};

pub(super) fn parse_admonition(
    kind: rinx_ast::AdmonitionKind,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let title = if kind == rinx_ast::AdmonitionKind::Admonition {
        if argument.is_empty() {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::DirectiveTitleArgumentMissing,
                "Generic 'admonition' directive requires a title argument.",
                body_span(body_lines, ctx),
            ));
            Some("Admonition".to_string())
        } else {
            Some(argument)
        }
    } else {
        None
    };

    let mut collapsible = None;

    // Strip common indentation and parse options
    let unindented_lines = unindent_body_lines(body_lines);
    if unindented_lines.is_empty() {
        return Directive::Admonition {
            kind,
            title,
            collapsible: None,
            body: vec![],
        };
    }

    // Parse options (specifically :collapsible:)
    let mut opt_idx = 0;
    while opt_idx < unindented_lines.len() {
        let line = unindented_lines[opt_idx].trim();
        if line.is_empty() {
            opt_idx += 1;
            continue;
        }
        if line.starts_with(':') && line.contains(':') {
            if line.starts_with(":collapsible:") {
                let arg = line.strip_prefix(":collapsible:").unwrap().trim();
                if arg == "open" {
                    collapsible = Some(true);
                } else {
                    // Default to closed if ":collapsible:" or ":collapsible: close"
                    collapsible = Some(false);
                }
            }
            opt_idx += 1;
        } else {
            break;
        }
    }

    // The rest is the body
    let body_content: Vec<&str> = unindented_lines[opt_idx..]
        .iter()
        .map(String::as_str)
        .collect();
    let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    Directive::Admonition {
        kind,
        title,
        collapsible,
        body: body_nodes,
    }
}

pub(super) fn parse_version_change(
    kind: rinx_ast::VersionChangeKind,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let version = if argument.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveVersionArgumentMissing,
            format!("'{}' requires a version argument.", kind.as_str()),
            // The directive's own marker line, which is the line above the
            // body this parser was handed.
            body_span(body_lines, ctx),
        ));
        "unknown".to_string()
    } else {
        argument
    };

    let unindented_lines = unindent_body_lines(body_lines);
    if unindented_lines.is_empty() {
        return Directive::VersionChange {
            kind,
            version,
            body: vec![],
        };
    }

    let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
    let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    Directive::VersionChange {
        kind,
        version,
        body: body_nodes,
    }
}

pub(super) fn parse_seealso(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    if unindented_lines.is_empty() {
        return Directive::SeeAlso { body: vec![] };
    }

    let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
    let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    Directive::SeeAlso { body: body_nodes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::Domain;
    use rinx_ast::Node;

    #[test]
    fn test_parse_admonition_basic() {
        // Given
        let kind = rinx_ast::AdmonitionKind::Note;
        let argument = String::new();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        if let Directive::Admonition {
            kind, title, body, ..
        } = directive
        {
            assert_eq!(kind, rinx_ast::AdmonitionKind::Note);
            assert_eq!(title, None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_generic_with_title() {
        // Given
        let kind = rinx_ast::AdmonitionKind::Admonition;
        let argument = "Custom Title".to_string();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        if let Directive::Admonition { kind, title, .. } = directive {
            assert_eq!(kind, rinx_ast::AdmonitionKind::Admonition);
            assert_eq!(title, Some("Custom Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_collapsible() {
        // Given
        let kind = rinx_ast::AdmonitionKind::Warning;
        let argument = String::new();
        let body_lines = vec!["   :collapsible: open", "", "   Content"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        if let Directive::Admonition { collapsible, .. } = directive {
            assert_eq!(collapsible, Some(true));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_generic_requires_title_diagnostic() {
        // Given
        let kind = rinx_ast::AdmonitionKind::Admonition;
        let argument = String::new();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let _ = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("requires a title argument"));
    }

    #[test]
    fn test_parse_admonition_does_not_panic_on_multi_byte_char_in_a_short_line() {
        // Given a body whose first line has a 3-space indent and a second,
        // less-indented line containing a multi-byte character at the byte
        // offset the old byte-index slicing would have panicked on
        let kind = rinx_ast::AdmonitionKind::Note;
        let argument = String::new();
        let body_lines = vec!["   First line normal indent.", "  éfoo"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then it does not panic
        assert!(matches!(directive, Directive::Admonition { .. }));
    }

    #[test]
    fn test_parse_versionchanged_creates_directive() {
        // Given
        let input = ".. versionchanged:: 2.3\n\n   Added async support.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, rinx_ast::VersionChangeKind::Changed);
            assert_eq!(version, "2.3");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }

    #[test]
    fn test_parse_versionadded_creates_directive() {
        // Given
        let input = ".. versionadded:: 1.0\n\n   Initial release.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, rinx_ast::VersionChangeKind::Added);
            assert_eq!(version, "1.0");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }

    #[test]
    fn test_parse_deprecated_creates_directive() {
        // Given
        let input = ".. deprecated:: 3.0\n\n   Use new API.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, rinx_ast::VersionChangeKind::Deprecated);
            assert_eq!(version, "3.0");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }

    #[test]
    fn test_parse_version_change_emits_diagnostic_for_missing_version() {
        // Given
        let input = ".. versionchanged::\n\n   Missing version.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.diagnostics.len(), 1);
        assert_eq!(
            doc.diagnostics[0].message,
            "'versionchanged' requires a version argument."
        );
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, rinx_ast::VersionChangeKind::Changed);
            assert_eq!(version, "unknown");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }

    #[test]
    fn test_parse_version_change_does_not_panic_on_multi_byte_char_in_a_short_line() {
        // Given a body whose first line has a 3-space indent and a second,
        // less-indented line containing a multi-byte character at the byte
        // offset the old byte-index slicing would have panicked on
        let kind = rinx_ast::VersionChangeKind::Changed;
        let argument = "2.3".to_string();
        let body_lines = vec!["   First line normal indent.", "  éfoo"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_version_change(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then it does not panic
        assert!(matches!(directive, Directive::VersionChange { .. }));
    }

    #[test]
    fn test_parse_creates_seealso_with_paragraph_body() {
        // Given
        let input = ".. seealso::\n\n   Related information here.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::SeeAlso { body }) = &doc.nodes[0] {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected SeeAlso directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_seealso_with_empty_body() {
        // Given
        let input = ".. seealso::";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::SeeAlso { body }) = &doc.nodes[0] {
            assert!(body.is_empty());
        } else {
            panic!("Expected SeeAlso directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_seealso_with_bullet_list_body() {
        // Given
        let input = ".. seealso::\n\n   * Item A\n   * Item B";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::SeeAlso { body }) = &doc.nodes[0] {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::BulletList { .. }));
        } else {
            panic!("Expected SeeAlso directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_seealso_with_definition_list_body() {
        // Given the CPython benchmark's `curses` seealso block: a definition
        // list where each term carries a :mod:/:ref: role.
        let input = concat!(
            ".. seealso::\n",
            "\n",
            "   Module :mod:`curses.ascii`\n",
            "      Utilities for working with ASCII characters, regardless of your locale settings.\n",
            "\n",
            "   Module :mod:`curses.panel`\n",
            "      A panel stack extension that adds depth to  curses windows.\n",
            "\n",
            "   :ref:`curses-howto`\n",
            "      Tutorial material on using curses with Python, by Andrew Kuchling and Eric\n",
            "      Raymond.",
        );

        // When
        let doc = parse("test.rst", input);

        // Then the body is a single DefinitionList with three term/definition items
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::SeeAlso { body }) = &doc.nodes[0] {
            assert_eq!(body.len(), 1);
            if let Node::DefinitionList { items } = &body[0] {
                assert_eq!(items.len(), 3);
                assert!(matches!(
                    items[0].term[1],
                    rinx_ast::InlineNode::DomainObjectReference { .. }
                ));
                assert!(matches!(
                    items[2].term[0],
                    rinx_ast::InlineNode::Reference { .. }
                ));
            } else {
                panic!("Expected DefinitionList, got {:?}", body[0]);
            }
        } else {
            panic!("Expected SeeAlso directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_seealso_basic() {
        // Given
        let body_lines = vec!["   See the other page."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_seealso(
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        if let Directive::SeeAlso { body } = directive {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected SeeAlso directive");
        }
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_seealso_does_not_panic_on_multi_byte_char_in_a_short_line() {
        // Given a body whose first line has a 3-space indent and a second,
        // less-indented line containing a multi-byte character at the byte
        // offset the old byte-index slicing would have panicked on
        let body_lines = vec!["   First line normal indent.", "  éfoo"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_seealso(
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then it does not panic
        assert!(matches!(directive, Directive::SeeAlso { .. }));
    }
}

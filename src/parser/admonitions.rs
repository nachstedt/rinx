use super::blocks::parse_blocks;
use super::headings::Adornment;
use crate::ast::{Directive, Domain};

pub(super) fn parse_admonition(
    kind: crate::ast::AdmonitionKind,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Directive {
    let title = if kind == crate::ast::AdmonitionKind::Admonition {
        if argument.is_empty() {
            diagnostics
                .push("Generic 'admonition' directive requires a title argument.".to_string());
            Some("Admonition".to_string())
        } else {
            Some(argument)
        }
    } else {
        None
    };

    let mut collapsible = None;

    // Strip common indentation and parse options
    if let Some(first) = body_lines.iter().find(|l| !l.trim().is_empty()) {
        let indent = first.chars().take_while(|c| c.is_whitespace()).count();
        let unindented_lines: Vec<String> = body_lines
            .iter()
            .map(|l| {
                if l.len() >= indent {
                    l[indent..].to_string()
                } else {
                    l.trim().to_string()
                }
            })
            .collect();

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
        let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

        Directive::Admonition {
            kind,
            title,
            collapsible,
            body: body_nodes,
        }
    } else {
        Directive::Admonition {
            kind,
            title,
            collapsible: None,
            body: vec![],
        }
    }
}

pub(super) fn parse_version_change(
    kind: crate::ast::VersionChangeKind,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Directive {
    let version = if argument.is_empty() {
        diagnostics.push(format!("'{}' requires a version argument.", kind.as_str()));
        "unknown".to_string()
    } else {
        argument
    };

    if let Some(first) = body_lines.iter().find(|l| !l.trim().is_empty()) {
        let indent = first.chars().take_while(|c| c.is_whitespace()).count();
        let unindented_lines: Vec<String> = body_lines
            .iter()
            .map(|l| {
                if l.len() >= indent {
                    l[indent..].to_string()
                } else {
                    l.trim().to_string()
                }
            })
            .collect();

        let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
        let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

        Directive::VersionChange {
            kind,
            version,
            body: body_nodes,
        }
    } else {
        Directive::VersionChange {
            kind,
            version,
            body: vec![],
        }
    }
}

pub(super) fn parse_seealso(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Directive {
    if let Some(first) = body_lines.iter().find(|l| !l.trim().is_empty()) {
        let indent = first.chars().take_while(|c| c.is_whitespace()).count();
        let unindented_lines: Vec<String> = body_lines
            .iter()
            .map(|l| {
                if l.len() >= indent {
                    l[indent..].to_string()
                } else {
                    l.trim().to_string()
                }
            })
            .collect();

        let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
        let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

        Directive::SeeAlso { body: body_nodes }
    } else {
        Directive::SeeAlso { body: vec![] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Node;
    use crate::parser::parse;

    #[test]
    fn test_parse_admonition_basic() {
        // Given
        let kind = crate::ast::AdmonitionKind::Note;
        let argument = String::new();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let Directive::Admonition {
            kind, title, body, ..
        } = directive
        {
            assert_eq!(kind, crate::ast::AdmonitionKind::Note);
            assert_eq!(title, None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_generic_with_title() {
        // Given
        let kind = crate::ast::AdmonitionKind::Admonition;
        let argument = "Custom Title".to_string();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let Directive::Admonition { kind, title, .. } = directive {
            assert_eq!(kind, crate::ast::AdmonitionKind::Admonition);
            assert_eq!(title, Some("Custom Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_collapsible() {
        // Given
        let kind = crate::ast::AdmonitionKind::Warning;
        let argument = String::new();
        let body_lines = vec!["   :collapsible: open", "", "   Content"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
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
        let kind = crate::ast::AdmonitionKind::Admonition;
        let argument = String::new();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let _ = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].contains("requires a title argument"));
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
            assert_eq!(*kind, crate::ast::VersionChangeKind::Changed);
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
            assert_eq!(*kind, crate::ast::VersionChangeKind::Added);
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
            assert_eq!(*kind, crate::ast::VersionChangeKind::Deprecated);
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
            doc.diagnostics[0],
            "'versionchanged' requires a version argument."
        );
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, crate::ast::VersionChangeKind::Changed);
            assert_eq!(version, "unknown");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
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
    fn test_parse_seealso_basic() {
        // Given
        let body_lines = vec!["   See the other page."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_seealso(
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
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
}

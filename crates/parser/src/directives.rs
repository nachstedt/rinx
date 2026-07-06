use super::admonitions::{parse_admonition, parse_seealso, parse_version_change};
use super::blocks::{collect_directive_body, join_body_lines};
use super::domains::parse_domain_object;
use super::glossary::parse_glossary;
use super::headings::Adornment;
use rusty_sphinx_ast::{Directive, Domain, Node, ObjectType};

pub(super) fn parse_toctree(body_lines: &[&str], diagnostics: &mut Vec<String>) -> Directive {
    let mut paths = Vec::new();
    let mut maxdepth = None;
    let mut ignored_options = Vec::new();

    for l in body_lines {
        let line = l.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with(':') {
            let opt_name = line.split(':').nth(1).unwrap_or("");
            match opt_name {
                "maxdepth" => {
                    if let Some(rest) = line.strip_prefix(":maxdepth:")
                        && let Ok(depth) = rest.trim().parse::<usize>()
                    {
                        maxdepth = Some(depth);
                    }
                }
                "numbered" | "caption" | "name" | "titlesonly" | "glob" | "reversed" | "hidden"
                | "includehidden" => {
                    ignored_options.push(line.to_string());
                }
                _ => {
                    diagnostics.push(format!(
                        "Invalid or non-standard Sphinx toctree option encountered: {line}"
                    ));
                }
            }
            continue;
        }
        paths.push(line.to_string());
    }
    Directive::Toctree {
        paths,
        maxdepth,
        ignored_options,
    }
}

pub(super) fn try_parse_directive(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Option<(usize, Node)> {
    let line = lines[i].trim_end();
    if !(line.trim().starts_with(".. ") && line.contains("::")) {
        return None;
    }

    let trimmed = line.trim();
    let (name_part, arg_part) = trimmed.split_once("::")?;
    let name = name_part.strip_prefix(".. ")?.trim().to_string();
    let argument = arg_part.trim().to_string();

    let (consumed_lines, body_lines) = collect_directive_body(lines, i + 1);

    if name == "toctree" {
        let directive = parse_toctree(&body_lines, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "plantuml" {
        let directive = Directive::PlantUml(rusty_sphinx_ast::HashedContent::new(join_body_lines(
            &body_lines,
        )));
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "code-block" {
        let language = if argument.is_empty() {
            None
        } else {
            Some(argument)
        };
        // body_lines was collected by collect_directive_body; strip common indentation
        // to preserve relative indentation within the block (RST spec behaviour).
        let min_indent = body_lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
            .min()
            .unwrap_or(0);
        let content = body_lines
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    String::new()
                } else {
                    l.chars().skip(min_indent).collect()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Some((1 + consumed_lines, Node::LiteralBlock { language, content }));
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::VersionChangeKind>() {
        let directive = parse_version_change(
            kind,
            argument,
            &body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        );
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "seealso" {
        let directive = parse_seealso(&body_lines, adornment_order, diagnostics, default_domain);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::AdmonitionKind>() {
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        );
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "glossary" {
        let directive = parse_glossary(&body_lines, adornment_order, diagnostics, default_domain);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if let Some(object_type) = resolve_domain_object_type(&name, default_domain) {
        let directive = parse_domain_object(
            object_type,
            argument,
            &body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        );
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    let directive = Directive::Unknown {
        name,
        argument,
        body: join_body_lines(&body_lines),
    };
    Some((1 + consumed_lines, Node::Directive(directive)))
}

/// Resolves a directive name to a domain object type: either an explicit
/// `domain:objtype` form (e.g. `py:function`), or a bare `objtype` name
/// (e.g. `function`) resolved via `default_domain`.
fn resolve_domain_object_type(name: &str, default_domain: Domain) -> Option<ObjectType> {
    if let Some((domain_str, objtype_str)) = name.split_once(':') {
        let domain = domain_str.parse::<Domain>().ok()?;
        return ObjectType::from_directive_name(domain, objtype_str);
    }
    ObjectType::from_directive_name(default_domain, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::HashedContent;

    #[test]
    fn test_parse_creates_admonition() {
        // Given
        let input = ".. note::\n\n   This is a note.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition {
            kind, title, body, ..
        }) = &doc.nodes[0]
        {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Note);
            assert_eq!(title, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_admonition_with_title() {
        // Given
        let input = ".. admonition:: My Title\n\n   Custom content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { kind, title, .. }) = &doc.nodes[0] {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Admonition);
            assert_eq!(title, &Some("My Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_admonition() {
        // Given
        let input = ".. note::\n   :collapsible:\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(false));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_open_admonition() {
        // Given
        let input = ".. note::\n   :collapsible: open\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(true));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_directive() {
        // Given
        let input = ".. toctree::\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_directive_with_maxdepth() {
        // Given
        let input =
            ".. toctree::\n   :maxdepth: 2\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: Some(2),
                ignored_options: vec![],
            })
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_directive_with_argument_and_trailing_indents() {
        // Given
        let input = ".. code-block:: rust\n\n   let x = 1;\n   \n   let y = 2;\n\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::LiteralBlock {
                language: Some("rust".to_string()),
                content: "let x = 1;\n\nlet y = 2;".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_creates_plantuml_directive_with_hash() {
        // Given
        let input = ".. plantuml::\n\n   A -> B\n   B -> C\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);

        let expected = HashedContent::new("A -> B\nB -> C".to_string());
        assert_eq!(doc.nodes[0], Node::Directive(Directive::PlantUml(expected)));
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_toctree_collects_diagnostic_for_invalid_option() {
        // Given
        let input = ".. toctree::\n   :invalid_opt:\n\n   foo";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.diagnostics.len(), 1);
        assert!(doc.diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
        assert!(doc.diagnostics[0].contains(":invalid_opt:"));

        if let Node::Directive(Directive::Toctree {
            paths,
            ignored_options,
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0], "foo");
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree directive");
        }
    }

    #[test]
    fn test_parse_toctree_whitelists_standard_options() {
        let standard_options = vec![
            "numbered",
            "caption: My Caption",
            "name: myname",
            "titlesonly",
            "glob",
            "reversed",
            "hidden",
            "includehidden",
        ];

        for opt in standard_options {
            // Given
            let input = format!(".. toctree::\n   :{opt}:\n\n   foo");

            // When
            let doc = parse("test.rst", &input);

            // Then
            assert!(
                doc.diagnostics.is_empty(),
                "Option :{opt} generated a diagnostic!"
            );

            if let Node::Directive(Directive::Toctree {
                ignored_options, ..
            }) = &doc.nodes[0]
            {
                assert_eq!(ignored_options.len(), 1);
                assert_eq!(ignored_options[0], format!(":{opt}:"));
            } else {
                panic!("Expected Toctree directive");
            }
        }
    }

    #[test]
    fn test_parse_toctree_collects_paths() {
        // Given
        let body_lines = vec!["path1", "path2/index"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1", "path2/index"]);
            assert_eq!(maxdepth, None);
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_parses_maxdepth_option() {
        // Given
        let body_lines = vec![":maxdepth: 2", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1"]);
            assert_eq!(maxdepth, Some(2));
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_ignores_known_options() {
        // Given
        let body_lines = vec![":hidden:", ":caption: Some text", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1"]);
            assert_eq!(maxdepth, None);
            assert_eq!(ignored_options.len(), 2);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_emits_diagnostic_for_unknown_option() {
        // Given
        let body_lines = vec![":unknown_opt:", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
        if let Directive::Toctree { paths, .. } = directive {
            assert_eq!(paths, vec!["path1"]);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_skips_blank_lines() {
        // Given
        let body_lines = vec!["path1", "  ", "", "path2"];
        let mut diagnostics = vec![];
        // When
        let directive = parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree { paths, .. } = directive {
            assert_eq!(paths, vec!["path1", "path2"]);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_code_block_directive_with_language() {
        // Given
        let input = ".. code-block:: python

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert_eq!(language.as_deref(), Some("python"));
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_code_block_directive_without_language() {
        // Given
        let input = ".. code-block::

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert!(language.is_none());
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_prefix_ignores_default_domain() {
        // Given
        let name = "c:function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(
            result,
            Some(rusty_sphinx_ast::ObjectType::C(
                rusty_sphinx_ast::CObjectType::Function
            ))
        );
    }

    #[test]
    fn test_resolve_domain_object_type_bare_name_uses_default_domain() {
        // Given
        let name = "function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(
            result,
            Some(rusty_sphinx_ast::ObjectType::C(
                rusty_sphinx_ast::CObjectType::Function
            ))
        );
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_domain_prefix() {
        // Given
        let name = "rust:function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_object_type() {
        // Given
        let name = "py:class";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }
}

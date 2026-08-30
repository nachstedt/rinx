use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Directive};

pub(super) fn parse_toctree(
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let mut paths = Vec::new();
    let mut maxdepth = None;
    let mut ignored_options = Vec::new();

    for (index, l) in body_lines.iter().enumerate() {
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
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::DirectiveToctreeUnknownOption,
                        format!(
                            "Invalid or non-standard Sphinx toctree option encountered: {line}"
                        ),
                        ctx.line_span(index, l),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Domain;
    use rusty_sphinx_ast::Node;

    #[test]
    fn test_parse_toctree_collects_diagnostic_for_invalid_option() {
        // Given
        let input = ".. toctree::\n   :invalid_opt:\n\n   foo";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.diagnostics.len(), 1);
        assert!(
            doc.diagnostics[0]
                .message
                .contains("Invalid or non-standard Sphinx toctree option")
        );
        assert!(doc.diagnostics[0].message.contains(":invalid_opt:"));

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
        let mut diagnostics = Diagnostics::default();
        // When
        let directive = parse_toctree(
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
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
        let mut diagnostics = Diagnostics::default();
        // When
        let directive = parse_toctree(
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
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
        let mut diagnostics = Diagnostics::default();
        // When
        let directive = parse_toctree(
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
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
        let mut diagnostics = Diagnostics::default();
        // When
        let directive = parse_toctree(
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("Invalid or non-standard Sphinx toctree option")
        );
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
        let mut diagnostics = Diagnostics::default();
        // When
        let directive = parse_toctree(
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        // Then
        if let Directive::Toctree { paths, .. } = directive {
            assert_eq!(paths, vec!["path1", "path2"]);
        } else {
            panic!("Expected Toctree");
        }
    }
}

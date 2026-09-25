use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rinx_ast::DomainObjectBody;

/// Parses a `.. py:module::` body: strips `:platform:`/`:synopsis:`/
/// `:deprecated:` option lines off the front before parsing the rest as the
/// docstring body.
pub(crate) fn parse_py_module(
    name: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (platform, synopsis, deprecated, options_consumed) =
        extract_module_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    DomainObjectBody::PyModule {
        name,
        platform,
        synopsis,
        deprecated,
        body,
    }
}

/// Extracts `.. py:module::`-specific options (`:platform:`, `:synopsis:`,
/// `:deprecated:`) from the leading lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_module_options(lines: &[String]) -> (Option<String>, Option<String>, bool, usize) {
    let mut platform = None;
    let mut synopsis = None;
    let mut deprecated = false;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(":platform:") {
            platform = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":synopsis:") {
            synopsis = Some(rest.trim().to_string());
        } else if trimmed == ":deprecated:" {
            deprecated = true;
        } else {
            break;
        }
        consumed += 1;
    }

    (platform, synopsis, deprecated, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Directive, Node};

    #[test]
    fn test_extract_module_options_parses_all_three_options() {
        // Given
        let lines = vec![
            ":platform: Unix, Windows".to_string(),
            ":synopsis: Greeting utilities.".to_string(),
            ":deprecated:".to_string(),
            String::new(),
            "A module of greetings.".to_string(),
        ];

        // When
        let (platform, synopsis, deprecated, consumed) = extract_module_options(&lines);

        // Then
        assert_eq!(platform.as_deref(), Some("Unix, Windows"));
        assert_eq!(synopsis.as_deref(), Some("Greeting utilities."));
        assert!(deprecated);
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_extract_module_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![
            ":platform: Unix".to_string(),
            "A module of greetings.".to_string(),
        ];

        // When
        let (platform, synopsis, deprecated, consumed) = extract_module_options(&lines);

        // Then
        assert_eq!(platform.as_deref(), Some("Unix"));
        assert_eq!(synopsis, None);
        assert!(!deprecated);
        assert_eq!(consumed, 1);
    }
    #[test]
    fn test_extract_module_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["A module of greetings.".to_string()];

        // When
        let (platform, synopsis, deprecated, consumed) = extract_module_options(&lines);

        // Then
        assert_eq!(platform, None);
        assert_eq!(synopsis, None);
        assert!(!deprecated);
        assert_eq!(consumed, 0);
    }
    #[test]
    fn test_parse_creates_py_module_domain_object() {
        // Given
        let input = ".. py:module:: greetings\n\n   A module of greetings.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            name,
            platform,
            synopsis,
            deprecated,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(name, "greetings");
            assert_eq!(platform, &None);
            assert_eq!(synopsis, &None);
            assert!(!deprecated);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_module_with_platform_synopsis_and_deprecated_options() {
        // Given
        let input = ".. py:module:: greetings\n   :platform: Unix, Windows\n   :synopsis: Greeting utilities.\n   :deprecated:\n\n   A module of greetings.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            platform,
            synopsis,
            deprecated,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(platform.as_deref(), Some("Unix, Windows"));
            assert_eq!(synopsis.as_deref(), Some("Greeting utilities."));
            assert!(*deprecated);
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_module_options_in_any_order_with_no_body() {
        // Given — synopsis and deprecated before platform, and no docstring body
        let input = ".. py:module:: greetings\n   :synopsis: Greeting utilities.\n   :deprecated:\n   :platform: Unix";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            platform,
            synopsis,
            deprecated,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(platform.as_deref(), Some("Unix"));
            assert_eq!(synopsis.as_deref(), Some("Greeting utilities."));
            assert!(*deprecated);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_module_treats_a_following_line_as_body_not_a_second_name() {
        // Given — real Sphinx's `module` directive takes exactly one
        // argument, so an immediately following line is body content even
        // though it looks like a continuation.
        let input = ".. py:module:: greetings\n   A module of greetings.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            name,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(name, "greetings");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }
}

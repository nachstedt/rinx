use super::dispatch::parse_module_option_line;
use crate::blocks::parse_blocks;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{Domain, DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:data::` body: strips `:type:`/`:value:` option lines off
/// the front before parsing the rest as the docstring body.
pub(crate) fn parse_py_data(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (type_, value, module, options_consumed) = extract_data_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::PyData {
        module,
        signatures,
        type_,
        value,
        body,
    }
}

/// Extracts `.. py:data::`-specific options (`:type:`, `:value:`, `:module:`)
/// from the leading lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_data_options(
    lines: &[String],
) -> (Option<String>, Option<String>, Option<String>, usize) {
    let mut type_ = None;
    let mut value = None;
    let mut module = None;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(":type:") {
            type_ = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":value:") {
            value = Some(rest.trim().to_string());
        } else if let Some(module_value) = parse_module_option_line(trimmed) {
            module = Some(module_value);
        } else {
            break;
        }
        consumed += 1;
    }

    (type_, value, module, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{Directive, Node};

    #[test]
    fn test_extract_data_options_parses_both_options() {
        // Given
        let lines = vec![
            ":type: int".to_string(),
            ":value: 30".to_string(),
            String::new(),
            "The default timeout in seconds.".to_string(),
        ];

        // When
        let (type_, value, module, consumed) = extract_data_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("int"));
        assert_eq!(value.as_deref(), Some("30"));
        assert_eq!(module, None);
        assert_eq!(consumed, 2);
    }
    #[test]
    fn test_extract_data_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![
            ":type: int".to_string(),
            "The default timeout in seconds.".to_string(),
        ];

        // When
        let (type_, value, module, consumed) = extract_data_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("int"));
        assert_eq!(value, None);
        assert_eq!(module, None);
        assert_eq!(consumed, 1);
    }
    #[test]
    fn test_extract_data_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["The default timeout in seconds.".to_string()];

        // When
        let (type_, value, module, consumed) = extract_data_options(&lines);

        // Then
        assert_eq!(type_, None);
        assert_eq!(value, None);
        assert_eq!(module, None);
        assert_eq!(consumed, 0);
    }
    #[test]
    fn test_extract_data_options_parses_module_option_alongside_others() {
        // Given — the `known_bugs.md` shape: `ctypes.util`'s constants
        // documented under a different module than the enclosing `.. module::`.
        let lines = vec![
            ":type: int".to_string(),
            ":module: ctypes.util".to_string(),
            ":value: 30".to_string(),
            String::new(),
            "The default timeout in seconds.".to_string(),
        ];

        // When
        let (type_, value, module, consumed) = extract_data_options(&lines);

        // Then — order-independent, like the other options.
        assert_eq!(type_.as_deref(), Some("int"));
        assert_eq!(value.as_deref(), Some("30"));
        assert_eq!(module.as_deref(), Some("ctypes.util"));
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_parse_creates_py_data_domain_object() {
        // Given
        let input = ".. py:data:: DEFAULT_TIMEOUT\n\n   The default timeout in seconds.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            signatures,
            type_,
            value,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["DEFAULT_TIMEOUT"]);
            assert_eq!(type_, &None);
            assert_eq!(value, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_data_with_type_and_value_options() {
        // Given
        let input = ".. py:data:: DEFAULT_TIMEOUT\n   :type: int\n   :value: 30\n\n   The default timeout in seconds.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            type_,
            value,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(type_.as_deref(), Some("int"));
            assert_eq!(value.as_deref(), Some("30"));
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_data_collects_every_argument_line_as_a_signature() {
        // Given — the shape `library/socket.rst` uses to declare three
        // aliases for one documented object, which real Sphinx indexes as
        // three separate targets sharing one docstring.
        let input = ".. py:data:: AF_UNIX\n             AF_INET\n             AF_INET6\n\n   The address families.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            signatures,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["AF_UNIX", "AF_INET", "AF_INET6"]);
            // The continuation lines must not leak into the docstring.
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_data_collects_signatures_before_option_lines() {
        // Given — continuation lines come first, then the option block; both
        // have to be recognized.
        let input = ".. py:data:: A\n             ASCII\n   :type: int\n\n   The ASCII flag.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            signatures,
            type_,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["A", "ASCII"]);
            assert_eq!(type_.as_deref(), Some("int"));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_data_options_in_any_order_with_no_body() {
        // Given — value before type, and no docstring body
        let input = ".. py:data:: DEFAULT_TIMEOUT\n   :value: 30\n   :type: int";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            type_,
            value,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(type_.as_deref(), Some("int"));
            assert_eq!(value.as_deref(), Some("30"));
            assert!(body.is_empty());
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }
}

use super::dispatch::parse_module_option_line;
use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:attribute::` body: strips `:type:`/`:value:`/`:canonical:`
/// option lines off the front before parsing the rest as the docstring body.
pub(crate) fn parse_py_attribute(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (type_, value, canonical, module, options_consumed) =
        extract_attribute_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    DomainObjectBody::PyAttribute {
        module,
        signatures,
        type_,
        value,
        canonical,
        body,
    }
}

/// Extracts `.. py:attribute::`-specific options (`:type:`, `:value:`,
/// `:canonical:`) from the leading lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_attribute_options(
    lines: &[String],
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    usize,
) {
    let mut type_ = None;
    let mut value = None;
    let mut canonical = None;
    let mut module = None;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(":type:") {
            type_ = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":value:") {
            value = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":canonical:") {
            canonical = Some(rest.trim().to_string());
        } else if let Some(module_value) = parse_module_option_line(trimmed) {
            module = Some(module_value);
        } else {
            break;
        }
        consumed += 1;
    }

    (type_, value, canonical, module, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Directive, Node};

    #[test]
    fn test_extract_attribute_options_parses_all_three_options() {
        // Given
        let lines = vec![
            ":type: str".to_string(),
            ":value: \"anonymous\"".to_string(),
            ":canonical: mymodule.MyClass.name".to_string(),
            String::new(),
            "The greeter's name.".to_string(),
        ];

        // When
        let (type_, value, canonical, module, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("str"));
        assert_eq!(value.as_deref(), Some("\"anonymous\""));
        assert_eq!(canonical.as_deref(), Some("mymodule.MyClass.name"));
        assert_eq!(module, None);
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_extract_attribute_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![":type: str".to_string(), "The greeter's name.".to_string()];

        // When
        let (type_, value, canonical, module, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("str"));
        assert_eq!(value, None);
        assert_eq!(canonical, None);
        assert_eq!(module, None);
        assert_eq!(consumed, 1);
    }
    #[test]
    fn test_extract_attribute_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["The greeter's name.".to_string()];

        // When
        let (type_, value, canonical, module, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_, None);
        assert_eq!(value, None);
        assert_eq!(canonical, None);
        assert_eq!(module, None);
        assert_eq!(consumed, 0);
    }
    #[test]
    fn test_extract_attribute_options_parses_options_in_any_order() {
        // Given
        let lines = vec![
            ":canonical: mymodule.MyClass.name".to_string(),
            ":value: \"anonymous\"".to_string(),
            ":type: str".to_string(),
        ];

        // When
        let (type_, value, canonical, module, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("str"));
        assert_eq!(value.as_deref(), Some("\"anonymous\""));
        assert_eq!(canonical.as_deref(), Some("mymodule.MyClass.name"));
        assert_eq!(module, None);
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_extract_attribute_options_parses_module_option_alongside_others() {
        // Given
        let lines = vec![
            ":type: str".to_string(),
            ":module: mymodule.other".to_string(),
            ":value: \"anonymous\"".to_string(),
        ];

        // When
        let (type_, value, canonical, module, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("str"));
        assert_eq!(value.as_deref(), Some("\"anonymous\""));
        assert_eq!(canonical, None);
        assert_eq!(module.as_deref(), Some("mymodule.other"));
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_parse_creates_py_attribute_domain_object() {
        // Given
        let input = ".. py:attribute:: Greeter.name\n\n   The greeter's name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyAttribute {
            signatures,
            type_,
            value,
            canonical,
            module: _,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter.name"]);
            assert_eq!(type_, &None);
            assert_eq!(value, &None);
            assert_eq!(canonical, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyAttribute, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_attribute_with_type_value_and_canonical_options() {
        // Given
        let input = ".. py:attribute:: Greeter.name\n   :type: str\n   :value: \"anonymous\"\n   :canonical: mymodule.MyClass.name\n\n   The greeter's name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyAttribute {
            type_,
            value,
            canonical,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(type_.as_deref(), Some("str"));
            assert_eq!(value.as_deref(), Some("\"anonymous\""));
            assert_eq!(canonical.as_deref(), Some("mymodule.MyClass.name"));
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyAttribute, got {:?}", doc.nodes[0]);
        }
    }
}

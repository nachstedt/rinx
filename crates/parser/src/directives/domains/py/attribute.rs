use super::super::body::{ObjectOptions, parse_object_body};
use super::dispatch::read_module_option;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;
use crate::headings::Adornment;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:attribute::` body: its option block, then its content.
pub(crate) fn parse_py_attribute(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let parsed = parse_object_body::<AttributeOptions>(
        "py:attribute",
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    );
    let options = parsed.options;
    DomainObjectBody::PyAttribute {
        flags: parsed.flags,
        module: options.module,
        signatures,
        type_: options.type_,
        value: options.value,
        canonical: options.canonical,
        body: parsed.content,
    }
}

/// The options `.. py:attribute::` takes beyond the object-description
/// flags: `:type:`, `:value:`, `:canonical:`, and the `:module:` every `py`
/// object takes.
#[derive(Debug, Default, PartialEq, Eq)]
struct AttributeOptions {
    type_: Option<String>,
    value: Option<String>,
    canonical: Option<String>,
    module: Option<String>,
}

impl ObjectOptions for AttributeOptions {
    fn read(&mut self, line: &OptionLine) -> bool {
        let field = match line.name.as_str() {
            "type" => &mut self.type_,
            "value" => &mut self.value,
            "canonical" => &mut self.canonical,
            _ => return read_module_option(line, &mut self.module),
        };
        *field = Some(line.value.clone());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::body::test_support::read_options;
    use super::*;
    use crate::parse;
    use rinx_ast::{Directive, Node};

    #[test]
    fn test_attribute_options_read_every_option_in_any_order() {
        // Given
        let body = [
            ":canonical: mymodule.MyClass.name",
            ":module: mymodule.other",
            ":value: \"anonymous\"",
            ":type: str",
            "",
            "The greeter's name.",
        ];

        // When
        let (options, _, unrecognized) = read_options::<AttributeOptions>(&body);

        // Then
        assert_eq!(
            options,
            AttributeOptions {
                type_: Some("str".to_string()),
                value: Some("\"anonymous\"".to_string()),
                canonical: Some("mymodule.MyClass.name".to_string()),
                module: Some("mymodule.other".to_string()),
            }
        );
        assert!(unrecognized.is_empty());
    }
    #[test]
    fn test_attribute_options_read_nothing_from_a_plain_body() {
        // Given
        let body = ["The greeter's name."];

        // When
        let (options, _, unrecognized) = read_options::<AttributeOptions>(&body);

        // Then
        assert_eq!(options, AttributeOptions::default());
        assert!(unrecognized.is_empty());
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
            flags: _,
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

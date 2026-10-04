use super::super::body::{ObjectOptions, parse_object_body};
use super::dispatch::read_module_option;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;
use crate::headings::Adornment;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:data::` body: its option block, then its content.
pub(crate) fn parse_py_data(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let parsed =
        parse_object_body::<DataOptions>("py:data", body_lines, adornment_order, diagnostics, ctx);
    DomainObjectBody::PyData {
        flags: parsed.flags,
        module: parsed.options.module,
        signatures,
        type_: parsed.options.type_,
        value: parsed.options.value,
        body: parsed.content,
    }
}

/// The options `.. py:data::` takes beyond the object-description flags:
/// `:type:`, `:value:`, and the `:module:` every `py` object takes.
#[derive(Debug, Default, PartialEq, Eq)]
struct DataOptions {
    type_: Option<String>,
    value: Option<String>,
    module: Option<String>,
}

impl ObjectOptions for DataOptions {
    fn read(&mut self, line: &OptionLine) -> bool {
        let field = match line.name.as_str() {
            "type" => &mut self.type_,
            "value" => &mut self.value,
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
    fn test_data_options_read_type_value_and_module_in_any_order() {
        // Given
        let body = [
            ":value: 30",
            ":module: socket",
            ":type: int",
            "",
            "The timeout.",
        ];

        // When
        let (options, _, unrecognized) = read_options::<DataOptions>(&body);

        // Then
        assert_eq!(
            options,
            DataOptions {
                type_: Some("int".to_string()),
                value: Some("30".to_string()),
                module: Some("socket".to_string()),
            }
        );
        assert!(unrecognized.is_empty());
    }
    #[test]
    fn test_data_options_read_nothing_from_a_plain_body() {
        // Given
        let body = ["The default timeout in seconds."];

        // When
        let (options, _, unrecognized) = read_options::<DataOptions>(&body);

        // Then
        assert_eq!(options, DataOptions::default());
        assert!(unrecognized.is_empty());
    }
    #[test]
    fn test_data_options_leave_an_attribute_only_option() {
        // Given
        let body = [":canonical: pkg.VALUE"];

        // When
        let (_, _, unrecognized) = read_options::<DataOptions>(&body);

        // Then
        assert_eq!(unrecognized, ["canonical"]);
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
            flags: _,
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

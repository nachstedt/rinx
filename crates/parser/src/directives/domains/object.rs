//! The top-level domain-object dispatch: routes a resolved
//! [`DirectiveObjectType`] to the `c`, `std` or `py` domain's own parser.

use crate::diagnostics::Diagnostics;
use crate::directives::invocation::DirectiveInvocation;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

use crate::headings::Adornment;

use super::c::parse_c_domain_object;
use super::object_type::DirectiveObjectType;
use super::py::parse_py_domain_object;
use super::std_::cmdoption::parse_cmdoption;

/// Parses a domain object, its signatures read from its argument: Sphinx's
/// `ObjectDescription` takes one argument keeping the rest of its lines, and
/// splits it into one signature per line, so further signatures continue it
/// on the lines below the marker. A `py:module` takes a single word instead.
pub(crate) fn parse_domain_object(
    object_type: DirectiveObjectType,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> DomainObjectBody {
    let signature_span = invocation.span;
    let mut lines = invocation
        .argument(0)
        .lines()
        .map(|line| line.trim().to_string());
    let argument = lines.next().unwrap_or_default();
    let signatures = join_escaped_line_breaks(argument, lines.collect());
    match object_type {
        DirectiveObjectType::CFunction
        | DirectiveObjectType::CMacro
        | DirectiveObjectType::CStruct
        | DirectiveObjectType::CUnion
        | DirectiveObjectType::CMember
        | DirectiveObjectType::CType => parse_c_domain_object(
            object_type,
            &signatures,
            invocation,
            adornment_order,
            diagnostics,
            signature_span,
        ),
        DirectiveObjectType::StdCmdoption => parse_cmdoption(
            signatures,
            invocation,
            adornment_order,
            diagnostics,
            signature_span,
        ),
        _ => parse_py_domain_object(
            object_type,
            signatures,
            invocation,
            adornment_order,
            diagnostics,
        ),
    }
}

/// The signatures a directive declares, with every line that ends in a
/// backslash joined to the line below it.
///
/// Sphinx's `get_signatures` removes each backslash-newline from the argument
/// before splitting it into one signature per line, so a long signature can
/// wrap (`asyncio-stream.rst` wraps `start_unix_server` over three lines). Read
/// line by line instead, each wrapped piece became a signature of its own, and
/// a junk name such as `asyncio.\` was indexed. The pieces join with one
/// space: the continuation's indentation is what Sphinx keeps between them,
/// and a rendered signature shows any run of it as one.
///
/// A backslash ending the *last* line escapes no line break, so — as in
/// Sphinx — it stays.
fn join_escaped_line_breaks(
    argument: String,
    continuations: Vec<String>,
) -> NonEmptyVector<String> {
    let mut first = argument;
    let mut rest: Vec<String> = Vec::new();
    for line in continuations {
        let previous = rest.last_mut().unwrap_or(&mut first);
        if let Some(wrapped) = previous.strip_suffix('\\') {
            *previous = format!("{} {line}", wrapped.trim_end());
        } else {
            rest.push(line);
        }
    }
    NonEmptyVector::new(first, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParseCtx;
    use crate::parse;
    use rinx_ast::Domain;
    use rinx_ast::{Directive, Node};

    #[test]
    fn test_parse_domain_object_with_paragraph_body() {
        // Given
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   Greets the given name."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let ctx = ParseCtx::with_domain(Domain::Py);
        let written = crate::directives::reader::written_after_blank(&body_lines);
        let invocation = crate::directives::reader::test_invocation(
            "function",
            super::super::object_type::object_spec(object_type),
            &signature,
            &written,
            &mut diagnostics,
            &ctx,
        );
        let domain_object = parse_domain_object(
            object_type,
            &invocation,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        if let DomainObjectBody::PyFunction {
            is_async: _,
            flags: _,
            signatures,
            is_decorator,
            module: _,
            body,
            ..
        } = domain_object
        {
            assert_eq!(signatures.as_slice(), [signature]);
            assert!(!is_decorator);
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }
    #[test]
    fn test_parse_domain_object_with_bullet_list_body() {
        // Given
        let object_type = DirectiveObjectType::CFunction;
        let signature = "int add(int a, int b)".to_string();
        let body_lines = vec!["   * Adds two numbers.", "   * Returns their sum."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let ctx = ParseCtx::with_domain(Domain::C);
        let written = crate::directives::reader::written_after_blank(&body_lines);
        let invocation = crate::directives::reader::test_invocation(
            "c:function",
            super::super::object_type::object_spec(object_type),
            &signature,
            &written,
            &mut diagnostics,
            &ctx,
        );
        let domain_object = parse_domain_object(
            object_type,
            &invocation,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        if let DomainObjectBody::CFunction { body, .. } = domain_object {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::BulletList { .. }));
        } else {
            panic!("Expected CFunction, got {domain_object:?}");
        }
    }
    #[test]
    fn test_parse_domain_object_with_empty_body() {
        // Given
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines: Vec<&str> = vec![];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let ctx = ParseCtx::with_domain(Domain::Py);
        let written = crate::directives::reader::written_after_blank(&body_lines);
        let invocation = crate::directives::reader::test_invocation(
            "function",
            super::super::object_type::object_spec(object_type),
            &signature,
            &written,
            &mut diagnostics,
            &ctx,
        );
        let domain_object = parse_domain_object(
            object_type,
            &invocation,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        if let DomainObjectBody::PyFunction { body, .. } = domain_object {
            assert!(body.is_empty());
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }
    #[test]
    fn test_parse_domain_object_strips_common_indentation() {
        // Given
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines = vec!["     Indented more than needed."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let ctx = ParseCtx::with_domain(Domain::Py);
        let written = crate::directives::reader::written_after_blank(&body_lines);
        let invocation = crate::directives::reader::test_invocation(
            "function",
            super::super::object_type::object_spec(object_type),
            &signature,
            &written,
            &mut diagnostics,
            &ctx,
        );
        let domain_object = parse_domain_object(
            object_type,
            &invocation,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        if let DomainObjectBody::PyFunction { body, .. } = domain_object {
            if let Node::Paragraph(inlines) = &body[0] {
                assert_eq!(
                    inlines[0],
                    rinx_ast::InlineNode::Text("Indented more than needed.".to_string())
                );
            } else {
                panic!("Expected Paragraph, got {:?}", body[0]);
            }
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }
    #[test]
    fn test_parse_domain_object_does_not_panic_on_multi_byte_char_in_a_short_line() {
        // Given a body whose first line has a 3-char indent and a second,
        // less-indented line containing a multi-byte character at the byte
        // offset the old byte-index slicing would have panicked on
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   First line normal indent.", "  éfoo"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When parsing the domain object body
        let ctx = ParseCtx::with_domain(Domain::Py);
        let written = crate::directives::reader::written_after_blank(&body_lines);
        let invocation = crate::directives::reader::test_invocation(
            "function",
            super::super::object_type::object_spec(object_type),
            &signature,
            &written,
            &mut diagnostics,
            &ctx,
        );
        let domain_object = parse_domain_object(
            object_type,
            &invocation,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then it does not panic
        if let DomainObjectBody::PyFunction { .. } = domain_object {
            // no-op: reaching here means parsing succeeded without panicking
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }
    #[test]
    fn test_parse_unknown_domain_prefix_falls_through_to_unknown() {
        // Given
        let input = ".. rust:function:: greet(name)\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(Directive::Unknown { .. })),
            "Expected Unknown directive, got {:?}",
            doc.nodes[0]
        );
    }
    #[test]
    fn test_parse_unknown_object_type_in_known_domain_falls_through_to_unknown() {
        // Given
        let input = ".. py:struct:: Greeter\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(Directive::Unknown { .. })),
            "Expected Unknown directive, got {:?}",
            doc.nodes[0]
        );
    }

    #[test]
    fn test_join_escaped_line_breaks_joins_a_wrapped_signature() {
        // Given — `asyncio-stream.rst`'s `start_unix_server`, shortened.
        let argument = "start_unix_server(client_connected_cb, path=None, \\".to_string();
        let continuations = vec![
            "*, limit=None, \\".to_string(),
            "cleanup_socket=True)".to_string(),
        ];

        // When
        let signatures = join_escaped_line_breaks(argument, continuations);

        // Then
        assert_eq!(
            signatures.as_slice(),
            [
                "start_unix_server(client_connected_cb, path=None, *, limit=None, cleanup_socket=True)"
            ]
        );
    }

    #[test]
    fn test_join_escaped_line_breaks_keeps_unescaped_lines_apart() {
        // Given — two signatures, the first of them wrapped.
        let argument = "spawnl(mode, \\".to_string();
        let continuations = vec!["file)".to_string(), "spawnle(mode, file, env)".to_string()];

        // When
        let signatures = join_escaped_line_breaks(argument, continuations);

        // Then
        assert_eq!(
            signatures.as_slice(),
            ["spawnl(mode, file)", "spawnle(mode, file, env)"]
        );
    }

    #[test]
    fn test_join_escaped_line_breaks_keeps_a_backslash_ending_the_last_line() {
        // Given
        let argument = "f(a, \\".to_string();

        // When
        let signatures = join_escaped_line_breaks(argument, Vec::new());

        // Then
        assert_eq!(signatures.as_slice(), ["f(a, \\"]);
    }

    #[test]
    fn test_parse_indexes_one_name_for_a_wrapped_signature() {
        // Given
        let input =
            ".. function:: start_unix_server(cb, \\\n                 path=None)\n\n   Starts.";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Some(Node::Directive(Directive::DomainObject { object, .. })) = doc.nodes.first()
        else {
            panic!("Expected a domain object, got {:?}", doc.nodes);
        };
        assert_eq!(object.names().as_slice(), ["start_unix_server"]);
    }
}

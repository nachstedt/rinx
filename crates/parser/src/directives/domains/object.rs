//! The top-level domain-object dispatch: routes a resolved
//! [`DirectiveObjectType`] to the `c`, `std` or `py` domain's own parser.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rinx_ast::{DomainObjectBody, NonEmptyVector, Span};

use crate::headings::Adornment;

use super::c::parse_c_domain_object;
use super::object_type::DirectiveObjectType;
use super::py::parse_py_domain_object;
use super::std_::cmdoption::parse_cmdoption;

/// What a domain-object directive declared, and where it declared it.
///
/// The three parts arrive together and stay together: a directive's argument
/// and its continuation lines are one list of signatures split across lines
/// (see [`crate::directives::body::collect_argument_continuation_lines`]), and
/// `span` is the marker line they were all written on. It is worth carrying
/// because it is *not* derivable downstream — the marker sits one line above
/// the body every later `ParseCtx` is positioned at.
pub(crate) struct DirectiveSignatures {
    /// The directive's own argument, e.g. `AF_UNIX` in `.. data:: AF_UNIX`.
    pub(crate) argument: String,
    /// Any further signature lines written below the marker.
    pub(crate) continuations: Vec<String>,
    /// The marker line, for diagnostics about the signatures themselves.
    pub(crate) span: Option<Span>,
}

pub(crate) fn parse_domain_object(
    object_type: DirectiveObjectType,
    declared: DirectiveSignatures,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let DirectiveSignatures {
        argument,
        continuations,
        span: signature_span,
    } = declared;
    let signatures = join_escaped_line_breaks(argument, continuations);
    match object_type {
        DirectiveObjectType::CFunction
        | DirectiveObjectType::CMacro
        | DirectiveObjectType::CStruct
        | DirectiveObjectType::CUnion
        | DirectiveObjectType::CMember
        | DirectiveObjectType::CType => parse_c_domain_object(
            object_type,
            &signatures,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
            signature_span,
        ),
        DirectiveObjectType::StdCmdoption => parse_cmdoption(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
            signature_span,
        ),
        _ => parse_py_domain_object(
            object_type,
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
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
        let domain_object = parse_domain_object(
            object_type,
            DirectiveSignatures {
                argument: signature.clone(),
                continuations: Vec::new(),
                span: None,
            },
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        if let DomainObjectBody::PyFunction {
            is_async: _,
            flags: _,
            signatures,
            is_decorator,
            module: _,
            body,
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
        let domain_object = parse_domain_object(
            object_type,
            DirectiveSignatures {
                argument: signature,
                continuations: Vec::new(),
                span: None,
            },
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::C),
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
        let domain_object = parse_domain_object(
            object_type,
            DirectiveSignatures {
                argument: signature,
                continuations: Vec::new(),
                span: None,
            },
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
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
        let domain_object = parse_domain_object(
            object_type,
            DirectiveSignatures {
                argument: signature,
                continuations: Vec::new(),
                span: None,
            },
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
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
        let domain_object = parse_domain_object(
            object_type,
            DirectiveSignatures {
                argument: signature,
                continuations: Vec::new(),
                span: None,
            },
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
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
        let Some(Node::Directive(Directive::DomainObject(object))) = doc.nodes.first() else {
            panic!("Expected a domain object, got {:?}", doc.nodes);
        };
        assert_eq!(object.names().as_slice(), ["start_unix_server"]);
    }
}

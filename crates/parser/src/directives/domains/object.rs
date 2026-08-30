//! The top-level domain-object dispatch: routes a resolved
//! [`DirectiveObjectType`] to the `c`, `std` or `py` domain's own parser.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{DomainObjectBody, NonEmptyVector, Span};

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
    let signatures = NonEmptyVector::new(argument, continuations);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Domain;
    use rusty_sphinx_ast::{Directive, Node};

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
                    rusty_sphinx_ast::InlineNode::Text("Indented more than needed.".to_string())
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
}

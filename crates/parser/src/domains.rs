use super::blocks::parse_blocks;
use super::bullet_list::unindent_body_lines;
use super::directives::domain_object_type::DirectiveObjectType;
use super::headings::Adornment;
use rusty_sphinx_ast::{CSignature, Domain, DomainObjectBody, NameSource, Node, NonEmptyVector};

mod c_objects;
mod cmdoption;
mod py;

use c_objects::{parse_c_member, parse_c_struct, parse_c_type, parse_c_union};
use cmdoption::parse_cmdoption;
use py::attribute::parse_py_attribute;
use py::class::{parse_py_class, parse_py_exception};
use py::data::parse_py_data;
use py::function::parse_py_function;
use py::method::{ForcedMethodFlags, parse_py_method};
use py::module::parse_py_module;

pub(super) fn parse_domain_object(
    object_type: DirectiveObjectType,
    argument: String,
    continuations: Vec<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
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
            default_domain,
        ),
        DirectiveObjectType::StdCmdoption => parse_cmdoption(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        _ => parse_py_domain_object(
            object_type,
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
    }
}

/// Dispatches the ten `py`-domain object types (including the
/// `classmethod`/`staticmethod`/`decorator`/`decoratormethod` directive-name
/// aliases, which carry no `ast::ObjectType`/[`DomainObjectBody`] variant of
/// their own) to their respective parsers — factored out of
/// [`parse_domain_object`] purely to keep that function's line count
/// manageable, mirroring [`parse_c_domain_object`]'s split for the `c`
/// domain. Only ever called with a non-`c` variant (enforced by
/// `parse_domain_object`'s own match arm), so the `C*` variants are
/// unreachable here.
fn parse_py_domain_object(
    object_type: DirectiveObjectType,
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    match object_type {
        DirectiveObjectType::PyFunction => parse_py_function(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
            false,
        ),
        DirectiveObjectType::PyDecorator => parse_py_function(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
            true,
        ),
        DirectiveObjectType::PyModule => parse_py_module(
            signatures.first().clone(),
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyData => parse_py_data(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyMethod
        | DirectiveObjectType::PyClassmethod
        | DirectiveObjectType::PyStaticmethod
        | DirectiveObjectType::PyDecoratorMethod => parse_py_method(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
            ForcedMethodFlags::for_directive(object_type),
        ),
        DirectiveObjectType::PyClass => parse_py_class(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyException => parse_py_exception(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyAttribute => parse_py_attribute(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::CFunction
        | DirectiveObjectType::CMacro
        | DirectiveObjectType::CStruct
        | DirectiveObjectType::CUnion
        | DirectiveObjectType::CMember
        | DirectiveObjectType::CType
        | DirectiveObjectType::StdCmdoption => {
            unreachable!("parse_py_domain_object called with a non-py object type")
        }
    }
}

/// Derives each signature's declared name up front, so it is computed once
/// here at parse time rather than re-derived in every later phase.
///
/// Signatures whose declaration grammar isn't covered still yield a name (via
/// [`CSignature`]'s heuristic fallback) so no cross-reference target is lost;
/// each one emits a diagnostic instead, which is what makes the remaining
/// grammar gaps countable against a real corpus.
pub(super) fn parse_c_signatures(
    signatures: &NonEmptyVector<String>,
    diagnostics: &mut Vec<String>,
) -> NonEmptyVector<CSignature> {
    let parsed = signatures.map(|text| CSignature::parse(text.clone()));
    for signature in parsed.as_slice() {
        if signature.name_source() == NameSource::Fallback {
            diagnostics.push(format!(
                "c signature: could not parse declaration '{}'; fell back to the name heuristic, which read '{}'",
                signature.text(),
                signature.name()
            ));
        }
    }
    parsed
}

/// Dispatches the six `c`-domain object types to their respective parsers —
/// factored out of [`parse_domain_object`] purely to keep that function's
/// line count manageable. Only ever called with a `DirectiveObjectType::C*`
/// variant (enforced by `parse_domain_object`'s own match arm), so the
/// non-`c` variants are unreachable here.
fn parse_c_domain_object(
    object_type: DirectiveObjectType,
    signatures: &NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let signatures = parse_c_signatures(signatures, diagnostics);
    match object_type {
        DirectiveObjectType::CFunction => DomainObjectBody::CFunction {
            signatures,
            body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
        },
        DirectiveObjectType::CMacro => DomainObjectBody::CMacro {
            signatures,
            body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
        },
        DirectiveObjectType::CStruct => parse_c_struct(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::CUnion => parse_c_union(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::CMember => parse_c_member(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::CType => parse_c_type(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyFunction
        | DirectiveObjectType::PyDecorator
        | DirectiveObjectType::PyModule
        | DirectiveObjectType::PyData
        | DirectiveObjectType::PyMethod
        | DirectiveObjectType::PyClassmethod
        | DirectiveObjectType::PyStaticmethod
        | DirectiveObjectType::PyDecoratorMethod
        | DirectiveObjectType::PyClass
        | DirectiveObjectType::PyAttribute
        | DirectiveObjectType::PyException
        | DirectiveObjectType::StdCmdoption => {
            unreachable!("parse_c_domain_object called with a non-c object type")
        }
    }
}

/// Strips the body's common leading indentation and parses the remaining
/// lines as block-level nodes. Shared by every domain object type that has
/// no directive-specific options to strip out first.
pub(super) fn parse_body(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Vec<Node> {
    let unindented_lines = unindent_body_lines(body_lines);
    let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
    parse_blocks(&body_content, adornment_order, diagnostics, default_domain)
}

/// Parses a single leading option line as `:module:`, if it is one, e.g.
/// `":module: multiprocessing.managers"` -> `Some("multiprocessing.managers")`
/// (or `Some("")` for a bare `:module:` with no value). Shared by every
/// per-object-type extractor below, since real Sphinx's `:module:` option is
/// common to every `py:*` object-description directive except `py:module`
/// itself (which is not a `PyObject` and has its own, disjoint
/// `platform`/`synopsis`/`deprecated` option set — see
/// [`extract_module_options`]).
pub(super) fn parse_module_option_line(trimmed: &str) -> Option<String> {
    trimmed
        .strip_prefix(":module:")
        .map(|rest| rest.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Directive;

    #[test]
    fn test_parse_domain_object_with_paragraph_body() {
        // Given
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   Greets the given name."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature.clone(),
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
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
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::C,
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
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
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
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
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
        let mut diagnostics = Vec::new();

        // When parsing the domain object body
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
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

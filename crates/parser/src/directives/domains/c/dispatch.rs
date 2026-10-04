//! The `c`-domain object-type dispatch and up-front signature parsing.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rinx_ast::{
    CSignature, Diagnostic, DiagnosticCode, DomainObjectBody, NameSource, NonEmptyVector, Span,
};

use crate::headings::Adornment;

use super::super::body::parse_object_body;
use super::super::object_type::DirectiveObjectType;
use super::member::parse_c_member;
use super::struct_::parse_c_struct;
use super::type_::parse_c_type;
use super::union::parse_c_union;

/// Dispatches the six `c`-domain object types to their respective parsers.
/// Only ever called with a `DirectiveObjectType::C*` variant (enforced by
/// [`super::super::object::parse_domain_object`]'s own match arm), so the non-`c`
/// variants are unreachable here.
pub(crate) fn parse_c_domain_object(
    object_type: DirectiveObjectType,
    signatures: &NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
    signature_span: Option<Span>,
) -> DomainObjectBody {
    let signatures = parse_c_signatures(signatures, diagnostics, signature_span);
    match object_type {
        DirectiveObjectType::CFunction => {
            let parsed = parse_object_body::<()>(
                "c:function",
                body_lines,
                adornment_order,
                diagnostics,
                ctx,
            );
            DomainObjectBody::CFunction {
                signatures,
                flags: parsed.flags,
                body: parsed.content,
            }
        }
        DirectiveObjectType::CMacro => {
            let parsed =
                parse_object_body::<()>("c:macro", body_lines, adornment_order, diagnostics, ctx);
            DomainObjectBody::CMacro {
                signatures,
                flags: parsed.flags,
                body: parsed.content,
            }
        }
        DirectiveObjectType::CStruct => {
            parse_c_struct(signatures, body_lines, adornment_order, diagnostics, ctx)
        }
        DirectiveObjectType::CUnion => {
            parse_c_union(signatures, body_lines, adornment_order, diagnostics, ctx)
        }
        DirectiveObjectType::CMember => {
            parse_c_member(signatures, body_lines, adornment_order, diagnostics, ctx)
        }
        DirectiveObjectType::CType => {
            parse_c_type(signatures, body_lines, adornment_order, diagnostics, ctx)
        }
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

/// Derives each signature's declared name up front, so it is computed once
/// here at parse time rather than re-derived in every later phase.
///
/// Signatures whose declaration grammar isn't covered still yield a name (via
/// [`CSignature`]'s heuristic fallback) so no cross-reference target is lost;
/// each one emits a diagnostic instead, which is what makes the remaining
/// grammar gaps countable against a real corpus.
fn parse_c_signatures(
    signatures: &NonEmptyVector<String>,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> NonEmptyVector<CSignature> {
    let parsed = signatures.map(|text| CSignature::parse(text.clone()));
    for signature in parsed.as_slice() {
        if signature.name_source() == NameSource::Fallback {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::CDeclUnparsed,
                format!(
                    "c signature: could not parse declaration '{}'; fell back to the name heuristic, which read '{}'",
                    signature.text(),
                    signature.name()
                ),
                span,
            ));
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::Domain;
    use rinx_ast::{Directive, Node};

    /// The raw texts of parsed `c`-domain signatures, for asserting on what
    /// was written rather than on the name derived from it.
    fn signature_texts(signatures: &NonEmptyVector<CSignature>) -> Vec<&str> {
        signatures.as_slice().iter().map(CSignature::text).collect()
    }

    #[test]
    fn test_parse_c_function_reads_the_object_description_flags() {
        // Given — `c-api/` writes these on functions documented twice.
        let input = ".. c:function:: int add(int a, int b)\n   :no-index-entry:\n   :no-contents-entry:\n\n   Adds.";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Some(Node::Directive(Directive::DomainObject(object))) = doc.nodes.first() else {
            panic!("Expected a domain object, got {:?}", doc.nodes);
        };
        assert!(object.no_index_entry());
        assert!(object.no_contents_entry());
        assert!(!object.no_index());
        assert_eq!(object.body().len(), 1);
    }
    #[test]
    fn test_parse_c_macro_reads_no_index() {
        // Given
        let input = ".. c:macro:: PY_SSIZE_T_MAX\n   :noindex:\n\n   The maximum.";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Some(Node::Directive(Directive::DomainObject(object))) = doc.nodes.first() else {
            panic!("Expected a domain object, got {:?}", doc.nodes);
        };
        assert!(object.no_index());
    }
    #[test]
    fn test_parse_creates_c_function_domain_object() {
        // Given
        let input = ".. c:function:: int add(int a, int b)\n\n   Adds two numbers.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CFunction {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["int add(int a, int b)"]);
        } else {
            panic!("Expected CFunction, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_c_macro_domain_object_bare_name() {
        // Given — an object-like macro, with no parens
        let input = ".. c:macro:: PY_SSIZE_T_MAX\n\n   The maximum value of a Py_ssize_t.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro {
            flags: _,
            signatures,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["PY_SSIZE_T_MAX"]);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected CMacro, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_c_macro_domain_object_function_like() {
        // Given — a function-like macro, with no return type or param types
        let input = ".. c:macro:: MAX(a, b)\n\n   Expands to whichever of a or b is greater.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["MAX(a, b)"]);
        } else {
            panic!("Expected CMacro, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_bare_macro_directive_resolves_via_default_domain() {
        // Given
        let input = ".. macro:: MAX(a, b)\n\n   Expands to whichever of a or b is greater.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::C);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro { .. }))
        ));
    }
    #[test]
    fn test_parse_bare_function_directive_resolves_via_default_domain() {
        // Given
        let input = ".. function:: greet(name)\n\n   Greets the given name.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::C);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(DomainObjectBody::CFunction { .. }))
        ));
    }
    #[test]
    fn test_parse_derives_the_name_of_a_function_pointer_typedef() {
        // Given — `Doc/c-api/init.rst`'s real declaration.
        let input =
            ".. c:type:: int (*Py_tracefunc)(PyObject *obj, int what)\n\n   A tracing function.";

        // When
        let doc = parse("test.rst", input);

        // Then — the name comes from the declarator, not the return type.
        let Node::Directive(Directive::DomainObject(object)) = &doc.nodes[0] else {
            panic!("Expected a domain object, got {:?}", doc.nodes[0]);
        };
        assert_eq!(object.names().as_slice(), ["Py_tracefunc"]);
    }
    #[test]
    fn test_parse_emits_no_diagnostic_for_a_well_formed_c_signature() {
        // Given
        let input = ".. c:type:: int (*Py_tracefunc)(PyObject *obj)\n\n   A tracing function.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(
            !doc.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.starts_with("c signature:")),
            "unexpected signature diagnostics: {:?}",
            doc.diagnostics
        );
    }
    #[test]
    fn test_parse_emits_a_diagnostic_for_an_unparseable_c_signature() {
        // Given — prose where a declaration belongs, which still has to
        // produce *some* target rather than being dropped.
        let input = ".. c:type:: >>> not a declaration <<<\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then — one diagnostic, naming the offending signature.
        let signature_diagnostics: Vec<&Diagnostic> = doc
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.starts_with("c signature:"))
            .collect();
        assert_eq!(signature_diagnostics.len(), 1);
        assert!(
            signature_diagnostics[0]
                .message
                .contains(">>> not a declaration <<<"),
            "diagnostic should quote the signature: {}",
            signature_diagnostics[0].message
        );
    }
    #[test]
    fn test_parse_reports_only_the_unparseable_signature_of_a_multi_signature_object() {
        // Given — one directive declaring two aliases, only one of which is
        // malformed; each signature is parsed independently.
        let input = ".. c:type:: int (*Py_tracefunc)(PyObject *obj)\n            >>> nonsense <<<\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then
        let signature_diagnostics: Vec<&Diagnostic> = doc
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.starts_with("c signature:"))
            .collect();
        assert_eq!(signature_diagnostics.len(), 1);
        assert!(
            signature_diagnostics[0]
                .message
                .contains(">>> nonsense <<<")
        );
    }
    #[test]
    fn test_parse_c_signatures_records_the_name_source_per_signature() {
        // Given — a parseable declaration followed by an unparseable one.
        let signatures = NonEmptyVector::new(
            "int (*Py_tracefunc)(PyObject *obj)".to_string(),
            vec![">>> nonsense <<<".to_string()],
        );
        let mut diagnostics = Diagnostics::default();

        // When
        let parsed = parse_c_signatures(&signatures, &mut diagnostics, None);

        // Then
        assert_eq!(parsed.as_slice()[0].name_source(), NameSource::Parsed);
        assert_eq!(parsed.as_slice()[0].name(), "Py_tracefunc");
        assert_eq!(parsed.as_slice()[1].name_source(), NameSource::Fallback);
        assert_eq!(diagnostics.len(), 1);
    }
    #[test]
    fn test_parse_c_signatures_emits_no_diagnostics_when_all_signatures_parse() {
        // Given
        let signatures = NonEmptyVector::new(
            "unsigned long ulong".to_string(),
            vec!["PyObject *(*unaryfunc)(PyObject *)".to_string()],
        );
        let mut diagnostics = Diagnostics::default();

        // When
        let parsed = parse_c_signatures(&signatures, &mut diagnostics, None);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(parsed.as_slice()[0].name(), "ulong");
        assert_eq!(parsed.as_slice()[1].name(), "unaryfunc");
    }
}

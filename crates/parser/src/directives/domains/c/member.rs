use super::super::body::parse_object_body;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use rinx_ast::{CSignature, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:member::`/`.. c:var::` body — same common flags as
/// [`super::struct_::parse_c_struct`]/[`super::union::parse_c_union`]; the
/// real spec has no member-specific options beyond them (the type is
/// embedded in the signature itself, e.g. `int count`, unlike
/// `py:data`/`py:attribute`'s separate `:type:` option).
pub(crate) fn parse_c_member(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let parsed = parse_object_body::<()>("c:member", body_lines, adornment_order, diagnostics, ctx);
    DomainObjectBody::CMember {
        signatures,
        flags: parsed.flags,
        body: parsed.content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Directive, Node};

    /// The raw texts of parsed `c`-domain signatures, for asserting on what
    /// was written rather than on the name derived from it.
    fn signature_texts(signatures: &NonEmptyVector<CSignature>) -> Vec<&str> {
        signatures.as_slice().iter().map(CSignature::text).collect()
    }

    #[test]
    fn test_parse_creates_c_member_domain_object_flat_dotted_signature() {
        // Given — the real CPython-docs shape: no enclosing `.. c:struct::`.
        let input = ".. c:member:: PyObject *PyTypeObject.tp_bases\n\n   The type's base classes.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(
                signature_texts(signatures),
                ["PyObject *PyTypeObject.tp_bases"]
            );
        } else {
            panic!("Expected CMember, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_c_member_domain_object_with_no_index_option() {
        // Given
        let input = ".. c:member:: int count\n   :no-index:\n\n   A count.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
            flags, ..
        })) = &doc.nodes[0]
        {
            assert!(flags.has(rinx_ast::DescriptionFlag::NoIndex));
        } else {
            panic!("Expected CMember, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_c_var_directive_creates_c_member_domain_object() {
        // Given — `.. c:var::` is a pure directive-name alias for `c:member`.
        let input = ".. c:var:: int errno\n\n   The last error number.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["int errno"]);
        } else {
            panic!("Expected CMember, got {:?}", doc.nodes[0]);
        }
    }
}

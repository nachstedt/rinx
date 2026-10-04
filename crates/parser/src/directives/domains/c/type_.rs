use super::super::body::parse_object_body;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use rinx_ast::{CSignature, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:type::` body — same common flags and shape as
/// [`super::struct_::parse_c_struct`]/[`super::union::parse_c_union`]; real
/// Sphinx documents no options specific to `c:type` beyond the shared
/// object-description ones. Unlike `c:struct`/`c:union`, `c:type` has no
/// members of its own, but its body is still parsed as full block content
/// (rather than left opaque) so that nested definitions (e.g. enum-style
/// `.. c:macro::` constants) are indexed instead of silently dropped — see
/// `docs/dev/known_bugs.md`'s former `c:type` entry.
pub(crate) fn parse_c_type(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let parsed = parse_object_body::<()>("c:type", body_lines, adornment_order, diagnostics, ctx);
    DomainObjectBody::CType {
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
    fn test_parse_creates_c_type_domain_object_bare_name() {
        // Given
        let input = ".. c:type:: PyMemAllocatorDomain\n\n   Enumeration of allocator domains.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
            signatures,
            flags,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["PyMemAllocatorDomain"]);
            assert_eq!(flags, &rinx_ast::DescriptionFlags::default());
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_c_type_domain_object_typedef_alias_signature() {
        // Given — real Sphinx's `type name` typedef-alias form.
        let input = ".. c:type:: unsigned long ulong\n\n   An unsigned long alias.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["unsigned long ulong"]);
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_c_type_domain_object_with_no_index_option() {
        // Given
        let input = ".. c:type:: Hidden\n   :no-index:\n\n   A hidden type.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType { flags, .. })) =
            &doc.nodes[0]
        {
            assert!(flags.has(rinx_ast::DescriptionFlag::NoIndex));
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_nested_c_macro_under_c_type_is_recursively_parsed() {
        // Given — the real CPython `c-api/memory.rst` shape (`docs/dev/known_bugs.md`):
        // a `.. c:type::` body nesting `.. c:macro::` constants, previously
        // swallowed as opaque `Directive::Unknown` text.
        let input = ".. c:type:: PyMemAllocatorDomain\n\n   .. c:macro:: PYMEM_DOMAIN_RAW\n\n      The raw domain.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType { body, .. })) =
            &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
            assert!(matches!(
                &body[0],
                Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro { .. }))
            ));
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }
}

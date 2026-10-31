use super::super::body::parse_object_body;
use super::dispatch::{CObjectOption, CObjectOptions};
use crate::diagnostics::Diagnostics;
use crate::directives::invocation::DirectiveInvocation;
use crate::directives::options::DirectiveOptions;
use crate::directives::spec::{Arguments, DirectiveSpec};
use crate::headings::Adornment;
use rinx_ast::{CSignature, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:struct::` body: the object-description flags, then its
/// content.
pub(crate) fn parse_c_struct(
    signatures: NonEmptyVector<CSignature>,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> DomainObjectBody {
    let parsed =
        parse_object_body::<CObjectOptions>("c:struct", invocation, adornment_order, diagnostics);
    DomainObjectBody::CStruct {
        signatures,
        flags: parsed.flags,
        body: parsed.content,
    }
}

/// `.. c:struct::`'s entry among the domain objects.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["struct"],
    doc: "Describes a C struct.",
    arguments: Arguments::ONE_TEXT,
    takes_content: true,
    content_hint: None,
    options: <CObjectOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_document;
    use rinx_ast::{Directive, Node};

    /// The raw texts of parsed `c`-domain signatures, for asserting on what
    /// was written rather than on the name derived from it.
    fn signature_texts(signatures: &NonEmptyVector<CSignature>) -> Vec<&str> {
        signatures.as_slice().iter().map(CSignature::text).collect()
    }

    #[test]
    fn test_parse_creates_c_struct_domain_object() {
        // Given
        let input = ".. c:struct:: Data\n\n   A data record.";

        // When
        let doc = parse_document("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject {
            object:
                DomainObjectBody::CStruct {
                    signatures,
                    flags,
                    body,
                    ..
                },
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["Data"]);
            assert_eq!(flags, &rinx_ast::DescriptionFlags::default());
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected CStruct, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_nested_c_member_under_c_struct() {
        // Given — real nesting: the member's signature is bare, relying on
        // the enclosing struct for qualification (an analyzer/renderer-time
        // concern; the parser just needs to nest the node correctly).
        let input = ".. c:struct:: Data\n\n   .. c:member:: int count\n\n      A count.";

        // When
        let doc = parse_document("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject {
            object: DomainObjectBody::CStruct { body, .. },
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
            assert!(matches!(
                &body[0],
                Node::Directive(Directive::DomainObject {
                    object: DomainObjectBody::CMember { .. },
                    ..
                })
            ));
        } else {
            panic!("Expected CStruct, got {:?}", doc.nodes[0]);
        }
    }
}

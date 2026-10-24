use super::super::body::parse_object_body;
use super::dispatch::{CObjectOption, CObjectOptions};
use crate::diagnostics::Diagnostics;
use crate::directives::invocation::DirectiveInvocation;
use crate::directives::options::DirectiveOptions;
use crate::directives::spec::{Arguments, DirectiveSpec};
use crate::headings::Adornment;
use rinx_ast::{CSignature, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:union::` body — identical shape to
/// [`super::struct_::parse_c_struct`], just producing the other container
/// variant.
pub(crate) fn parse_c_union(
    signatures: NonEmptyVector<CSignature>,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> DomainObjectBody {
    let parsed =
        parse_object_body::<CObjectOptions>("c:union", invocation, adornment_order, diagnostics);
    DomainObjectBody::CUnion {
        signatures,
        flags: parsed.flags,
        body: parsed.content,
    }
}

/// `.. c:union::`'s entry among the domain objects.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["union"],
    doc: "Describes a C union.",
    arguments: Arguments::ONE_TEXT,
    takes_content: true,
    content_hint: None,
    options: <CObjectOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

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
    fn test_parse_creates_c_union_domain_object() {
        // Given
        let input = ".. c:union:: Number\n\n   A numeric union.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject {
            object: DomainObjectBody::CUnion { signatures, .. },
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["Number"]);
        } else {
            panic!("Expected CUnion, got {:?}", doc.nodes[0]);
        }
    }
}

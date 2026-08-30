use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{CSignature, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:union::` body — identical shape to
/// [`super::struct_::parse_c_struct`], just producing the other container
/// variant.
pub(crate) fn parse_c_union(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        super::dispatch::extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    DomainObjectBody::CUnion {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{Directive, Node};

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
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CUnion {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["Number"]);
        } else {
            panic!("Expected CUnion, got {:?}", doc.nodes[0]);
        }
    }
}

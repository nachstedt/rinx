use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{CSignature, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:struct::` body: strips the common object-description flag
/// lines (`:no-index:`, `:no-index-entry:`, `:no-contents-entry:`, and their
/// legacy spellings) off the front before parsing the rest as the docstring
/// body.
pub(crate) fn parse_c_struct(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
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

    DomainObjectBody::CStruct {
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
    fn test_parse_creates_c_struct_domain_object() {
        // Given
        let input = ".. c:struct:: Data\n\n   A data record.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CStruct {
            signatures,
            no_index,
            no_index_entry,
            no_contents_entry,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["Data"]);
            assert!(!no_index);
            assert!(!no_index_entry);
            assert!(!no_contents_entry);
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
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CStruct {
            body, ..
        })) = &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
            assert!(matches!(
                &body[0],
                Node::Directive(Directive::DomainObject(DomainObjectBody::CMember { .. }))
            ));
        } else {
            panic!("Expected CStruct, got {:?}", doc.nodes[0]);
        }
    }
}

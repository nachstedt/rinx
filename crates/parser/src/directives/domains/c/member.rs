use crate::blocks::parse_blocks;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{CSignature, Domain, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:member::`/`.. c:var::` body — same common flags as
/// [`super::struct_::parse_c_struct`]/[`super::union::parse_c_union`]; the
/// real spec has no member-specific options beyond them (the type is
/// embedded in the signature itself, e.g. `int count`, unlike
/// `py:data`/`py:attribute`'s separate `:type:` option).
pub(crate) fn parse_c_member(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        super::dispatch::extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CMember {
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
            no_index,
            ..
        })) = &doc.nodes[0]
        {
            assert!(no_index);
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

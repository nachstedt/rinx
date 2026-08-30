use crate::blocks::parse_blocks;
use crate::bullet_list::unindent_body_lines;
use crate::headings::Adornment;
use rusty_sphinx_ast::{CSignature, Domain, DomainObjectBody, NonEmptyVector};

/// Parses a `.. c:struct::` body: strips the common object-description flag
/// lines (`:no-index:`, `:no-index-entry:`, `:no-contents-entry:`, and their
/// legacy spellings) off the front before parsing the rest as the docstring
/// body.
pub(super) fn parse_c_struct(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CStruct {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

/// Parses a `.. c:union::` body — identical shape to [`parse_c_struct`],
/// just producing the other container variant.
pub(super) fn parse_c_union(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CUnion {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

/// Parses a `.. c:member::`/`.. c:var::` body — same common flags as
/// [`parse_c_struct`]/[`parse_c_union`]; the real spec has no member-specific
/// options beyond them (the type is embedded in the signature itself, e.g.
/// `int count`, unlike `py:data`/`py:attribute`'s separate `:type:` option).
pub(super) fn parse_c_member(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

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

/// Parses a `.. c:type::` body — same common flags and shape as
/// [`parse_c_struct`]/[`parse_c_union`]; real Sphinx documents no options
/// specific to `c:type` beyond the shared object-description ones. Unlike
/// `c:struct`/`c:union`, `c:type` has no members of its own, but its body is
/// still parsed as full block content (rather than left opaque) so that
/// nested definitions (e.g. enum-style `.. c:macro::` constants) are indexed
/// instead of silently dropped — see `known_bugs.md`'s former `c:type` entry.
pub(super) fn parse_c_type(
    signatures: NonEmptyVector<CSignature>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CType {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

/// Extracts the object-description flag options common across domains
/// (`:no-index:`, `:no-index-entry:`, `:no-contents-entry:`, plus their
/// legacy pre-Sphinx-7 spellings `:noindex:`/`:noindexentry:`/
/// `:nocontentsentry:`) from the leading lines of a domain object's body.
/// Currently only wired up for `c:struct`/`c:union`/`c:member`, the first
/// object types in this codebase to model them.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized flags (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_common_object_description_options(lines: &[String]) -> (bool, bool, bool, usize) {
    let mut no_index = false;
    let mut no_index_entry = false;
    let mut no_contents_entry = false;
    let mut consumed = 0;

    for line in lines {
        match line.trim() {
            ":no-index:" | ":noindex:" => no_index = true,
            ":no-index-entry:" | ":noindexentry:" => no_index_entry = true,
            ":no-contents-entry:" | ":nocontentsentry:" => no_contents_entry = true,
            _ => break,
        }
        consumed += 1;
    }

    (no_index, no_index_entry, no_contents_entry, consumed)
}

#[cfg(test)]
mod tests {
    use super::super::parse_c_signatures;
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{Directive, NameSource, Node};

    /// The raw texts of parsed `c`-domain signatures, for asserting on what
    /// was written rather than on the name derived from it.
    fn signature_texts(signatures: &NonEmptyVector<CSignature>) -> Vec<&str> {
        signatures.as_slice().iter().map(CSignature::text).collect()
    }

    #[test]
    fn test_extract_common_object_description_options_parses_hyphenated_spellings() {
        // Given
        let lines = vec![
            ":no-index:".to_string(),
            ":no-index-entry:".to_string(),
            ":no-contents-entry:".to_string(),
            String::new(),
            "A struct.".to_string(),
        ];

        // When
        let (no_index, no_index_entry, no_contents_entry, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(no_index);
        assert!(no_index_entry);
        assert!(no_contents_entry);
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_extract_common_object_description_options_parses_legacy_spellings() {
        // Given
        let lines = vec![
            ":noindex:".to_string(),
            ":noindexentry:".to_string(),
            ":nocontentsentry:".to_string(),
        ];

        // When
        let (no_index, no_index_entry, no_contents_entry, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(no_index);
        assert!(no_index_entry);
        assert!(no_contents_entry);
        assert_eq!(consumed, 3);
    }
    #[test]
    fn test_extract_common_object_description_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![
            ":no-index:".to_string(),
            "A struct.".to_string(),
            ":no-index-entry:".to_string(),
        ];

        // When
        let (no_index, no_index_entry, _, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(no_index);
        assert!(!no_index_entry);
        assert_eq!(consumed, 1);
    }
    #[test]
    fn test_extract_common_object_description_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["A struct.".to_string()];

        // When
        let (no_index, no_index_entry, no_contents_entry, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(!no_index);
        assert!(!no_index_entry);
        assert!(!no_contents_entry);
        assert_eq!(consumed, 0);
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
            no_index,
            no_index_entry,
            no_contents_entry,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature_texts(signatures), ["PyMemAllocatorDomain"]);
            assert!(!no_index);
            assert!(!no_index_entry);
            assert!(!no_contents_entry);
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
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
            no_index, ..
        })) = &doc.nodes[0]
        {
            assert!(no_index);
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_nested_c_macro_under_c_type_is_recursively_parsed() {
        // Given — the real CPython `c-api/memory.rst` shape (`known_bugs.md`):
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
                .any(|diagnostic| diagnostic.starts_with("c signature:")),
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
        let signature_diagnostics: Vec<&String> = doc
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.starts_with("c signature:"))
            .collect();
        assert_eq!(signature_diagnostics.len(), 1);
        assert!(
            signature_diagnostics[0].contains(">>> not a declaration <<<"),
            "diagnostic should quote the signature: {}",
            signature_diagnostics[0]
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
        let signature_diagnostics: Vec<&String> = doc
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.starts_with("c signature:"))
            .collect();
        assert_eq!(signature_diagnostics.len(), 1);
        assert!(signature_diagnostics[0].contains(">>> nonsense <<<"));
    }
    #[test]
    fn test_parse_c_signatures_records_the_name_source_per_signature() {
        // Given — a parseable declaration followed by an unparseable one.
        let signatures = NonEmptyVector::new(
            "int (*Py_tracefunc)(PyObject *obj)".to_string(),
            vec![">>> nonsense <<<".to_string()],
        );
        let mut diagnostics = Vec::new();

        // When
        let parsed = parse_c_signatures(&signatures, &mut diagnostics);

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
        let mut diagnostics = Vec::new();

        // When
        let parsed = parse_c_signatures(&signatures, &mut diagnostics);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(parsed.as_slice()[0].name(), "ulong");
        assert_eq!(parsed.as_slice()[1].name(), "unaryfunc");
    }
}

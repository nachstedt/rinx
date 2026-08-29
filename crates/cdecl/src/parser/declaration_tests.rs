use super::*;

/// The declared name, for the many tests that only care about that.
fn name_of(signature: &str) -> String {
    declared_name(signature).unwrap_or_else(|error| panic!("{signature:?}: {error}"))
}

// ── Specifiers and plain declarations ────────────────────────────────

#[test]
fn test_parses_a_bare_identifier_as_the_name() {
    // Given — `.. c:type:: FILE`, a name with no type at all.
    let signature = "FILE";

    // When / Then
    assert_eq!(name_of(signature), "FILE");
}

#[test]
fn test_parses_a_typedef_alias_pair() {
    // Given — `.. c:type:: unsigned long ulong`
    let signature = "unsigned long ulong";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then — the leading words are the type, the last is the name.
    assert_eq!(declaration.specifiers, vec!["unsigned", "long"]);
    assert_eq!(declaration.name(), Some("ulong"));
}

#[test]
fn test_parses_a_plain_declaration() {
    // Given
    let signature = "int x";

    // When / Then
    assert_eq!(name_of(signature), "x");
}

#[test]
fn test_parses_a_non_keyword_type_name() {
    // Given — `Py_ssize_t` is a typedef, not a keyword, so the "is this
    // the type or the name?" rule has to resolve it.
    let signature = "Py_ssize_t ob_refcnt";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    assert_eq!(declaration.specifiers, vec!["Py_ssize_t"]);
    assert_eq!(declaration.name(), Some("ob_refcnt"));
}

#[test]
fn test_parses_storage_class_and_qualifiers() {
    // Given
    let signature = "static const unsigned long long x";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    assert_eq!(
        declaration.specifiers,
        vec!["static", "const", "unsigned", "long", "long"]
    );
    assert_eq!(declaration.name(), Some("x"));
}

#[test]
fn test_parses_the_typedef_keyword() {
    // Given
    let signature = "typedef int myint";

    // When / Then
    assert_eq!(name_of(signature), "myint");
}

#[test]
fn test_parses_a_struct_tag_type() {
    // Given
    let signature = "struct Foo *x";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    assert_eq!(declaration.specifiers, vec!["struct", "Foo"]);
    assert_eq!(declaration.name(), Some("x"));
}

#[test]
fn test_parses_an_enum_tag_type() {
    // Given
    let signature = "enum E e";

    // When / Then
    assert_eq!(name_of(signature), "e");
}

// ── Pointers ──────────────────────────────────────────────────────────

#[test]
fn test_parses_a_pointer_declarator() {
    // Given
    let signature = "int *x";

    // When / Then
    assert_eq!(name_of(signature), "x");
}

#[test]
fn test_parses_a_pointer_with_whitespace_on_both_sides() {
    // Given
    let signature = "int * x";

    // When / Then
    assert_eq!(name_of(signature), "x");
}

#[test]
fn test_parses_a_double_pointer() {
    // Given
    let signature = "int **x";

    // When / Then
    assert_eq!(name_of(signature), "x");
}

#[test]
fn test_parses_a_qualified_pointer() {
    // Given — the `const` binds to the pointer, not the pointee.
    let signature = "char * const * x";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Pointer { qualifiers, .. } = &declaration.declarator else {
        panic!("expected a pointer declarator");
    };
    assert_eq!(qualifiers, &["const"]);
    assert_eq!(declaration.name(), Some("x"));
}

// ── Arrays ────────────────────────────────────────────────────────────

#[test]
fn test_parses_an_unsized_array() {
    // Given
    let signature = "int x[]";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Array { size, .. } = &declaration.declarator else {
        panic!("expected an array declarator");
    };
    assert_eq!(size, &None);
    assert_eq!(declaration.name(), Some("x"));
}

#[test]
fn test_parses_a_sized_array_capturing_the_size_verbatim() {
    // Given
    let signature = "int x[10]";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Array { size, .. } = &declaration.declarator else {
        panic!("expected an array declarator");
    };
    assert_eq!(size.as_deref(), Some("10"));
}

#[test]
fn test_parses_an_array_sized_by_an_identifier() {
    // Given
    let signature = "int x[N]";

    // When / Then
    assert_eq!(name_of(signature), "x");
}

#[test]
fn test_parses_a_static_sized_array_parameter() {
    // Given — C99's `[static N]` form.
    let signature = "int x[static 4]";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Array { size, .. } = &declaration.declarator else {
        panic!("expected an array declarator");
    };
    assert_eq!(size.as_deref(), Some("static 4"));
}

#[test]
fn test_parses_a_two_dimensional_array() {
    // Given
    let signature = "int x[2][3]";

    // When / Then
    assert_eq!(name_of(signature), "x");
}

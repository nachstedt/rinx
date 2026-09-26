//! Domain-object indexing tests for [`super::analyze`]: multi-signature
//! objects, per-alias entries, and how module/class scope qualifies the
//! keys each one is registered under.

use super::*;
use rinx_ast::{Domain, NonEmptyVector, ObjectType};

/// Looks up a domain object by the pre-refactor flat `"domain:objtype:name"`
/// key shape (e.g. `"py:function:greet"`), so test expectations can stay
/// expressed as a single string instead of repeating two-level map
/// navigation at every call site below.
fn lookup_domain_object<'a>(index: &'a ProjectIndex, flat_key: &str) -> Option<&'a String> {
    let mut parts = flat_key.splitn(3, ':');
    let domain: Domain = parts.next()?.parse().ok()?;
    let objtype_str = parts.next()?;
    let name = parts.next()?;
    let object_type = ObjectType::from_directive_name(domain, objtype_str)?;
    index
        .domain_objects
        .get(&TargetName::new(name))?
        .get(&object_type)
}

#[test]
fn test_analyze_registers_every_declared_name_of_a_multi_signature_object() {
    // Given — the confirmed `library/socket.rst` shape: one directive
    // declaring three aliases, which real Sphinx resolves individually.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyData {
                module: None,
                signatures: NonEmptyVector::new(
                    "AF_UNIX".to_string(),
                    vec!["AF_INET".to_string(), "AF_INET6".to_string()],
                ),
                type_: None,
                value: None,
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — each alias is an independently resolvable target.
    assert_eq!(index.domain_objects.len(), 3);
    for name in ["AF_UNIX", "AF_INET", "AF_INET6"] {
        assert_eq!(
            lookup_domain_object(&index, &format!("py:data:{name}")),
            Some(&"api.rst".to_string()),
            "{name} should resolve"
        );
    }
}

#[test]
fn test_analyze_gives_every_declared_name_its_own_genindex_entry() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyData {
                module: None,
                signatures: NonEmptyVector::new("A".to_string(), vec!["ASCII".to_string()]),
                type_: None,
                value: None,
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — one entry per alias, each anchored to its own name.
    assert_eq!(index.genindex_entries.len(), 2);
    assert_eq!(index.genindex_entries[0].anchor, "py:data:a");
    assert_eq!(index.genindex_entries[1].anchor, "py:data:ascii");
}

#[test]
fn test_analyze_indexes_a_multi_signature_objects_body_only_once() {
    // Given — the aliases share one docstring; indexing it per alias
    // would register its contents several times over.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyData {
                module: None,
                signatures: NonEmptyVector::new("AF_UNIX".to_string(), vec!["AF_INET".to_string()]),
                type_: None,
                value: None,
                body: vec![Node::Target {
                    name: rinx_ast::TargetName::new("address-families"),
                    uri: None,
                }],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — the body's target is registered exactly once, even though
    // two names were.
    assert_eq!(index.domain_objects.len(), 2);
    assert_eq!(index.targets.len(), 1);
}

#[test]
fn test_analyze_qualifies_every_alias_of_a_multi_signature_object_by_module() {
    // Given — a multi-signature object under a current module: every
    // alias, not just the primary, has to pick the module qualifier up.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "socket".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyData {
                    module: None,
                    signatures: NonEmptyVector::new(
                        "AF_UNIX".to_string(),
                        vec!["AF_INET".to_string()],
                    ),
                    type_: None,
                    value: None,
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "py:data:socket.AF_UNIX"),
        Some(&"api.rst".to_string())
    );
    assert_eq!(
        lookup_domain_object(&index, "py:data:socket.AF_INET"),
        Some(&"api.rst".to_string())
    );
}

#[test]
fn test_analyze_does_not_double_qualify_already_qualified_nested_attribute() {
    // Given — mirrors CPython's `Doc/library/exceptions.rst`, which
    // nests `.. attribute:: StopIteration.value` (already fully
    // qualified) inside `.. exception:: StopIteration`, rather than
    // writing the bare name `value`.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyException {
                module: None,
                signatures: NonEmptyVector::single("StopIteration".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyAttribute {
                        module: None,
                        signatures: NonEmptyVector::single("StopIteration.value".to_string()),
                        type_: None,
                        value: None,
                        canonical: None,
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — the attribute is indexed under its own already-qualified
    // name, not doubled to "StopIteration.StopIteration.value"
    assert_eq!(index.domain_objects.len(), 2);
    assert!(lookup_domain_object(&index, "py:exception:StopIteration").is_some());
    assert!(lookup_domain_object(&index, "py:attribute:StopIteration.value").is_some());
}

#[test]
fn test_analyze_object_before_any_module_directive_stays_unqualified() {
    // Given — a `py:function` appearing before any `py:module` in the
    // document has no current module to fall back to.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("greet(name)".to_string()),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:function:greet").is_some());
}

#[test]
fn test_analyze_composes_module_and_class_qualifiers() {
    // Given — a `py:class` documented as a sibling after `py:module`
    // (module-qualified), with a `py:method` nested inside the class
    // (class-qualified) — both qualifiers must compose.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "types".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: None,
                    signatures: NonEmptyVector::single("DynamicClassAttribute".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyMethod {
                            module: None,
                            is_decorator: false,
                            signatures: NonEmptyVector::single(
                                "__get__(self, instance, owner)".to_string(),
                            ),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:class:types.DynamicClassAttribute").is_some());
    assert!(
        lookup_domain_object(&index, "py:method:types.DynamicClassAttribute.__get__").is_some()
    );
}

#[test]
fn test_apply_scope_directive_sets_and_clears_the_python_module() {
    // Given
    let mut scope = Scope::default();

    // When — set, then clear
    apply_scope_directive(
        &Directive::PyCurrentModule {
            module: Some("pkg.mod".to_string()),
        },
        &mut scope,
    );
    // Asserted through `qualify`, the scope's observable effect, rather than
    // through a getter added just for this test.
    let after_set = scope.python.qualify("f").qualified_name;
    apply_scope_directive(&Directive::PyCurrentModule { module: None }, &mut scope);

    // Then
    assert_eq!(after_set, "pkg.mod.f");
    assert_eq!(scope.python.qualify("f").qualified_name, "f");
}

#[test]
fn test_apply_scope_directive_pushes_and_pops_a_c_namespace() {
    // Given
    let mut scope = Scope::default();

    // When
    apply_scope_directive(
        &Directive::CNamespacePush {
            namespace: "inner".to_string(),
        },
        &mut scope,
    );
    let after_push = scope.c.qualify("f").qualified_name;
    apply_scope_directive(&Directive::CNamespacePop, &mut scope);

    // Then
    assert_eq!(after_push, "inner.f");
    assert_eq!(scope.c.qualify("f").qualified_name, "f");
}

#[test]
fn test_apply_scope_directive_ignores_a_directive_that_moves_no_scope() {
    // Given — the caller's match decides which directives arrive here, so an
    // unrelated one must be a no-op rather than a panic
    let mut scope = Scope::default();
    scope.python.set_module("pkg");

    // When
    apply_scope_directive(&Directive::SeeAlso { body: vec![] }, &mut scope);

    // Then — the unrelated directive changed nothing
    assert_eq!(scope.python.qualify("f").qualified_name, "pkg.f");
}

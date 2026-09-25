//! `c`-domain qualification/scoping tests (`CScope` nesting under
//! `c:struct`/`c:union`/`c:type`, `c:namespace` handling). Split out of
//! `super::domain_object_index` purely for its line count.

use super::super::document_index::analyze;
use super::*;
use rinx_ast::{Directive, Document, Domain, Node, NonEmptyVector, ObjectType, TargetName};

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

fn c_member(signature: &str) -> Node {
    Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
        signatures: NonEmptyVector::single(signature.into()),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    }))
}

fn c_macro(signature: &str) -> Node {
    Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro {
        signatures: NonEmptyVector::single(signature.into()),
        body: vec![],
    }))
}

#[test]
fn test_analyze_qualifies_bare_c_member_nested_under_c_struct() {
    // Given — `.. c:member:: int count` nested inside `.. c:struct:: Data`.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CStruct {
                signatures: NonEmptyVector::single("Data".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![c_member("int count")],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — the bare member name is auto-qualified against the
    // enclosing struct.
    assert_eq!(
        lookup_domain_object(&index, "c:member:Data.count"),
        Some(&"api.rst".to_string())
    );
    assert!(lookup_domain_object(&index, "c:member:count").is_none());
}
#[test]
fn test_analyze_qualifies_bare_c_member_nested_under_c_union() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CUnion {
                signatures: NonEmptyVector::single("Number".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![c_member("int as_int")],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:member:Number.as_int"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_flat_dotted_c_member_without_enclosing_struct() {
    // Given — the real CPython-docs shape: no `.. c:struct::` at all.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![c_member("PyObject *PyTypeObject.tp_bases")],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:member:PyTypeObject.tp_bases"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_c_function_nested_in_c_struct_is_qualified_by_it() {
    // Given — an (unrealistic) `c:function` written inside a `c:struct`
    // body. `c:function`/`c:macro` joined `uses_c_scope` (`known_bugs.md`
    // #2) so that nesting one inside a `py:class`/`py:exception` body no
    // longer wrongly picks up the enclosing Python module+class — but
    // `CScope`'s container stack is shared by every `c`-domain object, so
    // nesting under `c:struct`/`c:union`/`c:type` now qualifies
    // `c:function`/`c:macro` too, unlike before. rinx doesn't
    // implement `.. c:namespace::`, which is what real Sphinx would use
    // to reset this back to a bare name, so this is a known, accepted
    // trade-off — see `known_bugs.md`'s entry for it.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CStruct {
                signatures: NonEmptyVector::single("Data".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    DomainObjectBody::CFunction {
                        signatures: NonEmptyVector::single("int helper(void)".into()),
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "c:function:Data.helper").is_some());
    assert!(lookup_domain_object(&index, "c:function:helper").is_none());
}
#[test]
fn test_analyze_qualifies_c_macro_nested_under_c_type() {
    // Given — a `.. c:macro::` nested inside `.. c:type::` with no
    // `.. c:namespace::` to reset the scope: `c:macro` qualifies against
    // the enclosing type like every other `c`-domain object. (CPython's
    // own `c-api/memory.rst` adds the reset — see
    // `test_analyze_c_namespace_null_inside_c_type_body_unqualifies_nested_macro`.)
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CType {
                signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![c_macro("PYMEM_DOMAIN_RAW")],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:macro:PyMemAllocatorDomain.PYMEM_DOMAIN_RAW"),
        Some(&"api.rst".to_string())
    );
    assert!(lookup_domain_object(&index, "c:macro:PYMEM_DOMAIN_RAW").is_none());
}
#[test]
fn test_analyze_c_namespace_null_inside_c_type_body_unqualifies_nested_macro() {
    // Given — the real CPython `c-api/memory.rst` shape verbatim: a
    // `.. c:namespace:: NULL` written *inside* the `c:type` body, before
    // the enum-style macro constants, resetting the qualification that
    // body would otherwise apply.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CType {
                signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![
                    Node::Directive(Directive::CNamespace { namespace: None }),
                    c_macro("PYMEM_DOMAIN_RAW"),
                ],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — bare, matching what CPython's docs actually publish.
    assert_eq!(
        lookup_domain_object(&index, "c:macro:PYMEM_DOMAIN_RAW"),
        Some(&"api.rst".to_string())
    );
    assert!(
        lookup_domain_object(&index, "c:macro:PyMemAllocatorDomain.PYMEM_DOMAIN_RAW").is_none()
    );
}
#[test]
fn test_analyze_c_type_body_restores_scope_after_an_inner_namespace_reset() {
    // Given — a sibling declaration *after* the `c:type` whose body reset
    // the namespace: the reset must not leak past the body's close.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![
            Node::Directive(Directive::CNamespacePush {
                namespace: "Outer".to_string(),
            }),
            Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
                signatures: NonEmptyVector::single("Inner".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![Node::Directive(Directive::CNamespace { namespace: None })],
            })),
            c_macro("AFTER"),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then — back under the pushed namespace, not stranded at global.
    assert_eq!(
        lookup_domain_object(&index, "c:macro:Outer.AFTER"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_qualifies_declaration_after_c_namespace_directive() {
    // Given — `.. c:namespace:: A.B` applies to subsequent siblings, the
    // document-order semantics `py:currentmodule` already has.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![
            Node::Directive(Directive::CNamespace {
                namespace: Some("A.B".to_string()),
            }),
            c_macro("CONSTANT"),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:macro:A.B.CONSTANT"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_c_namespace_pop_undoes_a_whole_multi_segment_push() {
    // Given — real Sphinx's documented pop semantics: after pushing
    // "C.D" onto "A.B", a pop returns to "A.B", not to "A.B.C".
    let doc = Document::new(
        "api.rst".to_string(),
        vec![
            Node::Directive(Directive::CNamespace {
                namespace: Some("A.B".to_string()),
            }),
            Node::Directive(Directive::CNamespacePush {
                namespace: "C.D".to_string(),
            }),
            c_macro("INNER"),
            Node::Directive(Directive::CNamespacePop),
            c_macro("OUTER"),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "c:macro:A.B.C.D.INNER").is_some());
    assert!(lookup_domain_object(&index, "c:macro:A.B.OUTER").is_some());
}
#[test]
fn test_analyze_c_function_nested_in_py_class_is_not_qualified_by_it() {
    // Given — `known_bugs.md` #2's own reproducer: a `c:function`
    // (structurally) nested inside a `py:class` body. Real Sphinx's C
    // domain has no concept of an enclosing Python class at all, so this
    // must register under its own bare name, not
    // `greeter_module.Greeter.helper` — unlike before `c:function`/
    // `c:macro` joined `uses_c_scope`, when it wrongly picked up
    // `PythonScope`'s module+class stack.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "greeter_module".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: None,
                    signatures: NonEmptyVector::single("Greeter".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        DomainObjectBody::CFunction {
                            signatures: NonEmptyVector::single("int helper(void)".into()),
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
    assert!(lookup_domain_object(&index, "c:function:helper").is_some());
    assert!(lookup_domain_object(&index, "c:function:greeter_module.Greeter.helper").is_none());
}
#[test]
fn test_analyze_qualifies_bare_c_member_nested_under_c_type() {
    // Given — `c:member` consults `CScope`, so nesting it under `c:type`
    // (rather than `c:struct`/`c:union`) still qualifies it against the
    // enclosing type's name.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CType {
                signatures: NonEmptyVector::single("Data".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![c_member("int count")],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:member:Data.count"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_bare_c_type_without_nesting() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CType {
                signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:type:PyMemAllocatorDomain"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_a_function_pointer_typedef_under_its_declared_name() {
    // Given — `Doc/c-api/init.rst`'s `Py_tracefunc`. Before the
    // declaration parser this registered as `c:type:int` (the return
    // type), leaving the eight `:c:type:` references to it in
    // `Doc/c-api/profiling.rst` unresolvable.
    let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CType {
                    signatures: NonEmptyVector::single(
                        "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)"
                            .into(),
                    ),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:type:Py_tracefunc"),
        Some(&"api.rst".to_string())
    );
    assert_eq!(lookup_domain_object(&index, "c:type:int"), None);
}
#[test]
fn test_analyze_registers_slot_typedefs_with_pointer_return_types() {
    // Given — `Doc/c-api/typeobj.rst` declares dozens of these, all of
    // the `RETTYPE *(*NAME)(ARGS)` shape.
    let doc = Document::new(
        "typeobj.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
                signatures: NonEmptyVector::single("PyObject *(*unaryfunc)(PyObject *)".into()),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![],
            })),
            Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
                signatures: NonEmptyVector::single(
                    "int (*visitproc)(PyObject *object, void *arg)".into(),
                ),
                no_index: false,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![],
            })),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:type:unaryfunc"),
        Some(&"typeobj.rst".to_string())
    );
    assert_eq!(
        lookup_domain_object(&index, "c:type:visitproc"),
        Some(&"typeobj.rst".to_string())
    );
}
#[test]
fn test_analyze_no_index_suppresses_target_and_genindex_entry_for_c_type() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CType {
                signatures: NonEmptyVector::single("Hidden".into()),
                no_index: true,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "c:type:Hidden").is_none());
    assert!(index.genindex_entries.is_empty());
}

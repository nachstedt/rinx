//! `py`-domain qualification/scoping tests: class/exception/method
//! nesting, `py:module`/`py:currentmodule` context, and the `:module:`
//! option override. Split out of `super::domain_object_index` purely
//! for its line count.

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

#[test]
fn test_analyze_qualifies_method_nested_in_class() {
    // Given — a `py:method` nested inside a `py:class` body
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("Greeter".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyMethod {
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                        is_classmethod: false,
                        is_staticmethod: false,
                        is_abstractmethod: false,
                        is_async: false,
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — both the class and its qualified method are indexed
    assert_eq!(index.domain_objects.len(), 2);
    assert!(lookup_domain_object(&index, "py:class:Greeter").is_some());
    assert!(lookup_domain_object(&index, "py:method:Greeter.greet").is_some());
}
#[test]
fn test_analyze_qualifies_method_nested_in_exception() {
    // Given — a `py:method` nested inside a `py:exception` body, exactly
    // like the `py:class` nesting case, since exceptions are classes.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyException {
                module: None,
                signatures: NonEmptyVector::single("GreeterError".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyMethod {
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("reason(self)".to_string()),
                        is_classmethod: false,
                        is_staticmethod: false,
                        is_abstractmethod: false,
                        is_async: false,
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — both the exception and its qualified method are indexed
    assert_eq!(index.domain_objects.len(), 2);
    assert!(lookup_domain_object(&index, "py:exception:GreeterError").is_some());
    assert!(lookup_domain_object(&index, "py:method:GreeterError.reason").is_some());
}
#[test]
fn test_analyze_qualifies_nested_classes_two_levels_deep() {
    // Given — a class nested inside another class, both containing a method
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("Outer".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyClass {
                        module: None,
                        signatures: NonEmptyVector::single("Inner".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rinx_ast::DomainObjectBody::PyMethod {
                                module: None,
                                is_decorator: false,
                                signatures: NonEmptyVector::single("method(self)".to_string()),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:class:Outer.Inner").is_some());
    assert!(lookup_domain_object(&index, "py:method:Outer.Inner.method").is_some());
}
#[test]
fn test_analyze_qualifies_non_method_object_nested_in_class() {
    // Given — a `py:data` (class attribute) nested inside a `py:class`
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("Greeter".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyData {
                        module: None,
                        signatures: NonEmptyVector::single("DEFAULT_GREETING".to_string()),
                        type_: None,
                        value: None,
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:data:Greeter.DEFAULT_GREETING").is_some());
}
#[test]
fn test_analyze_qualifies_object_nested_in_module_domain_object() {
    // Given — a `py:function` nested inside a `py:module` body picks up
    // the module's name, matching real Sphinx.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyModule {
                name: "greetings".to_string(),
                options: rinx_ast::ModuleOptions::default(),
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyFunction {
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:function:greetings.greet").is_some());
    assert!(lookup_domain_object(&index, "py:function:greet").is_none());
}
#[test]
fn test_analyze_qualifies_sibling_object_after_module_domain_object() {
    // Given — the real-world CPython shape: `py:module` and the
    // `py:function` it documents are *siblings* at the document's top
    // level, not nested — this is what surfaced the `types.coroutine`
    // broken-domain-object bug.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "types".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("coroutine(gen_func)".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:function:types.coroutine").is_some());
}
#[test]
fn test_analyze_does_not_dedup_module_prefix_in_flat_sibling_signature() {
    // Given — the class/module conflation bug this change fixes: real
    // CPython's `datetime.rst` documents `.. classmethod::
    // datetime.strptime` as a column-0 sibling of `.. module::
    // datetime`, with no enclosing `.. class::`. The `datetime.` in the
    // signature is the *class* name (there is a separate `.. class::
    // datetime` elsewhere in the same file) — it only coincides with the
    // module name, and must not be mistaken for a repeat of it.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "datetime".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyMethod {
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single(
                        "datetime.strptime(date_string, format)".to_string(),
                    ),
                    is_classmethod: true,
                    is_staticmethod: false,
                    is_abstractmethod: false,
                    is_async: false,
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then — not collapsed to "datetime.strptime".
    assert!(lookup_domain_object(&index, "py:method:datetime.datetime.strptime").is_some());
}
#[test]
fn test_analyze_qualifies_object_after_current_module_directive() {
    // Given — the CPython `howto/enum.rst` shape: a document with no
    // `py:module` of its own, opening with `.. currentmodule:: enum`
    // before a class defined as if under that module.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string()),
            }),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: None,
                    signatures: NonEmptyVector::single("Enum".to_string()),
                    is_final: false,
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:class:enum.Enum").is_some());
}
#[test]
fn test_analyze_current_module_none_resets_qualification_to_module_free() {
    // Given — `.. currentmodule:: None` after a `py:module` restores
    // unqualified index keys, matching real Sphinx's reset form.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "pickle".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::PyCurrentModule { module: None }),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("example()".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then — not qualified as "pickle.example".
    assert!(lookup_domain_object(&index, "py:function:example").is_some());
    assert!(lookup_domain_object(&index, "py:function:pickle.example").is_none());
}
#[test]
fn test_analyze_switches_current_module_on_second_module_directive() {
    // Given — two sequential `py:module` directives in one document
    // (e.g. a package's docs covering a couple of submodules); the
    // second one becomes current for everything after it.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "email.mime".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "email.mime.text".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: None,
                    signatures: NonEmptyVector::single("MIMEText".to_string()),
                    is_final: false,
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:class:email.mime.text.MIMEText").is_some());
}
#[test]
fn test_analyze_dedups_class_name_repeated_in_flat_nested_signature() {
    // Given — the real CPython idiom from `random.rst`: `.. method::
    // Random.seed` indented inside `.. class:: Random`, itself a sibling
    // after `.. module:: random`. The method's own signature repeats the
    // class name; it must not double to "random.Random.Random.seed".
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "random".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: None,
                    signatures: NonEmptyVector::single("Random([seed])".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyMethod {
                            module: None,
                            is_decorator: false,
                            signatures: NonEmptyVector::single(
                                "Random.seed(a=None, version=2)".to_string(),
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
    assert!(lookup_domain_object(&index, "py:class:random.Random").is_some());
    assert!(lookup_domain_object(&index, "py:method:random.Random.seed").is_some());
}
#[test]
fn test_analyze_qualifies_genindex_entry_for_method_nested_in_class() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("Greeter".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyMethod {
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                        is_classmethod: false,
                        is_staticmethod: false,
                        is_abstractmethod: false,
                        is_async: false,
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — one entry for the class, one for the qualified method
    assert_eq!(index.genindex_entries.len(), 2);
    assert!(
        index
            .genindex_entries
            .iter()
            .any(|e| e.primary == "Greeter (class)")
    );
    assert!(
        index
            .genindex_entries
            .iter()
            .any(|e| e.primary == "Greeter.greet (method)")
    );
}
#[test]
fn test_analyze_module_option_overrides_the_enclosing_module() {
    // Given — the exact `docs/dev/known_bugs.md` shape:
    // `Doc/library/multiprocessing.shared_memory.rst` documents
    // `SharedMemoryManager` under a different module via `:module:`.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "multiprocessing.shared_memory".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: Some("multiprocessing.managers".to_string()),
                    signatures: NonEmptyVector::single("SharedMemoryManager".to_string()),
                    is_final: false,
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(
        lookup_domain_object(
            &index,
            "py:class:multiprocessing.managers.SharedMemoryManager"
        )
        .is_some()
    );
    assert!(
        lookup_domain_object(
            &index,
            "py:class:multiprocessing.shared_memory.SharedMemoryManager"
        )
        .is_none()
    );
}
#[test]
fn test_analyze_module_option_propagates_into_nested_body() {
    // Given — a `py:method` nested inside the overridden `py:class` must
    // also qualify under the override, not the enclosing module —
    // mirrors real Sphinx's `before_content()` pushing
    // `ref_context['py:module']` for the whole nested body, not just the
    // class's own signature.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "multiprocessing.shared_memory".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: Some("multiprocessing.managers".to_string()),
                    signatures: NonEmptyVector::single("SharedMemoryManager".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyMethod {
                            module: None,
                            is_decorator: false,
                            signatures: NonEmptyVector::single("get_server()".to_string()),
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
    assert!(
        lookup_domain_object(
            &index,
            "py:method:multiprocessing.managers.SharedMemoryManager.get_server"
        )
        .is_some()
    );
}
#[test]
fn test_analyze_module_option_is_restored_after_the_object_and_its_body() {
    // Given — a sibling documented *after* the overridden class, with no
    // override of its own, must revert to the enclosing module — the key
    // regression test proving `restore_module` actually pops rather than
    // sticking, mirroring real Sphinx's `after_content()`.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "multiprocessing.shared_memory".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    module: Some("multiprocessing.managers".to_string()),
                    signatures: NonEmptyVector::single("SharedMemoryManager".to_string()),
                    is_final: false,
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("track(size)".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(
        lookup_domain_object(&index, "py:function:multiprocessing.shared_memory.track").is_some()
    );
    assert!(lookup_domain_object(&index, "py:function:multiprocessing.managers.track").is_none());
}
#[test]
fn test_analyze_module_option_with_empty_value_leaves_object_unqualified() {
    // Given — real Sphinx's falsy-`modname` check: a bare `:module:`
    // deliberately un-qualifies the object even with an ambient module
    // in scope.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "ctypes".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    module: Some(String::new()),
                    is_decorator: false,
                    signatures: NonEmptyVector::single("standalone()".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:function:standalone").is_some());
    assert!(lookup_domain_object(&index, "py:function:ctypes.standalone").is_none());
}

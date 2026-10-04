//! `py`-domain qualification/scoping rendering tests (class/exception/
//! method nesting, module-qualified ids). Split from
//! `super::domain_object_python` purely for line count.

use rinx_ast::{Directive, Document, Node, NonEmptyVector};
use rinx_index::ProjectIndex;

fn render_doc(doc: &Document) -> String {
    let index = ProjectIndex::default();
    crate::render(doc, &index, &doc.path).html
}

#[test]
fn test_render_qualifies_method_nested_in_class_id() {
    // Given — a `py:method` nested inside a `py:class` body
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyClass {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::single("greeter".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyMethod {
                        flags: rinx_ast::DescriptionFlags::default(),
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
    let result = render_doc(&doc);

    // Then — the nested method's id and signature are qualified/plain
    // respectively, matching the analyzer's index key exactly.
    assert!(result.contains("<dt id=\"py:method:greeter.greet\">"));
    assert!(result.contains("<code class=\"sig-name\">greet(self, name)</code>"));
}
#[test]
fn test_render_qualifies_nested_classes_two_levels_deep() {
    // Given — a class nested inside another class, each containing a method
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyClass {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::single("outer".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyClass {
                        flags: rinx_ast::DescriptionFlags::default(),
                        module: None,
                        signatures: NonEmptyVector::single("inner".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rinx_ast::DomainObjectBody::PyMethod {
                                flags: rinx_ast::DescriptionFlags::default(),
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
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dt id=\"py:class:outer.inner\">"));
    assert!(result.contains("<dt id=\"py:method:outer.inner.method\">"));
}
#[test]
fn test_render_qualifies_sibling_function_after_module_id() {
    // Given — the real-world CPython shape: `py:module` and the
    // `py:function` it documents are siblings, not nested.
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
                    flags: rinx_ast::DescriptionFlags::default(),
                    is_async: false,
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("coroutine(gen_func)".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let result = render_doc(&doc);

    // Then — the function's anchor id is module-qualified, matching the
    // analyzer's index key for `:func:`types.coroutine``.
    assert!(result.contains("<dt id=\"py:function:types.coroutine\">"));
    assert!(result.contains("<code class=\"sig-name\">coroutine(gen_func)</code>"));
}
#[test]
fn test_render_does_not_dedup_module_prefix_in_flat_sibling_signature_id() {
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
                    flags: rinx_ast::DescriptionFlags::default(),
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
    let result = render_doc(&doc);

    // Then — not collapsed to "py:method:datetime.strptime", and matches
    // the analyzer's index key.
    assert!(result.contains("<dt id=\"py:method:datetime.datetime.strptime\">"));
}
#[test]
fn test_render_composes_module_and_class_qualifiers_in_id() {
    // Given — a class documented as a sibling after `py:module`, with a
    // method nested inside the class — both qualifiers must compose.
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
                rinx_ast::DomainObjectBody::PyClass {
                    flags: rinx_ast::DescriptionFlags::default(),
                    module: None,
                    signatures: NonEmptyVector::single("DynamicClassAttribute".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyMethod {
                            flags: rinx_ast::DescriptionFlags::default(),
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
    let result = render_doc(&doc);

    // Then — id is lowercased, matching `TargetName`'s normalization
    assert!(result.contains("<dt id=\"py:class:types.dynamicclassattribute\">"));
    assert!(result.contains("<dt id=\"py:method:types.dynamicclassattribute.__get__\">"));
}
#[test]
fn test_render_qualifies_method_nested_in_exception_id() {
    // Given — a `py:method` nested inside a `py:exception` body
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyException {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::single("greetererror".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyMethod {
                        flags: rinx_ast::DescriptionFlags::default(),
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
    let result = render_doc(&doc);

    // Then — the nested method's id is qualified, matching the
    // analyzer's index key exactly.
    assert!(result.contains("<dt id=\"py:method:greetererror.reason\">"));
    assert!(result.contains("<code class=\"sig-name\">reason(self)</code>"));
}
#[test]
fn test_render_does_not_double_qualify_already_qualified_nested_attribute() {
    // Given — mirrors CPython's `Doc/library/exceptions.rst`, which
    // nests `.. attribute:: StopIteration.value` (already fully
    // qualified) inside `.. exception:: StopIteration`.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyException {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::single("StopIteration".to_string()),
                is_final: false,
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyAttribute {
                        flags: rinx_ast::DescriptionFlags::default(),
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
    let result = render_doc(&doc);

    // Then — not doubled to "py:attribute:stopiteration.stopiteration.value"
    // (`TargetName` lowercases keys, same as every other domain object test).
    assert!(result.contains("<dt id=\"py:attribute:stopiteration.value\">"));
}
#[test]
fn test_render_class_stack_does_not_leak_across_sibling_classes() {
    // Given — two sibling classes, each with a method of the same name;
    // the second class's method must not inherit the first class's
    // qualifier from a stale, un-popped stack entry.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    flags: rinx_ast::DescriptionFlags::default(),
                    module: None,
                    signatures: NonEmptyVector::single("first".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyMethod {
                            flags: rinx_ast::DescriptionFlags::default(),
                            module: None,
                            is_decorator: false,
                            signatures: NonEmptyVector::single("run(self)".to_string()),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyClass {
                    flags: rinx_ast::DescriptionFlags::default(),
                    module: None,
                    signatures: NonEmptyVector::single("second".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyMethod {
                            flags: rinx_ast::DescriptionFlags::default(),
                            module: None,
                            is_decorator: false,
                            signatures: NonEmptyVector::single("run(self)".to_string()),
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
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dt id=\"py:method:first.run\">"));
    assert!(result.contains("<dt id=\"py:method:second.run\">"));
    assert!(!result.contains("py:method:first.second.run"));
}
#[test]
fn test_render_dotted_method_scopes_its_body_without_leaking_to_siblings() {
    // Given — the flat, dotted-signature shape CPython's `zipfile.rst`
    // uses: a method written as a sibling rather than nested in its
    // class. Its own name-prefix scopes its body (here, a nested
    // attribute), but must be popped again before the next sibling.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyMethod {
                    flags: rinx_ast::DescriptionFlags::default(),
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("ZipFile.open(name)".to_string()),
                    is_classmethod: false,
                    is_staticmethod: false,
                    is_abstractmethod: false,
                    is_async: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyAttribute {
                            flags: rinx_ast::DescriptionFlags::default(),
                            module: None,
                            signatures: NonEmptyVector::single("mode".to_string()),
                            type_: None,
                            value: None,
                            canonical: None,
                            body: vec![],
                        },
                    ))],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    flags: rinx_ast::DescriptionFlags::default(),
                    is_async: false,
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("is_zipfile(filename)".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let result = render_doc(&doc);

    // Then — the nested attribute is scoped by the method's prefix, and
    // the following sibling is untouched by that scope.
    assert!(result.contains("<dt id=\"py:method:zipfile.open\">"));
    assert!(result.contains("<dt id=\"py:attribute:zipfile.mode\">"));
    assert!(result.contains("<dt id=\"py:function:is_zipfile\">"));
    assert!(!result.contains("py:function:zipfile.is_zipfile"));
}

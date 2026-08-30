//! `py`-domain object rendering tests — `super::domain_object_directive`
//! holds the shared dispatcher every object type (including `py:*`)
//! renders through; these tests just exercise it for the `py:*` shapes.
//! Split from `super::domain_object_python_scoping_tests` purely for line count.

use rusty_sphinx_ast::{Directive, Document, InlineNode, Node, NonEmptyVector};
use rusty_sphinx_index::ProjectIndex;

fn render_doc(doc: &Document) -> String {
    let index = ProjectIndex::default();
    crate::render(doc, &index, &doc.path).html
}

#[test]
fn test_render_formats_py_function_domain_object() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyFunction {
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(name)".to_string()),
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Greets the given name.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dl class=\"py function\">"));
    assert!(result.contains("<dt id=\"py:function:greet\">"));
    assert!(result.contains("<code class=\"sig-name\">greet(name)</code>"));
    assert!(result.contains("<p>Greets the given name.</p>"));
}
#[test]
fn test_render_prefixes_decorator_signature_with_at_sign() {
    // Given — a `.. decorator::`-derived `PyFunction`: real Sphinx's
    // `PyDecoratorFunction` inserts a literal `@`
    // (`desc_addname('@', '@')`) directly before the signature name.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyFunction {
                module: None,
                is_decorator: true,
                signatures: NonEmptyVector::single("classmethod".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then — the target key/anchor is still the bare name, unaffected by
    // the `@` (which is display-only, fused into the same `<code>` the
    // way `signature_texts()` is already rendered raw).
    assert!(result.contains("<dt id=\"py:function:classmethod\">"));
    assert!(result.contains("<code class=\"sig-name\">@classmethod</code>"));
}
#[test]
fn test_render_prefixes_decoratormethod_signature_with_at_sign() {
    // Given — the `py:method` counterpart.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                module: None,
                signatures: NonEmptyVector::single("register(cls)".to_string()),
                is_classmethod: false,
                is_staticmethod: false,
                is_abstractmethod: false,
                is_async: false,
                is_decorator: true,
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dt id=\"py:method:register\">"));
    assert!(result.contains("<code class=\"sig-name\">@register(cls)</code>"));
}
#[test]
fn test_render_omits_id_attribute_when_no_index_is_set() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::CMember {
                signatures: NonEmptyVector::single("int count".into()),
                no_index: true,
                no_index_entry: false,
                no_contents_entry: false,
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then — still typeset, just no anchor.
    assert!(result.contains("<dt>"));
    assert!(!result.contains("id=\"c:member:count\""));
    assert!(result.contains("<code class=\"sig-name\">int count</code>"));
}
#[test]
fn test_render_formats_py_module_domain_object() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyModule {
                name: "greetings".to_string(),
                platform: None,
                synopsis: None,
                deprecated: false,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "A module of greetings.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dl class=\"py module\">"));
    assert!(result.contains("<dt id=\"py:module:greetings\">"));
    assert!(result.contains("<code class=\"sig-name\">greetings</code>"));
}
#[test]
fn test_render_formats_py_method_domain_object() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                is_classmethod: false,
                is_staticmethod: false,
                is_abstractmethod: false,
                is_async: false,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Greets the given name.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dl class=\"py method\">"));
    assert!(result.contains("<dt id=\"py:method:greet\">"));
    assert!(result.contains("<code class=\"sig-name\">greet(self, name)</code>"));
    assert!(!result.contains("class=\"property\""));
}
#[test]
fn test_render_py_method_modifier_prefixes_in_canonical_order() {
    // Given — options written out of canonical order
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("create(cls)".to_string()),
                is_classmethod: true,
                is_staticmethod: false,
                is_abstractmethod: true,
                is_async: false,
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then — abstractmethod is rendered before classmethod regardless of
    // struct-field/author order
    let abstractmethod_pos = result.find("abstractmethod").unwrap();
    let classmethod_pos = result.find("classmethod").unwrap();
    assert!(abstractmethod_pos < classmethod_pos);
    assert!(result.contains("<em class=\"property\">abstractmethod</em>"));
    assert!(result.contains("<em class=\"property\">classmethod</em>"));
    assert!(!result.contains("staticmethod"));
    assert!(!result.contains(">async<"));
}
#[test]
fn test_render_formats_py_class_domain_object() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("greeter".to_string()),
                is_final: false,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "A greeter.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dl class=\"py class\">"));
    assert!(result.contains("<dt id=\"py:class:greeter\">"));
    assert!(result.contains("<em class=\"property\">class</em>"));
    assert!(result.contains("<code class=\"sig-name\">greeter</code>"));
    assert!(!result.contains("final"));
}
#[test]
fn test_render_py_class_final_prefix_precedes_class_prefix() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyClass {
                module: None,
                signatures: NonEmptyVector::single("greeter".to_string()),
                is_final: true,
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    let final_pos = result.find("final").unwrap();
    let class_pos = result.find("class</em>").unwrap();
    assert!(final_pos < class_pos);
    assert!(result.contains("<em class=\"property\">final</em>"));
}
#[test]
fn test_render_formats_py_exception_domain_object() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyException {
                module: None,
                signatures: NonEmptyVector::single("greetererror".to_string()),
                is_final: false,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Raised when greeting fails.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dl class=\"py exception\">"));
    assert!(result.contains("<dt id=\"py:exception:greetererror\">"));
    assert!(result.contains("<em class=\"property\">exception</em>"));
    assert!(result.contains("<code class=\"sig-name\">greetererror</code>"));
    assert!(!result.contains("final"));
}
#[test]
fn test_render_py_exception_final_prefix_precedes_exception_prefix() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyException {
                module: None,
                signatures: NonEmptyVector::single("greetererror".to_string()),
                is_final: true,
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    let final_pos = result.find("final").unwrap();
    let exception_pos = result.find("exception</em>").unwrap();
    assert!(final_pos < exception_pos);
    assert!(result.contains("<em class=\"property\">final</em>"));
}
#[test]
fn test_render_formats_py_data_domain_object() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyData {
                module: None,
                signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                type_: None,
                value: None,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "The default timeout in seconds.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dl class=\"py data\">"));
    assert!(result.contains("<dt id=\"py:data:default_timeout\">"));
    assert!(result.contains("<code class=\"sig-name\">DEFAULT_TIMEOUT</code>"));
}
#[test]
fn test_render_formats_py_data_type_and_value() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyData {
                module: None,
                signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                type_: Some("int".to_string()),
                value: Some("30".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<p class=\"type\">Type: int</p>"));
    assert!(result.contains("<p class=\"value\">Value: 30</p>"));
}
#[test]
fn test_render_formats_py_attribute_domain_object() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                module: None,
                signatures: NonEmptyVector::single("Greeter.name".to_string()),
                type_: None,
                value: None,
                canonical: None,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "The greeter's name.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dl class=\"py attribute\">"));
    assert!(result.contains("<dt id=\"py:attribute:greeter.name\">"));
    assert!(result.contains("<code class=\"sig-name\">Greeter.name</code>"));
}
#[test]
fn test_render_formats_py_attribute_type_value_and_canonical() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                module: None,
                signatures: NonEmptyVector::single("Greeter.name".to_string()),
                type_: Some("str".to_string()),
                value: Some("\"anonymous\"".to_string()),
                canonical: Some("mymodule.MyClass.name".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<p class=\"type\">Type: str</p>"));
    assert!(result.contains("<p class=\"value\">Value: \"anonymous\"</p>"));
    assert!(result.contains("<p class=\"canonical\">Canonical: mymodule.MyClass.name</p>"));
}
#[test]
fn test_render_domain_object_resolves_nested_anonymous_hyperlink_in_body() {
    // Given — regression test for the collect_anonymous_targets catch-all:
    // an anonymous reference nested inside a DomainObject body must still resolve.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("greet(name)".to_string()),
                    body: vec![Node::Paragraph(vec![InlineNode::AnonymousReference(
                        "See more".to_string(),
                    )])],
                },
            )),
            Node::AnonymousTarget {
                uri: "https://example.com".to_string(),
            },
        ],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<a href=\"https://example.com\">See more</a>"));
}

//! Signature and anchor-id rendering tests for
//! [`super::render_domain_object`] and its option rendering — the
//! domain-agnostic shapes, with the `py:*` cases split into
//! [`super::python_tests`] and [`super::python_scoping_tests`].

use super::*;
use rinx_ast::{Directive, Document, InlineNode, Node, NonEmptyVector};
use rinx_index::ProjectIndex;

fn render_doc(doc: &Document) -> String {
    let index = ProjectIndex::default();
    crate::render(doc, &index, &doc.path).html
}

#[test]
fn test_render_domain_object_repeats_prefix_labels_on_every_dt() {
    // Given — flags such as `classmethod` belong to the directive as a
    // whole, so every alias's `<dt>` carries them.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyMethod {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::new(
                    "from_bytes(cls, data)".to_string(),
                    vec!["from_buffer(cls, buf)".to_string()],
                ),
                is_classmethod: true,
                is_staticmethod: false,
                is_abstractmethod: false,
                is_async: false,
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert_eq!(
        result
            .matches("<em class=\"property\">classmethod</em>")
            .count(),
        2
    );
}

#[test]
fn test_render_domain_object_emits_one_dt_per_declared_name() {
    // Given — the confirmed `library/socket.rst` shape.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyData {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::new(
                    "AF_UNIX".to_string(),
                    vec!["AF_INET".to_string(), "AF_INET6".to_string()],
                ),
                type_: None,
                value: None,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "The address families.".to_string(),
                )])],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then — one anchor per alias, all inside a single `<dl>`/`<dd>` pair.
    assert!(result.contains("<dt id=\"py:data:af_unix\">"));
    assert!(result.contains("<dt id=\"py:data:af_inet\">"));
    assert!(result.contains("<dt id=\"py:data:af_inet6\">"));
    assert_eq!(result.matches("<dt id=").count(), 3);
    assert_eq!(result.matches("<dd>").count(), 1);
    assert_eq!(result.matches("<dl class=").count(), 1);
    // The shared body renders once, under the single `<dd>`.
    assert_eq!(result.matches("The address families.").count(), 1);
}
#[test]
fn test_render_domain_object_shows_each_signature_as_its_own_dt_text() {
    // Given — display text keeps the full signature, while the anchor
    // uses only the extracted name.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyFunction {
                flags: rinx_ast::DescriptionFlags::default(),
                is_async: false,
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::new(
                    "spawnl(mode, file)".to_string(),
                    vec!["spawnle(mode, file, env)".to_string()],
                ),
                body: vec![],
            },
        ))],
    );

    // When
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dt id=\"py:function:spawnl\">"));
    assert!(result.contains("<code class=\"sig-name\">spawnl(mode, file)</code>"));
    assert!(result.contains("<dt id=\"py:function:spawnle\">"));
    assert!(result.contains("<code class=\"sig-name\">spawnle(mode, file, env)</code>"));
}
#[test]
fn test_render_domain_object_qualifies_every_alias_by_the_current_module() {
    // Given — the renderer's anchors must match the keys the analyzer
    // registered, for every alias and not just the primary.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "socket".to_string(),
                    options: rinx_ast::ModuleOptions::default(),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyData {
                    flags: rinx_ast::DescriptionFlags::default(),
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
    let result = render_doc(&doc);

    // Then
    assert!(result.contains("<dt id=\"py:data:socket.af_unix\">"));
    assert!(result.contains("<dt id=\"py:data:socket.af_inet\">"));
}
#[test]
fn test_render_domain_object_module_option_overrides_the_anchor_id() {
    // Given — the exact `docs/dev/known_bugs.md` shape: the anchor `id` must use
    // the `:module:` override, matching the analyzer's index key so the
    // cross-reference actually resolves to this anchor.
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
                    flags: rinx_ast::DescriptionFlags::default(),
                    module: Some("multiprocessing.managers".to_string()),
                    signatures: NonEmptyVector::single("SharedMemoryManager".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyMethod {
                            flags: rinx_ast::DescriptionFlags::default(),
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
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    flags: rinx_ast::DescriptionFlags::default(),
                    is_async: false,
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("track(size)".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let result = render_doc(&doc);

    // Then — the class and its nested method render under the override...
    assert!(result.contains("<dt id=\"py:class:multiprocessing.managers.sharedmemorymanager\">"));
    assert!(
        result.contains(
            "<dt id=\"py:method:multiprocessing.managers.sharedmemorymanager.get_server\">"
        )
    );
    // ...but the later sibling, with no override of its own, is restored
    // to the enclosing module rather than sticking with the override.
    assert!(result.contains("<dt id=\"py:function:multiprocessing.shared_memory.track\">"));
}
#[test]
fn test_render_domain_object_options_renders_nothing_for_py_function() {
    // Given
    let obj = rinx_ast::DomainObjectBody::PyFunction {
        flags: rinx_ast::DescriptionFlags::default(),
        is_async: false,
        module: None,
        is_decorator: false,
        signatures: NonEmptyVector::single("greet(name)".to_string()),
        body: vec![],
    };
    let mut html = String::new();

    // When
    render_domain_object_options(&mut html, &obj);

    // Then
    assert!(html.is_empty());
}
#[test]
fn test_render_domain_object_options_renders_nothing_for_py_exception() {
    // Given
    let obj = rinx_ast::DomainObjectBody::PyException {
        flags: rinx_ast::DescriptionFlags::default(),
        module: None,
        signatures: NonEmptyVector::single("GreeterError".to_string()),
        is_final: false,
        body: vec![],
    };
    let mut html = String::new();

    // When
    render_domain_object_options(&mut html, &obj);

    // Then
    assert!(html.is_empty());
}

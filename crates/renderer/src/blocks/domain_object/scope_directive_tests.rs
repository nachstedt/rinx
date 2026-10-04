//! How the directives that only move the scope — `.. py:currentmodule::`,
//! `.. program::` and the `.. c:namespace::` family — show on a page: as
//! nothing, and through the names and references written after them.

use crate::render;
use rinx_ast::{Directive, Document, InlineNode, Node, TargetSearchOrder};
use rinx_index::ProjectIndex;

#[test]
fn test_render_resolves_bare_reference_after_current_module_directive() {
    // Given — the `Doc/howto/enum.rst` shape: a document with no
    // `py:module` of its own, opening with `.. currentmodule:: enum`
    // before a bare reference to a class defined in another document
    // under that module.
    let doc = Document::new(
        "howto/enum.rst".to_string(),
        vec![
            Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string()),
            }),
            Node::Paragraph(vec![InlineNode::DomainObjectReference {
                object_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Class),
                name: "Enum".to_string(),
                display: "Enum".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }]),
        ],
    );
    let mut index = ProjectIndex::default();
    index.insert_domain_object(
        rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Class),
        "enum.Enum",
        "library/enum.rst",
    );

    // When
    let output = render(&doc, &index, &doc.path);

    // Then
    assert!(output.broken_links.is_empty());
    assert!(
        output
            .html
            .contains("href=\"../library/enum.html#py:class:enum.enum\"")
    );
}
#[test]
fn test_render_current_module_directive_emits_no_html() {
    // Given — real Sphinx's `currentmodule` documents nothing.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::PyCurrentModule {
            module: Some("enum".to_string()),
        })],
    );
    let index = ProjectIndex::default();

    // When
    let output = render(&doc, &index, &doc.path);

    // Then
    assert_eq!(output.html, "");
}
#[test]
fn test_render_program_directive_emits_no_html() {
    // Given — like `currentmodule`/`c:namespace`, `.. program::`
    // documents nothing; it only mutates scope.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::StdProgram {
            name: Some("dis".to_string()),
        })],
    );
    let index = ProjectIndex::default();

    // When
    let output = render(&doc, &index, &doc.path);

    // Then
    assert_eq!(output.html, "");
}
#[test]
fn test_render_c_namespace_directives_emit_no_html() {
    // Given — like `currentmodule`, the namespace family documents
    // nothing; it only mutates scope.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::CNamespace {
                namespace: Some("A.B".to_string()),
            }),
            Node::Directive(Directive::CNamespacePush {
                namespace: "C.D".to_string(),
            }),
            Node::Directive(Directive::CNamespacePop),
            Node::Directive(Directive::CNamespace { namespace: None }),
        ],
    );
    let index = ProjectIndex::default();

    // When
    let output = render(&doc, &index, &doc.path);

    // Then
    assert_eq!(output.html, "");
}
#[test]
fn test_render_c_namespace_null_inside_c_type_body_unqualifies_nested_macro_anchor() {
    // Given — the CPython `c-api/memory.rst` shape. The renderer must
    // agree with the analyzer's index key, or the anchor `id` and the
    // cross-reference target drift apart.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::CType {
                signatures: rinx_ast::NonEmptyVector::single("PyMemAllocatorDomain".into()),
                flags: rinx_ast::DescriptionFlags::default(),
                body: vec![
                    Node::Directive(Directive::CNamespace { namespace: None }),
                    Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::CMacro {
                            flags: rinx_ast::DescriptionFlags::default(),
                            signatures: rinx_ast::NonEmptyVector::single("PYMEM_DOMAIN_RAW".into()),
                            body: vec![],
                        },
                    )),
                ],
            },
        ))],
    );
    let index = ProjectIndex::default();

    // When
    let output = render(&doc, &index, &doc.path);

    // Then
    assert!(output.html.contains("<dt id=\"c:macro:pymem_domain_raw\">"));
    assert!(
        !output
            .html
            .contains("c:macro:pymemallocatordomain.pymem_domain_raw")
    );
}
#[test]
fn test_render_current_module_none_resets_scope_so_bare_reference_stays_unresolved() {
    // Given — `.. currentmodule:: None` clears the module, so a
    // subsequent bare reference that depended on it no longer resolves.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![
            Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string()),
            }),
            Node::Directive(Directive::PyCurrentModule { module: None }),
            Node::Paragraph(vec![InlineNode::DomainObjectReference {
                object_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Class),
                name: "Enum".to_string(),
                display: "Enum".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }]),
        ],
    );
    let mut index = ProjectIndex::default();
    index.insert_domain_object(
        rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Class),
        "enum.Enum",
        "library/enum.rst",
    );

    // When
    let output = render(&doc, &index, &doc.path);

    // Then
    assert_eq!(output.broken_links.len(), 1);
}

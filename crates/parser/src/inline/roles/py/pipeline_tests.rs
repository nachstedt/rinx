//! End-to-end `parse()` pipeline tests for the Python-domain inline roles
//! (`:func:`/`:mod:`/`:data:`/`:meth:`/`:class:`/`:attr:`/`:exc:`, plus the
//! `:const:` role alias). Kept separate purely for
//! `super::inline_domain_roles_py`'s line count.

use crate::{parse, parse_with_domain};
use rusty_sphinx_ast::{
    CObjectType, Domain, InlineNode, Node, ObjectType, PyObjectType, TargetSearchOrder,
};

/// The nodes with every source position cleared.
///
/// These tests are about what the roles *resolve to* — object type, name,
/// display text, link suppression. Where each one sits in the file is
/// asserted separately, in `crate::inline::pipeline_tests`, so that a change
/// to one concern cannot fail the other's tests.
fn without_spans(inlines: &[InlineNode]) -> Vec<InlineNode> {
    inlines
        .iter()
        .map(|inline| inline.clone().with_span(None))
        .collect()
}

#[test]
fn test_parse_bare_func_role_resolves_via_default_domain() {
    // Given
    let input = "See :func:`greet` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(
            inlines[1].clone().with_span(None),
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(CObjectType::Function),
                name: "greet".to_string(),
                display: "greet".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_prefixed_func_role_ignores_default_domain() {
    // Given
    let input = "See :py:func:`greet` and :c:func:`add`.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                name: "greet".to_string(),
                display: "greet".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::C(CObjectType::Function),
                name: "add".to_string(),
                display: "add".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_bare_mod_role_resolves_via_default_domain() {
    // Given
    let input = "See :mod:`greetings` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(
            inlines[1].clone().with_span(None),
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_prefixed_mod_role_ignores_default_domain() {
    // Given
    let input = "See :py:mod:`greetings`.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_mod_role_with_bang_prefix_suppresses_link_end_to_end() {
    // Given
    let input = "See :mod:`!curses` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Module),
                name: "curses".to_string(),
                display: "curses".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_bare_data_role_resolves_via_default_domain() {
    // Given
    let input = "See :data:`DEFAULT_TIMEOUT` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(
            inlines[1].clone().with_span(None),
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_prefixed_const_role_ignores_default_domain_and_matches_data_target() {
    // Given — `:py:const:` referencing the same name as a `.. py:data::`
    // definition must resolve to the identical `DomainObjectReference` a
    // `:py:data:` role would produce, since both share one namespace.
    let input = "See :py:const:`DEFAULT_TIMEOUT`.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_data_role_with_bang_prefix_suppresses_link_end_to_end() {
    // Given
    let input = "See :data:`!SECRET_KEY` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Data),
                name: "SECRET_KEY".to_string(),
                display: "SECRET_KEY".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_bare_meth_role_resolves_via_default_domain() {
    // Given
    let input = "See :meth:`greet` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(
            inlines[1].clone().with_span(None),
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Method),
                name: "greet".to_string(),
                display: "greet".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_prefixed_meth_role_ignores_default_domain() {
    // Given
    let input = "See :py:meth:`greet`.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Method),
                name: "greet".to_string(),
                display: "greet".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_meth_role_with_bang_prefix_suppresses_link_end_to_end() {
    // Given
    let input = "See :meth:`!secret_method` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Method),
                name: "secret_method".to_string(),
                display: "secret_method".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_bare_class_role_resolves_via_default_domain() {
    // Given
    let input = "See :class:`Greeter` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(
            inlines[1].clone().with_span(None),
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Class),
                name: "Greeter".to_string(),
                display: "Greeter".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_prefixed_class_role_ignores_default_domain() {
    // Given
    let input = "See :py:class:`Greeter`.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Class),
                name: "Greeter".to_string(),
                display: "Greeter".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_bare_attr_role_resolves_via_default_domain() {
    // Given
    let input = "See :attr:`Greeter.name` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert_eq!(
            inlines[1].clone().with_span(None),
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "Greeter.name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_prefixed_attr_role_ignores_default_domain() {
    // Given
    let input = "See :py:attr:`Greeter.name`.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "Greeter.name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_bare_attr_role_under_c_default_domain_falls_back_to_text() {
    // Given — `attr` is Python-only, like `mod`/`data`, so a bare role
    // under a `c` default domain doesn't resolve to a cross-reference.
    let input = "See :attr:`Greeter.name` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::Text(":attr:`Greeter.name`".to_string()))
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_attr_role_with_bang_prefix_suppresses_link_end_to_end() {
    // Given
    let input = "See :attr:`!Greeter.secret` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Attribute),
                name: "Greeter.secret".to_string(),
                display: "Greeter.secret".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_func_role_with_tilde_prefix_shortens_display_end_to_end() {
    // Given
    let input = "See :func:`~greetings.shout` for details.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::Py);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                name: "greetings.shout".to_string(),
                display: "shout".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_func_role_with_call_parens_resolves_the_bare_name_end_to_end() {
    // Given — `known_bugs.md` #1's reproducer sentence.
    let input = "Use :c:func:`Py_TYPE()` to inspect an object's type.";

    // When
    let doc = parse_with_domain("test.rst", input, Domain::C);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            without_spans(inlines).contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "Py_TYPE".to_string(),
                display: "Py_TYPE()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            })
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}
#[test]
fn test_parse_mixed_term_and_func_roles_in_one_paragraph() {
    // Given
    let input = "The :term:`environment` affects :func:`greet`.";

    // When
    let doc = parse("test.rst", input);

    // Then
    if let Node::Paragraph(inlines) = &doc.nodes[0] {
        assert!(
            inlines
                .iter()
                .any(|n| matches!(n, InlineNode::TermReference { .. }))
        );
        assert!(
            inlines
                .iter()
                .any(|n| matches!(n, InlineNode::DomainObjectReference { .. }))
        );
    } else {
        panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
    }
}

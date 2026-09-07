use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::FUNC_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:func:`/`:py:func:`/`:c:func:` role.
pub(crate) fn handle_func_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = FUNC_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    let target = parse_domain_object_target(&caps["name"], domain);
    // Both domains currently define "func", so this always resolves.
    let object_type =
        ObjectType::from_role_name(domain, "func").expect("every domain defines a 'func' role");
    target.into_inline_node(object_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::dispatch::handle_inline_match;
    use rusty_sphinx_ast::TargetSearchOrder;
    use rusty_sphinx_entity::EntitySchema;

    /// Escapes `raw` the way `parse_inline_text` does before any of the
    /// helpers below see it, so a unit test exercises the form those helpers
    /// are actually handed rather than a raw backslash they never meet.
    fn escaped(raw: &str) -> String {
        crate::inline::escapes::EscapedText::new(raw)
            .as_str()
            .to_string()
    }

    #[test]
    fn test_handle_func_match_resolves_via_given_domain() {
        let result = handle_func_match(":func:`foo`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_bare_uses_default_domain() {
        let result = handle_inline_match(
            "func",
            ":func:`foo`",
            None,
            Domain::C,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_explicit_py_domain() {
        let result = handle_inline_match(
            "func",
            ":py:func:`foo`",
            None,
            Domain::C,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_explicit_c_domain() {
        let result = handle_inline_match(
            "func",
            ":c:func:`add`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "add".to_string(),
                display: "add".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match(
            "func",
            ":func:`!foo`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match(
            "func",
            ":func:`~pkg.mod.foo`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "pkg.mod.foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_explicit_title() {
        // Given / When — the confirmed known_bugs.md example, with the role
        // markup escaped as `parse_inline_text` would hand it over.
        let result = handle_inline_match(
            "func",
            &escaped(r":func:`spawn\* <spawnl>`"),
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "spawnl".to_string(),
                display: "spawn*".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_func_match_strips_call_parens_from_a_c_domain_target() {
        // Given / When — `Doc/c-api/object.rst`'s own spelling.
        let result = handle_func_match(":c:func:`Py_TYPE()`", Domain::C);

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "Py_TYPE".to_string(),
                display: "Py_TYPE()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_func_match_strips_call_parens_from_a_py_domain_target() {
        // Given / When — `Doc/library/compileall.rst`'s own spelling.
        let result = handle_func_match(":py:func:`sys.getrecursionlimit()`", Domain::Py);

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "sys.getrecursionlimit".to_string(),
                display: "sys.getrecursionlimit()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_strips_call_parens() {
        // Given / When — routed through the dispatcher with an unrelated
        // default domain, so the explicit `c:` prefix is what picks the rule.
        let result = handle_inline_match(
            "func",
            ":c:func:`Py_SIZE()`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "Py_SIZE".to_string(),
                display: "Py_SIZE()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
}

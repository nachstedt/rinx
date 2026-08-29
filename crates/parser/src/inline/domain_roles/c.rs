use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use super::super::domain_target::parse_domain_object_target;
use super::super::{MACRO_ROLE_REGEX, STRUCT_ROLE_REGEX, TYPE_ROLE_REGEX, UNION_ROLE_REGEX};

/// Builds the `InlineNode` for a matched `:macro:`/`:c:macro:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`macro` is C-only, unlike the Python-only roles above).
pub(crate) fn handle_macro_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = MACRO_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "macro") {
        Some(object_type) => {
            parse_domain_object_target(&caps["name"], domain).into_inline_node(object_type)
        }
        None => InlineNode::Text(m_str.to_string()),
    }
}

/// Builds the `InlineNode` for a matched `:struct:`/`:c:struct:` role,
/// falling back to plain text if the role doesn't resolve for the given
/// domain (`struct` is C-only, like `macro`).
pub(crate) fn handle_struct_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = STRUCT_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "struct") {
        Some(object_type) => {
            parse_domain_object_target(&caps["name"], domain).into_inline_node(object_type)
        }
        None => InlineNode::Text(m_str.to_string()),
    }
}

/// Builds the `InlineNode` for a matched `:union:`/`:c:union:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`union` is C-only, like `macro`/`struct`).
pub(crate) fn handle_union_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = UNION_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "union") {
        Some(object_type) => {
            parse_domain_object_target(&caps["name"], domain).into_inline_node(object_type)
        }
        None => InlineNode::Text(m_str.to_string()),
    }
}

/// Builds the `InlineNode` for a matched `:type:`/`:c:type:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`type` is C-only, like `macro`/`struct`/`union`).
pub(crate) fn handle_type_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = TYPE_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "type") {
        Some(object_type) => {
            parse_domain_object_target(&caps["name"], domain).into_inline_node(object_type)
        }
        None => InlineNode::Text(m_str.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::handle_inline_match;
    use rusty_sphinx_ast::TargetSearchOrder;

    #[test]
    fn test_handle_macro_match_resolves_via_explicit_c_domain() {
        let result = handle_macro_match(":c:macro:`MAX`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "MAX".to_string(),
                display: "MAX".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_macro_match_resolves_bare_role_via_default_domain() {
        let result = handle_macro_match(":macro:`MAX`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "MAX".to_string(),
                display: "MAX".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_macro_match_falls_back_to_text_when_domain_lacks_macro_role() {
        let result = handle_macro_match(":macro:`MAX`", Domain::Py);
        assert_eq!(result, InlineNode::Text(":macro:`MAX`".to_string()));
    }
    #[test]
    fn test_handle_macro_match_bang_prefix_suppresses_link() {
        let result = handle_macro_match(":macro:`!MAX`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "MAX".to_string(),
                display: "MAX".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_macro_match_tilde_prefix_shortens_display() {
        let result = handle_macro_match(":macro:`~pkg.MAX`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "pkg.MAX".to_string(),
                display: "MAX".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_struct_match_resolves_via_explicit_c_domain() {
        let result = handle_struct_match(":c:struct:`Data`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Struct),
                name: "Data".to_string(),
                display: "Data".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_struct_match_resolves_bare_role_via_default_domain() {
        let result = handle_struct_match(":struct:`Data`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Struct),
                name: "Data".to_string(),
                display: "Data".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_struct_match_falls_back_to_text_when_domain_lacks_struct_role() {
        let result = handle_struct_match(":struct:`Data`", Domain::Py);
        assert_eq!(result, InlineNode::Text(":struct:`Data`".to_string()));
    }
    #[test]
    fn test_handle_union_match_resolves_via_explicit_c_domain() {
        let result = handle_union_match(":c:union:`Number`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Union),
                name: "Number".to_string(),
                display: "Number".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_union_match_resolves_bare_role_via_default_domain() {
        let result = handle_union_match(":union:`Number`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Union),
                name: "Number".to_string(),
                display: "Number".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_union_match_falls_back_to_text_when_domain_lacks_union_role() {
        let result = handle_union_match(":union:`Number`", Domain::Py);
        assert_eq!(result, InlineNode::Text(":union:`Number`".to_string()));
    }
    #[test]
    fn test_handle_type_match_resolves_via_explicit_c_domain() {
        let result = handle_type_match(":c:type:`PyMemAllocatorDomain`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Type),
                name: "PyMemAllocatorDomain".to_string(),
                display: "PyMemAllocatorDomain".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_type_match_resolves_bare_role_via_default_domain() {
        let result = handle_type_match(":type:`PyMemAllocatorDomain`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Type),
                name: "PyMemAllocatorDomain".to_string(),
                display: "PyMemAllocatorDomain".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_type_match_falls_back_to_text_when_domain_lacks_type_role() {
        let result = handle_type_match(":type:`PyMemAllocatorDomain`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::Text(":type:`PyMemAllocatorDomain`".to_string())
        );
    }
    #[test]
    fn test_handle_inline_match_type_variant_dispatches_through_handle_inline_match() {
        let result =
            handle_inline_match("type", ":c:type:`PyMemAllocatorDomain`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Type),
                name: "PyMemAllocatorDomain".to_string(),
                display: "PyMemAllocatorDomain".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_struct_variant_dispatches_through_handle_inline_match() {
        let result = handle_inline_match("struct", ":c:struct:`Data`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Struct),
                name: "Data".to_string(),
                display: "Data".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_union_variant_dispatches_through_handle_inline_match() {
        let result = handle_inline_match("union", ":c:union:`Number`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Union),
                name: "Number".to_string(),
                display: "Number".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_macro_variant_bare_uses_default_domain() {
        let result = handle_inline_match("macro", ":macro:`MAX`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "MAX".to_string(),
                display: "MAX".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_macro_variant_falls_back_to_text_when_unresolvable() {
        let result = handle_inline_match("macro", ":macro:`MAX`", None, Domain::Py);
        assert_eq!(result, InlineNode::Text(":macro:`MAX`".to_string()));
    }
    #[test]
    fn test_handle_macro_match_strips_call_parens() {
        // Given / When — `Doc/whatsnew/2.6.rst` references function-like
        // macros as :c:macro:`PyModule_AddIntMacro()`.
        let result = handle_macro_match(":c:macro:`MAX()`", Domain::C);

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "MAX".to_string(),
                display: "MAX()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_type_match_strips_call_parens() {
        // Given / When — the rule lives in the shared target parser, so it is
        // not confined to the call-shaped roles.
        let result = handle_type_match(":c:type:`Py_tracefunc()`", Domain::C);

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Type),
                name: "Py_tracefunc".to_string(),
                display: "Py_tracefunc()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
}

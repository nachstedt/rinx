use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::MACRO_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:macro:`/`:c:macro:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`macro` is C-only, unlike the Python-only roles).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::dispatch::handle_inline_match;
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
                span: None
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
                span: None
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
                span: None
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
                span: None
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
                span: None
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
                span: None
            }
        );
    }
}

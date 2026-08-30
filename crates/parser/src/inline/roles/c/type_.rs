use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::TYPE_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

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
    use crate::inline::dispatch::handle_inline_match;
    use rusty_sphinx_ast::TargetSearchOrder;

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
                span: None
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
                span: None
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
                span: None
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
                span: None
            }
        );
    }
}

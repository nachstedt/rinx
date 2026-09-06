use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::UNION_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::dispatch::handle_inline_match;
    use rusty_sphinx_ast::TargetSearchOrder;
    use rusty_sphinx_entity::EntitySchema;

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
                span: None
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
                span: None
            }
        );
    }
    #[test]
    fn test_handle_union_match_falls_back_to_text_when_domain_lacks_union_role() {
        let result = handle_union_match(":union:`Number`", Domain::Py);
        assert_eq!(result, InlineNode::Text(":union:`Number`".to_string()));
    }
    #[test]
    fn test_handle_inline_match_union_variant_dispatches_through_handle_inline_match() {
        let result = handle_inline_match(
            "union",
            ":c:union:`Number`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Union),
                name: "Number".to_string(),
                display: "Number".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
}

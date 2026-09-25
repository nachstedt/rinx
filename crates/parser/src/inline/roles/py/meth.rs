use rinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::METH_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:meth:`/`:py:meth:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`meth` is Python-only, like `mod`/`data`).
pub(crate) fn handle_meth_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = METH_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "meth") {
        Some(object_type) => {
            parse_domain_object_target(&caps["name"], domain).into_inline_node(object_type)
        }
        None => InlineNode::Text(m_str.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::TargetSearchOrder;

    #[test]
    fn test_handle_meth_match_resolves_meth_role() {
        let result = handle_meth_match(":meth:`greet`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rinx_ast::PyObjectType::Method),
                name: "greet".to_string(),
                display: "greet".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_meth_match_falls_back_to_text_when_domain_lacks_meth_role() {
        let result = handle_meth_match(":meth:`greet`", Domain::C);
        assert_eq!(result, InlineNode::Text(":meth:`greet`".to_string()));
    }
}

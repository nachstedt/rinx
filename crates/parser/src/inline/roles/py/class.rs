use rinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::CLASS_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:class:`/`:py:class:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`class` is Python-only, like `mod`/`data`/`meth`).
pub(crate) fn handle_class_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = CLASS_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "class") {
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
    fn test_handle_class_match_resolves_class_role() {
        let result = handle_class_match(":class:`Greeter`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rinx_ast::PyObjectType::Class),
                name: "Greeter".to_string(),
                display: "Greeter".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_class_match_falls_back_to_text_when_domain_lacks_class_role() {
        let result = handle_class_match(":class:`Greeter`", Domain::C);
        assert_eq!(result, InlineNode::Text(":class:`Greeter`".to_string()));
    }
}

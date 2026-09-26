use rinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::ATTR_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:attr:`/`:py:attr:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`attr` is Python-only, like `mod`/`data`).
pub(crate) fn handle_attr_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = ATTR_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "attr") {
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
    use rinx_ast::TargetSearchOrder;
    use rinx_entity::EntitySchema;

    #[test]
    fn test_handle_attr_match_resolves_when_domain_defines_attr_role() {
        let result = handle_attr_match(":attr:`Greeter.name`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "Greeter.name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_attr_match_explicit_title_splits_display_from_target() {
        let result = handle_attr_match(":attr:`the name <Greeter.name>`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "the name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_attr_match_falls_back_to_text_when_domain_lacks_attr_role() {
        let result = handle_attr_match(":attr:`Greeter.name`", Domain::C);
        assert_eq!(result, InlineNode::Text(":attr:`Greeter.name`".to_string()));
    }
    #[test]
    fn test_handle_inline_match_attr_variant() {
        let result = handle_inline_match(
            "attr",
            ":attr:`Greeter.name`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "Greeter.name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_attr_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match(
            "attr",
            ":attr:`!Greeter.secret`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rinx_ast::PyObjectType::Attribute),
                name: "Greeter.secret".to_string(),
                display: "Greeter.secret".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_attr_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match(
            "attr",
            ":attr:`~Greeter.name`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
}

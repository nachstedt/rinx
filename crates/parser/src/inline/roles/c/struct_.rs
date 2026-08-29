use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::STRUCT_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::dispatch::handle_inline_match;
    use rusty_sphinx_ast::TargetSearchOrder;

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
}

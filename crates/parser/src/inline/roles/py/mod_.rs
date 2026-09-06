use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::MOD_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:mod:`/`:py:mod:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`mod` is Python-only, so a bare role under a `c` default domain fails).
pub(crate) fn handle_mod_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = MOD_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "mod") {
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
    fn test_handle_mod_match_resolves_when_domain_defines_mod_role() {
        let result = handle_mod_match(":mod:`greetings`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_mod_match_falls_back_to_text_when_domain_lacks_mod_role() {
        let result = handle_mod_match(":mod:`greetings`", Domain::C);
        assert_eq!(result, InlineNode::Text(":mod:`greetings`".to_string()));
    }
    #[test]
    fn test_handle_inline_match_mod_variant_bare_uses_default_domain() {
        let result = handle_inline_match(
            "mod",
            ":mod:`greetings`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_explicit_py_domain() {
        let result = handle_inline_match(
            "mod",
            ":py:mod:`greetings`",
            None,
            Domain::C,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match(
            "mod",
            ":mod:`!curses`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "curses".to_string(),
                display: "curses".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match(
            "mod",
            ":mod:`~pkg.submodule`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "pkg.submodule".to_string(),
                display: "submodule".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_falls_back_to_text_when_unresolvable() {
        // Given — the `mod` role is Python-only, so a bare `:mod:` role in a
        // library whose default domain is `c` doesn't resolve to any object type.
        let result = handle_inline_match(
            "mod",
            ":mod:`greetings`",
            None,
            Domain::C,
            &EntitySchema::empty(),
        );
        assert_eq!(result, InlineNode::Text(":mod:`greetings`".to_string()));
    }
}

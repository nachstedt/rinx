use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::EXC_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:exc:`/`:py:exc:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`exc` is Python-only, like `mod`/`data`/`meth`/`class`/`attr`).
pub(crate) fn handle_exc_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = EXC_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "exc") {
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
    fn test_handle_exc_match_resolves_exc_role() {
        let result = handle_exc_match(":exc:`GreeterError`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "GreeterError".to_string(),
                display: "GreeterError".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_exc_match_falls_back_to_text_when_domain_lacks_exc_role() {
        let result = handle_exc_match(":exc:`GreeterError`", Domain::C);
        assert_eq!(result, InlineNode::Text(":exc:`GreeterError`".to_string()));
    }
    #[test]
    fn test_handle_exc_match_bang_prefix_suppresses_link() {
        let result = handle_exc_match(":exc:`!GreeterError`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "GreeterError".to_string(),
                display: "GreeterError".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_exc_match_tilde_prefix_shortens_display() {
        let result = handle_exc_match(":exc:`~greetings.GreeterError`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "greetings.GreeterError".to_string(),
                display: "GreeterError".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_exc_variant_bare_uses_default_domain() {
        let result = handle_inline_match("exc", ":exc:`GreeterError`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "GreeterError".to_string(),
                display: "GreeterError".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_exc_variant_explicit_py_domain() {
        let result = handle_inline_match("exc", ":py:exc:`GreeterError`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "GreeterError".to_string(),
                display: "GreeterError".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None
            }
        );
    }
    #[test]
    fn test_handle_inline_match_exc_variant_falls_back_to_text_when_unresolvable() {
        let result = handle_inline_match("exc", ":exc:`GreeterError`", None, Domain::C);
        assert_eq!(result, InlineNode::Text(":exc:`GreeterError`".to_string()));
    }
}

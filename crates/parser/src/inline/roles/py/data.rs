use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use crate::inline::regexes::DATA_ROLE_REGEX;

use super::super::target::parse_domain_object_target;

/// Builds the `InlineNode` for a matched `:data:`/`:py:data:`/`:const:`/
/// `:py:const:`/`:c:data:`/`:var:`/`:c:var:`/`:member:`/`:c:member:` role,
/// falling back to plain text if the role doesn't resolve for the given
/// domain (`const` is Python-only; `var`/`member` are C-only). `data`/`const`
/// resolve to the same [`rusty_sphinx_ast::PyObjectType::Data`] in the `py`
/// domain, and `data`/`var`/`member` all resolve to the same
/// [`rusty_sphinx_ast::CObjectType::Member`] in the `c` domain — matching
/// real Sphinx's C domain docs, which describe `:c:member:`/`:c:data:`/
/// `:c:var:` as equivalent role spellings — so e.g. `:c:data:` and `:c:var:`
/// targeting the same name link to the same `.. c:member::`/`.. c:var::`
/// definition.
pub(crate) fn handle_data_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = DATA_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, &caps["role"]) {
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
    fn test_handle_data_match_resolves_data_role() {
        let result = handle_data_match(":data:`DEFAULT_TIMEOUT`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_data_match_resolves_const_role_to_same_object_type_as_data() {
        // Given — real Sphinx has no separate `py:const` directive; `:const:`
        // is just an alternate role spelling for a `py:data` object.
        let data_result = handle_data_match(":data:`DEFAULT_TIMEOUT`", Domain::Py);
        let const_result = handle_data_match(":const:`DEFAULT_TIMEOUT`", Domain::Py);

        // Then
        assert_eq!(data_result, const_result);
    }
    #[test]
    fn test_handle_data_match_resolves_data_role_for_c_domain() {
        let result = handle_data_match(":data:`Py_mod_exec`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Member),
                name: "Py_mod_exec".to_string(),
                display: "Py_mod_exec".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_data_match_resolves_var_role_to_same_object_type_as_data_for_c_domain() {
        // Given — real Sphinx documents `:c:member:`/`:c:data:`/`:c:var:` as
        // equivalent role spellings for the same object.
        let data_result = handle_data_match(":data:`errno`", Domain::C);
        let var_result = handle_data_match(":var:`errno`", Domain::C);

        // Then
        assert_eq!(data_result, var_result);
    }
    #[test]
    fn test_handle_data_match_resolves_member_role_to_same_object_type_as_data_for_c_domain() {
        // Given — `:c:member:` is the canonical spelling; `:c:data:`/`:c:var:`
        // are equivalent alternates.
        let data_result = handle_data_match(":data:`count`", Domain::C);
        let member_result = handle_data_match(":member:`count`", Domain::C);

        // Then
        assert_eq!(data_result, member_result);
    }
    #[test]
    fn test_handle_data_match_falls_back_to_text_when_domain_lacks_member_role() {
        // Given — `member` is C-only, so it doesn't resolve for `py`.
        let result = handle_data_match(":member:`count`", Domain::Py);
        assert_eq!(result, InlineNode::Text(":member:`count`".to_string()));
    }
    #[test]
    fn test_handle_data_match_falls_back_to_text_when_domain_lacks_const_role() {
        // Given — `const` is Python-only, so it doesn't resolve for `c`.
        let result = handle_data_match(":const:`DEFAULT_TIMEOUT`", Domain::C);
        assert_eq!(
            result,
            InlineNode::Text(":const:`DEFAULT_TIMEOUT`".to_string())
        );
    }
    #[test]
    fn test_handle_data_match_falls_back_to_text_when_domain_lacks_var_role() {
        // Given — `var` is C-only, so it doesn't resolve for `py`.
        let result = handle_data_match(":var:`errno`", Domain::Py);
        assert_eq!(result, InlineNode::Text(":var:`errno`".to_string()));
    }
    #[test]
    fn test_handle_inline_match_data_variant_bare_uses_default_domain() {
        let result = handle_inline_match(
            "data",
            ":data:`DEFAULT_TIMEOUT`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_const_spelling_explicit_py_domain() {
        let result = handle_inline_match(
            "data",
            ":py:const:`DEFAULT_TIMEOUT`",
            None,
            Domain::C,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match(
            "data",
            ":data:`!SECRET_KEY`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "SECRET_KEY".to_string(),
                display: "SECRET_KEY".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match(
            "data",
            ":data:`~pkg.CONST`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "pkg.CONST".to_string(),
                display: "CONST".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_falls_back_to_text_when_unresolvable() {
        // Given — `data`/`const` roles are Python-only, so a bare `:const:`
        // role in a library whose default domain is `c` doesn't resolve.
        let result = handle_inline_match(
            "data",
            ":const:`DEFAULT_TIMEOUT`",
            None,
            Domain::C,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::Text(":const:`DEFAULT_TIMEOUT`".to_string())
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_explicit_c_domain() {
        // Given — the confirmed known_bugs.md regression: `:c:data:` must
        // resolve with `domain = Some("c")`, not fall through to a truncated
        // bare `:data:` match with `domain = None`.
        let result = handle_inline_match(
            "data",
            ":c:data:`Py_mod_exec`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Member),
                name: "Py_mod_exec".to_string(),
                display: "Py_mod_exec".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_var_spelling_explicit_c_domain() {
        let result = handle_inline_match(
            "data",
            ":c:var:`errno`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Member),
                name: "errno".to_string(),
                display: "errno".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_bare_role_uses_default_c_domain() {
        let result = handle_inline_match(
            "data",
            ":data:`Py_tp_bases`",
            None,
            Domain::C,
            &EntitySchema::empty(),
        );
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Member),
                name: "Py_tp_bases".to_string(),
                display: "Py_tp_bases".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: rusty_sphinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_bare_var_falls_back_to_text_under_py_domain() {
        // Given — `var` is C-only, so a bare `:var:` role in a library whose
        // default domain is `py` doesn't resolve.
        let result = handle_inline_match(
            "data",
            ":var:`errno`",
            None,
            Domain::Py,
            &EntitySchema::empty(),
        );
        assert_eq!(result, InlineNode::Text(":var:`errno`".to_string()));
    }
}

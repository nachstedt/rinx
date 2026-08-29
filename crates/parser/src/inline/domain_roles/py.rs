use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};

use super::super::domain_target::parse_domain_object_target;
use super::super::{
    ATTR_ROLE_REGEX, CLASS_ROLE_REGEX, DATA_ROLE_REGEX, EXC_ROLE_REGEX, FUNC_ROLE_REGEX,
    METH_ROLE_REGEX, MOD_ROLE_REGEX,
};

/// Builds the `InlineNode` for a matched `:func:`/`:py:func:`/`:c:func:` role.
pub(crate) fn handle_func_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = FUNC_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    let target = parse_domain_object_target(&caps["name"], domain);
    // Both domains currently define "func", so this always resolves.
    let object_type =
        ObjectType::from_role_name(domain, "func").expect("every domain defines a 'func' role");
    target.into_inline_node(object_type)
}

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
    use crate::inline::handle_inline_match;
    use rusty_sphinx_ast::TargetSearchOrder;

    /// Escapes `raw` the way `parse_inline_text` does before any of the
    /// helpers below see it, so a unit test exercises the form those helpers
    /// are actually handed rather than a raw backslash they never meet.
    fn escaped(raw: &str) -> String {
        crate::escapes::EscapedText::new(raw).as_str().to_string()
    }

    #[test]
    fn test_handle_func_match_resolves_via_given_domain() {
        let result = handle_func_match(":func:`foo`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
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
            }
        );
    }
    #[test]
    fn test_handle_mod_match_falls_back_to_text_when_domain_lacks_mod_role() {
        let result = handle_mod_match(":mod:`greetings`", Domain::C);
        assert_eq!(result, InlineNode::Text(":mod:`greetings`".to_string()));
    }
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
    fn test_handle_meth_match_resolves_meth_role() {
        let result = handle_meth_match(":meth:`greet`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
                name: "greet".to_string(),
                display: "greet".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_meth_match_falls_back_to_text_when_domain_lacks_meth_role() {
        let result = handle_meth_match(":meth:`greet`", Domain::C);
        assert_eq!(result, InlineNode::Text(":meth:`greet`".to_string()));
    }
    #[test]
    fn test_handle_class_match_resolves_class_role() {
        let result = handle_class_match(":class:`Greeter`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Class),
                name: "Greeter".to_string(),
                display: "Greeter".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_class_match_falls_back_to_text_when_domain_lacks_class_role() {
        let result = handle_class_match(":class:`Greeter`", Domain::C);
        assert_eq!(result, InlineNode::Text(":class:`Greeter`".to_string()));
    }
    #[test]
    fn test_handle_attr_match_resolves_when_domain_defines_attr_role() {
        let result = handle_attr_match(":attr:`Greeter.name`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "Greeter.name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_attr_match_explicit_title_splits_display_from_target() {
        let result = handle_attr_match(":attr:`the name <Greeter.name>`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "the name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_attr_match_falls_back_to_text_when_domain_lacks_attr_role() {
        let result = handle_attr_match(":attr:`Greeter.name`", Domain::C);
        assert_eq!(result, InlineNode::Text(":attr:`Greeter.name`".to_string()));
    }
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
            }
        );
    }
    #[test]
    fn test_handle_inline_match_exc_variant_falls_back_to_text_when_unresolvable() {
        let result = handle_inline_match("exc", ":exc:`GreeterError`", None, Domain::C);
        assert_eq!(result, InlineNode::Text(":exc:`GreeterError`".to_string()));
    }
    #[test]
    fn test_handle_inline_match_func_variant_bare_uses_default_domain() {
        let result = handle_inline_match("func", ":func:`foo`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_explicit_py_domain() {
        let result = handle_inline_match("func", ":py:func:`foo`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_explicit_c_domain() {
        let result = handle_inline_match("func", ":c:func:`add`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "add".to_string(),
                display: "add".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match("func", ":func:`!foo`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match("func", ":func:`~pkg.mod.foo`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "pkg.mod.foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_explicit_title() {
        // Given / When — the confirmed known_bugs.md example, with the role
        // markup escaped as `parse_inline_text` would hand it over.
        let result = handle_inline_match(
            "func",
            &escaped(r":func:`spawn\* <spawnl>`"),
            None,
            Domain::Py,
        );

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "spawnl".to_string(),
                display: "spawn*".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_func_match_strips_call_parens_from_a_c_domain_target() {
        // Given / When — `Doc/c-api/object.rst`'s own spelling.
        let result = handle_func_match(":c:func:`Py_TYPE()`", Domain::C);

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "Py_TYPE".to_string(),
                display: "Py_TYPE()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_func_match_strips_call_parens_from_a_py_domain_target() {
        // Given / When — `Doc/library/compileall.rst`'s own spelling.
        let result = handle_func_match(":py:func:`sys.getrecursionlimit()`", Domain::Py);

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "sys.getrecursionlimit".to_string(),
                display: "sys.getrecursionlimit()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_func_variant_strips_call_parens() {
        // Given / When — routed through the dispatcher with an unrelated
        // default domain, so the explicit `c:` prefix is what picks the rule.
        let result = handle_inline_match("func", ":c:func:`Py_SIZE()`", None, Domain::Py);

        // Then
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "Py_SIZE".to_string(),
                display: "Py_SIZE()".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_bare_uses_default_domain() {
        let result = handle_inline_match("mod", ":mod:`greetings`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_explicit_py_domain() {
        let result = handle_inline_match("mod", ":py:mod:`greetings`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match("mod", ":mod:`!curses`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "curses".to_string(),
                display: "curses".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match("mod", ":mod:`~pkg.submodule`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "pkg.submodule".to_string(),
                display: "submodule".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_mod_variant_falls_back_to_text_when_unresolvable() {
        // Given — the `mod` role is Python-only, so a bare `:mod:` role in a
        // library whose default domain is `c` doesn't resolve to any object type.
        let result = handle_inline_match("mod", ":mod:`greetings`", None, Domain::C);
        assert_eq!(result, InlineNode::Text(":mod:`greetings`".to_string()));
    }
    #[test]
    fn test_handle_inline_match_data_variant_bare_uses_default_domain() {
        let result = handle_inline_match("data", ":data:`DEFAULT_TIMEOUT`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_const_spelling_explicit_py_domain() {
        let result = handle_inline_match("data", ":py:const:`DEFAULT_TIMEOUT`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match("data", ":data:`!SECRET_KEY`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "SECRET_KEY".to_string(),
                display: "SECRET_KEY".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match("data", ":data:`~pkg.CONST`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "pkg.CONST".to_string(),
                display: "CONST".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_attr_variant() {
        let result = handle_inline_match("attr", ":attr:`Greeter.name`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "Greeter.name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_attr_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match("attr", ":attr:`!Greeter.secret`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Attribute),
                name: "Greeter.secret".to_string(),
                display: "Greeter.secret".to_string(),
                link: false,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_attr_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match("attr", ":attr:`~Greeter.name`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Attribute),
                name: "Greeter.name".to_string(),
                display: "name".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_falls_back_to_text_when_unresolvable() {
        // Given — `data`/`const` roles are Python-only, so a bare `:const:`
        // role in a library whose default domain is `c` doesn't resolve.
        let result = handle_inline_match("data", ":const:`DEFAULT_TIMEOUT`", None, Domain::C);
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
        let result = handle_inline_match("data", ":c:data:`Py_mod_exec`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Member),
                name: "Py_mod_exec".to_string(),
                display: "Py_mod_exec".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_var_spelling_explicit_c_domain() {
        let result = handle_inline_match("data", ":c:var:`errno`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Member),
                name: "errno".to_string(),
                display: "errno".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_bare_role_uses_default_c_domain() {
        let result = handle_inline_match("data", ":data:`Py_tp_bases`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Member),
                name: "Py_tp_bases".to_string(),
                display: "Py_tp_bases".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }
    #[test]
    fn test_handle_inline_match_data_variant_bare_var_falls_back_to_text_under_py_domain() {
        // Given — `var` is C-only, so a bare `:var:` role in a library whose
        // default domain is `py` doesn't resolve.
        let result = handle_inline_match("data", ":var:`errno`", None, Domain::Py);
        assert_eq!(result, InlineNode::Text(":var:`errno`".to_string()));
    }
}

#[cfg(test)]
mod pipeline_tests;

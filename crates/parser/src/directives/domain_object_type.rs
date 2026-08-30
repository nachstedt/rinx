use super::scope::split_domain_qualified_name;
use rusty_sphinx_ast::Domain;

/// The domain-object directive names the parser recognizes, resolved from a
/// directive's `domain:objtype` (or bare, default-domain-resolved) name.
/// Deliberately independent of `ast::ObjectType`: directive-name syntax
/// (including the legacy `classmethod`/`staticmethod` aliases, and
/// `decorator`/`decoratormethod`, none of which have an `ast::ObjectType` of
/// their own — they're just `py:function`/`py:method` with a flag forced) is
/// a parser concern, so it's modeled entirely here rather than borrowing the
/// shared object-type vocabulary the analyzer/renderer use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectiveObjectType {
    PyFunction,
    PyDecorator,
    PyModule,
    PyData,
    PyMethod,
    PyClassmethod,
    PyStaticmethod,
    PyDecoratorMethod,
    PyClass,
    PyAttribute,
    PyException,
    CFunction,
    CMacro,
    CStruct,
    CUnion,
    CMember,
    CType,
    /// `.. option::` or its legacy directive-name alias `.. cmdoption::` —
    /// domain-agnostic (the `std` domain), recognized regardless of
    /// `default_domain` and with no `domain:objtype`-qualified spelling
    /// modeled, matching this parser's existing bare-only treatment of
    /// `:ref:`/`:term:`/`.. toctree::`/`.. index::`. See
    /// [`resolve_domain_object_type`].
    StdCmdoption,
}

/// Resolves a directive name to a [`DirectiveObjectType`]: either an explicit
/// `domain:objtype` form (e.g. `py:function`), or a bare `objtype` name
/// (e.g. `function`) resolved via `default_domain`. The `classmethod`/
/// `staticmethod`/`decorator`/`decoratormethod` legacy aliases are recognized
/// here too, and only in the `py` domain — the domain/bare-name split gates
/// them for free, so a bare `.. classmethod::` under a `c` default domain
/// resolves `domain` to `Domain::C`, matches no arm, and falls through to
/// `Directive::Unknown`. Real Sphinx's `PythonDomain` registers `decorator`/
/// `decoratormethod` the same `py`-only way (`PyDecoratorFunction`/
/// `PyDecoratorMethod`, both delegating to `py:function`/`py:method`), so
/// there's no `c:decorator` to reject specially — it simply matches no arm.
pub(crate) fn resolve_domain_object_type(
    name: &str,
    default_domain: Domain,
) -> Option<DirectiveObjectType> {
    // `std`-domain directives bypass `default_domain`/`split_domain_qualified_name`
    // entirely, unlike every `py`/`c` arm below: they're recognized
    // unconditionally, the same way `.. toctree::`/`.. index::` are matched
    // by plain string equality before any domain logic runs. `cmdoption` is
    // real Sphinx's legacy directive-name alias for the same directive.
    if matches!(name, "option" | "cmdoption") {
        return Some(DirectiveObjectType::StdCmdoption);
    }
    let (domain, objtype_str) = split_domain_qualified_name(name, default_domain)?;
    match (domain, objtype_str) {
        (Domain::Py, "function") => Some(DirectiveObjectType::PyFunction),
        (Domain::Py, "decorator") => Some(DirectiveObjectType::PyDecorator),
        (Domain::Py, "module") => Some(DirectiveObjectType::PyModule),
        (Domain::Py, "data") => Some(DirectiveObjectType::PyData),
        (Domain::Py, "method") => Some(DirectiveObjectType::PyMethod),
        (Domain::Py, "classmethod") => Some(DirectiveObjectType::PyClassmethod),
        (Domain::Py, "staticmethod") => Some(DirectiveObjectType::PyStaticmethod),
        (Domain::Py, "decoratormethod") => Some(DirectiveObjectType::PyDecoratorMethod),
        (Domain::Py, "class") => Some(DirectiveObjectType::PyClass),
        (Domain::Py, "attribute") => Some(DirectiveObjectType::PyAttribute),
        (Domain::Py, "exception") => Some(DirectiveObjectType::PyException),
        (Domain::C, "function") => Some(DirectiveObjectType::CFunction),
        (Domain::C, "macro") => Some(DirectiveObjectType::CMacro),
        (Domain::C, "struct") => Some(DirectiveObjectType::CStruct),
        (Domain::C, "union") => Some(DirectiveObjectType::CUnion),
        // `.. c:var::` is a pure directive-name alias for `.. c:member::` in
        // real Sphinx (both register the same handler) — no forced-flag
        // distinction to carry, unlike `classmethod`/`staticmethod` above.
        (Domain::C, "member" | "var") => Some(DirectiveObjectType::CMember),
        (Domain::C, "type") => Some(DirectiveObjectType::CType),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_domain_object_type_explicit_prefix_ignores_default_domain() {
        // Given
        let name = "c:function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CFunction));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_name_uses_default_domain() {
        // Given
        let name = "function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CFunction));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_struct_resolves() {
        // Given
        let name = "c:struct";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CStruct));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_union_resolves() {
        // Given
        let name = "c:union";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CUnion));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_member_resolves() {
        // Given
        let name = "c:member";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_c_var_resolves_as_member_alias() {
        // Given — `.. c:var::` is a pure directive-name alias for `c:member`.
        let name = "c:var";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_option_resolves_regardless_of_default_domain() {
        // Given — `std`-domain, so it must not be gated by `default_domain`.
        let name = "option";

        // When / Then
        assert_eq!(
            resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py),
            Some(DirectiveObjectType::StdCmdoption)
        );
        assert_eq!(
            resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C),
            Some(DirectiveObjectType::StdCmdoption)
        );
    }

    #[test]
    fn test_resolve_domain_object_type_cmdoption_is_a_legacy_alias_for_option() {
        // Given — real Sphinx's legacy directive name for the same directive.
        let name = "cmdoption";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::StdCmdoption));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_type_resolves() {
        // Given
        let name = "c:type";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CType));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_type_uses_default_domain() {
        // Given
        let name = "type";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CType));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_member_uses_default_domain() {
        // Given
        let name = "member";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_domain_prefix() {
        // Given
        let name = "rust:function";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_object_type() {
        // Given
        let name = "py:struct";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_bare_classmethod_resolves_in_py_domain() {
        // Given — a bare `classmethod` directive name under the `py` default
        // domain (the legacy `py:method` alias)
        let name = "classmethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyClassmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_staticmethod_resolves_in_py_domain() {
        // Given
        let name = "staticmethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyStaticmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_py_classmethod_resolves() {
        // Given — the explicit `py:classmethod` domain-prefixed form
        let name = "py:classmethod";

        // When — resolved even when the default domain is `c`
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyClassmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_classmethod_rejected_in_c_domain() {
        // Given — a bare `classmethod` under a `c` default domain: the alias
        // is `py`-only, so this must not resolve
        let name = "classmethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decorator_resolves_in_py_domain() {
        // Given — a bare `decorator` directive name under the `py` default
        // domain (the legacy `py:function` alias; see `known_bugs.md` #1)
        let name = "decorator";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecorator));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decoratormethod_resolves_in_py_domain() {
        // Given
        let name = "decoratormethod";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecoratorMethod));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_py_decorator_resolves() {
        // Given — the explicit `py:decorator` domain-prefixed form
        let name = "py:decorator";

        // When — resolved even when the default domain is `c`
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecorator));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decorator_rejected_in_c_domain() {
        // Given — a bare `decorator` under a `c` default domain: the alias is
        // `py`-only, so this must not resolve (real Sphinx defines no
        // `c:decorator`)
        let name = "decorator";

        // When
        let result = resolve_domain_object_type(name, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(result, None);
    }
}

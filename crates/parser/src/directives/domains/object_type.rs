use super::super::scope::split_domain_qualified_name;
use rinx_ast::Domain;

use super::{c, py, std_};
use crate::directives::spec::DirectiveSpec;

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
    // entirely, unlike every `py`/`c` row: they're recognized unconditionally,
    // the same way `.. toctree::`/`.. index::` are matched by plain string
    // equality before any domain logic runs.
    if let Some(object_type) = find_object_directive(None, name) {
        return Some(object_type);
    }
    let (domain, objtype_str) = split_domain_qualified_name(name, default_domain)?;
    find_object_directive(Some(domain), objtype_str)
}

/// The spec of a domain object of `object_type`, beside its parser: Sphinx's
/// `ObjectDescription`'s one argument keeping the rest of its lines — one
/// signature per line — options and content, but a single word for
/// `PyModule`, which declares no `final_argument_whitespace`.
pub(crate) const fn object_spec(
    object_type: DirectiveObjectType,
) -> &'static DirectiveSpec<'static> {
    match object_type {
        DirectiveObjectType::PyFunction => &py::function::FUNCTION_SPEC,
        DirectiveObjectType::PyDecorator => &py::function::DECORATOR_SPEC,
        DirectiveObjectType::PyModule => &py::module::SPEC,
        DirectiveObjectType::PyData => &py::data::SPEC,
        DirectiveObjectType::PyMethod => &py::method::METHOD_SPEC,
        DirectiveObjectType::PyClassmethod => &py::method::CLASSMETHOD_SPEC,
        DirectiveObjectType::PyStaticmethod => &py::method::STATICMETHOD_SPEC,
        DirectiveObjectType::PyDecoratorMethod => &py::method::DECORATOR_METHOD_SPEC,
        DirectiveObjectType::PyClass => &py::class::CLASS_SPEC,
        DirectiveObjectType::PyAttribute => &py::attribute::SPEC,
        DirectiveObjectType::PyException => &py::class::EXCEPTION_SPEC,
        DirectiveObjectType::CFunction => &c::FUNCTION_SPEC,
        DirectiveObjectType::CMacro => &c::MACRO_SPEC,
        DirectiveObjectType::CStruct => &c::struct_::SPEC,
        DirectiveObjectType::CUnion => &c::union::SPEC,
        DirectiveObjectType::CMember => &c::member::SPEC,
        DirectiveObjectType::CType => &c::type_::SPEC,
        DirectiveObjectType::StdCmdoption => &std_::cmdoption::SPEC,
    }
}

/// Every domain-object directive: the domain its names are qualified in, and
/// its spec.
pub(crate) fn object_directive_specs()
-> impl Iterator<Item = (Option<Domain>, &'static DirectiveSpec<'static>)> {
    OBJECT_DIRECTIVES
        .iter()
        .map(|&(domain, object_type)| (domain, object_spec(object_type)))
}

/// Every name a domain-object directive is written as, unqualified — what an
/// entity schema's section may not be called.
pub(crate) fn object_directive_names() -> impl Iterator<Item = &'static str> {
    object_directive_specs().flat_map(|(_, spec)| spec.names.iter().copied())
}

/// The object type `name` spells in `domain`, if any.
fn find_object_directive(domain: Option<Domain>, name: &str) -> Option<DirectiveObjectType> {
    OBJECT_DIRECTIVES
        .iter()
        .find(|&&(row_domain, object_type)| {
            row_domain == domain && object_spec(object_type).names.contains(&name)
        })
        .map(|&(_, object_type)| object_type)
}

/// Every domain-object directive: the domain its names are qualified in —
/// `None` for a `std` one, recognized bare whatever the default domain — and
/// the object type its names resolve to, spelled by its spec.
///
/// An alias is a further name of a spec: `cmdoption` is real Sphinx's legacy
/// directive-name alias for `option`, and `.. c:var::` a pure directive-name
/// alias for `.. c:member::` (both register the same handler) — no
/// forced-flag distinction to carry, unlike `classmethod`/`staticmethod`,
/// which are object types of their own.
const OBJECT_DIRECTIVES: &[(Option<Domain>, DirectiveObjectType)] = &[
    (None, DirectiveObjectType::StdCmdoption),
    (Some(Domain::Py), DirectiveObjectType::PyFunction),
    (Some(Domain::Py), DirectiveObjectType::PyDecorator),
    (Some(Domain::Py), DirectiveObjectType::PyModule),
    (Some(Domain::Py), DirectiveObjectType::PyData),
    (Some(Domain::Py), DirectiveObjectType::PyMethod),
    (Some(Domain::Py), DirectiveObjectType::PyClassmethod),
    (Some(Domain::Py), DirectiveObjectType::PyStaticmethod),
    (Some(Domain::Py), DirectiveObjectType::PyDecoratorMethod),
    (Some(Domain::Py), DirectiveObjectType::PyClass),
    (Some(Domain::Py), DirectiveObjectType::PyAttribute),
    (Some(Domain::Py), DirectiveObjectType::PyException),
    (Some(Domain::C), DirectiveObjectType::CFunction),
    (Some(Domain::C), DirectiveObjectType::CMacro),
    (Some(Domain::C), DirectiveObjectType::CStruct),
    (Some(Domain::C), DirectiveObjectType::CUnion),
    (Some(Domain::C), DirectiveObjectType::CMember),
    (Some(Domain::C), DirectiveObjectType::CType),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_domain_object_type_explicit_prefix_ignores_default_domain() {
        // Given
        let name = "c:function";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CFunction));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_name_uses_default_domain() {
        // Given
        let name = "function";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CFunction));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_struct_resolves() {
        // Given
        let name = "c:struct";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CStruct));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_union_resolves() {
        // Given
        let name = "c:union";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CUnion));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_member_resolves() {
        // Given
        let name = "c:member";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_c_var_resolves_as_member_alias() {
        // Given — `.. c:var::` is a pure directive-name alias for `c:member`.
        let name = "c:var";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_option_resolves_regardless_of_default_domain() {
        // Given — `std`-domain, so it must not be gated by `default_domain`.
        let name = "option";

        // When / Then
        assert_eq!(
            resolve_domain_object_type(name, rinx_ast::Domain::Py),
            Some(DirectiveObjectType::StdCmdoption)
        );
        assert_eq!(
            resolve_domain_object_type(name, rinx_ast::Domain::C),
            Some(DirectiveObjectType::StdCmdoption)
        );
    }

    #[test]
    fn test_resolve_domain_object_type_cmdoption_is_a_legacy_alias_for_option() {
        // Given — real Sphinx's legacy directive name for the same directive.
        let name = "cmdoption";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::StdCmdoption));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_c_type_resolves() {
        // Given
        let name = "c:type";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CType));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_type_uses_default_domain() {
        // Given
        let name = "type";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CType));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_member_uses_default_domain() {
        // Given
        let name = "member";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::CMember));
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_domain_prefix() {
        // Given
        let name = "rust:function";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_rejects_unknown_object_type() {
        // Given
        let name = "py:struct";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_bare_classmethod_resolves_in_py_domain() {
        // Given — a bare `classmethod` directive name under the `py` default
        // domain (the legacy `py:method` alias)
        let name = "classmethod";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyClassmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_staticmethod_resolves_in_py_domain() {
        // Given
        let name = "staticmethod";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyStaticmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_py_classmethod_resolves() {
        // Given — the explicit `py:classmethod` domain-prefixed form
        let name = "py:classmethod";

        // When — resolved even when the default domain is `c`
        let result = resolve_domain_object_type(name, rinx_ast::Domain::C);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyClassmethod));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_classmethod_rejected_in_c_domain() {
        // Given — a bare `classmethod` under a `c` default domain: the alias
        // is `py`-only, so this must not resolve
        let name = "classmethod";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::C);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decorator_resolves_in_py_domain() {
        // Given — a bare `decorator` directive name under the `py` default
        // domain (the legacy `py:function` alias; see `docs/dev/known_bugs.md` #1)
        let name = "decorator";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecorator));
    }

    #[test]
    fn test_resolve_domain_object_type_bare_decoratormethod_resolves_in_py_domain() {
        // Given
        let name = "decoratormethod";

        // When
        let result = resolve_domain_object_type(name, rinx_ast::Domain::Py);

        // Then
        assert_eq!(result, Some(DirectiveObjectType::PyDecoratorMethod));
    }

    #[test]
    fn test_resolve_domain_object_type_explicit_py_decorator_resolves() {
        // Given — the explicit `py:decorator` domain-prefixed form
        let name = "py:decorator";

        // When — resolved even when the default domain is `c`
        let result = resolve_domain_object_type(name, rinx_ast::Domain::C);

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
        let result = resolve_domain_object_type(name, rinx_ast::Domain::C);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_object_directive_matches_domain_and_name_together() {
        // Given / When / Then — `function` is a row in both domains
        assert_eq!(
            find_object_directive(Some(Domain::C), "function"),
            Some(DirectiveObjectType::CFunction)
        );
        assert_eq!(find_object_directive(Some(Domain::C), "class"), None);
        assert_eq!(find_object_directive(None, "function"), None);
    }

    #[test]
    fn test_object_directive_names_lists_every_spelling_once_per_domain() {
        // Given / When
        let names: Vec<&str> = object_directive_names().collect();

        // Then — aliases are listed, and a name two domains share is listed
        // for each
        assert!(names.contains(&"cmdoption"));
        assert!(names.contains(&"var"));
        assert_eq!(names.iter().filter(|name| **name == "function").count(), 2);
    }

    #[test]
    fn test_every_listed_name_resolves_in_its_own_domain() {
        // Given / When / Then
        for &(domain, object_type) in OBJECT_DIRECTIVES {
            for name in object_spec(object_type).names {
                let default_domain = domain.unwrap_or(Domain::Py);
                assert_eq!(
                    resolve_domain_object_type(name, default_domain),
                    Some(object_type),
                    "`{name}` does not resolve under {default_domain:?}"
                );
            }
        }
    }

    #[test]
    fn test_object_spec_reads_a_module_as_one_word_and_any_other_object_as_text() {
        // Given / When / Then
        assert!(
            !object_spec(DirectiveObjectType::PyModule)
                .arguments
                .final_whitespace
        );
        assert!(
            object_spec(DirectiveObjectType::PyFunction)
                .arguments
                .final_whitespace
        );
        assert!(
            object_spec(DirectiveObjectType::CMember)
                .arguments
                .final_whitespace
        );
    }

    #[test]
    fn test_object_directive_specs_list_every_object_type_once() {
        // Given / When
        let specs: Vec<(Option<Domain>, &DirectiveSpec)> = object_directive_specs().collect();

        // Then — no domain spells one name twice
        for (index, (domain, spec)) in specs.iter().enumerate() {
            for name in spec.names {
                assert!(
                    specs[..index]
                        .iter()
                        .all(|(other_domain, other)| other_domain != domain
                            || !other.names.contains(name)),
                    "`{name}` is listed twice in {domain:?}"
                );
            }
        }
    }
}

use rusty_sphinx_ast::{Directive, Domain};

/// Tries to parse `name`/`argument` as one of the *scope* directives: the
/// content-less, domain-qualified directives that document nothing and only
/// move the current scope the analyzer and renderer qualify against —
/// `.. currentmodule::` in the `py` domain, and the `.. c:namespace::`
/// family in the `c` domain.
///
/// They share one function because they share one dispatch rule
/// ([`split_domain_qualified_name`]), which is also what gates each to its
/// own domain: a bare `.. currentmodule::` under a `c` default domain, or a
/// bare `.. namespace::` under a `py` one, matches no arm and falls through
/// to [`Directive::Unknown`].
///
/// The hyphenated namespace names need no special handling:
/// `split_domain_qualified_name` only splits on `:`, so the hyphen rides
/// along in the bare name exactly as `code-block`/`list-table` already do.
///
/// A `namespace-push` with no scope argument is malformed — it returns
/// `None` so the caller falls through to `Directive::Unknown`, rather than
/// being silently accepted as a no-op push that a later `namespace-pop`
/// would then unbalance.
pub(super) fn try_parse_scope_directive(
    name: &str,
    argument: &str,
    default_domain: Domain,
) -> Option<Directive> {
    // `.. program::` is `std`-domain and bypasses `default_domain` entirely,
    // exactly like `.. option::`/`.. cmdoption::` above (see
    // `resolve_domain_object_type`) — recognized unconditionally rather than
    // gated by `split_domain_qualified_name`.
    if name == "program" {
        return Some(Directive::StdProgram {
            name: parse_program_argument(argument),
        });
    }
    match split_domain_qualified_name(name, default_domain) {
        Some((Domain::Py, "currentmodule")) => Some(Directive::PyCurrentModule {
            module: parse_current_module_argument(argument),
        }),
        Some((Domain::C, "namespace")) => Some(Directive::CNamespace {
            namespace: parse_c_namespace_argument(argument),
        }),
        Some((Domain::C, "namespace-push")) => {
            (!argument.is_empty()).then(|| Directive::CNamespacePush {
                namespace: argument.to_string(),
            })
        }
        // Any argument is ignored: real Sphinx's pop takes none, and undoes
        // the previous push whatever it was.
        Some((Domain::C, "namespace-pop")) => Some(Directive::CNamespacePop),
        _ => None,
    }
}

/// Parses a `.. c:namespace::` argument. `NULL` and `0` (real Sphinx's two
/// documented spellings for "reset to global scope") and an empty argument
/// all clear the scope; anything else becomes the new scope verbatim.
///
/// Deliberately distinct from [`parse_current_module_argument`]'s `None`
/// sentinel — the two domains spell their reset differently, and neither
/// should accept the other's spelling as magic.
fn parse_c_namespace_argument(argument: &str) -> Option<String> {
    if argument.is_empty() || argument == "NULL" || argument == "0" {
        None
    } else {
        Some(argument.to_string())
    }
}

/// Splits a directive name into its domain and bare name — either an
/// explicit `domain:name` form (e.g. `py:function`), or a bare name (e.g.
/// `function`) resolved via `default_domain`. Shared by
/// [`resolve_domain_object_type`] and [`try_parse_scope_directive`], so both
/// obey the same domain rules: a bare `.. currentmodule::` under a `c`
/// default domain, or an explicit `.. c:currentmodule::`, must not match the
/// `py`-only arm that consumes it, and likewise a bare `.. namespace::`
/// under a `py` default domain must not match the `c`-only arms.
pub(super) fn split_domain_qualified_name(
    name: &str,
    default_domain: Domain,
) -> Option<(Domain, &str)> {
    match name.split_once(':') {
        Some((domain_str, rest)) => Some((domain_str.parse::<Domain>().ok()?, rest)),
        None => Some((default_domain, name)),
    }
}

/// Parses a `.. currentmodule::`/`.. py:currentmodule::` argument. `None`
/// (the reset form Sphinx uses to clear the current module, written
/// `.. currentmodule:: None`) and an empty argument both clear the module;
/// anything else becomes the new module name verbatim.
fn parse_current_module_argument(argument: &str) -> Option<String> {
    if argument.is_empty() || argument == "None" {
        None
    } else {
        Some(argument.to_string())
    }
}

/// Parses a `.. program::` argument. `None` (real Sphinx's `.. program:: None`
/// reset form) and an empty argument both clear the current program;
/// anything else is normalized like real Sphinx's `ws_re.sub('-', name)` —
/// every run of whitespace collapsed to a single `-` — and kept as the new
/// current program, e.g. `"python -m py_compile"` -> `"python--m-py_compile"`,
/// `"unittest discover"` -> `"unittest-discover"`.
fn parse_program_argument(argument: &str) -> Option<String> {
    if argument.is_empty() || argument == "None" {
        None
    } else {
        Some(argument.split_whitespace().collect::<Vec<_>>().join("-"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Node;

    #[test]
    fn test_parse_creates_py_current_module_directive_from_bare_form() {
        // Given — the `py` domain is the parser's default, so the bare form
        // is what CPython's docs actually write.
        let input = ".. currentmodule:: enum";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_creates_py_current_module_directive_from_explicit_domain_form() {
        // Given
        let input = ".. py:currentmodule:: enum";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_current_module_none_argument_clears_module() {
        // Given
        let input = ".. currentmodule:: None";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::PyCurrentModule { module: None })]
        );
    }

    #[test]
    fn test_parse_bare_current_module_under_c_default_domain_is_unknown() {
        // Given — `currentmodule` is `py`-only; a bare directive under a `c`
        // default domain must not resolve to it.
        let input = ".. currentmodule:: enum";

        // When
        let doc = crate::parse_with_domain("test.rst", input, rusty_sphinx_ast::Domain::C);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "currentmodule"
        ));
    }

    #[test]
    fn test_parse_explicit_c_current_module_is_unknown() {
        // Given — `c:currentmodule` names no real directive.
        let input = ".. c:currentmodule:: enum";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "c:currentmodule"
        ));
    }

    #[test]
    fn test_parse_creates_c_namespace_directive_from_explicit_domain_form() {
        // Given
        let input = ".. c:namespace:: A.B";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace {
                namespace: Some("A.B".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_creates_c_namespace_directive_from_bare_form_under_c_default_domain() {
        // Given — a library whose `default_domain` is `c` writes it bare.
        let input = ".. namespace:: A.B";

        // When
        let doc = crate::parse_with_domain("test.rst", input, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace {
                namespace: Some("A.B".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_c_namespace_null_argument_resets_to_global_scope() {
        // Given — the spelling CPython's `c-api/memory.rst` actually uses.
        let input = ".. c:namespace:: NULL";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace { namespace: None })]
        );
    }

    #[test]
    fn test_parse_c_namespace_zero_argument_resets_to_global_scope() {
        // Given — real Sphinx documents `0` as an alternative to `NULL`.
        let input = ".. c:namespace:: 0";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespace { namespace: None })]
        );
    }

    #[test]
    fn test_parse_creates_c_namespace_push_directive() {
        // Given
        let input = ".. c:namespace-push:: C.D";

        // When
        let doc = parse("test.rst", input);

        // Then — the hyphenated name resolves despite the domain split only
        // ever splitting on ':'.
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::CNamespacePush {
                namespace: "C.D".to_string()
            })]
        );
    }

    #[test]
    fn test_parse_creates_c_namespace_pop_directive() {
        // Given
        let input = ".. c:namespace-pop::";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes, vec![Node::Directive(Directive::CNamespacePop)]);
    }

    #[test]
    fn test_parse_c_namespace_push_without_argument_is_unknown() {
        // Given — a push with no scope is malformed; accepting it as a no-op
        // would leave a later `namespace-pop` unbalanced.
        let input = ".. c:namespace-push::";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "c:namespace-push"
        ));
    }

    #[test]
    fn test_parse_bare_c_namespace_under_py_default_domain_is_unknown() {
        // Given — the namespace family is `c`-only; there is no
        // `py:namespace`, and `py` is the parser's default domain.
        let input = ".. namespace:: A.B";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Unknown { name, .. }) if name == "namespace"
        ));
    }

    #[test]
    fn test_parse_creates_program_directive() {
        // Given
        let input = ".. program:: dis";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::StdProgram {
                name: Some("dis".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_program_none_argument_resets() {
        // Given
        let input = ".. program:: None";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::StdProgram { name: None })]
        );
    }

    #[test]
    fn test_parse_program_recognized_under_any_default_domain() {
        // Given — `std`-domain, so unlike `.. currentmodule::`/`.. namespace::`
        // it must not be gated by `default_domain`.
        let input = ".. program:: dis";

        // When
        let doc = crate::parse_with_domain("test.rst", input, rusty_sphinx_ast::Domain::C);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::StdProgram {
                name: Some("dis".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_program_multi_word_name_collapses_whitespace_to_hyphens() {
        // Given — matches real Sphinx's `ws_re.sub('-', name)`.
        let input = ".. program:: python -m py_compile";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes,
            vec![Node::Directive(Directive::StdProgram {
                name: Some("python--m-py_compile".to_string())
            })]
        );
    }

    #[test]
    fn test_parse_program_argument_empty_clears() {
        // Given / When / Then
        assert_eq!(parse_program_argument(""), None);
    }
}

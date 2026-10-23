use super::error_node::malformed_node;
use super::invocation::DirectiveInvocation;
use super::spec::{Arguments, DirectiveSpec, OptionSet};
use rinx_ast::{Directive, Domain};

/// Interprets one of the *scope* directives, read by its
/// [`ScopeDirective::spec`]: the content-less, domain-qualified directives
/// that document nothing and only move the current scope the analyzer and
/// renderer qualify against — `.. currentmodule::` in the `py` domain, and the
/// `.. c:namespace::` family in the `c` domain.
///
/// A `namespace-push` with no scope argument — reported by the reader — is
/// malformed: it degrades to [`Directive::Malformed`] rather than being
/// silently accepted as a no-op push that a later `namespace-pop` would then
/// unbalance.
pub(super) fn parse_scope_directive(
    scope: ScopeDirective,
    invocation: &DirectiveInvocation<'_, '_>,
) -> Directive {
    let argument = invocation.argument(0);
    match scope {
        ScopeDirective::Program => Directive::StdProgram {
            name: parse_program_argument(argument),
        },
        ScopeDirective::CurrentModule => Directive::PyCurrentModule {
            module: parse_current_module_argument(argument),
        },
        ScopeDirective::Namespace => Directive::CNamespace {
            namespace: parse_c_namespace_argument(argument),
        },
        ScopeDirective::NamespacePush if argument.is_empty() => malformed_node(
            invocation.name,
            argument,
            invocation.source_body,
            format!(
                "{}: the directive needs a scope name as its argument",
                invocation.name
            ),
        ),
        ScopeDirective::NamespacePush => Directive::CNamespacePush {
            namespace: argument.to_string(),
        },
        ScopeDirective::NamespacePop => Directive::CNamespacePop,
    }
}

/// The scope directives, by what each one moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScopeDirective {
    Program,
    CurrentModule,
    Namespace,
    NamespacePush,
    NamespacePop,
}

impl ScopeDirective {
    /// Its spec, transcribed from Sphinx's classes: one argument each — the
    /// whole line for a program or a C scope, a single word for a Python
    /// module — but none for a pop, and no options or content for any.
    pub(super) const fn spec(self) -> &'static DirectiveSpec<'static> {
        match self {
            Self::Program => &PROGRAM_SPEC,
            Self::CurrentModule => &CURRENT_MODULE_SPEC,
            Self::Namespace => &NAMESPACE_SPEC,
            Self::NamespacePush => &NAMESPACE_PUSH_SPEC,
            Self::NamespacePop => &NAMESPACE_POP_SPEC,
        }
    }
}

/// A scope directive's spec: `arguments`, and no options or content.
const fn scope_spec(
    names: &'static [&'static str],
    doc: &'static str,
    arguments: Arguments,
) -> DirectiveSpec<'static> {
    DirectiveSpec {
        names,
        doc,
        arguments,
        takes_content: false,
        content_hint: None,
        options: OptionSet::NONE,
        accepts_further: false,
    }
}

/// `.. program::`'s entry among the scope directives.
const PROGRAM_SPEC: DirectiveSpec = scope_spec(
    &["program"],
    "Makes a program the one the options described below belong to.",
    Arguments::ONE_TEXT,
);

/// `.. py:currentmodule::`'s entry among the scope directives.
const CURRENT_MODULE_SPEC: DirectiveSpec = scope_spec(
    &["currentmodule"],
    "Makes a Python module the current one without describing it.",
    Arguments::ONE_WORD,
);

/// `.. c:namespace::`'s entry among the scope directives.
const NAMESPACE_SPEC: DirectiveSpec = scope_spec(
    &["namespace"],
    "Makes a C scope the current one, replacing every scope entered.",
    Arguments::ONE_TEXT,
);

/// `.. c:namespace-push::`'s entry among the scope directives.
const NAMESPACE_PUSH_SPEC: DirectiveSpec = scope_spec(
    &["namespace-push"],
    "Enters a nested C scope, which a `namespace-pop` leaves.",
    Arguments::ONE_TEXT,
);

/// `.. c:namespace-pop::`'s entry among the scope directives.
const NAMESPACE_POP_SPEC: DirectiveSpec = scope_spec(
    &["namespace-pop"],
    "Leaves the C scope the last `namespace-push` entered.",
    Arguments::NONE,
);

/// Every scope directive: the domain its name is qualified in — `None` for
/// `.. program::`, which is `std`-domain and bypasses `default_domain`
/// entirely, exactly like `.. option::`/`.. cmdoption::` (see
/// `resolve_domain_object_type`) — and what it moves, spelled by its spec.
const SCOPE_DIRECTIVES: &[(Option<Domain>, ScopeDirective)] = &[
    (None, ScopeDirective::Program),
    (Some(Domain::Py), ScopeDirective::CurrentModule),
    (Some(Domain::C), ScopeDirective::Namespace),
    (Some(Domain::C), ScopeDirective::NamespacePush),
    (Some(Domain::C), ScopeDirective::NamespacePop),
];

/// Which scope directive `name` spells under `default_domain`: an unqualified
/// row by its name alone, any other through [`split_domain_qualified_name`].
pub(super) fn resolve_scope_directive(
    name: &str,
    default_domain: Domain,
) -> Option<ScopeDirective> {
    let find = |domain: Option<Domain>, name: &str| {
        SCOPE_DIRECTIVES
            .iter()
            .find(|&&(row_domain, scope)| {
                row_domain == domain && scope.spec().names.contains(&name)
            })
            .map(|&(_, scope)| scope)
    };
    if let Some(scope) = find(None, name) {
        return Some(scope);
    }
    let (domain, bare_name) = split_domain_qualified_name(name, default_domain)?;
    find(Some(domain), bare_name)
}

/// Every scope directive: the domain its name is qualified in, and its spec.
pub(super) fn scope_directive_specs()
-> impl Iterator<Item = (Option<Domain>, &'static DirectiveSpec<'static>)> {
    SCOPE_DIRECTIVES
        .iter()
        .map(|&(domain, scope)| (domain, scope.spec()))
}

/// Every name a scope directive is written as, unqualified — what an entity
/// schema's section may not be called.
pub(super) fn scope_directive_names() -> impl Iterator<Item = &'static str> {
    scope_directive_specs().flat_map(|(_, spec)| spec.names.iter().copied())
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
/// [`resolve_domain_object_type`] and [`resolve_scope_directive`], so both
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
    use rinx_ast::Node;

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
        let doc = crate::parse_with_domain("test.rst", input, rinx_ast::Domain::C);

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
        let doc = crate::parse_with_domain("test.rst", input, rinx_ast::Domain::C);

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
    fn test_parse_c_namespace_push_without_argument_is_malformed() {
        // Given — a push with no scope is malformed; accepting it as a no-op
        // would leave a later `namespace-pop` unbalanced.
        let input = ".. c:namespace-push::";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Malformed { name, .. }) if name == "c:namespace-push"
        ));
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.code == rinx_ast::DiagnosticCode::DirectiveMissingArgument)
        );
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
        let doc = crate::parse_with_domain("test.rst", input, rinx_ast::Domain::C);

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

    #[test]
    fn test_resolve_scope_directive_reads_a_qualified_name_in_its_own_domain() {
        // Given / When / Then
        assert_eq!(
            resolve_scope_directive("c:namespace-push", Domain::Py),
            Some(ScopeDirective::NamespacePush)
        );
        assert_eq!(resolve_scope_directive("py:namespace", Domain::Py), None);
    }

    #[test]
    fn test_resolve_scope_directive_reads_program_under_any_default_domain() {
        // Given / When / Then
        assert_eq!(
            resolve_scope_directive("program", Domain::C),
            Some(ScopeDirective::Program)
        );
    }

    #[test]
    fn test_scope_directive_names_lists_every_scope_directive() {
        // Given / When
        let names: Vec<&str> = scope_directive_names().collect();

        // Then
        assert_eq!(
            names,
            [
                "program",
                "currentmodule",
                "namespace",
                "namespace-push",
                "namespace-pop"
            ]
        );
    }
}

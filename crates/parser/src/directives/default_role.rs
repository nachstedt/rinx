//! `.. default-role:: name` — choosing the role interpreted text written
//! without one is read as, for the rest of the document.
//!
//! Like `.. role::`, it contributes no node: the choice is recorded in the
//! document's [`crate::document_roles::DocumentRoles`] as it is parsed, so it
//! applies from where it is written onwards and the library's own default
//! until then. Without an argument it restores docutils' `title-reference`
//! rather than the library's default, as Sphinx's directive does — it
//! unregisters the document's default role, leaving docutils' own.

use crate::directives::invocation::{DirectiveInvocation, ParseState};
use crate::directives::spec::{Arguments, DirectiveSpec, OptionSet};
use rinx_ast::{Diagnostic, DiagnosticCode, Node, Span};

use crate::context::ParseCtx;
use crate::default_role::DefaultRole;
use crate::diagnostics::Diagnostics;

const DIRECTIVE: &str = "default-role";

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["default-role"],
    doc: "Sets the role interpreted text without one is read as.",
    arguments: Arguments::OPTIONAL_WORD,
    takes_content: false,
    content_hint: None,
    options: OptionSet::NONE,
    accepts_further: false,
};

/// Interprets `.. default-role::`, which records the default role and yields
/// no node.
pub(in crate::directives) fn interpret_default_role(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    parse_default_role(input, state.diagnostics)
}

/// Parses a `.. default-role::`, making its role the document's default when
/// it names one. Always answers with no nodes: a choice renders to nothing.
pub(in crate::directives) fn parse_default_role(
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Vec<Node> {
    let ctx = &invocation.ctx;
    if let Some(role) = read_default_role(invocation.argument(0), invocation.span, diagnostics, ctx)
        && let Some(roles) = ctx.document_roles()
    {
        roles.set_default(role);
    }
    Vec::new()
}

/// The role `argument` names, `title-reference` when it names none, or
/// `None` — reported — when it is not one role this build knows.
fn read_default_role(
    argument: &str,
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<DefaultRole> {
    let words: Vec<&str> = argument.split_whitespace().collect();
    let refuse = |code: DiagnosticCode, message: String, diagnostics: &mut Diagnostics| {
        diagnostics.push(Diagnostic::at(code, message, directive_span));
    };
    match words.as_slice() {
        [] => Some(DefaultRole::TITLE_REFERENCE),
        [name] => DefaultRole::parse(name, ctx)
            .map_err(|error| {
                refuse(
                    DiagnosticCode::DefaultRoleUnknownRole,
                    format!("{DIRECTIVE}: {error}"),
                    diagnostics,
                );
            })
            .ok(),
        _ => {
            refuse(
                DiagnosticCode::DefaultRoleInvalidArgument,
                format!("{DIRECTIVE}: '{argument}' is not one role name"),
                diagnostics,
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use rinx_ast::Domain;

    use super::*;
    use crate::document_roles::DocumentRoles;

    /// Parses a `.. default-role::` with `argument` and `body` after `roles`
    /// already chose a default, returning the default afterwards and the
    /// diagnostic codes.
    fn choose(
        argument: &str,
        body: &[&str],
        roles: &DocumentRoles,
    ) -> (Vec<Node>, Option<DefaultRole>, Vec<DiagnosticCode>) {
        let base = ParseCtx::with_domain(Domain::Py);
        let ctx = base.with_document_roles(roles);
        let mut diagnostics = Diagnostics::default();
        let invocation = crate::directives::reader::test_invocation(
            "default-role",
            &SPEC,
            argument,
            body,
            &mut diagnostics,
            &ctx,
        );
        let nodes = parse_default_role(&invocation, &mut diagnostics);
        let (entries, _, _) = diagnostics.into_parts();
        (
            nodes,
            roles.default_role(),
            entries.iter().map(|d| d.code).collect(),
        )
    }

    fn role(name: &str) -> DefaultRole {
        DefaultRole::parse(name, &ParseCtx::with_domain(Domain::Py)).unwrap()
    }

    #[test]
    fn test_parse_default_role_sets_the_named_role() {
        // Given / When
        let (nodes, default, codes) = choose("any", &[], &DocumentRoles::default());

        // Then
        assert!(nodes.is_empty());
        assert_eq!(default, Some(role("any")));
        assert!(codes.is_empty(), "{codes:?}");
    }

    #[test]
    fn test_parse_default_role_without_an_argument_restores_title_reference() {
        // Given a document that chose another default
        let roles = DocumentRoles::default();
        roles.set_default(role("any"));

        // When
        let (_, default, codes) = choose("", &[], &roles);

        // Then
        assert_eq!(default, Some(DefaultRole::TITLE_REFERENCE));
        assert!(codes.is_empty(), "{codes:?}");
    }

    #[test]
    fn test_parse_default_role_keeps_the_default_for_an_unknown_role() {
        // Given
        let roles = DocumentRoles::default();
        roles.set_default(role("any"));

        // When
        let (_, default, codes) = choose("nonsense", &[], &roles);

        // Then
        assert_eq!(default, Some(role("any")));
        assert_eq!(codes, vec![DiagnosticCode::DefaultRoleUnknownRole]);
    }

    #[test]
    fn test_parse_default_role_reports_surplus_words_and_reads_the_first() {
        // Given / When — docutils allows one argument, of one word
        let (_, default, codes) = choose("any ref", &[], &DocumentRoles::default());

        // Then
        assert_eq!(default, Some(role("any")));
        assert_eq!(codes, vec![DiagnosticCode::DirectiveTooManyArguments]);
    }

    #[test]
    fn test_parse_default_role_reads_an_option_line_as_surplus_words_and_still_applies() {
        // Given / When — the directive takes no options, so the line below
        // its marker continues the argument, as docutils reads it
        let (_, default, codes) = choose("any", &["   :class: x"], &DocumentRoles::default());

        // Then
        assert_eq!(default, Some(role("any")));
        assert_eq!(codes, vec![DiagnosticCode::DirectiveTooManyArguments]);
    }

    #[test]
    fn test_parse_default_role_reports_content_and_still_applies() {
        // Given / When — content below a blank line
        let (_, default, codes) = choose("any", &["", "   text"], &DocumentRoles::default());

        // Then
        assert_eq!(default, Some(role("any")));
        assert_eq!(codes, vec![DiagnosticCode::DirectiveContentNotPermitted]);
    }

    #[test]
    fn test_read_default_role_ignores_surrounding_whitespace() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let default = read_default_role(
            "  t  ",
            None,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(default, Some(DefaultRole::TITLE_REFERENCE));
    }
}

//! `.. default-role:: name` — choosing the role interpreted text written
//! without one is read as, for the rest of the document.
//!
//! Like `.. role::`, it contributes no node: the choice is recorded in the
//! document's [`crate::document_roles::DocumentRoles`] as it is parsed, so it
//! applies from where it is written onwards and the library's own default
//! until then. Without an argument it restores docutils' `title-reference`
//! rather than the library's default, as Sphinx's directive does — it
//! unregisters the document's default role, leaving docutils' own.

use rinx_ast::{Diagnostic, DiagnosticCode, Node, Span};

use crate::context::ParseCtx;
use crate::default_role::DefaultRole;
use crate::diagnostics::Diagnostics;

const DIRECTIVE: &str = "default-role";

/// Parses a `.. default-role::`, making its role the document's default when
/// it names one. Always answers with no nodes: a choice renders to nothing.
pub(in crate::directives) fn parse_default_role(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    if body_lines.iter().any(|line| !line.trim().is_empty()) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DefaultRoleUnexpectedContent,
            format!("{DIRECTIVE}: takes no options and no content"),
            directive_span,
        ));
    }
    if let Some(role) = read_default_role(argument, directive_span, diagnostics, ctx)
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
        let nodes = parse_default_role(argument, None, body, &mut diagnostics, &ctx);
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
    fn test_parse_default_role_refuses_several_words() {
        // Given / When
        let (_, default, codes) = choose("any ref", &[], &DocumentRoles::default());

        // Then
        assert_eq!(default, None);
        assert_eq!(codes, vec![DiagnosticCode::DefaultRoleInvalidArgument]);
    }

    #[test]
    fn test_parse_default_role_reports_content_and_still_applies() {
        // Given / When
        let (_, default, codes) = choose("any", &["   :class: x"], &DocumentRoles::default());

        // Then
        assert_eq!(default, Some(role("any")));
        assert_eq!(codes, vec![DiagnosticCode::DefaultRoleUnexpectedContent]);
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

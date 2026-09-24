//! `.. needservice::` — sphinx-needs' directive that creates needs from what
//! an external service (GitHub, Open-Needs, a `conf.py`-registered Python
//! class) answers *while the build runs*.
//!
//! This build refuses it by name rather than letting it fall through to
//! `directive.unknown`, whose wording would suggest a typo or a missing
//! feature. Neither half is something to implement: the query is a network
//! call a sandboxed Bazel action cannot make (the reason `needimport.remote-
//! source` exists), and turning its answer into entities is exactly what
//! `.. needimport::` already does over a `needs.json` snapshot. The message
//! therefore names that route. Accepting the directive against a checked-in
//! snapshot was rejected: its `:query:` would then describe data it no longer
//! selects. See `docs/decisions/016-needimport.md`.

use super::error_node::malformed_directive;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{DiagnosticCode, Directive, Span};

/// Why `.. needservice::` is refused, and what to write instead — stored on the
/// node as well as reported, so the page says the same as the build log.
const NEEDSERVICE_UNSUPPORTED_MESSAGE: &str = "needservice: querying an external service while \
     building is not supported, since a sandboxed build may only read files declared before it \
     runs; save the needs to a needs.json, declare it in the library's `parse_data` and read it \
     with `.. needimport::`";

/// Degrades a `.. needservice::` to a [`Directive::Malformed`] reporting
/// `needservice.unsupported`, keeping its source for the rendered error block.
///
/// The body is never parsed: it is the service's content template, not prose
/// of this document.
pub(super) fn parse_needservice(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
) -> Directive {
    malformed_directive(
        "needservice",
        argument,
        body_lines,
        DiagnosticCode::NeedServiceUnsupported,
        NEEDSERVICE_UNSUPPORTED_MESSAGE.to_string(),
        directive_span,
        diagnostics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_needservice_degrades_to_a_malformed_directive_quoting_its_source() {
        // Given
        let mut diagnostics = Diagnostics::default();
        let body = [
            "   :query: repo:useblocks/sphinx-needs",
            "   :max_amount: 5",
        ];

        // When
        let directive = parse_needservice("github-issues", None, &body, &mut diagnostics);

        // Then
        assert_eq!(
            directive,
            Directive::Malformed {
                name: "needservice".to_string(),
                argument: "github-issues".to_string(),
                body: ":query: repo:useblocks/sphinx-needs\n:max_amount: 5".to_string(),
                message: NEEDSERVICE_UNSUPPORTED_MESSAGE.to_string(),
            }
        );
    }

    #[test]
    fn test_parse_needservice_reports_needservice_unsupported_naming_the_migration_route() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        parse_needservice("github-issues", None, &[], &mut diagnostics);

        // Then
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, DiagnosticCode::NeedServiceUnsupported);
        assert!(found[0].message.contains("needs.json"));
        assert!(found[0].message.contains("parse_data"));
        assert!(found[0].message.contains(".. needimport::"));
    }
}

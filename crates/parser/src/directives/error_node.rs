//! The two nodes a directive degrades to when it cannot become itself, and the
//! diagnostic each one reports.
//!
//! Both exist so that a construct this build cannot turn into content is still
//! *visible*: the renderer draws an error block quoting the source, rather than
//! dropping the directive and everything written inside it. The body is kept
//! with its relative indentation intact (via
//! [`strip_common_indent`](crate::indent::strip_common_indent)) precisely
//! because it is about to be shown as written — flattening every line's own
//! leading whitespace, which `body::join_body_lines` does for the callers that
//! want data rather than source, would destroy nested content's shape.
//!
//! Each builder pushes the diagnostic *and* returns the node, from the same
//! message, so the build log and the page cannot end up saying different
//! things about one directive.

use crate::diagnostics::Diagnostics;
use crate::indent::strip_common_indent;
use rinx_ast::{Diagnostic, DiagnosticCode, Directive, Span};

/// Degrades a directive whose name this build does not recognize.
///
/// The wording follows docutils' own (`unknown directive type "foo"`), since a
/// reader who knows Sphinx will recognize it.
pub(in crate::directives) fn unknown_directive(
    name: String,
    argument: String,
    body_lines: &[&str],
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveUnknown,
        format!("unknown directive type '{name}'"),
        span,
    ));
    Directive::Unknown {
        name,
        argument,
        body: strip_common_indent(body_lines),
    }
}

/// Degrades a directive whose name this build recognizes but whose content it
/// cannot accept, reporting `code`/`message` as the reason.
///
/// `message` is stored on the node as well as reported, because it is what the
/// rendered block prints: a page that said only "malformed directive" would
/// send its reader to the build log for the half that matters.
pub(in crate::directives) fn malformed_directive(
    name: &str,
    argument: &str,
    body_lines: &[&str],
    code: DiagnosticCode,
    message: String,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    diagnostics.push(Diagnostic::at(code, message.clone(), span));
    malformed_node(name, argument, body_lines, message)
}

/// Builds the same degraded node *without* reporting, for a directive whose
/// reason was already reported by a shared checker that returns only
/// success/failure (`check_encoding`, `select`) and so cannot hand its wording
/// back.
///
/// Prefer [`malformed_directive`] wherever the reason is in hand: it makes the
/// node's message and the diagnostic's the same string by construction. Where
/// this is used instead, `message` should say what the *outcome* was — the
/// reported diagnostic says why — rather than restating the checker's wording,
/// which would be a second copy free to drift.
pub(in crate::directives) fn malformed_node(
    name: &str,
    argument: &str,
    body_lines: &[&str],
    message: String,
) -> Directive {
    Directive::Malformed {
        name: name.to_string(),
        argument: argument.to_string(),
        body: strip_common_indent(body_lines),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_directive_reports_the_name_and_keeps_the_body() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = unknown_directive(
            "mermaid".to_string(),
            "flow".to_string(),
            &["   graph TD;", "     A --> B;"],
            None,
            &mut diagnostics,
        );

        // Then
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, DiagnosticCode::DirectiveUnknown);
        assert_eq!(found[0].message, "unknown directive type 'mermaid'");
        assert_eq!(
            directive,
            Directive::Unknown {
                name: "mermaid".to_string(),
                argument: "flow".to_string(),
                body: "graph TD;\n  A --> B;".to_string(),
            }
        );
    }

    #[test]
    fn test_malformed_directive_stores_the_message_it_reports() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = malformed_directive(
            "figure",
            "",
            &["   A caption."],
            DiagnosticCode::ImageMissingUri,
            "figure: the directive needs an image path".to_string(),
            None,
            &mut diagnostics,
        );

        // Then
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, DiagnosticCode::ImageMissingUri);
        assert_eq!(
            directive,
            Directive::Malformed {
                name: "figure".to_string(),
                argument: String::new(),
                body: "A caption.".to_string(),
                message: "figure: the directive needs an image path".to_string(),
            }
        );
    }

    #[test]
    fn test_malformed_directive_keeps_relative_indentation_of_its_body() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = malformed_directive(
            "csv-table",
            "Fruits",
            &["   :file: missing.csv", "", "      indented"],
            DiagnosticCode::DirectiveUnknownOption,
            "csv-table: cannot read the data".to_string(),
            None,
            &mut diagnostics,
        );

        // Then
        let Directive::Malformed { body, .. } = directive else {
            panic!("expected a malformed directive");
        };
        assert_eq!(body, ":file: missing.csv\n\n   indented");
    }
}

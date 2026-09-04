//! Warning formatting and the `--strict-links` enforcement, shared by the
//! `parse`, `render` and `preview` subcommands.
//!
//! Every warning this crate prints goes through one of the formatters here,
//! so they all read the same way:
//!
//! ```text
//! warning: guide/intro.rst:42:18: link.broken-ref: broken ref 'missing'
//! ```
//!
//! The position is the *start* of the diagnostic's span — a range reads as
//! noise on a terminal, and the end is there for the language server, not for
//! this output. A diagnostic with no span (see
//! [`rusty_sphinx_ast::Diagnostic::span`]) simply omits that part rather than
//! pointing at a line it cannot vouch for.

use anyhow::{Result, anyhow};
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Span};
use rusty_sphinx_renderer::{self as renderer};

/// Renders `path` plus a span's start as the `file:line:column:` prefix every
/// warning opens with, or just `path:` when there is no span.
fn location(doc_path: &str, span: Option<Span>) -> String {
    span.map_or_else(
        || format!("{doc_path}:"),
        |span| format!("{doc_path}:{}:{}:", span.start.line, span.start.column),
    )
}

/// Formats a parse-time diagnostic as a human-readable warning line.
pub(super) fn format_diagnostic(doc_path: &str, diagnostic: &Diagnostic) -> String {
    format!(
        "warning: {} {}: {}",
        location(doc_path, diagnostic.span),
        diagnostic.code,
        diagnostic.message
    )
}

/// Prints one parse diagnostic to stderr.
///
/// Printing lives here rather than in `rusty_sphinx_parser` — where it used to
/// happen, inside `parse_with_ctx` — because the parser must only *record*
/// what went wrong. Deciding whether a diagnostic is shown at all is the
/// build step's business, and a parser that printed as it went could not
/// honour a `.. noqa:` comment appearing anywhere in the document. Which
/// diagnostics reach here is [`super::suppression`]'s decision.
pub(super) fn report_diagnostic(doc_path: &str, diagnostic: &Diagnostic) {
    eprintln!("{}", format_diagnostic(doc_path, diagnostic));
}

/// Formats a single broken-link diagnostic as a human-readable warning line.
///
/// For a broken domain-object reference the role's requested object type (the
/// "missed type", e.g. `py:function`) is included — it's known at the point
/// resolution failed and pinpoints what kind of object couldn't be found. An
/// ambiguous reference additionally lists the qualified names it matched:
/// unlike a plain miss, the fix is to pick one of them, so they are the
/// actionable part of the message.
pub(super) fn format_broken_link_warning(doc_path: &str, link: &renderer::BrokenLink) -> String {
    let requested = match &link.kind {
        renderer::BrokenLinkKind::DomainObjectReference(object_type) => {
            format!(" (referenced as {})", object_type.domain_qualified_str())
        }
        renderer::BrokenLinkKind::AmbiguousDomainObjectReference {
            object_type,
            candidates,
        } => format!(
            " (referenced as {}, matches {})",
            object_type.domain_qualified_str(),
            candidates.join(", ")
        ),
        _ => String::new(),
    };
    format!(
        "warning: {} {}: broken {} '{}'{requested}",
        location(doc_path, link.span),
        link.code(),
        link.kind.as_str(),
        link.target
    )
}

/// Formats a single object-type-mismatch diagnostic as a human-readable
/// warning line. Both the requested and resolved object types are shown
/// domain-qualified (e.g. `"py:class"`, not just `"class"`) via
/// [`rusty_sphinx_ast::ObjectType::domain_qualified_str`] — the alias
/// fallback is domain-scoped today (`py`'s `class`/`exception`, and `c`'s
/// `macro`/`member` and `function`/`macro`), so the two domains always match
/// in practice, but spelling both out avoids the reader having to assume
/// that rather than see it.
/// Unlike [`format_broken_link_warning`], this never feeds into
/// [`check_broken_links_strict`] — the reference did resolve, so `--strict-links`
/// never fails the build for it; the warning only flags that the reference's
/// role (e.g. `:exc:`) and the definition's actual object type (e.g. `class`)
/// are inconsistent.
pub(super) fn format_object_type_mismatch_warning(
    doc_path: &str,
    mismatch: &renderer::ObjectTypeMismatch,
) -> String {
    format!(
        "warning: {} {}: domain object '{}' referenced as '{}' but defined as '{}'",
        location(doc_path, mismatch.span),
        DiagnosticCode::LinkTypeMismatch,
        mismatch.name,
        mismatch.requested_type.domain_qualified_str(),
        mismatch.resolved_type.domain_qualified_str(),
    )
}

/// Formats a single invalid-LaTeX diagnostic as a human-readable warning line.
///
/// Like [`format_object_type_mismatch_warning`], this never feeds into
/// [`check_broken_links_strict`]: `--strict-links` is about references that
/// don't resolve, and an equation that fails to convert is a different
/// complaint. The page still renders, showing the LaTeX the author wrote.
pub(super) fn format_math_error_warning(doc_path: &str, error: &renderer::MathError) -> String {
    format!(
        "warning: {} {}: invalid math: {}",
        location(doc_path, error.span),
        error.code(),
        error.message
    )
}

/// Formats a single highlighting-failure diagnostic as a warning line.
///
/// Like [`format_math_error_warning`], this never feeds into
/// [`check_broken_links_strict`]: a block that could not be highlighted is not
/// a reference that failed to resolve. The page still renders, showing the
/// author's code as plain text.
pub(super) fn format_highlight_error_warning(
    doc_path: &str,
    error: &renderer::HighlightError,
) -> String {
    format!(
        "warning: {} {}: {}",
        location(doc_path, error.span),
        error.code(),
        error.message
    )
}

/// Returns an error listing every broken link when `strict` is true and
/// `broken_links` is non-empty. Diagnostics are always reported to stderr by
/// the caller regardless of `strict` — this only controls whether they also
/// fail the render.
pub(super) fn check_broken_links_strict(
    strict: bool,
    doc_path: &str,
    broken_links: &[renderer::BrokenLink],
) -> Result<()> {
    if !strict || broken_links.is_empty() {
        return Ok(());
    }
    let messages: Vec<String> = broken_links
        .iter()
        .map(|link| format_broken_link_warning(doc_path, link))
        .collect();
    Err(anyhow!(
        "Broken link validation failed:\n{}",
        messages.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusty_sphinx_ast::Position;

    fn a_span() -> Span {
        Span::new(Position::new(42, 18), Position::new(42, 35))
    }

    #[test]
    fn test_format_broken_link_warning_names_the_position_code_kind_and_target() {
        // Given a broken reference the parser could place
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing-section".to_string(),
            span: Some(a_span()),
        };

        // When
        let message = format_broken_link_warning("guide/intro.rst", &link);

        // Then — the span's *start* is shown, not the range
        assert_eq!(
            message,
            "warning: guide/intro.rst:42:18: link.broken-ref: broken ref 'missing-section'"
        );
    }

    #[test]
    fn test_format_broken_link_warning_omits_the_position_when_there_is_none() {
        // Given a reference from generated content, which has no source line
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing-section".to_string(),
            span: None,
        };

        // When
        let message = format_broken_link_warning("guide/intro.rst", &link);

        // Then — the document is still named; no line is invented for it
        assert_eq!(
            message,
            "warning: guide/intro.rst: link.broken-ref: broken ref 'missing-section'"
        );
    }

    #[test]
    fn test_format_diagnostic_names_the_position_code_and_message() {
        // Given a parse-time diagnostic
        let diagnostic = Diagnostic::new(
            DiagnosticCode::TableGridNoColumns,
            "grid table: top border defines no columns",
            a_span(),
        );

        // When
        let message = format_diagnostic("guide/tables.rst", &diagnostic);

        // Then
        assert_eq!(
            message,
            "warning: guide/tables.rst:42:18: table.grid.no-columns: grid table: top border defines no columns"
        );
    }

    #[test]
    fn test_format_diagnostic_omits_the_position_when_there_is_none() {
        // Given a diagnostic about content with no source position
        let diagnostic =
            Diagnostic::without_span(DiagnosticCode::CsvMalformedData, "csv-table: malformed row");

        // When
        let message = format_diagnostic("guide/tables.rst", &diagnostic);

        // Then
        assert_eq!(
            message,
            "warning: guide/tables.rst: csv.malformed-data: csv-table: malformed row"
        );
    }

    #[test]
    fn test_every_broken_link_kind_maps_to_a_distinct_code() {
        // Given every kind a broken link can have
        let kinds = [
            renderer::BrokenLinkKind::Reference,
            renderer::BrokenLinkKind::Hyperlink,
            renderer::BrokenLinkKind::AnonymousReference,
            renderer::BrokenLinkKind::TermReference,
            renderer::BrokenLinkKind::OptionReference,
            renderer::BrokenLinkKind::DomainObjectReference(rusty_sphinx_ast::ObjectType::Py(
                rusty_sphinx_ast::PyObjectType::Function,
            )),
            renderer::BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type: rusty_sphinx_ast::ObjectType::Py(
                    rusty_sphinx_ast::PyObjectType::Function,
                ),
                candidates: Vec::new(),
            },
        ];

        // When
        let codes: std::collections::HashSet<_> =
            kinds.iter().map(renderer::BrokenLinkKind::code).collect();

        // Then — a shared code would make one kind unsuppressible on its own
        assert_eq!(codes.len(), kinds.len());
    }

    #[test]
    fn test_check_broken_links_strict_passes_when_not_strict() {
        // Given
        let broken_links = vec![renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing".to_string(),
            span: None,
        }];

        // When
        let result = check_broken_links_strict(false, "doc.rst", &broken_links);

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_broken_links_strict_passes_when_no_broken_links() {
        // Given
        let broken_links: Vec<renderer::BrokenLink> = vec![];

        // When
        let result = check_broken_links_strict(true, "doc.rst", &broken_links);

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_broken_links_strict_fails_when_strict_and_broken_links_present() {
        // Given
        let broken_links = vec![renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing".to_string(),
            span: None,
        }];

        // When
        let result = check_broken_links_strict(true, "doc.rst", &broken_links);

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Broken link validation failed"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn test_format_object_type_mismatch_warning_includes_name_and_types() {
        // Given
        let mismatch = renderer::ObjectTypeMismatch {
            name: "fault".to_string(),
            requested_type: rusty_sphinx_ast::ObjectType::Py(
                rusty_sphinx_ast::PyObjectType::Exception,
            ),
            resolved_type: rusty_sphinx_ast::ObjectType::Py(rusty_sphinx_ast::PyObjectType::Class),
            span: Some(a_span()),
        };

        // When
        let message = format_object_type_mismatch_warning("xmlrpc.client.rst", &mismatch);

        // Then
        assert_eq!(
            message,
            "warning: xmlrpc.client.rst:42:18: link.type-mismatch: domain object 'fault' referenced as 'py:exception' but defined as 'py:class'"
        );
    }
}

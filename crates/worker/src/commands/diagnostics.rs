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
//! [`rinx_ast::Diagnostic::span`]) simply omits that part rather than
//! pointing at a line it cannot vouch for.
//!
//! A span from a file spliced in by `.. include::` names *that* file, with the
//! document it was included from in parentheses:
//!
//! ```text
//! warning: shared/params.rst:7:3 (included from guide/api.rst): table.grid.no-columns: ...
//! ```
//!
//! One line either way, because these are read with `grep` as often as with
//! eyes. Naming the fragment first is the important half: it is the file the
//! author has to open and the line they have to edit, and printing the
//! document's path against the fragment's line number — which is what happened
//! before spans carried a file — points confidently at the wrong place.

use anyhow::{Result, anyhow};
use rinx_ast::{Diagnostic, Reported, Span};
use rinx_renderer::{self as renderer};

/// The document a batch of warnings is about, plus the files it included —
/// between them, everything needed to turn a [`Span`] into a path a reader can
/// open.
///
/// Bundled into one type rather than passed as two arguments because every
/// formatter in this module needs both, and because the pair has an invariant
/// worth naming: `source_files` must be the table the spans were interned
/// against, which in practice means it and `doc_path` come from the same
/// [`Document`](rinx_ast::Document).
pub(super) struct WarningOrigin<'a> {
    doc_path: &'a str,
    source_files: &'a [String],
}

impl<'a> WarningOrigin<'a> {
    /// The origin for a document that included `source_files`.
    pub(super) const fn new(doc_path: &'a str, source_files: &'a [String]) -> Self {
        Self {
            doc_path,
            source_files,
        }
    }

    /// The origin for a document known to include nothing.
    ///
    /// Kept distinct from [`Self::new`] with an empty slice so a caller says
    /// which it means: an empty table is also what a *mangled* `.ast` has, and
    /// the two should not be spelled the same way at a call site. Only tests
    /// need it: every production caller has the document's include table.
    #[cfg(test)]
    pub(super) const fn document_only(doc_path: &'a str) -> Self {
        Self::new(doc_path, &[])
    }

    /// The `file:line:column:` prefix every warning opens with — or just
    /// `path:` when there is no span, and with `(included from …)` when the
    /// span belongs to a file this document included.
    fn location(&self, span: Option<Span>) -> String {
        let Some(span) = span else {
            return format!("{}:", self.doc_path);
        };
        let position = format!("{}:{}", span.start.line, span.start.column);
        match span
            .file
            .and_then(|file| self.source_files.get(file.index()))
        {
            // A span whose file id has no entry falls back to naming the
            // document with no position: an `.ast` written by another version
            // of the parser should cost a warning its precision, never make it
            // lie about a line.
            None if span.file.is_some() => format!("{}:", self.doc_path),
            None => format!("{}:{position}:", self.doc_path),
            Some(included) => format!("{included}:{position} (included from {}):", self.doc_path),
        }
    }
}

/// Formats anything a phase reported — a parse diagnostic or one of the
/// renderer's findings — as a human-readable warning line.
///
/// The message is the finding's own [`Reported::message`], the wording the
/// language server shows too, so the editor and the build cannot drift apart.
pub(super) fn format_diagnostic(origin: &WarningOrigin<'_>, finding: &impl Reported) -> String {
    format!(
        "warning: {} {}: {}",
        origin.location(finding.span()),
        finding.code(),
        finding.message()
    )
}

/// Prints one parse diagnostic to stderr.
///
/// Printing lives here rather than in `rinx_parser` — where it used to
/// happen, inside `parse_with_ctx` — because the parser must only *record*
/// what went wrong. Deciding whether a diagnostic is shown at all is the
/// build step's business, and a parser that printed as it went could not
/// honour a `.. noqa:` comment appearing anywhere in the document. Which
/// diagnostics reach here is [`super::suppression`]'s decision.
/// Formats a diagnostic that fails the build, in the same shape as a warning.
///
/// One shape for both, so an editor or CI log parser that understands one line
/// understands the other; only the leading word differs.
pub(super) fn format_error_diagnostic(
    origin: &WarningOrigin<'_>,
    diagnostic: &Diagnostic,
) -> String {
    format!(
        "error: {} {}: {}",
        origin.location(diagnostic.span),
        diagnostic.code,
        diagnostic.message
    )
}

pub(super) fn report_diagnostic(origin: &WarningOrigin<'_>, diagnostic: &Diagnostic) {
    eprintln!("{}", format_diagnostic(origin, diagnostic));
}

/// Returns an error listing every broken link when `strict` is true and
/// `broken_links` is non-empty. Diagnostics are always reported to stderr by
/// the caller regardless of `strict` — this only controls whether they also
/// fail the render.
pub(super) fn check_broken_links_strict(
    strict: bool,
    origin: &WarningOrigin<'_>,
    broken_links: &[renderer::BrokenLink],
) -> Result<()> {
    if !strict || broken_links.is_empty() {
        return Ok(());
    }
    let messages: Vec<String> = broken_links
        .iter()
        .map(|link| format_diagnostic(origin, link))
        .collect();
    Err(anyhow!(
        "Broken link validation failed:\n{}",
        messages.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::DiagnosticCode;

    use rinx_ast::Position;

    fn a_span() -> Span {
        Span::new(Position::new(42, 18), Position::new(42, 35))
    }

    #[test]
    fn test_format_diagnostic_of_a_broken_link_names_the_position_code_kind_and_target() {
        // Given a broken reference the parser could place
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing-section".to_string(),
            span: Some(a_span()),
        };

        // When
        let message = format_diagnostic(&WarningOrigin::document_only("guide/intro.rst"), &link);

        // Then — the span's *start* is shown, not the range
        assert_eq!(
            message,
            "warning: guide/intro.rst:42:18: link.broken-ref: broken ref 'missing-section'"
        );
    }

    #[test]
    fn test_format_diagnostic_of_a_broken_link_names_a_broken_doc_reference() {
        // Given
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::DocReference,
            target: "../missing".to_string(),
            span: Some(a_span()),
        };

        // When
        let message = format_diagnostic(&WarningOrigin::document_only("guide/intro.rst"), &link);

        // Then
        assert_eq!(
            message,
            "warning: guide/intro.rst:42:18: link.broken-doc: broken doc reference '../missing'"
        );
    }

    #[test]
    fn test_format_diagnostic_of_a_broken_link_explains_each_numref_problem() {
        // Given one link per `:numref:` problem
        let cases = [
            (
                renderer::BrokenLinkKind::NumberReference,
                "link.broken-numref: broken numref 'fig' (no captioned figure",
            ),
            (
                renderer::BrokenLinkKind::NumberingDisabled,
                "numref.disabled: broken numref 'fig' (numfig is off in rinx.toml",
            ),
            (
                renderer::BrokenLinkKind::UnnumberedReference,
                "numref.unnumbered: broken numref 'fig' (it has no number",
            ),
            (
                renderer::BrokenLinkKind::UncaptionedReference,
                "numref.no-caption: broken numref 'fig' (its format shows {name}",
            ),
        ];

        for (kind, expected) in cases {
            let link = renderer::BrokenLink {
                kind,
                target: "fig".to_string(),
                span: Some(a_span()),
            };

            // When
            let message = format_diagnostic(&WarningOrigin::document_only("guide.rst"), &link);

            // Then
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn test_format_diagnostic_of_a_broken_link_lists_the_roles_an_ambiguous_any_could_be() {
        // Given
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::AmbiguousAnyReference {
                candidates: vec![
                    ":std:ref:`shared`".to_string(),
                    ":py:func:`shared`".to_string(),
                ],
            },
            target: "shared".to_string(),
            span: Some(a_span()),
        };

        // When
        let message = format_diagnostic(&WarningOrigin::document_only("guide/intro.rst"), &link);

        // Then
        assert_eq!(
            message,
            "warning: guide/intro.rst:42:18: link.ambiguous-any: broken ambiguous any reference \
             'shared' (could be :std:ref:`shared` or :py:func:`shared`)"
        );
    }

    #[test]
    fn test_format_diagnostic_of_a_broken_link_names_an_undeclared_inventory() {
        // Given an `:external+numpy:` role in a site that declared no `numpy`
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::UnknownInventory(
                rinx_ast::InventoryName::new("numpy").unwrap(),
            ),
            target: "ndarray".to_string(),
            span: Some(a_span()),
        };

        // When
        let message = format_diagnostic(&WarningOrigin::document_only("guide/intro.rst"), &link);

        // Then
        assert_eq!(
            message,
            "warning: guide/intro.rst:42:18: link.unknown-inventory: broken reference into an \
             undeclared inventory 'ndarray' (no inventory is declared as 'numpy')"
        );
    }

    #[test]
    fn test_format_diagnostic_of_a_broken_link_omits_the_position_when_there_is_none() {
        // Given a reference from generated content, which has no source line
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing-section".to_string(),
            span: None,
        };

        // When
        let message = format_diagnostic(&WarningOrigin::document_only("guide/intro.rst"), &link);

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
        let message = format_diagnostic(
            &WarningOrigin::document_only("guide/tables.rst"),
            &diagnostic,
        );

        // Then
        assert_eq!(
            message,
            "warning: guide/tables.rst:42:18: table.grid.no-columns: grid table: top border defines no columns"
        );
    }

    #[test]
    fn test_format_error_diagnostic_has_the_warning_shape_with_an_error_prefix() {
        // Given
        let diagnostic = Diagnostic::new(
            DiagnosticCode::UmlDiagramsDisabled,
            "set diagrams = True",
            a_span(),
        );

        // When
        let message = format_error_diagnostic(
            &WarningOrigin::document_only("guide/tables.rst"),
            &diagnostic,
        );

        // Then
        assert_eq!(
            message,
            "error: guide/tables.rst:42:18: uml.diagrams-disabled: set diagrams = True"
        );
    }

    #[test]
    fn test_format_diagnostic_omits_the_position_when_there_is_none() {
        // Given a diagnostic about content with no source position
        let diagnostic =
            Diagnostic::without_span(DiagnosticCode::CsvMalformedData, "csv-table: malformed row");

        // When
        let message = format_diagnostic(
            &WarningOrigin::document_only("guide/tables.rst"),
            &diagnostic,
        );

        // Then
        assert_eq!(
            message,
            "warning: guide/tables.rst: csv.malformed-data: csv-table: malformed row"
        );
    }

    #[test]
    fn test_format_diagnostic_names_the_included_file_and_its_includer() {
        // Given a problem found inside a fragment the document included
        let files = ["shared/params.rst".to_string()];
        let origin = WarningOrigin::new("guide/api.rst", &files);
        let diagnostic = Diagnostic::new(
            DiagnosticCode::TableGridNoColumns,
            "grid table: top border defines no columns",
            Span::new(Position::new(7, 3), Position::new(7, 20))
                .with_file(Some(rinx_ast::FileId::new(0))),
        );

        // When
        let message = format_diagnostic(&origin, &diagnostic);

        // Then — the fragment is named first: it is the file to open, and its
        // line 7 is the line to edit.
        assert_eq!(
            message,
            "warning: shared/params.rst:7:3 (included from guide/api.rst): \
             table.grid.no-columns: grid table: top border defines no columns"
        );
    }

    #[test]
    fn test_format_diagnostic_of_a_broken_link_names_the_included_file() {
        // Given a `:ref:` inside an included fragment that resolves nowhere —
        // found at *render* time, long after the fragment was spliced in
        let files = ["shared/params.rst".to_string()];
        let origin = WarningOrigin::new("guide/api.rst", &files);
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing-section".to_string(),
            span: Some(a_span().with_file(Some(rinx_ast::FileId::new(0)))),
        };

        // When
        let message = format_diagnostic(&origin, &link);

        // Then
        assert_eq!(
            message,
            "warning: shared/params.rst:42:18 (included from guide/api.rst): \
             link.broken-ref: broken ref 'missing-section'"
        );
    }

    #[test]
    fn test_a_span_in_the_document_is_unaffected_by_a_source_file_table() {
        // Given a document that includes a fragment
        let files = ["shared/params.rst".to_string()];
        let origin = WarningOrigin::new("guide/api.rst", &files);
        let diagnostic = Diagnostic::new(DiagnosticCode::CsvNoData, "no data", a_span());

        // When the problem is in the document's own text
        let message = format_diagnostic(&origin, &diagnostic);

        // Then — no parenthetical, exactly as before includes existed
        assert_eq!(
            message,
            "warning: guide/api.rst:42:18: csv.no-data: no data"
        );
    }

    #[test]
    fn test_an_unresolvable_file_id_drops_the_position_rather_than_lying() {
        // Given an `.ast` whose span names a file its table does not have
        let origin = WarningOrigin::document_only("guide/api.rst");
        let diagnostic = Diagnostic::new(
            DiagnosticCode::CsvNoData,
            "no data",
            a_span().with_file(Some(rinx_ast::FileId::new(4))),
        );

        // When
        let message = format_diagnostic(&origin, &diagnostic);

        // Then — line 42 belongs to a file we cannot name, so naming the
        // document at line 42 would point confidently at the wrong text
        assert_eq!(message, "warning: guide/api.rst: csv.no-data: no data");
    }

    #[test]
    fn test_the_second_included_file_resolves_by_its_own_id() {
        // Given a document including two fragments
        let files = [
            "shared/params.rst".to_string(),
            "shared/returns.rst".to_string(),
        ];
        let origin = WarningOrigin::new("guide/api.rst", &files);
        let diagnostic = Diagnostic::new(
            DiagnosticCode::CsvNoData,
            "no data",
            a_span().with_file(Some(rinx_ast::FileId::new(1))),
        );

        // When
        let message = format_diagnostic(&origin, &diagnostic);

        // Then — ids index the table, so the second entry is the second file
        assert!(
            message.starts_with("warning: shared/returns.rst:42:18"),
            "{message}"
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
            renderer::BrokenLinkKind::DomainObjectReference(rinx_ast::ObjectType::Py(
                rinx_ast::PyObjectType::Function,
            )),
            renderer::BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Function),
                candidates: Vec::new(),
            },
            renderer::BrokenLinkKind::DocReference,
            renderer::BrokenLinkKind::AnyReference,
            renderer::BrokenLinkKind::AmbiguousAnyReference {
                candidates: Vec::new(),
            },
            renderer::BrokenLinkKind::NumberReference,
            renderer::BrokenLinkKind::NumberingDisabled,
            renderer::BrokenLinkKind::UnnumberedReference,
            renderer::BrokenLinkKind::UncaptionedReference,
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
        let result = check_broken_links_strict(
            false,
            &WarningOrigin::document_only("doc.rst"),
            &broken_links,
        );

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_broken_links_strict_passes_when_no_broken_links() {
        // Given
        let broken_links: Vec<renderer::BrokenLink> = vec![];

        // When
        let result = check_broken_links_strict(
            true,
            &WarningOrigin::document_only("doc.rst"),
            &broken_links,
        );

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
        let result = check_broken_links_strict(
            true,
            &WarningOrigin::document_only("doc.rst"),
            &broken_links,
        );

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Broken link validation failed"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn test_format_diagnostic_of_a_type_mismatch_includes_name_and_types() {
        // Given
        let mismatch = renderer::ObjectTypeMismatch {
            name: "fault".to_string(),
            requested_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Exception),
            resolved_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Class),
            span: Some(a_span()),
        };

        // When
        let message = format_diagnostic(
            &WarningOrigin::document_only("xmlrpc.client.rst"),
            &mismatch,
        );

        // Then
        assert_eq!(
            message,
            "warning: xmlrpc.client.rst:42:18: link.type-mismatch: domain object 'fault' referenced as 'py:exception' but defined as 'py:class'"
        );
    }
}

//! Parsing an open document and reporting what the parser found.
//!
//! The diagnostics are the build's own: the same parser, the same codes, the
//! same messages. Only their shape changes here — a span becomes a protocol
//! range, and the code travels as the dotted id a `.. noqa:` names.

use lsp_types::{DiagnosticSeverity, NumberOrString, Uri};
use rinx_ast::{Diagnostic, Domain};
use rinx_parser::{ParseCtx, RejectParseFiles};

use crate::files::DiskFiles;
use crate::position::{PositionEncoding, to_lsp_range};

/// What every published diagnostic names as its producer.
const SOURCE: &str = "rinx";

/// Parses `text`, the content of the document at `uri`, and converts what the
/// parser reported.
///
/// The parse uses the build's defaults — the `py` domain, `title-reference`
/// as the default role, no entity schema, no Jinja — since nothing tells the
/// server yet how the document's library is configured.
#[must_use]
pub fn document_diagnostics(
    uri: &Uri,
    text: &str,
    encoding: PositionEncoding,
) -> Vec<lsp_types::Diagnostic> {
    let file_path = file_path(uri);
    let document = match &file_path {
        Some(path) => {
            let files = DiskFiles::for_document(path);
            rinx_parser::parse_with_ctx(
                &path.to_string_lossy(),
                text,
                &ParseCtx::new(Domain::Py, &files),
            )
        }
        // An unsaved buffer has no directory to resolve a file against.
        None => rinx_parser::parse_with_ctx(
            uri.as_str(),
            text,
            &ParseCtx::new(Domain::Py, &RejectParseFiles),
        ),
    };
    to_lsp_diagnostics(&document.diagnostics, text, encoding)
}

/// The filesystem path behind `uri`, when it names a file on this machine.
///
/// Read off the URI `lsp-types` already parsed, rather than through the `url`
/// crate, whose international-domain support brings in a score of crates to
/// answer a question about `file:` URIs that never have a domain.
fn file_path(uri: &Uri) -> Option<std::path::PathBuf> {
    let is_file = uri
        .scheme()
        .is_some_and(|scheme| scheme.as_str().eq_ignore_ascii_case("file"));
    let is_local = uri
        .authority()
        .is_none_or(|authority| matches!(authority.as_str(), "" | "localhost"));
    if !is_file || !is_local {
        return None;
    }
    let path = uri.path().as_estr().decode().into_string().ok()?;
    Some(std::path::PathBuf::from(path.as_ref()))
}

/// Converts the diagnostics found in a document whose text is `text`.
///
/// One found inside an `.. include::`d fragment is left out: its span counts
/// lines in the fragment, so placing it in this document would underline an
/// unrelated line.
#[must_use]
pub fn to_lsp_diagnostics(
    diagnostics: &[Diagnostic],
    text: &str,
    encoding: PositionEncoding,
) -> Vec<lsp_types::Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.span.is_none_or(|span| span.file.is_none()))
        .map(|diagnostic| to_lsp_diagnostic(diagnostic, text, encoding))
        .collect()
}

/// Converts one diagnostic. One without a span — generated content has no
/// source line — is placed at the very start of the document.
fn to_lsp_diagnostic(
    diagnostic: &Diagnostic,
    text: &str,
    encoding: PositionEncoding,
) -> lsp_types::Diagnostic {
    let range = diagnostic
        .span
        .map(|span| to_lsp_range(span, text, encoding))
        .unwrap_or_default();
    lsp_types::Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String(diagnostic.code.as_str().to_string())),
        source: Some(SOURCE.to_string()),
        message: diagnostic.message.clone(),
        ..lsp_types::Diagnostic::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{DiagnosticCode, FileId, Position, Span};

    fn uri(text: &str) -> Uri {
        text.parse().expect("valid uri")
    }

    #[test]
    fn test_document_diagnostics_reports_an_unknown_directive() {
        // Given
        let text = "Title\n=====\n\n.. foo::\n";

        // When
        let diagnostics = document_diagnostics(
            &uri("file:///docs/index.rst"),
            text,
            PositionEncoding::Utf16,
        );

        // Then
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let diagnostic = &diagnostics[0];
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("directive.unknown".to_string()))
        );
        assert_eq!(diagnostic.range.start.line, 3);
        assert_eq!(diagnostic.source.as_deref(), Some("rinx"));
    }

    #[test]
    fn test_document_diagnostics_reports_nothing_for_clean_text() {
        // Given
        let text = "Title\n=====\n\nSome *prose*.\n";

        // When
        let diagnostics = document_diagnostics(
            &uri("file:///docs/index.rst"),
            text,
            PositionEncoding::Utf16,
        );

        // Then
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn test_document_diagnostics_parses_an_unsaved_buffer() {
        // Given
        let text = ".. foo::\n";

        // When
        let diagnostics =
            document_diagnostics(&uri("untitled:Untitled-1"), text, PositionEncoding::Utf16);

        // Then
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn test_file_path_reads_a_file_uri() {
        // Given / When
        let path = file_path(&uri("file:///docs/a%20b.rst"));

        // Then
        assert_eq!(path, Some(std::path::PathBuf::from("/docs/a b.rst")));
    }

    #[test]
    fn test_file_path_accepts_localhost() {
        // Given / When
        let path = file_path(&uri("file://localhost/docs/index.rst"));

        // Then
        assert_eq!(path, Some(std::path::PathBuf::from("/docs/index.rst")));
    }

    #[test]
    fn test_file_path_is_none_for_a_remote_host() {
        // Given / When / Then
        assert_eq!(file_path(&uri("file://server/docs/index.rst")), None);
    }

    #[test]
    fn test_file_path_is_none_for_another_scheme() {
        // Given / When / Then
        assert_eq!(file_path(&uri("untitled:Untitled-1")), None);
    }

    #[test]
    fn test_to_lsp_diagnostics_places_a_spanless_diagnostic_at_the_start() {
        // Given
        let diagnostics = [Diagnostic::without_span(
            DiagnosticCode::CsvMalformedData,
            "bad row",
        )];

        // When
        let converted = to_lsp_diagnostics(&diagnostics, "text\n", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].range, lsp_types::Range::default());
        assert_eq!(converted[0].message, "bad row");
    }

    #[test]
    fn test_to_lsp_diagnostics_leaves_out_a_fragment_diagnostic() {
        // Given
        let span =
            Span::new(Position::new(1, 1), Position::new(1, 5)).with_file(Some(FileId::new(0)));
        let diagnostics = [Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span)];

        // When
        let converted = to_lsp_diagnostics(&diagnostics, "text\n", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted, Vec::new());
    }

    #[test]
    fn test_to_lsp_diagnostic_converts_code_message_and_range() {
        // Given
        let span = Span::new(Position::new(1, 2), Position::new(1, 4));
        let diagnostic = Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span);

        // When
        let converted = to_lsp_diagnostic(&diagnostic, "π abc\n", PositionEncoding::Utf16);

        // Then
        assert_eq!(
            converted.range,
            lsp_types::Range::new(
                lsp_types::Position::new(0, 1),
                lsp_types::Position::new(0, 3)
            )
        );
        assert_eq!(
            converted.code,
            Some(NumberOrString::String(
                DiagnosticCode::CsvNoData.as_str().to_string()
            ))
        );
        assert_eq!(converted.severity, Some(DiagnosticSeverity::WARNING));
    }
}

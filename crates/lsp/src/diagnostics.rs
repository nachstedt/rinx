//! Parsing an open document and reporting what the parser found.
//!
//! The diagnostics are the build's own: the same parser, the same codes, the
//! same messages. Only their shape changes here — a span becomes a protocol
//! range, and the code travels as the dotted id a `.. noqa:` names.

use std::borrow::Cow;

use lsp_types::{CodeDescription, DiagnosticSeverity, NumberOrString, Uri};
use rinx_ast::{Diagnostic, DiagnosticCode, Domain, Suppression, retain_reportable, section_slug};
use rinx_parser::{ParseCtx, RejectParseFiles};

use crate::files::DiskFiles;
use crate::position::{PositionEncoding, to_lsp_range};

/// What every published diagnostic names as its producer.
const SOURCE: &str = "rinx";

/// The documentation page listing every diagnostic code, generated from
/// `DiagnosticCode::ALL` by `rinx diagnostic_codes_rst`. The `latest/` build,
/// since the server cannot know which published version it matches.
const CODE_DOCS: &str = "https://nachstedt.github.io/rinx/latest/diagnostics.html";

/// Parses `text`, the content of the document at `uri`, and converts what the
/// parser reported.
///
/// The text is parsed with its line endings as the protocol counts them (see
/// [`protocol_line_endings`]), so every reported line is one the editor shows.
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
    let text = protocol_line_endings(text);
    let text = text.as_ref();
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
    to_lsp_diagnostics(
        &document.diagnostics,
        &document.suppressions,
        text,
        encoding,
    )
}

/// `text` with every line ending written as `\n`.
///
/// The protocol ends a line at `\r\n`, `\n` or a lone `\r`, and so do VS Code
/// and docutils, but the parser splits at `\n` alone: a lone `\r` left in
/// place would shift every diagnostic below it onto the wrong line. Each
/// ending stays one character wide or is dropped with its `\r\n` partner, so
/// columns are unchanged.
fn protocol_line_endings(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
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

/// Converts the diagnostics found in a document whose text is `text`, leaving
/// out what its `.. noqa:` comments (`suppressions`) silence.
///
/// The filter is the build's own [`rinx_ast::retain_reportable`], so the
/// editor and CI cannot disagree about what a comment silences.
///
/// One found inside an `.. include::`d fragment is left out: its span counts
/// lines in the fragment, so placing it in this document would underline an
/// unrelated line.
#[must_use]
pub fn to_lsp_diagnostics(
    diagnostics: &[Diagnostic],
    suppressions: &[Suppression],
    text: &str,
    encoding: PositionEncoding,
) -> Vec<lsp_types::Diagnostic> {
    retain_reportable(diagnostics, suppressions)
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
        code_description: code_description(diagnostic.code),
        source: Some(SOURCE.to_string()),
        message: diagnostic.message.clone(),
        ..lsp_types::Diagnostic::default()
    }
}

/// Where `code` is documented: its section of [`CODE_DOCS`], whose id is the
/// one the page's heading — the bare code — gets from [`section_slug`].
///
/// `None` only if that address were not a URI, which a test rules out for
/// every code.
fn code_description(code: DiagnosticCode) -> Option<CodeDescription> {
    let href = format!("{CODE_DOCS}#{}", section_slug(code.as_str()));
    href.parse().ok().map(|href| CodeDescription { href })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{FileId, Position, Span, SuppressionCodes};

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
    fn test_document_diagnostics_honours_a_noqa_naming_the_code() {
        // Given
        let text = ".. noqa: directive.unknown\n\n.. foo::\n";

        // When
        let diagnostics =
            document_diagnostics(&uri("untitled:Untitled-1"), text, PositionEncoding::Utf16);

        // Then
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn test_document_diagnostics_keeps_what_a_noqa_does_not_name() {
        // Given
        let text = ".. noqa: link.broken-ref\n\n.. foo::\n";

        // When
        let diagnostics =
            document_diagnostics(&uri("untitled:Untitled-1"), text, PositionEncoding::Utf16);

        // Then
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
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
    fn test_document_diagnostics_counts_a_lone_carriage_return_as_a_line_end() {
        // Given — old Mac line endings, which an editor shows as two lines
        let text = "Prose.\r\r.. foo::\r";

        // When
        let diagnostics =
            document_diagnostics(&uri("untitled:Untitled-1"), text, PositionEncoding::Utf16);

        // Then
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].range.start, lsp_types::Position::new(2, 0));
    }

    #[test]
    fn test_protocol_line_endings_leaves_newline_text_borrowed() {
        // Given / When
        let normalized = protocol_line_endings("one\ntwo\n");

        // Then
        assert!(matches!(normalized, Cow::Borrowed("one\ntwo\n")));
    }

    #[test]
    fn test_protocol_line_endings_rewrites_crlf_and_lone_cr() {
        // Given / When
        let normalized = protocol_line_endings("a\r\nb\rc\n");

        // Then
        assert_eq!(normalized, "a\nb\nc\n");
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
        let converted = to_lsp_diagnostics(&diagnostics, &[], "text\n", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].range, lsp_types::Range::default());
        assert_eq!(converted[0].message, "bad row");
    }

    #[test]
    fn test_to_lsp_diagnostics_leaves_out_a_suppressed_diagnostic() {
        // Given
        let span = Span::new(Position::new(3, 1), Position::new(3, 5));
        let diagnostics = [Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span)];
        let suppressions = [Suppression {
            start_line: 3,
            end_line: 3,
            codes: SuppressionCodes::Only(vec![DiagnosticCode::CsvNoData]),
            file: None,
        }];

        // When
        let converted = to_lsp_diagnostics(
            &diagnostics,
            &suppressions,
            "a\nb\nc\n",
            PositionEncoding::Utf16,
        );

        // Then
        assert_eq!(converted, Vec::new());
    }

    #[test]
    fn test_to_lsp_diagnostics_leaves_out_a_fragment_diagnostic() {
        // Given
        let span =
            Span::new(Position::new(1, 1), Position::new(1, 5)).with_file(Some(FileId::new(0)));
        let diagnostics = [Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span)];

        // When
        let converted = to_lsp_diagnostics(&diagnostics, &[], "text\n", PositionEncoding::Utf16);

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
        assert_eq!(
            converted.code_description,
            code_description(DiagnosticCode::CsvNoData)
        );
    }

    #[test]
    fn test_code_description_links_the_codes_section() {
        // Given / When
        let description = code_description(DiagnosticCode::DirectiveUnknown);

        // Then
        assert_eq!(
            description.map(|description| description.href.as_str().to_string()),
            Some(
                "https://nachstedt.github.io/rinx/latest/diagnostics.html#directive-unknown"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_every_code_has_a_code_description() {
        // Given / When
        let undescribed: Vec<&str> = DiagnosticCode::ALL
            .iter()
            .filter(|code| code_description(**code).is_none())
            .map(|code| code.as_str())
            .collect();

        // Then
        assert_eq!(undescribed, Vec::<&str>::new());
    }

    /// Properties over generated documents: whatever the text, the parse must
    /// not panic and every range must point into the text the editor shows.
    mod properties {
        use super::*;
        use proptest::prelude::*;

        /// Text built from reStructuredText's building blocks, so the
        /// constructs that report diagnostics are actually reached — plus
        /// arbitrary characters, astral ones and every line terminator.
        fn rst_like_text() -> impl Strategy<Value = String> {
            let piece = prop_oneof![
                Just(".. ".to_string()),
                Just("::".to_string()),
                Just(":ref:`".to_string()),
                Just("`".to_string()),
                Just("|".to_string()),
                Just("*".to_string()),
                Just("_".to_string()),
                Just("=====".to_string()),
                Just("- ".to_string()),
                Just("#. ".to_string()),
                Just("+---+".to_string()),
                Just(":option: ".to_string()),
                Just("   ".to_string()),
                Just("\t".to_string()),
                Just("\n".to_string()),
                Just("\n\n".to_string()),
                Just("\r\n".to_string()),
                Just("\r".to_string()),
                Just("🦀".to_string()),
                Just("foo".to_string()),
                Just("include".to_string()),
                Just("toctree".to_string()),
                "\\PC{0,8}",
            ];
            prop::collection::vec(piece, 0..40).prop_map(|pieces| pieces.concat())
        }

        /// The document's lines as the protocol counts them: `\r\n`, `\n`
        /// and a lone `\r` each end one.
        fn protocol_lines(text: &str) -> Vec<String> {
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            normalized.split('\n').map(str::to_string).collect()
        }

        proptest! {
            #[test]
            fn test_document_diagnostics_never_panics_and_stays_in_the_text(
                text in rst_like_text(),
            ) {
                // Given
                let lines = protocol_lines(&text);

                // When
                let diagnostics = document_diagnostics(
                    &uri("untitled:Untitled-1"),
                    &text,
                    PositionEncoding::Utf16,
                );

                // Then
                for diagnostic in &diagnostics {
                    for point in [diagnostic.range.start, diagnostic.range.end] {
                        let line = lines.get(point.line as usize);
                        prop_assert!(line.is_some(), "{diagnostic:?} past the end of {text:?}");
                        let width = line.map_or(0, |line| line.encode_utf16().count());
                        prop_assert!(
                            point.character as usize <= width,
                            "{diagnostic:?} past the end of its line in {text:?}"
                        );
                    }
                }
            }
        }
    }
}

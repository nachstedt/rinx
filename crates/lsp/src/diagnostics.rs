//! Parsing an open document and reporting what the parser found.
//!
//! The diagnostics are the build's own: the same parser, the same codes, the
//! same messages. Only their shape changes here — a span becomes a protocol
//! range, and the code travels as the dotted id a `.. noqa:` names.
//!
//! One parse can report on several files: a diagnostic found inside an
//! `.. include::`d fragment carries that fragment's [`FileId`](rinx_ast::FileId)
//! and counts lines in it, so it is published under the fragment's URI rather
//! than underlining an unrelated line of the document.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lsp_types::{CodeDescription, DiagnosticSeverity, NumberOrString, Range, Uri};
use rinx_ast::{
    Diagnostic, DiagnosticCode, Document, Domain, Span, retain_reportable, section_slug,
};
use rinx_parser::{ParseCtx, RejectParseFiles};

use crate::documents::DocumentStore;
use crate::files::{FileReads, WorkspaceFiles};
use crate::include_summary::summarize_includes;
use crate::position::{PositionEncoding, to_lsp_range};
use crate::uri::{file_path, file_uri};

/// What every published diagnostic names as its producer.
pub(crate) const SOURCE: &str = "rinx";

/// The documentation page listing every diagnostic code, generated from
/// `DiagnosticCode::ALL` by `rinx diagnostic_codes_rst`. The `latest/` build,
/// since the server cannot know which published version it matches.
const CODE_DOCS: &str = "https://nachstedt.github.io/rinx/latest/diagnostics.html";

/// What parsing one open document found, file by file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentDiagnosis {
    /// The diagnostics to publish, by the URI of the file they are in. The
    /// document's own URI is always present, even with nothing to report, so
    /// a fixed mistake is cleared.
    pub by_uri: BTreeMap<Uri, Vec<lsp_types::Diagnostic>>,
    /// Every file the parse asked to read, so a change to one of them can
    /// re-diagnose this document.
    pub reads: BTreeSet<PathBuf>,
}

/// Parses `text`, the content of the document at `uri`, and converts what the
/// parser reported. A file a directive reads is taken from `open` when the
/// editor holds it, and from the disk otherwise.
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
    open: &DocumentStore,
    encoding: PositionEncoding,
) -> DocumentDiagnosis {
    let text = protocol_line_endings(text);
    let text = text.as_ref();
    let (document, reads) = match file_path(uri) {
        Some(path) => {
            let files = WorkspaceFiles::for_document(&path, open);
            let document = rinx_parser::parse_with_ctx(
                &path.to_string_lossy(),
                text,
                &ParseCtx::new(Domain::Py, &files),
            );
            (document, files.into_reads())
        }
        // An unsaved buffer has no directory to resolve a file against.
        None => (
            rinx_parser::parse_with_ctx(
                uri.as_str(),
                text,
                &ParseCtx::new(Domain::Py, &RejectParseFiles),
            ),
            FileReads::default(),
        ),
    };
    let by_uri = to_lsp_diagnostics(
        &document,
        &Placement {
            uri,
            text,
            reads: &reads,
            open,
        },
        encoding,
    );
    DocumentDiagnosis {
        by_uri,
        reads: reads.paths,
    }
}

/// `text` with every line ending written as `\n`.
///
/// The protocol ends a line at `\r\n`, `\n` or a lone `\r`, and so do VS Code
/// and docutils, but the parser splits at `\n` alone: a lone `\r` left in
/// place would shift every diagnostic below it onto the wrong line. Each
/// ending stays one character wide or is dropped with its `\r\n` partner, so
/// columns are unchanged.
pub(crate) fn protocol_line_endings(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// Where the files of one parse are, for placing what it found.
pub struct Placement<'a> {
    /// The document's own URI.
    pub uri: &'a Uri,
    /// The document's text, as parsed.
    pub text: &'a str,
    /// The files the parse read, whose texts place a fragment's diagnostics.
    pub reads: &'a FileReads,
    /// The open documents, whose URIs name an open fragment as the client does.
    pub open: &'a DocumentStore,
}

impl Placement<'_> {
    /// The URI and text of the file a span with `file` points into.
    ///
    /// `None` when the file cannot be named — an id with no entry, or a path
    /// that is no URI — in which case the diagnostic cannot be placed on a line
    /// at all.
    fn file<'b>(&'b self, document: &Document, file: rinx_ast::FileId) -> Option<(Uri, &'b str)> {
        let path = Path::new(document.source_files.get(file.index())?);
        // An open fragment is published under the URI the client knows it by,
        // which may encode its path differently than `file_uri` does.
        let uri = match self.open.uri_of(path) {
            Some(uri) => uri.clone(),
            None => file_uri(path)?,
        };
        let text = self.reads.texts.get(path).map_or("", String::as_str);
        Some((uri, text))
    }

    /// The URI of the file `span` is in, and its range there.
    ///
    /// A span in a fragment that cannot be named is placed at the start of the
    /// document, as a missing span is: its line counts in another file, so any
    /// line of this one would be the wrong one.
    pub(crate) fn place(
        &self,
        document: &Document,
        span: Option<Span>,
        encoding: PositionEncoding,
    ) -> (Uri, Range) {
        let at_start = || (self.uri.clone(), Range::default());
        let Some(span) = span else {
            return at_start();
        };
        match span.file {
            None => (self.uri.clone(), to_lsp_range(span, self.text, encoding)),
            Some(file) => self
                .file(document, file)
                .map_or_else(at_start, |(uri, text)| {
                    (uri, to_lsp_range(span, text, encoding))
                }),
        }
    }
}

/// One diagnostic the editor shows, and where: the file's URI and the range
/// in it.
pub(crate) struct Placed<'a> {
    pub diagnostic: &'a Diagnostic,
    pub uri: Uri,
    pub range: Range,
}

/// Converts the diagnostics found in `document`, by the URI of the file each
/// one is in, leaving out what its `.. noqa:` comments silence — and adds, on
/// each `.. include::` that brought problems in, a summary of them (see
/// [`crate::include_summary`]).
///
/// The filter is the build's own [`rinx_ast::retain_reportable`], so the
/// editor and CI cannot disagree about what a comment silences — and since a
/// suppression records its file, a comment only silences its own file.
#[must_use]
pub fn to_lsp_diagnostics(
    document: &Document,
    placement: &Placement<'_>,
    encoding: PositionEncoding,
) -> BTreeMap<Uri, Vec<lsp_types::Diagnostic>> {
    let placed: Vec<Placed<'_>> = retain_reportable(&document.diagnostics, &document.suppressions)
        .map(|diagnostic| {
            let (uri, range) = placement.place(document, diagnostic.span, encoding);
            Placed {
                diagnostic,
                uri,
                range,
            }
        })
        .collect();
    let mut by_uri: BTreeMap<Uri, Vec<lsp_types::Diagnostic>> = BTreeMap::new();
    by_uri.insert(placement.uri.clone(), Vec::new());
    for found in &placed {
        by_uri
            .entry(found.uri.clone())
            .or_default()
            .push(to_lsp_diagnostic(found.diagnostic, found.range));
    }
    for (uri, summary) in summarize_includes(document, &placed, placement, encoding) {
        by_uri.entry(uri).or_default().push(summary);
    }
    by_uri
}

/// Converts one diagnostic, placed at `range`.
fn to_lsp_diagnostic(diagnostic: &Diagnostic, range: Range) -> lsp_types::Diagnostic {
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
    use rinx_ast::{FileId, Position, Span, Suppression, SuppressionCodes};

    fn uri(text: &str) -> Uri {
        text.parse().expect("valid uri")
    }

    /// What parsing `text` at `uri`, with nothing else open, reports on the
    /// document itself.
    fn own_diagnostics(uri: &Uri, text: &str) -> Vec<lsp_types::Diagnostic> {
        let mut diagnosis = document_diagnostics(
            uri,
            text,
            &DocumentStore::default(),
            PositionEncoding::Utf16,
        );
        diagnosis
            .by_uri
            .remove(uri)
            .expect("the document's own entry")
    }

    /// A scratch directory holding `files`.
    fn temp_directory(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rinx_lsp_diagnostics_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        for (relative, contents) in files {
            let target = dir.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("create temp subdir");
            }
            std::fs::write(target, contents).expect("write temp file");
        }
        dir
    }

    fn file(path: &Path) -> Uri {
        file_uri(path).expect("an absolute path")
    }

    /// The `(line, code)` of every diagnostic in `diagnostics`.
    fn lines_and_codes(diagnostics: &[lsp_types::Diagnostic]) -> Vec<(u32, String)> {
        diagnostics
            .iter()
            .map(|diagnostic| {
                let code = match &diagnostic.code {
                    Some(NumberOrString::String(code)) => code.clone(),
                    other => format!("{other:?}"),
                };
                (diagnostic.range.start.line, code)
            })
            .collect()
    }

    #[test]
    fn test_document_diagnostics_reports_an_unknown_directive() {
        // Given
        let text = "Title\n=====\n\n.. foo::\n";

        // When
        let diagnostics = own_diagnostics(&uri("file:///docs/index.rst"), text);

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
        let diagnostics = own_diagnostics(&uri("file:///docs/index.rst"), text);

        // Then
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn test_document_diagnostics_honours_a_noqa_naming_the_code() {
        // Given
        let text = ".. noqa: directive.unknown\n\n.. foo::\n";

        // When
        let diagnostics = own_diagnostics(&uri("untitled:Untitled-1"), text);

        // Then
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn test_document_diagnostics_keeps_what_a_noqa_does_not_name() {
        // Given
        let text = ".. noqa: link.broken-ref\n\n.. foo::\n";

        // When
        let diagnostics = own_diagnostics(&uri("untitled:Untitled-1"), text);

        // Then
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn test_document_diagnostics_parses_an_unsaved_buffer() {
        // Given
        let text = ".. foo::\n";

        // When
        let diagnostics = own_diagnostics(&uri("untitled:Untitled-1"), text);

        // Then
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn test_document_diagnostics_counts_a_lone_carriage_return_as_a_line_end() {
        // Given — old Mac line endings, which an editor shows as two lines
        let text = "Prose.\r\r.. foo::\r";

        // When
        let diagnostics = own_diagnostics(&uri("untitled:Untitled-1"), text);

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

    /// `diagnostics` and `suppressions` on a document at `/docs/index.rst`
    /// whose text is `text`, which included the files `included` (path, text).
    fn convert(
        diagnostics: Vec<Diagnostic>,
        suppressions: Vec<Suppression>,
        text: &str,
        included: &[(&str, &str)],
        open: &DocumentStore,
    ) -> BTreeMap<Uri, Vec<lsp_types::Diagnostic>> {
        let mut document = Document::new("/docs/index.rst".to_string(), Vec::new());
        document.diagnostics = diagnostics;
        document.suppressions = suppressions;
        let mut reads = FileReads::default();
        for (path, contents) in included {
            document.intern_source_file(*path);
            reads.paths.insert(PathBuf::from(path));
            reads
                .texts
                .insert(PathBuf::from(path), (*contents).to_string());
        }
        to_lsp_diagnostics(
            &document,
            &Placement {
                uri: &uri("file:///docs/index.rst"),
                text,
                reads: &reads,
                open,
            },
            PositionEncoding::Utf16,
        )
    }

    fn in_fragment(line: u32, column: u32) -> Span {
        Span::new(Position::new(line, column), Position::new(line, column + 1))
            .with_file(Some(FileId::new(0)))
    }

    #[test]
    fn test_to_lsp_diagnostics_places_a_spanless_diagnostic_at_the_start() {
        // Given
        let diagnostics = vec![Diagnostic::without_span(
            DiagnosticCode::CsvMalformedData,
            "bad row",
        )];

        // When
        let converted = convert(
            diagnostics,
            Vec::new(),
            "text\n",
            &[],
            &DocumentStore::default(),
        );

        // Then
        let own = &converted[&uri("file:///docs/index.rst")];
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].range, Range::default());
        assert_eq!(own[0].message, "bad row");
    }

    #[test]
    fn test_to_lsp_diagnostics_leaves_out_a_suppressed_diagnostic() {
        // Given
        let span = Span::new(Position::new(3, 1), Position::new(3, 5));
        let diagnostics = vec![Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span)];
        let suppressions = vec![Suppression {
            start_line: 3,
            end_line: 3,
            codes: SuppressionCodes::Only(vec![DiagnosticCode::CsvNoData]),
            file: None,
        }];

        // When
        let converted = convert(
            diagnostics,
            suppressions,
            "a\nb\nc\n",
            &[],
            &DocumentStore::default(),
        );

        // Then — the document is still listed, with nothing to show
        assert_eq!(
            converted,
            BTreeMap::from([(uri("file:///docs/index.rst"), Vec::new())])
        );
    }

    #[test]
    fn test_to_lsp_diagnostics_places_a_fragment_diagnostic_in_the_fragment() {
        // Given a diagnostic on the fragment's second line, after an astral
        // character the range must count in the fragment's own text
        let diagnostics = vec![Diagnostic::new(
            DiagnosticCode::DirectiveUnknown,
            "unknown",
            in_fragment(2, 2),
        )];

        // When
        let converted = convert(
            diagnostics,
            Vec::new(),
            "a\n",
            &[("/docs/part.rst", "x\n🦀.. foo::\n")],
            &DocumentStore::default(),
        );

        // Then
        assert_eq!(converted[&uri("file:///docs/index.rst")], Vec::new());
        let fragment = &converted[&uri("file:///docs/part.rst")];
        assert_eq!(fragment[0].range.start, lsp_types::Position::new(1, 2));
    }

    #[test]
    fn test_to_lsp_diagnostics_names_an_open_fragment_by_the_clients_uri() {
        // Given a fragment the client opened under a differently encoded URI
        let client_uri = uri("file:///docs/p%61rt.rst");
        let mut open = DocumentStore::default();
        open.open(client_uri.clone(), 1, "x\n".to_string());
        let diagnostics = vec![Diagnostic::new(
            DiagnosticCode::DirectiveUnknown,
            "unknown",
            in_fragment(1, 1),
        )];

        // When
        let converted = convert(
            diagnostics,
            Vec::new(),
            "a\n",
            &[("/docs/part.rst", "x\n")],
            &open,
        );

        // Then
        assert!(converted.contains_key(&client_uri), "{converted:?}");
    }

    #[test]
    fn test_to_lsp_diagnostics_places_an_unnamed_fragment_at_the_documents_start() {
        // Given a span whose file id the document never recorded
        let diagnostics = vec![Diagnostic::new(
            DiagnosticCode::DirectiveUnknown,
            "unknown",
            in_fragment(5, 1),
        )];

        // When
        let converted = convert(
            diagnostics,
            Vec::new(),
            "a\n",
            &[],
            &DocumentStore::default(),
        );

        // Then — never on line 5 of the document, which is another file's line
        let own = &converted[&uri("file:///docs/index.rst")];
        assert_eq!(own[0].range, Range::default());
    }

    #[test]
    fn test_to_lsp_diagnostic_converts_code_message_and_range() {
        // Given
        let span = Span::new(Position::new(1, 2), Position::new(1, 4));
        let diagnostic = Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span);
        let range = to_lsp_range(span, "π abc\n", PositionEncoding::Utf16);

        // When
        let converted = to_lsp_diagnostic(&diagnostic, range);

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
    fn test_document_diagnostics_underlines_a_mistake_in_an_included_file() {
        // Given
        let dir = temp_directory("included", &[("part.rst", "Fine.\n\n.. foo::\n")]);
        let index = dir.join("index.rst");

        // When
        let diagnosis = document_diagnostics(
            &file(&index),
            "Title\n=====\n\n.. include:: part.rst\n",
            &DocumentStore::default(),
            PositionEncoding::Utf16,
        );

        // Then — on the fragment, and summarized on the include's line
        let summary = &diagnosis.by_uri[&file(&index)];
        assert_eq!(summary.len(), 1, "{summary:?}");
        assert_eq!(summary[0].range.start.line, 3);
        assert_eq!(summary[0].severity, Some(DiagnosticSeverity::INFORMATION));
        assert_eq!(
            lines_and_codes(&diagnosis.by_uri[&file(&dir.join("part.rst"))]),
            vec![(2, "directive.unknown".to_string())]
        );
        assert_eq!(diagnosis.reads, BTreeSet::from([dir.join("part.rst")]));
    }

    #[test]
    fn test_document_diagnostics_reads_an_open_fragment_from_its_buffer() {
        // Given a fragment saved clean but broken in the editor
        let dir = temp_directory("buffered", &[("part.rst", "Fine.\n")]);
        let part = file(&dir.join("part.rst"));
        let mut open = DocumentStore::default();
        open.open(part.clone(), 3, "Fine.\n\n.. foo::\n".to_string());

        // When
        let diagnosis = document_diagnostics(
            &file(&dir.join("index.rst")),
            ".. include:: part.rst\n",
            &open,
            PositionEncoding::Utf16,
        );

        // Then
        assert_eq!(
            lines_and_codes(&diagnosis.by_uri[&part]),
            vec![(2, "directive.unknown".to_string())]
        );
    }

    #[test]
    fn test_document_diagnostics_honours_a_noqa_in_the_fragment_only_there() {
        // Given one fragment silencing its own mistake, and an includer whose
        // noqa names the same code for its own following block
        let dir = temp_directory(
            "noqa",
            &[
                ("quiet.rst", ".. noqa: directive.unknown\n\n.. foo::\n"),
                ("loud.rst", ".. foo::\n"),
            ],
        );

        // When
        let diagnosis = document_diagnostics(
            &file(&dir.join("index.rst")),
            ".. include:: quiet.rst\n\n.. noqa: directive.unknown\n\n.. include:: loud.rst\n",
            &DocumentStore::default(),
            PositionEncoding::Utf16,
        );

        // Then — the includer's comment does not reach into the fragment
        assert_eq!(diagnosis.by_uri.get(&file(&dir.join("quiet.rst"))), None);
        assert_eq!(
            lines_and_codes(&diagnosis.by_uri[&file(&dir.join("loud.rst"))]),
            vec![(0, "directive.unknown".to_string())]
        );
    }

    #[test]
    fn test_document_diagnostics_attributes_a_nested_include_to_the_inner_file() {
        // Given
        let dir = temp_directory(
            "nested",
            &[
                ("outer.rst", ".. include:: inner/inner.rst\n"),
                ("inner/inner.rst", "\n.. foo::\n"),
            ],
        );

        // When
        let diagnosis = document_diagnostics(
            &file(&dir.join("index.rst")),
            ".. include:: outer.rst\n",
            &DocumentStore::default(),
            PositionEncoding::Utf16,
        );

        // Then
        assert_eq!(
            lines_and_codes(&diagnosis.by_uri[&file(&dir.join("inner/inner.rst"))]),
            vec![(1, "directive.unknown".to_string())]
        );
        assert_eq!(
            diagnosis.reads,
            BTreeSet::from([dir.join("inner/inner.rst"), dir.join("outer.rst")])
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
                let diagnostics = own_diagnostics(&uri("untitled:Untitled-1"), &text);

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

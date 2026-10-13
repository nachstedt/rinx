//! A summary, on an `.. include::`, of the problems the file it brings in has.
//!
//! A problem inside an included file is published on that file, which the
//! author of the including document may never open. So each `.. include::`
//! whose file — or a file included from it in turn — holds a problem also
//! carries one diagnostic on its own line, saying how many there are and
//! linking to each through `relatedInformation`.
//!
//! The summary is the editor's alone: the build already names both places in
//! one warning (`part.rst:3:1 (included from index.rst)`), so it has no code
//! and no `.. noqa:` of its own — silencing the problems in the fragment is
//! what removes it. It is *Information* rather than a warning, so a problem is
//! not counted twice in the editor's totals. Only warnings are summarized:
//! a problem the project's strictness lowered — a directive of an extension
//! rinx does not analyse — is no reason to flag the include.
//!
//! A fragment included twice shows its problems on both `.. include::` lines,
//! since a diagnostic records the file it is in rather than the inclusion that
//! found it.

use std::path::Path;

use lsp_types::{DiagnosticRelatedInformation, DiagnosticSeverity, Location, Uri};
use rinx_ast::{Document, IncludeSite};

use crate::diagnostics::{Placed, Placement, SOURCE};
use crate::position::PositionEncoding;
use crate::uri::file_path;

/// A summary for every include site in `document` that brought in one of the
/// diagnostics in `placed`, each with the URI of the file the `.. include::`
/// is written in.
pub(crate) fn summarize_includes(
    document: &Document,
    placed: &[Placed<'_>],
    placement: &Placement<'_>,
    encoding: PositionEncoding,
) -> Vec<(Uri, lsp_types::Diagnostic)> {
    document
        .include_sites
        .iter()
        .filter_map(|site| {
            let (uri, range) = placement.place(document, site.directive, encoding);
            // A problem reported on the `.. include::` line itself — a file
            // including itself is reported there — is already in plain view.
            let related: Vec<DiagnosticRelatedInformation> =
                related_problems(site, document, placed)
                    .into_iter()
                    .filter(|related| {
                        related.location.uri != uri
                            || related.location.range.start.line != range.start.line
                    })
                    .collect();
            let first = related.first()?;
            let name = included_name(site, document, file_path(placement.uri).as_deref());
            let message = match related.len() {
                1 => format!("problem in included file '{name}': {}", first.message),
                count => format!(
                    "{count} problems in included file '{name}', the first: {}",
                    first.message
                ),
            };
            let summary = lsp_types::Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::INFORMATION),
                source: Some(SOURCE.to_string()),
                message,
                related_information: Some(related),
                ..lsp_types::Diagnostic::default()
            };
            Some((uri, summary))
        })
        .collect()
}

/// A link to each distinct problem `site` brought in, in the order found.
///
/// Distinct because a fragment included twice is parsed twice and reports the
/// same mistake twice, at the same place.
fn related_problems(
    site: &IncludeSite,
    document: &Document,
    placed: &[Placed<'_>],
) -> Vec<DiagnosticRelatedInformation> {
    let reached = site.reached_files(&document.include_sites);
    let mut related: Vec<DiagnosticRelatedInformation> = Vec::new();
    for found in placed {
        if found.judgement.severity != DiagnosticSeverity::WARNING {
            continue;
        }
        let file = found.diagnostic.span.and_then(|span| span.file);
        if !file.is_some_and(|file| reached.contains(&file)) {
            continue;
        }
        let information = DiagnosticRelatedInformation {
            location: Location::new(found.uri.clone(), found.range),
            message: format!("{}: {}", found.diagnostic.code, found.diagnostic.message),
        };
        if !related.contains(&information) {
            related.push(information);
        }
    }
    related
}

/// The included file's path as its includer would write it: relative to the
/// directory of the file the `.. include::` is in, or in full when it is not
/// below it.
///
/// An include written in the document itself is placed by `location`, the
/// document's file: the document's own path is its name within its source
/// root, while an included file is named by where it is.
fn included_name(site: &IncludeSite, document: &Document, location: Option<&Path>) -> String {
    let included = document
        .source_files
        .get(site.file.index())
        .map_or("", String::as_str);
    let written_in = match site.directive.and_then(|span| span.file) {
        // In an included fragment, named by where it is.
        Some(file) => document
            .source_files
            .get(file.index())
            .map_or_else(|| Path::new(""), Path::new),
        // In the document itself.
        None => location.unwrap_or_else(|| Path::new(&document.path)),
    };
    written_in
        .parent()
        .and_then(|directory| Path::new(included).strip_prefix(directory).ok())
        .map_or_else(
            || included.to_string(),
            |relative| relative.to_string_lossy().into_owned(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::DocumentStore;
    use crate::files::FileReads;
    use crate::project::Strictness;
    use rinx_ast::{Diagnostic, DiagnosticCode, DiagnosticSubject, FileId, Position, Span};

    fn uri(text: &str) -> Uri {
        text.parse().expect("valid uri")
    }

    fn span(line: u32, file: Option<u32>) -> Span {
        Span::new(Position::new(line, 1), Position::new(line, 2)).with_file(file.map(FileId::new))
    }

    /// A document at `/docs/index.rst` that included `files`, through `sites`
    /// (directive line, file written in, file brought in), finding `found`.
    fn document(
        files: &[&str],
        sites: &[(u32, Option<u32>, u32)],
        found: Vec<Diagnostic>,
    ) -> Document {
        let mut document = Document::new("/docs/index.rst".to_string(), Vec::new());
        for file in files {
            document.intern_source_file(*file);
        }
        document.include_sites = sites
            .iter()
            .map(|(line, within, file)| IncludeSite {
                directive: Some(span(*line, *within)),
                file: FileId::new(*file),
            })
            .collect();
        document.diagnostics = found;
        document
    }

    fn unknown(line: u32, file: u32) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::DirectiveUnknown,
            format!("unknown directive on line {line}"),
            span(line, Some(file)),
        )
    }

    /// The summaries for `document`, whose fragments all read as `text`.
    fn summaries(document: &Document) -> Vec<(Uri, lsp_types::Diagnostic)> {
        summaries_under(document, &Strictness::default())
    }

    /// [`summaries`], for a document of a project with `strictness`.
    fn summaries_under(
        document: &Document,
        strictness: &Strictness,
    ) -> Vec<(Uri, lsp_types::Diagnostic)> {
        let mut reads = FileReads::default();
        for file in &document.source_files {
            reads
                .texts
                .insert(file.into(), "a\nb\nc\nd\ne\n".to_string());
        }
        let open = DocumentStore::default();
        let index = uri("file:///docs/index.rst");
        let placement = Placement {
            uri: &index,
            text: "a\nb\nc\nd\ne\n",
            reads: &reads,
            open: &open,
        };
        let placed: Vec<Placed<'_>> = document
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let (uri, range) =
                    placement.place(document, diagnostic.span, PositionEncoding::Utf16);
                Placed {
                    diagnostic,
                    judgement: strictness.judge(diagnostic),
                    uri,
                    range,
                }
            })
            .collect();
        summarize_includes(document, &placed, &placement, PositionEncoding::Utf16)
    }

    #[test]
    fn test_an_include_bringing_in_one_problem_quotes_it() {
        // Given
        let document = document(&["/docs/part.rst"], &[(3, None, 0)], vec![unknown(2, 0)]);

        // When
        let found = summaries(&document);

        // Then — on the directive's line of the document, as information
        let [(on, summary)] = found.as_slice() else {
            panic!("one summary expected: {found:?}");
        };
        assert_eq!(on, &uri("file:///docs/index.rst"));
        assert_eq!(summary.range.start.line, 2);
        assert_eq!(summary.severity, Some(DiagnosticSeverity::INFORMATION));
        assert_eq!(summary.code, None);
        assert_eq!(
            summary.message,
            "problem in included file 'part.rst': directive.unknown: unknown directive on line 2"
        );
        let related = summary.related_information.as_deref().unwrap_or_default();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].location.uri, uri("file:///docs/part.rst"));
        assert_eq!(related[0].location.range.start.line, 1);
        assert_eq!(
            related[0].message,
            "directive.unknown: unknown directive on line 2"
        );
    }

    #[test]
    fn test_an_include_bringing_in_several_problems_counts_them() {
        // Given
        let document = document(
            &["/docs/shared/part.rst"],
            &[(1, None, 0)],
            vec![unknown(2, 0), unknown(4, 0)],
        );

        // When
        let found = summaries(&document);

        // Then
        assert_eq!(
            found[0].1.message,
            "2 problems in included file 'shared/part.rst', the first: directive.unknown: unknown directive on line 2"
        );
    }

    #[test]
    fn test_an_include_bringing_in_nothing_wrong_has_no_summary() {
        // Given a problem in the document itself, and a clean fragment
        let mut document = document(&["/docs/part.rst"], &[(1, None, 0)], Vec::new());
        document.diagnostics = vec![Diagnostic::new(
            DiagnosticCode::DirectiveUnknown,
            "own",
            span(4, None),
        )];

        // When / Then
        assert_eq!(summaries(&document), Vec::new());
    }

    #[test]
    fn test_an_include_bringing_in_only_lowered_problems_has_no_summary() {
        // Given an autodoc project whose fragment holds an autodoc directive
        let automodule = Diagnostic::new(
            DiagnosticCode::DirectiveUnknown,
            "unknown directive type 'automodule'",
            span(3, Some(0)),
        )
        .about(DiagnosticSubject::Directive("automodule".to_string()));
        let document = document(&["/docs/part.rst"], &[(1, None, 0)], vec![automodule]);
        let strictness = Strictness::legacy(&["sphinx.ext.autodoc".to_string()]);

        // When / Then — the fragment shows it as Information; the include is clean
        assert_eq!(summaries_under(&document, &strictness), Vec::new());
    }

    #[test]
    fn test_a_nested_problem_is_summarized_on_both_includes() {
        // Given the document includes outer (0) on line 1, which includes
        // inner (1) on its line 3, where the problem is
        let document = document(
            &["/docs/outer.rst", "/docs/inner.rst"],
            &[(1, None, 0), (3, Some(0), 1)],
            vec![unknown(2, 1)],
        );

        // When
        let found = summaries(&document);

        // Then — once in the document, once in the outer fragment
        let places: Vec<(Uri, u32)> = found
            .iter()
            .map(|(on, summary)| (on.clone(), summary.range.start.line))
            .collect();
        assert_eq!(
            places,
            vec![
                (uri("file:///docs/index.rst"), 0),
                (uri("file:///docs/outer.rst"), 2),
            ]
        );
        assert!(found[0].1.message.contains("'outer.rst'"), "{found:?}");
        assert!(found[1].1.message.contains("'inner.rst'"), "{found:?}");
    }

    #[test]
    fn test_a_problem_found_twice_is_linked_once() {
        // Given the same fragment included twice, so parsed and reported twice
        let document = document(
            &["/docs/part.rst"],
            &[(1, None, 0), (3, None, 0)],
            vec![unknown(2, 0), unknown(2, 0)],
        );

        // When
        let found = summaries(&document);

        // Then — both includes, each linking the one problem
        assert_eq!(found.len(), 2);
        for (_, summary) in &found {
            assert_eq!(summary.related_information.as_ref().map(Vec::len), Some(1));
        }
    }

    #[test]
    fn test_a_problem_on_the_include_line_itself_is_not_summarized() {
        // Given a document including itself, whose cycle is reported in the
        // spliced copy on the very line of the include
        let document = document(&["/docs/index.rst"], &[(1, None, 0)], vec![unknown(1, 0)]);

        // When / Then
        assert_eq!(summaries(&document), Vec::new());
    }

    #[test]
    fn test_included_name_falls_back_to_the_full_path_outside_the_directory() {
        // Given
        let document = document(&["/elsewhere/part.rst"], &[(1, None, 0)], Vec::new());

        // When / Then
        assert_eq!(
            included_name(&document.include_sites[0], &document, None),
            "/elsewhere/part.rst"
        );
    }

    #[test]
    fn test_included_name_places_the_documents_own_include_by_its_file() {
        // Given — the document named by its path within its source root, as a
        // workspace document is, and the fragment by where it is.
        let mut document = document(&["/work/docs/part.rst"], &[(1, None, 0)], Vec::new());
        document.path = "index.rst".to_string();

        // When
        let name = included_name(
            &document.include_sites[0],
            &document,
            Some(Path::new("/work/docs/index.rst")),
        );

        // Then
        assert_eq!(name, "part.rst");
    }
}

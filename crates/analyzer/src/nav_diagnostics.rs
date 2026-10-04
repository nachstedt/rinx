//! The problems only a project-wide view can see.
//!
//! A missing document, a glob matching nothing and an orphan are all invisible
//! to the parser, which reads one file at a time and cannot know what else
//! exists. They are found here, while the project index is built, and reported
//! by the `index` subcommand.
//!
//! Every diagnostic is attached to the document whose line caused it, carrying
//! that line's span — which is why [`rinx_ast::TocEntry`] keeps a span
//! all the way into the serialized index. Suppression is applied by the
//! reporter, never here: this phase records everything it finds, exactly as
//! `docs/decisions/003-diagnostics.md` requires.

use std::collections::{BTreeMap, BTreeSet};

use rinx_ast::{Diagnostic, DiagnosticCode};
use rinx_index::ProjectIndex;

use super::document_analysis::DocumentAnalysis;
use rinx_toctree::{TocTarget, UnmatchedKind, expand_toctree};

/// One document's project-wide diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDiagnostics {
    /// The `.rst` path the diagnostics belong to, which is the file a reader
    /// can open and the key the suppressions are stored under.
    pub source_path: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// Every project-wide toctree problem in `docs`.
///
/// Returns one entry per document that has at least one, so the caller can
/// filter each against that document's own `.. noqa:` suppressions.
#[must_use]
pub(super) fn collect_nav_diagnostics(
    analyses: &BTreeMap<String, DocumentAnalysis>,
    index: &ProjectIndex,
) -> Vec<DocumentDiagnostics> {
    let universe: BTreeSet<String> = analyses.keys().cloned().collect();
    let mut by_document: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();

    // How many toctrees name each document, so a second one can be reported.
    let mut referenced: BTreeMap<String, usize> = BTreeMap::new();

    for (owner, toctrees) in &index.toctrees {
        for placed in toctrees {
            let (targets, unmatched) = expand_toctree(&placed.toctree, owner, &universe);

            for entry in unmatched {
                let (code, message) = match entry.kind {
                    UnmatchedKind::MissingDocument => (
                        DiagnosticCode::ToctreeMissingDocument,
                        format!(
                            "Toctree entry '{}' names no document in this project.",
                            entry.text
                        ),
                    ),
                    UnmatchedKind::GlobMatchedNothing => (
                        DiagnosticCode::ToctreeGlobNoMatch,
                        format!("Toctree glob '{}' matched no document.", entry.text),
                    ),
                };
                by_document
                    .entry(owner.clone())
                    .or_default()
                    .push(Diagnostic::at(code, message, entry.span));
            }

            for target in targets {
                let TocTarget::Document { docname, span, .. } = target else {
                    continue;
                };
                // `self` names its own document without making it a second
                // arrival, so it is not counted as a reference.
                if docname == *owner {
                    continue;
                }
                let count = referenced.entry(docname.clone()).or_default();
                *count += 1;
                if *count == 2 {
                    by_document
                        .entry(owner.clone())
                        .or_default()
                        .push(Diagnostic::at(
                            DiagnosticCode::ToctreeDuplicateEntry,
                            format!(
                                "Document '{docname}' is already listed by another toctree; \
                                 it will appear twice in the navigation."
                            ),
                            span,
                        ));
                }
            }
        }
    }

    for (path, analysis) in analyses {
        if is_orphan_warning_warranted(path, analysis, index) {
            by_document
                .entry(path.clone())
                .or_default()
                .push(Diagnostic::at(
                    DiagnosticCode::ToctreeOrphanDocument,
                    format!(
                        "Document '{path}' is not included in any toctree, so it is unreachable \
                         from the navigation. Add it to one, or write ':orphan:' at the top \
                         of the file."
                    ),
                    // The problem is the *absence* of a line, in a different
                    // file, so there is no position to point at. Reporting none
                    // is better than inventing one.
                    None,
                ));
        }
    }

    by_document
        .into_iter()
        .map(|(source_path, diagnostics)| DocumentDiagnostics {
            source_path,
            diagnostics,
        })
        .collect()
}

/// Whether `doc` should be reported as unreachable.
///
/// A root document is reachable by definition, and an author who wrote
/// `:orphan:` has already said the omission is deliberate.
fn is_orphan_warning_warranted(
    path: &String,
    analysis: &DocumentAnalysis,
    index: &ProjectIndex,
) -> bool {
    !analysis.orphan && !index.root_documents.contains(path) && !index.page_order.contains(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Document;
    use rinx_ast::{Directive, Node, TocEntry, Toctree, ToctreeFlag, ToctreeOptions};

    fn toctree_node(entries: Vec<TocEntry>, glob: bool) -> Node {
        let mut options = ToctreeOptions::default();
        if glob {
            options.set(ToctreeFlag::Glob);
        }
        Node::Directive(Directive::Toctree(Toctree { entries, options }))
    }

    fn document(docname: &str) -> TocEntry {
        TocEntry::Document {
            title: None,
            docname: docname.to_string(),
            span: None,
        }
    }

    /// Builds the index for `docs` and collects their diagnostics.
    fn diagnose(docs: &[Document]) -> Vec<DocumentDiagnostics> {
        let index =
            super::super::build_project_index(docs, "index", &rinx_entity::EntitySchema::empty());
        let analyses = docs
            .iter()
            .map(|doc| (doc.path.clone(), DocumentAnalysis::of(doc)))
            .collect();
        collect_nav_diagnostics(&analyses, &index)
    }

    fn codes(reported: &[DocumentDiagnostics]) -> Vec<DiagnosticCode> {
        reported
            .iter()
            .flat_map(|entry| entry.diagnostics.iter().map(|d| d.code))
            .collect()
    }

    #[test]
    fn test_reports_an_entry_naming_no_document() {
        // Given
        let docs = vec![Document::new(
            "index.rst".to_string(),
            vec![toctree_node(vec![document("missing")], false)],
        )];

        // When
        let reported = diagnose(&docs);

        // Then
        assert!(codes(&reported).contains(&DiagnosticCode::ToctreeMissingDocument));
        assert_eq!(reported[0].source_path, "index.rst");
    }

    #[test]
    fn test_reports_a_glob_matching_nothing() {
        // Given
        let docs = vec![Document::new(
            "index.rst".to_string(),
            vec![toctree_node(
                vec![TocEntry::Glob {
                    pattern: "nope/*".to_string(),
                    span: None,
                }],
                true,
            )],
        )];

        // When
        let reported = diagnose(&docs);

        // Then
        assert!(codes(&reported).contains(&DiagnosticCode::ToctreeGlobNoMatch));
    }

    #[test]
    fn test_reports_a_document_listed_by_two_toctrees() {
        // Given — both `a` and `b` list `shared`.
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![toctree_node(vec![document("a"), document("b")], false)],
            ),
            Document::new(
                "a.rst".to_string(),
                vec![toctree_node(vec![document("shared")], false)],
            ),
            Document::new(
                "b.rst".to_string(),
                vec![toctree_node(vec![document("shared")], false)],
            ),
            Document::new("shared.rst".to_string(), vec![]),
        ];

        // When
        let reported = diagnose(&docs);

        // Then — reported once, on the second toctree to claim it.
        let duplicates: Vec<_> = codes(&reported)
            .into_iter()
            .filter(|code| *code == DiagnosticCode::ToctreeDuplicateEntry)
            .collect();
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn test_reports_an_orphan_document() {
        // Given
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![toctree_node(vec![document("a")], false)],
            ),
            Document::new("a.rst".to_string(), vec![]),
            Document::new("stray.rst".to_string(), vec![]),
        ];

        // When
        let reported = diagnose(&docs);

        // Then
        let orphans: Vec<_> = reported
            .iter()
            .filter(|entry| {
                entry
                    .diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::ToctreeOrphanDocument)
            })
            .map(|entry| entry.source_path.clone())
            .collect();
        assert_eq!(orphans, vec!["stray.rst"]);
    }

    #[test]
    fn test_orphan_metadata_silences_the_orphan_warning() {
        // Given — the author said the omission is deliberate.
        let mut stray = Document::new("stray.rst".to_string(), vec![]);
        stray.metadata.insert("orphan".to_string(), String::new());
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![toctree_node(vec![document("a")], false)],
            ),
            Document::new("a.rst".to_string(), vec![]),
            stray,
        ];

        // When
        let reported = diagnose(&docs);

        // Then
        assert!(!codes(&reported).contains(&DiagnosticCode::ToctreeOrphanDocument));
    }

    #[test]
    fn test_a_root_document_is_never_an_orphan() {
        // Given — the root is in no toctree by definition.
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![toctree_node(vec![document("a")], false)],
            ),
            Document::new("a.rst".to_string(), vec![]),
        ];

        // When
        let reported = diagnose(&docs);

        // Then
        assert!(!codes(&reported).contains(&DiagnosticCode::ToctreeOrphanDocument));
    }

    #[test]
    fn test_a_well_formed_project_reports_nothing() {
        // Given
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![toctree_node(vec![document("a")], false)],
            ),
            Document::new("a.rst".to_string(), vec![]),
        ];

        // When
        let reported = diagnose(&docs);

        // Then
        assert!(reported.is_empty(), "{reported:?}");
    }

    #[test]
    fn test_a_self_entry_does_not_count_as_a_duplicate_reference() {
        // Given — `index` lists itself with `self` and is also the root.
        let docs = vec![Document::new(
            "index.rst".to_string(),
            vec![toctree_node(
                vec![TocEntry::SelfRef {
                    title: None,
                    span: None,
                }],
                false,
            )],
        )];

        // When
        let reported = diagnose(&docs);

        // Then
        assert!(!codes(&reported).contains(&DiagnosticCode::ToctreeDuplicateEntry));
    }
}

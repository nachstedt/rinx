//! Which open documents read which files, and what each file therefore shows.
//!
//! Every open document is parsed on its own, and that parse may report on
//! files it `.. include::`s. A file may be included by several open documents,
//! and may itself be open — so what is published for a file is decided here,
//! from the latest parse of every open document:
//!
//! - **A file some *other* open document reads shows what those documents found
//!   in it**, merged and with duplicates removed. Its standalone parse is set
//!   aside, because a fragment is not a document: on its own it lacks whatever
//!   the includer defines before the `.. include::`, so it would report
//!   problems the build never sees.
//! - **Any other file shows its own parse**, as an open document always did.
//!
//! Which documents count is "the open ones", so that choice depends on what the
//! author has open until the server scans the workspace (roadmap step #4).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use lsp_types::Uri;

use crate::diagnostics::DocumentDiagnosis;
use crate::files::reads_at_or_under;
use crate::uri::file_path;

/// The latest diagnosis of every open document.
#[derive(Debug, Default)]
pub struct IncludeGraph {
    diagnoses: BTreeMap<Uri, DocumentDiagnosis>,
}

impl IncludeGraph {
    /// Records `diagnosis` as the latest for the open document `producer`, and
    /// returns the URIs whose published diagnostics may have changed.
    pub fn record(&mut self, producer: Uri, diagnosis: DocumentDiagnosis) -> BTreeSet<Uri> {
        let mut affected = self.affected_by(&diagnosis);
        if let Some(previous) = self.diagnoses.insert(producer, diagnosis) {
            affected.extend(self.affected_by(&previous));
        }
        affected
    }

    /// Forgets the document `producer`, which was closed, and returns the URIs
    /// whose published diagnostics may have changed.
    pub fn forget(&mut self, producer: &Uri) -> BTreeSet<Uri> {
        self.diagnoses
            .remove(producer)
            .map(|previous| self.affected_by(&previous))
            .unwrap_or_default()
    }

    /// The open documents whose latest parse read the file at `path`, or a
    /// file under it when `path` is a directory.
    #[must_use]
    pub fn includers_of(&self, path: &Path) -> Vec<Uri> {
        self.diagnoses
            .iter()
            .filter(|(_, diagnosis)| reads_at_or_under(&diagnosis.reads, path))
            .map(|(producer, _)| producer.clone())
            .collect()
    }

    /// What to publish for `uri`: what the other open documents reading it
    /// found there, or, when none does, its own diagnosis.
    #[must_use]
    pub fn diagnostics_for(&self, uri: &Uri) -> Vec<lsp_types::Diagnostic> {
        let includers: Vec<&DocumentDiagnosis> = match file_path(uri) {
            Some(path) => self
                .diagnoses
                .iter()
                .filter(|(producer, diagnosis)| *producer != uri && diagnosis.reads.contains(&path))
                .map(|(_, diagnosis)| diagnosis)
                .collect(),
            None => Vec::new(),
        };
        if includers.is_empty() {
            return self
                .diagnoses
                .get(uri)
                .and_then(|diagnosis| diagnosis.by_uri.get(uri))
                .cloned()
                .unwrap_or_default();
        }
        // Two documents including one fragment find the same mistake in it
        // twice; it is one mistake.
        let mut merged: Vec<lsp_types::Diagnostic> = Vec::new();
        for found in includers
            .into_iter()
            .filter_map(|diagnosis| diagnosis.by_uri.get(uri))
            .flatten()
        {
            if !merged.contains(found) {
                merged.push(found.clone());
            }
        }
        merged
    }

    /// The URIs whose published diagnostics `diagnosis` contributes to: every
    /// file it reported on, and every open document whose file it read —
    /// reading one is what sets that document's standalone parse aside.
    fn affected_by(&self, diagnosis: &DocumentDiagnosis) -> BTreeSet<Uri> {
        let read_open = self.diagnoses.keys().filter(|producer| {
            file_path(producer).is_some_and(|path| diagnosis.reads.contains(&path))
        });
        diagnosis.by_uri.keys().chain(read_open).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn uri(text: &str) -> Uri {
        text.parse().expect("valid uri")
    }

    fn finding(message: &str) -> lsp_types::Diagnostic {
        lsp_types::Diagnostic {
            message: message.to_string(),
            ..lsp_types::Diagnostic::default()
        }
    }

    /// A diagnosis reporting `findings` per URI, having read `reads`.
    fn diagnosis(findings: &[(&str, &[&str])], reads: &[&str]) -> DocumentDiagnosis {
        DocumentDiagnosis {
            by_uri: findings
                .iter()
                .map(|(target, messages)| {
                    (uri(target), messages.iter().map(|m| finding(m)).collect())
                })
                .collect(),
            reads: reads.iter().map(PathBuf::from).collect(),
        }
    }

    const INDEX: &str = "file:///docs/index.rst";
    const GUIDE: &str = "file:///docs/guide.rst";
    const PART: &str = "file:///docs/part.rst";

    #[test]
    fn test_a_document_nobody_includes_shows_its_own_diagnosis() {
        // Given
        let mut graph = IncludeGraph::default();
        graph.record(uri(PART), diagnosis(&[(PART, &["standalone"])], &[]));

        // When
        let shown = graph.diagnostics_for(&uri(PART));

        // Then
        assert_eq!(shown, vec![finding("standalone")]);
    }

    #[test]
    fn test_an_included_fragment_shows_what_its_includer_found_there() {
        // Given an includer reporting on the fragment, which is closed
        let mut graph = IncludeGraph::default();
        graph.record(
            uri(INDEX),
            diagnosis(&[(INDEX, &[]), (PART, &["in part"])], &["/docs/part.rst"]),
        );

        // When
        let shown = graph.diagnostics_for(&uri(PART));

        // Then
        assert_eq!(shown, vec![finding("in part")]);
    }

    #[test]
    fn test_an_open_included_fragment_sets_its_standalone_diagnosis_aside() {
        // Given a fragment open on its own and through an includer that finds
        // nothing in it
        let mut graph = IncludeGraph::default();
        graph.record(uri(PART), diagnosis(&[(PART, &["standalone"])], &[]));
        graph.record(uri(INDEX), diagnosis(&[(INDEX, &[])], &["/docs/part.rst"]));

        // When
        let shown = graph.diagnostics_for(&uri(PART));

        // Then
        assert_eq!(shown, Vec::new());
    }

    #[test]
    fn test_two_includers_finding_the_same_mistake_show_it_once() {
        // Given
        let mut graph = IncludeGraph::default();
        let both = diagnosis(&[(PART, &["same", "first only"])], &["/docs/part.rst"]);
        graph.record(uri(INDEX), both);
        graph.record(
            uri(GUIDE),
            diagnosis(&[(PART, &["same"])], &["/docs/part.rst"]),
        );

        // When
        let shown = graph.diagnostics_for(&uri(PART));

        // Then
        assert_eq!(shown, vec![finding("same"), finding("first only")]);
    }

    #[test]
    fn test_record_reports_the_files_of_the_old_and_new_diagnosis() {
        // Given an includer that used to report on the fragment
        let mut graph = IncludeGraph::default();
        graph.record(
            uri(INDEX),
            diagnosis(&[(INDEX, &[]), (PART, &["x"])], &["/docs/part.rst"]),
        );

        // When it no longer includes it
        let affected = graph.record(uri(INDEX), diagnosis(&[(INDEX, &[])], &[]));

        // Then — the fragment must be republished to clear it
        assert_eq!(affected, BTreeSet::from([uri(INDEX), uri(PART)]));
    }

    #[test]
    fn test_record_reports_an_open_document_it_reads() {
        // Given a fragment that is open
        let mut graph = IncludeGraph::default();
        graph.record(uri(PART), diagnosis(&[(PART, &["standalone"])], &[]));

        // When an includer that finds nothing in it appears
        let affected = graph.record(uri(INDEX), diagnosis(&[(INDEX, &[])], &["/docs/part.rst"]));

        // Then — the fragment's standalone diagnosis is now set aside
        assert!(affected.contains(&uri(PART)), "{affected:?}");
    }

    #[test]
    fn test_forget_reports_what_the_closed_document_contributed_to() {
        // Given
        let mut graph = IncludeGraph::default();
        graph.record(
            uri(INDEX),
            diagnosis(&[(INDEX, &[]), (PART, &["x"])], &["/docs/part.rst"]),
        );

        // When
        let affected = graph.forget(&uri(INDEX));

        // Then
        assert_eq!(affected, BTreeSet::from([uri(INDEX), uri(PART)]));
        assert_eq!(graph.diagnostics_for(&uri(PART)), Vec::new());
    }

    #[test]
    fn test_forget_of_an_unknown_document_affects_nothing() {
        // Given / When
        let affected = IncludeGraph::default().forget(&uri(INDEX));

        // Then
        assert_eq!(affected, BTreeSet::new());
    }

    #[test]
    fn test_includers_of_lists_the_documents_that_read_a_file() {
        // Given
        let mut graph = IncludeGraph::default();
        graph.record(uri(INDEX), diagnosis(&[(INDEX, &[])], &["/docs/part.rst"]));
        graph.record(uri(GUIDE), diagnosis(&[(GUIDE, &[])], &[]));

        // When
        let includers = graph.includers_of(Path::new("/docs/part.rst"));

        // Then
        assert_eq!(includers, vec![uri(INDEX)]);
    }

    #[test]
    fn test_an_unsaved_buffer_shows_its_own_diagnosis() {
        // Given a document with no file, which nothing can include
        let untitled = "untitled:Untitled-1";
        let mut graph = IncludeGraph::default();
        graph.record(uri(untitled), diagnosis(&[(untitled, &["own"])], &[]));

        // When
        let shown = graph.diagnostics_for(&uri(untitled));

        // Then
        assert_eq!(shown, vec![finding("own")]);
    }

    #[test]
    fn test_a_document_including_itself_still_shows_its_own_diagnosis() {
        // Given a cycle, which the parser reports on the document itself
        let mut graph = IncludeGraph::default();
        graph.record(
            uri(INDEX),
            diagnosis(&[(INDEX, &["cycle"])], &["/docs/index.rst"]),
        );

        // When
        let shown = graph.diagnostics_for(&uri(INDEX));

        // Then
        assert_eq!(shown, vec![finding("cycle")]);
    }
}

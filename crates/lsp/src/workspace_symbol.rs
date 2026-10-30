//! The answer to `workspace/symbol`: the definitions of every project whose
//! names fuzzily match a query, best first.
//!
//! Each project lists what its documents define ([`crate::project::Project::symbols`]),
//! built once per analysis; a search only matches the names, with frizbee's
//! Smith-Waterman matcher, case-insensitively and without typos, since a
//! client filters what it shows with its own matcher too and would hide a
//! name the query does not spell. Ties are broken by the shorter name, then
//! by document and position, so the answer never depends on the order the
//! projects or their documents are held in. Only the matches returned are
//! placed, which is when their files' texts are read.

use std::cmp::{Ordering, Reverse};
use std::collections::BTreeSet;

use frizbee::{CaseMatching, Config, Matcher};
use lsp_types::Uri;

use crate::definition::site_location;
use crate::document_symbol::FlatSymbol;
use crate::position::PositionEncoding;
use crate::project::{RST_SUFFIX, SymbolCandidate};

/// The most symbols one answer lists. A client asks again as the query grows,
/// so the best of a short query's matches are all it needs.
pub const MAX_SYMBOLS: usize = 200;

/// The `candidates` whose names match `query`, best first and at most
/// [`MAX_SYMBOLS`] of them, each definition once: a document two projects
/// hold — a source two Bazel sites list — is indexed by both. An empty query
/// matches every candidate.
#[must_use]
pub fn best_matches<'a>(
    query: &str,
    candidates: &[SymbolCandidate<'a>],
) -> Vec<SymbolCandidate<'a>> {
    let names: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.entry.name.as_str())
        .collect();
    let config = Config {
        casing: CaseMatching::Ignore,
        ..Config::default()
    };
    let mut matches: Vec<(u16, &SymbolCandidate<'a>)> = Matcher::new(query, &config)
        .match_list(&names)
        .into_iter()
        .filter_map(|found| Some((found.score, candidates.get(found.index as usize)?)))
        .collect();
    matches.sort_by(|(score, candidate), (other_score, other)| {
        Reverse(*score)
            .cmp(&Reverse(*other_score))
            .then_with(|| tie_break(candidate, other))
    });
    let mut listed = BTreeSet::new();
    matches
        .into_iter()
        .map(|(_, candidate)| *candidate)
        .filter(|candidate| {
            listed.insert((
                candidate.path(),
                candidate.entry.start,
                candidate.entry.name.clone(),
            ))
        })
        .take(MAX_SYMBOLS)
        .collect()
}

/// How two candidates matching equally well are ordered: the shorter name
/// first, then by name, document and place.
fn tie_break(candidate: &SymbolCandidate<'_>, other: &SymbolCandidate<'_>) -> Ordering {
    let (entry, other_entry) = (candidate.entry, other.entry);
    entry
        .name
        .chars()
        .count()
        .cmp(&other_entry.name.chars().count())
        .then_with(|| entry.name.cmp(&other_entry.name))
        .then_with(|| candidate.doc_path.cmp(other.doc_path))
        .then_with(|| entry.file.cmp(&other_entry.file))
        .then_with(|| entry.start.cmp(&other_entry.start))
}

/// `candidate` as the answer lists it, written in the file at `uri` whose
/// text is `text`: its name and kind, where it was written, and the document
/// defining it, which tells apart two sections both titled *Notes*.
#[must_use]
pub fn workspace_symbol(
    candidate: &SymbolCandidate<'_>,
    uri: Uri,
    text: Option<&str>,
    encoding: PositionEncoding,
) -> FlatSymbol {
    let entry = candidate.entry;
    FlatSymbol {
        name: entry.name.clone(),
        kind: entry.kind,
        location: site_location(uri, entry.start, entry.end, text, encoding),
        container_name: Some(document_name(candidate.doc_path).to_string()),
    }
}

/// The name a document is referred to by: `library/os` for `library/os.rst`.
fn document_name(doc_path: &str) -> &str {
    doc_path.strip_suffix(RST_SUFFIX).unwrap_or(doc_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SymbolEntry;
    use lsp_types::{Location, Range, SymbolKind};
    use rinx_ast::Position;
    use std::path::Path;
    use std::str::FromStr;

    fn entry(name: &str, line: u32) -> SymbolEntry {
        SymbolEntry {
            name: name.to_string(),
            kind: SymbolKind::KEY,
            file: None,
            start: Position::new(line, 1),
            end: Position::new(line, 4),
        }
    }

    fn candidate<'a>(doc_path: &'a str, entry: &'a SymbolEntry) -> SymbolCandidate<'a> {
        SymbolCandidate {
            doc_path,
            entry,
            document: Path::new("/work/guide.rst"),
            source_root: Path::new("/work"),
        }
    }

    fn names(matches: &[SymbolCandidate<'_>]) -> Vec<String> {
        matches
            .iter()
            .map(|candidate| candidate.entry.name.clone())
            .collect()
    }

    #[test]
    fn test_best_matches_lists_the_best_match_first() {
        // Given
        let entries = [
            entry("addict", 1),
            entry("dict.items", 2),
            entry("os.path", 3),
            entry("dict", 4),
        ];
        let candidates: Vec<_> = entries
            .iter()
            .map(|entry| candidate("guide.rst", entry))
            .collect();

        // When
        let matches = best_matches("dict", &candidates);

        // Then — the exact name first, one it merely contains last
        assert_eq!(names(&matches), ["dict", "dict.items", "addict"]);
    }

    #[test]
    fn test_best_matches_ignores_case_and_skips_characters() {
        // Given
        let entries = [entry("pkg.Box.open", 1), entry("Notes", 2)];
        let candidates: Vec<_> = entries
            .iter()
            .map(|entry| candidate("guide.rst", entry))
            .collect();

        // When / Then
        assert_eq!(names(&best_matches("PBO", &candidates)), ["pkg.Box.open"]);
        assert_eq!(names(&best_matches("notes", &candidates)), ["Notes"]);
        assert!(best_matches("zzz", &candidates).is_empty());
    }

    #[test]
    fn test_best_matches_lists_every_candidate_for_an_empty_query() {
        // Given
        let entries = [entry("usage", 1), entry("install", 2)];
        let candidates: Vec<_> = entries
            .iter()
            .map(|entry| candidate("guide.rst", entry))
            .collect();

        // When / Then — the shorter name first
        assert_eq!(names(&best_matches("", &candidates)), ["usage", "install"]);
    }

    #[test]
    fn test_best_matches_lists_a_definition_two_projects_hold_once() {
        // Given — one document, indexed by two sites
        let notes = entry("notes", 1);
        let candidates = [
            candidate("guide.rst", &notes),
            candidate("guide.rst", &notes),
        ];

        // When / Then
        assert_eq!(best_matches("notes", &candidates).len(), 1);
    }

    #[test]
    fn test_best_matches_keeps_equal_names_of_different_places() {
        // Given — two sections titled alike, in two documents
        let (first, second) = (entry("Notes", 3), entry("Notes", 1));
        let candidates = [candidate("b.rst", &first), candidate("a.rst", &second)];

        // When
        let matches = best_matches("notes", &candidates);

        // Then — in document order
        let documents: Vec<&str> = matches.iter().map(|found| found.doc_path).collect();
        assert_eq!(documents, ["a.rst", "b.rst"]);
    }

    #[test]
    fn test_best_matches_lists_at_most_the_maximum() {
        // Given
        let entries: Vec<SymbolEntry> = (1..=MAX_SYMBOLS + 5)
            .map(|line| entry("label", u32::try_from(line).unwrap()))
            .collect();
        let candidates: Vec<_> = entries
            .iter()
            .map(|entry| candidate("guide.rst", entry))
            .collect();

        // When
        let matches = best_matches("label", &candidates);

        // Then — the first ones written
        assert_eq!(matches.len(), MAX_SYMBOLS);
        assert_eq!(matches[0].entry.start.line, 1);
    }

    #[test]
    fn test_tie_break_orders_by_length_name_document_and_place() {
        // Given
        let (short, long, early, late) = (
            entry("ab", 1),
            entry("abc", 1),
            entry("x", 1),
            entry("x", 2),
        );

        // When / Then
        let order = |a, b| tie_break(&a, &b);
        assert_eq!(
            order(candidate("z.rst", &short), candidate("a.rst", &long)),
            Ordering::Less
        );
        assert_eq!(
            order(candidate("b.rst", &early), candidate("a.rst", &early)),
            Ordering::Greater
        );
        assert_eq!(
            order(candidate("a.rst", &early), candidate("a.rst", &late)),
            Ordering::Less
        );
    }

    #[test]
    fn test_workspace_symbol_names_its_document_and_place() {
        // Given
        let install = entry("install", 3);
        let found = candidate("guide/setup.rst", &install);
        let uri = Uri::from_str("file:///work/guide/setup.rst").unwrap();

        // When
        let symbol = workspace_symbol(
            &found,
            uri.clone(),
            Some("\n\n.. _install:\n"),
            PositionEncoding::Utf16,
        );

        // Then
        assert_eq!(symbol.name, "install");
        assert_eq!(symbol.kind, SymbolKind::KEY);
        assert_eq!(symbol.container_name.as_deref(), Some("guide/setup"));
        assert_eq!(
            symbol.location,
            Location::new(
                uri,
                Range::new(
                    lsp_types::Position::new(2, 0),
                    lsp_types::Position::new(2, 3)
                )
            )
        );
    }

    #[test]
    fn test_document_name_drops_the_suffix() {
        // When / Then
        assert_eq!(document_name("library/os.rst"), "library/os");
        assert_eq!(document_name("README"), "README");
    }
}

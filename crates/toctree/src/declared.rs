//! Whether a document's toctrees reach only documents the build declared.
//!
//! A Bazel build makes a toctree a dependency: an entry may name a document
//! of its own library or of a library in its `deps`, and the build fails on
//! any other. The worker's `validate_toctree` action and the language server
//! both ask [`undeclared_toctree_entries`], so the editor underlines exactly
//! what fails the build.
//!
//! Sharing [`expand_toctree`] is what makes `:glob:` enforceable. A pattern is
//! expanded against the *declared dependencies* here and against the whole
//! project later, so a glob that reaches an undeclared document still fails
//! the build — it simply matches nothing in this narrower universe.

use std::collections::BTreeSet;

use rinx_ast::{
    Diagnostic, DiagnosticCode, DiagnosticSubject, Directive, Document, Node, UndeclaredEntry,
};

use crate::resolve::{UnmatchedKind, expand_toctree};

/// The documents a library's toctrees may name: its own and its direct
/// dependencies', each as a `.rst` path from the source root.
///
/// Bazel passes the names without their extension (`rules/library.bzl`'s
/// `local_doc_names`); entry resolution works in `.rst` paths throughout, so
/// the extension is given once, here, rather than at every comparison.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeclaredDocuments(BTreeSet<String>);

impl DeclaredDocuments {
    /// The declared documents named `names`, with or without `.rst`.
    #[must_use]
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self(
            names
                .into_iter()
                .map(|name| {
                    if std::path::Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("rst"))
                    {
                        name.to_string()
                    } else {
                        format!("{name}.rst")
                    }
                })
                .collect(),
        )
    }
}

/// One error per entry of `doc`'s toctrees that names no document of
/// `declared`, at the entry.
///
/// `self` and external links name no dependency and are skipped, which
/// [`expand_toctree`] already does. Like the project index, only the
/// document's top-level toctrees are read.
#[must_use]
pub fn undeclared_toctree_entries(doc: &Document, declared: &DeclaredDocuments) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for node in &doc.nodes {
        let Node::Directive {
            directive: Directive::Toctree(toctree),
            ..
        } = node
        else {
            continue;
        };
        let (_, unmatched) = expand_toctree(toctree, &doc.path, &declared.0);
        for entry in unmatched {
            let (code, message, needed) = match entry.kind {
                UnmatchedKind::MissingDocument => (
                    DiagnosticCode::ToctreeUndeclaredDependency,
                    format!(
                        "toctree entry '{}' names a document of no library in this library's \
                         deps — if this is a Bazel build, add its rinx_library to `deps`",
                        entry.text
                    ),
                    UndeclaredEntry::Document(entry.resolved),
                ),
                UnmatchedKind::GlobMatchedNothing => (
                    DiagnosticCode::ToctreeGlobMatchesNoDependency,
                    format!(
                        "toctree glob '{}' matches no document of this library or its deps — if \
                         this is a Bazel build, add the rinx_library holding them to `deps`",
                        entry.text
                    ),
                    UndeclaredEntry::Glob(entry.resolved),
                ),
            };
            found.push(
                Diagnostic::at(code, message, entry.span)
                    .about(DiagnosticSubject::UndeclaredToctreeEntry(needed)),
            );
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{
        DiagnosticSubject, Position, Span, TocEntry, Toctree, ToctreeFlag, ToctreeOptions,
        UndeclaredEntry,
    };

    /// Where every entry of these tests is written.
    fn entry_span() -> Span {
        Span::new(Position::new(4, 4), Position::new(4, 16))
    }

    /// A document at `docs/index.rst` holding `toctree`.
    fn index_with(toctree: Toctree) -> Document {
        Document::new(
            "docs/index.rst".to_string(),
            vec![Node::directive(Directive::Toctree(toctree))],
        )
    }

    /// A toctree of plain document entries.
    fn toctree_of(docnames: &[&str]) -> Toctree {
        Toctree {
            entries: docnames
                .iter()
                .map(|docname| TocEntry::Document {
                    title: None,
                    docname: (*docname).to_string(),
                    span: Some(entry_span()),
                })
                .collect(),
            options: ToctreeOptions::default(),
        }
    }

    /// A toctree of one `:glob:` pattern.
    fn globbing_toctree(pattern: &str) -> Toctree {
        let mut options = ToctreeOptions::default();
        options.set(ToctreeFlag::Glob);
        Toctree {
            entries: vec![TocEntry::Glob {
                pattern: pattern.to_string(),
                span: Some(entry_span()),
            }],
            options,
        }
    }

    fn codes(found: &[Diagnostic]) -> Vec<DiagnosticCode> {
        found.iter().map(|diagnostic| diagnostic.code).collect()
    }

    #[test]
    fn test_from_names_gives_an_extensionless_name_its_rst_suffix() {
        // Given / When
        let declared = DeclaredDocuments::from_names(["docs/a", "docs/b.rst"]);

        // Then
        assert_eq!(
            declared.0,
            BTreeSet::from(["docs/a.rst".to_string(), "docs/b.rst".to_string()])
        );
    }

    #[test]
    fn test_accepts_an_entry_naming_a_declared_document() {
        // Given
        let doc = index_with(toctree_of(&["team_a/index"]));
        let declared = DeclaredDocuments::from_names(["docs/team_a/index"]);

        // When
        let found = undeclared_toctree_entries(&doc, &declared);

        // Then
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn test_reports_an_undeclared_entry_at_its_position() {
        // Given
        let doc = index_with(toctree_of(&["team_a/index"]));

        // When
        let found = undeclared_toctree_entries(&doc, &DeclaredDocuments::default());

        // Then
        assert_eq!(codes(&found), [DiagnosticCode::ToctreeUndeclaredDependency]);
        assert_eq!(found[0].span, Some(entry_span()));
        assert!(
            found[0].message.contains("team_a/index"),
            "{}",
            found[0].message
        );
        assert!(found[0].message.contains("`deps`"), "{}", found[0].message);
    }

    #[test]
    fn test_an_undeclared_entry_names_the_document_it_needs() {
        // Given
        let doc = index_with(toctree_of(&["team_a/index"]));

        // When
        let found = undeclared_toctree_entries(&doc, &DeclaredDocuments::default());

        // Then
        assert_eq!(
            found[0].subject,
            Some(DiagnosticSubject::UndeclaredToctreeEntry(
                UndeclaredEntry::Document("docs/team_a/index.rst".to_string())
            ))
        );
    }

    #[test]
    fn test_an_undeclared_glob_names_the_pattern_it_was_matched_as() {
        // Given
        let doc = index_with(globbing_toctree("team_b/*"));

        // When
        let found = undeclared_toctree_entries(&doc, &DeclaredDocuments::default());

        // Then
        assert_eq!(
            found[0].subject,
            Some(DiagnosticSubject::UndeclaredToctreeEntry(
                UndeclaredEntry::Glob("docs/team_b/*".to_string())
            ))
        );
    }

    #[test]
    fn test_accepts_an_entry_written_with_its_rst_suffix() {
        // Given
        let doc = index_with(toctree_of(&["team_a/index.rst"]));
        let declared = DeclaredDocuments::from_names(["docs/team_a/index"]);

        // When
        let found = undeclared_toctree_entries(&doc, &declared);

        // Then
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn test_accepts_an_empty_toctree() {
        // Given
        let doc = index_with(toctree_of(&[]));

        // When
        let found = undeclared_toctree_entries(&doc, &DeclaredDocuments::default());

        // Then
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn test_accepts_a_glob_matching_a_declared_document() {
        // Given
        let doc = index_with(globbing_toctree("team_a/*"));
        let declared = DeclaredDocuments::from_names(["docs/team_a/index"]);

        // When
        let found = undeclared_toctree_entries(&doc, &declared);

        // Then
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn test_reports_a_glob_matching_no_declared_document() {
        // Given — the pattern is legal, but nothing declared satisfies it.
        // This is the case that would go unchecked if globs were expanded
        // against the whole project instead of against the declared deps.
        let doc = index_with(globbing_toctree("team_b/*"));
        let declared = DeclaredDocuments::from_names(["docs/team_a/index"]);

        // When
        let found = undeclared_toctree_entries(&doc, &declared);

        // Then
        assert_eq!(
            codes(&found),
            [DiagnosticCode::ToctreeGlobMatchesNoDependency]
        );
        assert!(
            found[0].message.contains("team_b/*"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn test_ignores_self_and_external_entries() {
        // Given — neither names a document, so neither needs declaring.
        let doc = index_with(Toctree {
            entries: vec![
                TocEntry::SelfRef {
                    title: None,
                    span: None,
                },
                TocEntry::External {
                    title: None,
                    url: "https://example.org".to_string(),
                    span: None,
                },
            ],
            options: ToctreeOptions::default(),
        });

        // When
        let found = undeclared_toctree_entries(&doc, &DeclaredDocuments::default());

        // Then
        assert!(found.is_empty(), "{found:?}");
    }
}

//! Turning a toctree's written entries into the documents they name.
//!
//! One function does this for every phase that needs it, because each needs it
//! against a *different* set of documents:
//!
//! | caller | universe | purpose |
//! |---|---|---|
//! | `rinx_worker`'s `validate_toctree` | the Bazel `--allowed` deps | strict-deps enforcement |
//! | [`crate::build_project_index`] | every document in the project | numbering, page order, diagnostics |
//! | the renderer | the documents the index knows | rendering |
//!
//! That is why a glob is stored unexpanded in the AST and the index: expanding
//! it once, early, would leave the strict-deps check either reimplementing the
//! matcher against its narrower list, or not checking globs at all.

use rinx_ast::{Span, TocEntry, Toctree, ToctreeFlag};
use std::collections::BTreeSet;

use super::glob::matching_docnames;
use rinx_ast::normalize_path;

/// What one toctree entry turned out to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TocTarget {
    /// A document in the project, as a normalized `.rst` path.
    Document {
        docname: String,
        /// The author's explicit title, when they wrote one. `None` means the
        /// document's own title should be looked up instead.
        title: Option<String>,
        span: Option<Span>,
    },
    /// A link out of the project. It names no document, so no phase resolves,
    /// numbers or orders it.
    External {
        url: String,
        title: Option<String>,
        span: Option<Span>,
    },
}

/// An entry that named nothing, for the caller to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmatchedEntry {
    pub kind: UnmatchedKind,
    /// The entry as the author wrote it, for the diagnostic message.
    pub text: String,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmatchedKind {
    /// A plain entry naming a document that is not in the universe.
    MissingDocument,
    /// A `:glob:` pattern that matched no document.
    GlobMatchedNothing,
}

/// Resolves a toctree entry's written docname against the document containing
/// it, yielding a normalized `.rst` path.
///
/// A leading `/` makes the name absolute — relative to the source root rather
/// than to the referencing document — which is Sphinx's rule. Everything else
/// is relative to the referencing document's directory.
///
/// This is the one implementation: the analyzer and the Bazel strict-deps
/// validator previously had a copy each, differing only in whether they
/// re-appended `.rst`.
#[must_use]
pub fn resolve_docname(referencing_doc: &str, docname: &str) -> String {
    let combined = if let Some(absolute) = docname.strip_prefix('/') {
        std::path::PathBuf::from(absolute)
    } else {
        std::path::Path::new(referencing_doc)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""))
            .join(docname)
    };

    let normalized = normalize_path(&combined);
    let mut path = normalized.to_string_lossy().replace('\\', "/");
    if !std::path::Path::new(&path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rst"))
    {
        path.push_str(".rst");
    }
    path
}

/// Strips a `.rst` suffix, giving the extension-less form an author writes and
/// a glob pattern matches against.
#[must_use]
pub fn strip_rst(path: &str) -> &str {
    path.strip_suffix(".rst").unwrap_or(path)
}

/// Expands `toctree`'s entries into the documents and links they name.
///
/// `owner` is the `.rst` path of the document containing the toctree, used to
/// resolve relative entries, to give `self` its meaning, and to keep a glob
/// from matching the document that wrote it. `universe` holds every `.rst`
/// path the caller considers to exist.
///
/// `:reversed:` is applied here, *after* expansion, so that a reversed glob
/// reverses its matches rather than only the pattern's position — and so that
/// navigation, page order and section numbering all see one order.
#[must_use]
pub fn expand_toctree(
    toctree: &Toctree,
    owner: &str,
    universe: &BTreeSet<String>,
) -> (Vec<TocTarget>, Vec<UnmatchedEntry>) {
    let docnames: Vec<&str> = universe.iter().map(|path| strip_rst(path)).collect();
    let owner_docname = strip_rst(owner);

    let mut targets = Vec::new();
    let mut unmatched = Vec::new();

    for entry in &toctree.entries {
        match entry {
            TocEntry::SelfRef { title, span } => targets.push(TocTarget::Document {
                docname: owner.to_string(),
                title: title.clone(),
                span: *span,
            }),
            TocEntry::External { title, url, span } => targets.push(TocTarget::External {
                url: url.clone(),
                title: title.clone(),
                span: *span,
            }),
            TocEntry::Document {
                title,
                docname,
                span,
            } => {
                let resolved = resolve_docname(owner, docname);
                if universe.contains(&resolved) {
                    targets.push(TocTarget::Document {
                        docname: resolved,
                        title: title.clone(),
                        span: *span,
                    });
                } else {
                    unmatched.push(UnmatchedEntry {
                        kind: UnmatchedKind::MissingDocument,
                        text: docname.clone(),
                        span: *span,
                    });
                }
            }
            TocEntry::Glob { pattern, span } => {
                // A pattern is written relative to the referencing document
                // just as a plain entry is, so it is resolved the same way
                // before matching — then stripped back, since docnames match
                // without their extension.
                let resolved_pattern = resolve_docname(owner, pattern);
                let matched = matching_docnames(
                    strip_rst(&resolved_pattern),
                    docnames.iter().copied(),
                    owner_docname,
                );
                if matched.is_empty() {
                    unmatched.push(UnmatchedEntry {
                        kind: UnmatchedKind::GlobMatchedNothing,
                        text: pattern.clone(),
                        span: *span,
                    });
                }
                for docname in matched {
                    targets.push(TocTarget::Document {
                        docname: format!("{docname}.rst"),
                        title: None,
                        span: *span,
                    });
                }
            }
        }
    }

    if toctree.options.has(ToctreeFlag::Reversed) {
        targets.reverse();
    }

    (targets, unmatched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::ToctreeOptions;

    fn universe(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| (*p).to_string()).collect()
    }

    fn toctree(entries: Vec<TocEntry>) -> Toctree {
        Toctree {
            entries,
            options: ToctreeOptions::default(),
        }
    }

    fn document(docname: &str) -> TocEntry {
        TocEntry::Document {
            title: None,
            docname: docname.to_string(),
            span: None,
        }
    }

    fn resolved_docnames(targets: &[TocTarget]) -> Vec<String> {
        targets
            .iter()
            .filter_map(|target| match target {
                TocTarget::Document { docname, .. } => Some(docname.clone()),
                TocTarget::External { .. } => None,
            })
            .collect()
    }

    #[test]
    fn test_resolve_docname_appends_the_rst_extension() {
        // Given / When / Then
        assert_eq!(resolve_docname("index.rst", "intro"), "intro.rst");
    }

    #[test]
    fn test_resolve_docname_keeps_an_existing_rst_extension() {
        // Given / When / Then
        assert_eq!(resolve_docname("index.rst", "intro.rst"), "intro.rst");
    }

    #[test]
    fn test_resolve_docname_is_relative_to_the_referencing_document() {
        // Given / When / Then
        assert_eq!(
            resolve_docname("docs/index.rst", "team_a/index"),
            "docs/team_a/index.rst"
        );
    }

    #[test]
    fn test_resolve_docname_resolves_parent_segments() {
        // Given / When / Then
        assert_eq!(
            resolve_docname("docs/team_a/index.rst", "../team_b/index"),
            "docs/team_b/index.rst"
        );
    }

    #[test]
    fn test_resolve_docname_treats_a_leading_slash_as_source_root_relative() {
        // Given — Sphinx's absolute-docname form.
        let resolved = resolve_docname("docs/team_a/index.rst", "/intro");

        // When / Then — not `docs/team_a/intro.rst`.
        assert_eq!(resolved, "intro.rst");
    }

    #[test]
    fn test_expand_toctree_resolves_a_plain_document_entry() {
        // Given
        let toctree = toctree(vec![document("intro")]);

        // When
        let (targets, unmatched) = expand_toctree(&toctree, "index.rst", &universe(&["intro.rst"]));

        // Then
        assert!(unmatched.is_empty());
        assert_eq!(resolved_docnames(&targets), vec!["intro.rst"]);
    }

    #[test]
    fn test_expand_toctree_reports_a_document_outside_the_universe() {
        // Given
        let toctree = toctree(vec![document("missing")]);

        // When
        let (targets, unmatched) = expand_toctree(&toctree, "index.rst", &universe(&["intro.rst"]));

        // Then
        assert!(targets.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].kind, UnmatchedKind::MissingDocument);
        assert_eq!(unmatched[0].text, "missing");
    }

    #[test]
    fn test_expand_toctree_resolves_self_to_the_owning_document() {
        // Given
        let toctree = toctree(vec![TocEntry::SelfRef {
            title: Some("Overview".to_string()),
            span: None,
        }]);

        // When
        let (targets, unmatched) = expand_toctree(&toctree, "guide/index.rst", &universe(&[]));

        // Then — `self` needs no entry in the universe; it is the owner.
        assert!(unmatched.is_empty());
        assert_eq!(
            targets,
            vec![TocTarget::Document {
                docname: "guide/index.rst".to_string(),
                title: Some("Overview".to_string()),
                span: None,
            }]
        );
    }

    #[test]
    fn test_expand_toctree_passes_an_external_link_through_unresolved() {
        // Given
        let toctree = toctree(vec![TocEntry::External {
            title: Some("Upstream".to_string()),
            url: "https://example.org".to_string(),
            span: None,
        }]);

        // When
        let (targets, unmatched) = expand_toctree(&toctree, "index.rst", &universe(&[]));

        // Then — never looked up, so never missing.
        assert!(unmatched.is_empty());
        assert_eq!(
            targets,
            vec![TocTarget::External {
                url: "https://example.org".to_string(),
                title: Some("Upstream".to_string()),
                span: None,
            }]
        );
    }

    #[test]
    fn test_expand_toctree_expands_a_glob_in_sorted_order() {
        // Given
        let toctree = toctree(vec![TocEntry::Glob {
            pattern: "api/*".to_string(),
            span: None,
        }]);
        let universe = universe(&["api/server.rst", "api/client.rst", "intro.rst"]);

        // When
        let (targets, unmatched) = expand_toctree(&toctree, "index.rst", &universe);

        // Then
        assert!(unmatched.is_empty());
        assert_eq!(
            resolved_docnames(&targets),
            vec!["api/client.rst", "api/server.rst"]
        );
    }

    #[test]
    fn test_expand_toctree_resolves_a_glob_relative_to_the_referencing_document() {
        // Given — a pattern written in a nested document.
        let toctree = toctree(vec![TocEntry::Glob {
            pattern: "*".to_string(),
            span: None,
        }]);
        let universe = universe(&["guide/index.rst", "guide/setup.rst", "intro.rst"]);

        // When
        let (targets, _) = expand_toctree(&toctree, "guide/index.rst", &universe);

        // Then — matches inside `guide/`, and not the owner or the top level.
        assert_eq!(resolved_docnames(&targets), vec!["guide/setup.rst"]);
    }

    #[test]
    fn test_expand_toctree_reports_a_glob_that_matched_nothing() {
        // Given
        let toctree = toctree(vec![TocEntry::Glob {
            pattern: "nope/*".to_string(),
            span: None,
        }]);

        // When
        let (targets, unmatched) = expand_toctree(&toctree, "index.rst", &universe(&["intro.rst"]));

        // Then
        assert!(targets.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].kind, UnmatchedKind::GlobMatchedNothing);
        assert_eq!(unmatched[0].text, "nope/*");
    }

    #[test]
    fn test_expand_toctree_reverses_after_expanding() {
        // Given — `:reversed:` over a glob plus a plain entry.
        let mut toctree = toctree(vec![
            TocEntry::Glob {
                pattern: "api/*".to_string(),
                span: None,
            },
            document("intro"),
        ]);
        toctree.options.set(ToctreeFlag::Reversed);
        let universe = universe(&["api/client.rst", "api/server.rst", "intro.rst"]);

        // When
        let (targets, _) = expand_toctree(&toctree, "index.rst", &universe);

        // Then — the glob's own matches are reversed too, not just its slot.
        assert_eq!(
            resolved_docnames(&targets),
            vec!["intro.rst", "api/server.rst", "api/client.rst"]
        );
    }

    #[test]
    fn test_expand_toctree_keeps_written_order_without_reversed() {
        // Given
        let toctree = toctree(vec![document("b"), document("a")]);

        // When
        let (targets, _) = expand_toctree(&toctree, "index.rst", &universe(&["a.rst", "b.rst"]));

        // Then
        assert_eq!(resolved_docnames(&targets), vec!["b.rst", "a.rst"]);
    }

    #[test]
    fn test_expand_toctree_keeps_an_explicit_title() {
        // Given
        let toctree = toctree(vec![TocEntry::Document {
            title: Some("Getting started".to_string()),
            docname: "intro".to_string(),
            span: None,
        }]);

        // When
        let (targets, _) = expand_toctree(&toctree, "index.rst", &universe(&["intro.rst"]));

        // Then
        assert_eq!(
            targets[0],
            TocTarget::Document {
                docname: "intro.rst".to_string(),
                title: Some("Getting started".to_string()),
                span: None,
            }
        );
    }

    #[test]
    fn test_strip_rst_removes_the_extension() {
        // Given / When / Then
        assert_eq!(strip_rst("api/client.rst"), "api/client");
        assert_eq!(strip_rst("api/client"), "api/client");
    }
}

//! Which libraries a toctree entry outside `deps` needs (roadmap #21).
//!
//! The build fails on a toctree entry naming a document of no library in the
//! writing library's `deps` (`toctree.undeclared-dependency`), and on a glob
//! matching none (`toctree.glob-matches-no-dependency`). Both findings carry
//! what the entry resolved to as an [`UndeclaredEntry`]; this module turns
//! that into the labels whose addition to `deps` fixes it — or says why no
//! such edit would, since an offered fix the build then refuses is worse
//! than none.

use std::collections::{BTreeMap, BTreeSet};

use rinx_ast::UndeclaredEntry;
use rinx_toctree::{matching_docnames, strip_rst};

use super::model::LibraryModel;

/// Why no `deps` edit fixes an undeclared toctree entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoFix {
    /// No library of the site lists what the entry names, so no dependency
    /// declares it; the fold reports the missing document itself.
    ListedByNoLibrary,
    /// Adding the library would close a cycle: it already depends, through
    /// its own `deps`, on the library writing the entry.
    Cycle { library: String },
    /// The library writing the entry is in another repository, whose build
    /// file is not this workspace's to edit.
    ExternalLibrary,
}

/// The labels to add to the `deps` of `libraries[from]`, which writes the
/// document `doc_path` holding a toctree entry `needed` its `deps` do not
/// declare.
///
/// A document needs the one library of the site listing it, the first by
/// its order in the site when two do — or, when none does, the library
/// `outside` names for it: one no library of the site depends on yet, which
/// the site therefore does not collect. A glob needs every library of the
/// site listing a document it matches, and the edit adds them all, as one
/// entry needs them all.
///
/// A library from `outside` is not checked for a cycle: its own `deps` are
/// in no manifest, and the build reports a cycle it would close.
///
/// # Errors
///
/// [`NoFix`] when no edit of `deps` would make the build accept the entry.
pub fn missing_deps(
    libraries: &[LibraryModel],
    from: usize,
    doc_path: &str,
    needed: &UndeclaredEntry,
    outside: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<String>, NoFix> {
    let writer = &libraries[from];
    if writer.label.starts_with('@') {
        return Err(NoFix::ExternalLibrary);
    }
    let documents: Vec<String> = match needed {
        UndeclaredEntry::Document(document) => vec![document.clone()],
        UndeclaredEntry::Glob(pattern) => {
            let docnames: BTreeSet<&str> = libraries
                .iter()
                .flat_map(|library| &library.documents)
                .map(|document| strip_rst(document))
                .collect();
            matching_docnames(pattern, docnames, strip_rst(doc_path))
                .into_iter()
                .map(|docname| format!("{docname}.rst"))
                .collect()
        }
    };
    let mut labels = Vec::new();
    let mut collected = Vec::new();
    for document in &documents {
        let label = match libraries
            .iter()
            .find(|library| library.documents.contains(document))
        {
            Some(library) => {
                collected.push(library.label.clone());
                library.label.clone()
            }
            None if matches!(needed, UndeclaredEntry::Document(_)) => {
                let Some(label) = outside(document) else {
                    continue;
                };
                label
            }
            None => continue,
        };
        if label != writer.label && !labels.contains(&label) {
            labels.push(label);
        }
    }
    if labels.is_empty() {
        return Err(NoFix::ListedByNoLibrary);
    }
    let deps: BTreeMap<&str, &[String]> = libraries
        .iter()
        .map(|library| (library.label.as_str(), library.deps.as_slice()))
        .collect();
    if let Some(library) = collected
        .iter()
        .find(|label| reaches(&deps, label, &writer.label))
    {
        return Err(NoFix::Cycle {
            library: library.clone(),
        });
    }
    Ok(labels)
}

/// Whether the library `from` depends on `to`, directly or through its
/// `deps`' — or is it.
fn reaches(deps: &BTreeMap<&str, &[String]>, from: &str, to: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut pending = vec![from];
    while let Some(label) = pending.pop() {
        if label == to {
            return true;
        }
        if seen.insert(label) {
            pending.extend(
                deps.get(label)
                    .into_iter()
                    .flat_map(|direct| direct.iter().map(String::as_str)),
            );
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ParseSettings;

    /// A library of the label `label` listing `documents`, depending on
    /// `deps`.
    fn library(label: &str, documents: &[&str], deps: &[&str]) -> LibraryModel {
        LibraryModel {
            label: label.to_string(),
            deps: deps.iter().map(ToString::to_string).collect(),
            documents: documents.iter().map(ToString::to_string).collect(),
            parse: ParseSettings::default(),
        }
    }

    /// A site whose `//docs:root` writes `docs/index.rst` and depends on
    /// `//docs:guide`, beside `//api:a` and `//api:b`, which do not.
    fn site() -> Vec<LibraryModel> {
        vec![
            library("//docs:root", &["docs/index.rst"], &["//docs:guide"]),
            library("//docs:guide", &["docs/guide.rst"], &[]),
            library("//api:a", &["api/a.rst"], &[]),
            library("//api:b", &["api/b.rst", "api/c.rst"], &["//api:a"]),
        ]
    }

    /// A lookup finding no library outside the site.
    fn none(_: &str) -> Option<String> {
        None
    }

    fn document(name: &str) -> UndeclaredEntry {
        UndeclaredEntry::Document(name.to_string())
    }

    #[test]
    fn test_a_document_needs_the_library_listing_it() {
        // Given / When
        let labels = missing_deps(&site(), 0, "docs/index.rst", &document("api/a.rst"), &none);

        // Then
        assert_eq!(labels, Ok(vec!["//api:a".to_string()]));
    }

    #[test]
    fn test_a_glob_needs_every_library_listing_a_match_once() {
        // Given / When
        let labels = missing_deps(
            &site(),
            0,
            "docs/index.rst",
            &UndeclaredEntry::Glob("api/*".to_string()),
            &none,
        );

        // Then
        assert_eq!(
            labels,
            Ok(vec!["//api:a".to_string(), "//api:b".to_string()])
        );
    }

    #[test]
    fn test_a_glob_does_not_match_the_document_writing_it() {
        // Given — `docs/*` matches only `docs/index.rst`, and a glob never
        // matches the document writing it
        let libraries = vec![
            library("//docs:root", &["docs/index.rst"], &[]),
            library("//other:docs", &["other/index.rst"], &[]),
        ];

        // When
        let labels = missing_deps(
            &libraries,
            0,
            "docs/index.rst",
            &UndeclaredEntry::Glob("docs/*".to_string()),
            &none,
        );

        // Then
        assert_eq!(labels, Err(NoFix::ListedByNoLibrary));
    }

    #[test]
    fn test_a_document_no_library_lists_has_no_fix() {
        // Given / When
        let labels = missing_deps(&site(), 0, "docs/index.rst", &document("ghost.rst"), &none);

        // Then
        assert_eq!(labels, Err(NoFix::ListedByNoLibrary));
    }

    #[test]
    fn test_a_document_no_library_of_the_site_lists_needs_the_one_outside() {
        // Given — a library the site does not collect lists `team/index.rst`
        let outside =
            |document: &str| (document == "team/index.rst").then(|| "//team:docs".to_string());

        // When
        let labels = missing_deps(
            &site(),
            0,
            "docs/index.rst",
            &document("team/index.rst"),
            &outside,
        );

        // Then
        assert_eq!(labels, Ok(vec!["//team:docs".to_string()]));
    }

    #[test]
    fn test_a_glob_is_never_answered_from_outside_the_site() {
        // Given
        let outside = |_: &str| Some("//team:docs".to_string());

        // When
        let labels = missing_deps(
            &site(),
            0,
            "docs/index.rst",
            &UndeclaredEntry::Glob("team/*".to_string()),
            &outside,
        );

        // Then
        assert_eq!(labels, Err(NoFix::ListedByNoLibrary));
    }

    #[test]
    fn test_a_library_depending_on_the_writer_would_close_a_cycle() {
        // Given — `//api:b` reaches `//docs:root` through `//api:a`
        let mut libraries = site();
        libraries[2].deps.push("//docs:root".to_string());

        // When
        let labels = missing_deps(
            &libraries,
            0,
            "docs/index.rst",
            &document("api/b.rst"),
            &none,
        );

        // Then
        assert_eq!(
            labels,
            Err(NoFix::Cycle {
                library: "//api:b".to_string()
            })
        );
    }

    #[test]
    fn test_a_library_of_another_repository_is_not_edited() {
        // Given
        let mut libraries = site();
        libraries[0].label = "@other//docs:root".to_string();

        // When
        let labels = missing_deps(
            &libraries,
            0,
            "docs/index.rst",
            &document("api/a.rst"),
            &none,
        );

        // Then
        assert_eq!(labels, Err(NoFix::ExternalLibrary));
    }

    #[test]
    fn test_reaches_follows_deps_transitively_and_stops_on_a_cycle() {
        // Given
        let a = vec!["//b".to_string()];
        let b = vec!["//c".to_string(), "//a".to_string()];
        let deps = BTreeMap::from([("//a", a.as_slice()), ("//b", b.as_slice())]);

        // When / Then
        assert!(reaches(&deps, "//a", "//c"));
        assert!(reaches(&deps, "//a", "//a"));
        assert!(!reaches(&deps, "//c", "//a"));
        assert!(!reaches(&deps, "//a", "//d"));
    }
}

//! The `glob` a migrated library's `srcs` is written as.
//!
//! Sphinx decides which files are documents by `exclude_patterns`,
//! `include_patterns` and `source_suffix`; Bazel by a glob over the package.
//! Translating one pattern language into the other is lossy (Bazel's glob
//! has no `[...]`, and Sphinx's patterns exclude whole directories), so the
//! glob is not translated but *measured*: it matches every `.rst` file of
//! the package, and excludes exactly those the project does not take as
//! documents — a directory holding nothing but such files as one `dir/**`.

use std::collections::BTreeSet;

/// The pattern a migrated library's `srcs` globs for.
pub(crate) const SRCS_PATTERN: &str = "**/*.rst";

/// What `glob([SRCS_PATTERN], exclude = …)` must exclude so that, of the
/// `candidates` it would match, exactly the `documents` are left — each a
/// `/`-separated path relative to the package.
///
/// A document that is no candidate is not this function's to report; the
/// glob cannot list it whatever it excludes.
#[must_use]
pub(crate) fn srcs_excludes(
    candidates: &BTreeSet<String>,
    documents: &BTreeSet<String>,
) -> Vec<String> {
    let mut excludes = BTreeSet::new();
    for excluded in candidates.difference(documents) {
        excludes.insert(widest_exclusion(excluded, documents));
    }
    excludes.into_iter().collect()
}

/// The widest pattern excluding `file` that excludes no document: its
/// outermost directory holding none, as `dir/**`, else the file itself.
fn widest_exclusion(file: &str, documents: &BTreeSet<String>) -> String {
    let components: Vec<&str> = file.split('/').collect();
    let directories = &components[..components.len().saturating_sub(1)];
    let mut directory = String::new();
    for component in directories {
        if !directory.is_empty() {
            directory.push('/');
        }
        directory.push_str(component);
        let prefix = format!("{directory}/");
        if !documents
            .iter()
            .any(|document| document.starts_with(&prefix))
        {
            return format!("{directory}/**");
        }
    }
    file.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|path| (*path).to_string()).collect()
    }

    #[test]
    fn test_srcs_excludes_nothing_when_every_candidate_is_a_document() {
        // Given
        let files = set(&["index.rst", "guide/setup.rst"]);

        // When / Then
        assert!(srcs_excludes(&files, &files).is_empty());
    }

    #[test]
    fn test_srcs_excludes_a_directory_holding_no_document_once() {
        // Given — CPython's `includes/*.rst`, spliced in rather than published
        let candidates = set(&[
            "index.rst",
            "includes/a.rst",
            "includes/deep/b.rst",
            "library/os.rst",
        ]);
        let documents = set(&["index.rst", "library/os.rst"]);

        // When
        let excludes = srcs_excludes(&candidates, &documents);

        // Then
        assert_eq!(excludes, ["includes/**"]);
    }

    #[test]
    fn test_srcs_excludes_a_file_beside_documents_by_name() {
        // Given
        let candidates = set(&["README.rst", "index.rst", "guide/draft.rst", "guide/a.rst"]);
        let documents = set(&["index.rst", "guide/a.rst"]);

        // When
        let excludes = srcs_excludes(&candidates, &documents);

        // Then
        assert_eq!(excludes, ["README.rst", "guide/draft.rst"]);
    }

    #[test]
    fn test_widest_exclusion_stops_at_the_outermost_directory_without_documents() {
        // Given — `a` holds a document, `a/b` none
        let documents = set(&["a/page.rst"]);

        // When / Then
        assert_eq!(widest_exclusion("a/b/c/x.rst", &documents), "a/b/**");
        assert_eq!(widest_exclusion("a/x.rst", &documents), "a/x.rst");
        assert_eq!(widest_exclusion("x.rst", &documents), "x.rst");
    }
}

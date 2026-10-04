//! The names a `:doc:` completion writes for a document.
//!
//! The inverse of `rinx_toctree::resolve_docname`, the one rule `:doc:` is
//! read by: whatever name is written here must resolve back to the document
//! it was made for, which the property tests below check.

use std::path::{Component, Path};

use rinx_toctree::strip_rst;

/// The name the document at `doc_path` reads the document at `target` by:
/// relative to the directory of `doc_path`, without the `.rst` suffix, with
/// `..` to climb out of a directory.
#[must_use]
pub fn relative_doc_name(doc_path: &str, target: &str) -> String {
    let directory = Path::new(doc_path).parent().unwrap_or(Path::new(""));
    let target = Path::new(written_stem(target));
    let relative = pathdiff::diff_paths(target, directory).unwrap_or_else(|| target.into());
    relative
        .components()
        .map(|component| match component {
            Component::ParentDir => "..".into(),
            other => other.as_os_str().to_string_lossy(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The name of the document at `target` from the source root: `/` and its
/// path without the `.rst` suffix.
#[must_use]
pub fn absolute_doc_name(target: &str) -> String {
    format!("/{}", written_stem(target))
}

/// The document path `target` as a name is written: without its `.rst`
/// suffix — unless what is left still ends in `.rst`, which a name would
/// then be read as already carrying.
fn written_stem(target: &str) -> &str {
    let stem = strip_rst(target);
    let ambiguous = Path::new(stem)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rst"));
    if ambiguous { target } else { stem }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rinx_toctree::resolve_docname;

    #[test]
    fn test_relative_doc_name_names_a_sibling_by_its_stem() {
        // When / Then
        assert_eq!(
            relative_doc_name("guide/intro.rst", "guide/setup.rst"),
            "setup"
        );
        assert_eq!(relative_doc_name("index.rst", "about.rst"), "about");
    }

    #[test]
    fn test_relative_doc_name_descends_into_a_directory() {
        // When / Then
        assert_eq!(
            relative_doc_name("index.rst", "guide/setup.rst"),
            "guide/setup"
        );
    }

    #[test]
    fn test_relative_doc_name_climbs_out_of_a_directory() {
        // When / Then
        assert_eq!(
            relative_doc_name("guide/deep/intro.rst", "api/index.rst"),
            "../../api/index"
        );
    }

    #[test]
    fn test_absolute_doc_name_starts_at_the_source_root() {
        // When / Then
        assert_eq!(absolute_doc_name("guide/setup.rst"), "/guide/setup");
    }

    #[test]
    fn test_written_stem_keeps_a_suffix_the_stem_would_be_read_as() {
        // When / Then
        assert_eq!(written_stem("guide/setup.rst"), "guide/setup");
        assert_eq!(written_stem("notes.rst.rst"), "notes.rst.rst");
    }

    /// A document path of one to three short, possibly dotted, segments.
    fn doc_paths() -> impl Strategy<Value = String> {
        prop::collection::vec("[a-z][a-z0-9_.-]{0,4}", 1..4)
            .prop_filter("no `.` or `..` segment", |segments| {
                segments.iter().all(|segment| !segment.ends_with('.'))
            })
            .prop_map(|segments| format!("{}.rst", segments.join("/")))
    }

    proptest! {
        #[test]
        fn test_relative_doc_name_resolves_back_to_its_document(
            doc_path in doc_paths(),
            target in doc_paths(),
        ) {
            // When
            let name = relative_doc_name(&doc_path, &target);

            // Then
            prop_assert_eq!(resolve_docname(&doc_path, &name), target);
        }

        #[test]
        fn test_absolute_doc_name_resolves_back_to_its_document(
            doc_path in doc_paths(),
            target in doc_paths(),
        ) {
            // When
            let name = absolute_doc_name(&target);

            // Then
            prop_assert_eq!(resolve_docname(&doc_path, &name), target);
        }
    }
}

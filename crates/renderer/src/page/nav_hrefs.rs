//! Sidebar nav-href resolution: turning a nav tree's `.rst` paths into
//! relative `.html` hrefs for the page being rendered.

/// A nav entry with its path resolved to a relative HTML href.
///
/// `href` is a computed, build-controlled relative path rather than
/// document-authored text, so it's wrapped as a `MiniJinja` safe string to
/// avoid needless (if harmless) entity-escaping of its `/` separators.
#[derive(Debug, serde::Serialize)]
pub(super) struct ResolvedNavEntry<'a> {
    title: &'a str,
    href: minijinja::Value,
    children: Vec<ResolvedNavEntry<'a>>,
}

/// Converts a nav tree's `.rst` paths into relative `.html` hrefs
/// based on the current document's location.
pub(super) fn resolve_nav_hrefs<'a>(
    entries: &'a [rusty_sphinx_index::NavEntry],
    doc_path: &str,
) -> Vec<ResolvedNavEntry<'a>> {
    let doc_dir = std::path::Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    entries
        .iter()
        .map(|entry| {
            let path_without_ext = entry.path.strip_suffix(".rst").unwrap_or(&entry.path);
            let target_html = format!("{path_without_ext}.html");
            let target = std::path::Path::new(&target_html);
            let href = pathdiff::diff_paths(target, doc_dir)
                .map_or_else(|| target_html, |p| p.to_string_lossy().replace('\\', "/"));

            ResolvedNavEntry {
                title: &entry.title,
                href: minijinja::Value::from_safe_string(href),
                children: resolve_nav_hrefs(&entry.children, doc_path),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_nav_hrefs_same_directory() {
        // Given
        let entries = vec![rusty_sphinx_index::NavEntry {
            title: "Index".to_string(),
            path: "index.rst".to_string(),
            children: vec![],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].title, "Index");
        assert_eq!(resolved[0].href.to_string(), "index.html");
    }

    #[test]
    fn test_resolve_nav_hrefs_document_in_subdirectory() {
        // Given
        let entries = vec![rusty_sphinx_index::NavEntry {
            title: "Index".to_string(),
            path: "index.rst".to_string(),
            children: vec![],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "sub/doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].href.to_string(), "../index.html");
    }

    #[test]
    fn test_resolve_nav_hrefs_target_in_subdirectory() {
        // Given
        let entries = vec![rusty_sphinx_index::NavEntry {
            title: "Nested".to_string(),
            path: "sub/nested.rst".to_string(),
            children: vec![],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].href.to_string(), "sub/nested.html");
    }

    #[test]
    fn test_resolve_nav_hrefs_nested_entries() {
        // Given
        let entries = vec![rusty_sphinx_index::NavEntry {
            title: "Root".to_string(),
            path: "root.rst".to_string(),
            children: vec![rusty_sphinx_index::NavEntry {
                title: "Child".to_string(),
                path: "sub/child.rst".to_string(),
                children: vec![],
            }],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].href.to_string(), "root.html");
        assert_eq!(resolved[0].children.len(), 1);
        assert_eq!(resolved[0].children[0].href.to_string(), "sub/child.html");
    }
}

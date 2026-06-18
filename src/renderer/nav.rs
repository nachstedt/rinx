//! Navigation tree rendering helpers.

use crate::analyzer::ProjectIndex;
use std::fmt::Write as _;

/// Renders a single nav entry (and its children recursively) as HTML list items.
pub(super) fn render_nav_entry(
    html: &mut String,
    entry: &crate::analyzer::NavEntry,
    index: &ProjectIndex,
    current_dir: &std::path::Path,
    current_depth: usize,
    maxdepth: Option<usize>,
) {
    if maxdepth.is_some_and(|m| current_depth > m) {
        return;
    }

    let link_text = index
        .document_titles
        .get(&entry.path)
        .unwrap_or(&entry.path);

    let target_html = std::path::PathBuf::from(&entry.path).with_extension("html");
    let relative_path = pathdiff::diff_paths(&target_html, current_dir).unwrap_or(target_html);

    let href = format!("{}", relative_path.display());
    let escaped_text = html_escape::encode_text(link_text);

    let href_attr = html_escape::encode_double_quoted_attribute(&href);
    let _ = write!(html, "  <li><a href=\"{href_attr}\">{escaped_text}</a>");

    if !entry.children.is_empty() && maxdepth.is_none_or(|m| current_depth < m) {
        let _ = writeln!(html, "\n<ul>");
        for child in &entry.children {
            render_nav_entry(html, child, index, current_dir, current_depth + 1, maxdepth);
        }
        let _ = write!(html, "</ul>\n  ");
    }
    let _ = writeln!(html, "</li>");
}

/// Searches a nav tree recursively for an entry matching `path`.
pub(super) fn find_nav_entry<'a>(
    entries: &'a [crate::analyzer::NavEntry],
    path: &str,
) -> Option<&'a crate::analyzer::NavEntry> {
    for entry in entries {
        if entry.path == path {
            return Some(entry);
        }
        if let Some(found) = find_nav_entry(&entry.children, path) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::ProjectIndex;

    fn make_entry(
        path: &str,
        children: Vec<crate::analyzer::NavEntry>,
    ) -> crate::analyzer::NavEntry {
        crate::analyzer::NavEntry {
            title: path.to_string(),
            path: path.to_string(),
            children,
        }
    }

    #[test]
    fn test_render_nav_entry_renders_leaf_as_list_item() {
        // Given
        let entry = make_entry("page.rst", vec![]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("page.rst".to_string(), "My Page".to_string());
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then
        assert_eq!(html, "  <li><a href=\"page.html\">My Page</a></li>\n");
    }

    #[test]
    fn test_render_nav_entry_falls_back_to_path_when_no_title() {
        // Given — entry has no title in document_titles
        let entry = make_entry("section/page.rst", vec![]);
        let index = ProjectIndex::default(); // no titles
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then — path is used as fallback link text
        assert!(
            html.contains("section/page.rst"),
            "path should be used as fallback title"
        );
        assert!(
            html.contains("section/page.html"),
            "href should point to html"
        );
    }

    #[test]
    fn test_render_nav_entry_renders_children_as_nested_list() {
        // Given
        let child = make_entry("child.rst", vec![]);
        let entry = make_entry("parent.rst", vec![child]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("parent.rst".to_string(), "Parent".to_string());
        index
            .document_titles
            .insert("child.rst".to_string(), "Child".to_string());
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then
        assert!(html.contains("<ul>"), "nested list should be opened");
        assert!(html.contains("Child"), "child link text should be present");
        assert!(html.contains("child.html"), "child href should be present");
    }

    #[test]
    fn test_render_nav_entry_suppresses_children_when_maxdepth_reached() {
        // Given — maxdepth: 1, rendering at depth 1 means children must not appear
        let child = make_entry("child.rst", vec![]);
        let entry = make_entry("parent.rst", vec![child]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("parent.rst".to_string(), "Parent".to_string());
        index
            .document_titles
            .insert("child.rst".to_string(), "Child".to_string());
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When — rendered at depth 1 with maxdepth 1, children should not appear
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, Some(1));

        // Then
        assert!(html.contains("Parent"));
        assert!(
            !html.contains("Child"),
            "child must be suppressed when depth == maxdepth"
        );
        assert!(
            !html.contains("<ul>"),
            "nested list must not be emitted when depth == maxdepth"
        );
    }

    #[test]
    fn test_render_nav_entry_skips_entirely_when_depth_exceeds_maxdepth() {
        // Given — called at depth 2 with maxdepth 1 (already exceeded)
        let entry = make_entry("page.rst", vec![]);
        let index = ProjectIndex::default();
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 2, Some(1));

        // Then — nothing rendered
        assert!(
            html.is_empty(),
            "nothing should be rendered when depth > maxdepth"
        );
    }

    #[test]
    fn test_render_nav_entry_computes_relative_path_from_subdirectory() {
        // Given — the current document is inside a subdirectory
        let entry = make_entry("page.rst", vec![]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("page.rst".to_string(), "Top Page".to_string());
        let current_dir = std::path::Path::new("sub/dir");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then — href should be relative (../../page.html)
        assert!(
            html.contains("../../page.html"),
            "href must be relative to current_dir"
        );
    }

    #[test]
    fn test_find_nav_entry_returns_none_for_empty_tree() {
        // Given / When / Then
        assert!(find_nav_entry(&[], "a.rst").is_none());
    }

    #[test]
    fn test_find_nav_entry_finds_root_level_entry() {
        // Given
        let tree = vec![make_entry("a.rst", vec![]), make_entry("b.rst", vec![])];

        // When
        let result = find_nav_entry(&tree, "b.rst");

        // Then
        assert!(result.is_some());
        assert_eq!(result.unwrap().path, "b.rst");
    }

    #[test]
    fn test_find_nav_entry_finds_deeply_nested_entry() {
        // Given
        let tree = vec![make_entry(
            "root.rst",
            vec![make_entry(
                "child.rst",
                vec![make_entry("grandchild.rst", vec![])],
            )],
        )];

        // When
        let result = find_nav_entry(&tree, "grandchild.rst");

        // Then
        assert!(result.is_some());
        assert_eq!(result.unwrap().path, "grandchild.rst");
    }

    #[test]
    fn test_find_nav_entry_returns_none_for_missing_path() {
        // Given
        let tree = vec![make_entry("a.rst", vec![make_entry("b.rst", vec![])])];

        // When / Then
        assert!(find_nav_entry(&tree, "missing.rst").is_none());
    }
}

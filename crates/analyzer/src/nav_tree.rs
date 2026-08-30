use rusty_sphinx_ast::{Directive, Document, Node};
use rusty_sphinx_index::NavEntry;

use super::path_normalization::normalize_path;

/// Extracts toctree entries from a document, resolved to absolute paths.
pub(super) fn extract_toctree_paths(doc: &Document) -> Vec<String> {
    let doc_parent = std::path::Path::new(&doc.path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    let mut paths = Vec::new();
    for node in &doc.nodes {
        if let Node::Directive(Directive::Toctree { paths: entries, .. }) = node {
            for entry in entries {
                let combined = doc_parent.join(entry);
                let normalized = normalize_path(&combined);
                let mut path_str = normalized.to_string_lossy().replace('\\', "/");
                if !std::path::Path::new(&path_str)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rst"))
                {
                    path_str.push_str(".rst");
                }
                paths.push(path_str);
            }
        }
    }
    paths
}

/// Normalizes a path, resolving `.` and `..` components.
/// Recursively builds a navigation tree for a given document.
///
/// `visited` tracks the current ancestor chain to detect and break cycles:
/// a document already in the chain cannot be its own descendant.
/// It is restored after each recursive call so that the same document
/// can legitimately appear in different branches of the tree.
pub(super) fn build_nav_subtree(
    doc_path: &str,
    toctrees: &BTreeMap<String, Vec<String>>,
    titles: &BTreeMap<String, String>,
    visited: &mut std::collections::HashSet<String>,
) -> NavEntry {
    let title = titles.get(doc_path).cloned().unwrap_or_else(|| {
        doc_path
            .strip_suffix(".rst")
            .unwrap_or(doc_path)
            .to_string()
    });

    // Cycle detected: this path is already an ancestor — emit a leaf.
    if !visited.insert(doc_path.to_string()) {
        return NavEntry {
            title,
            path: doc_path.to_string(),
            children: Vec::new(),
        };
    }

    let children = toctrees
        .get(doc_path)
        .map(|child_paths| {
            child_paths
                .iter()
                .map(|child| build_nav_subtree(child, toctrees, titles, visited))
                .collect()
        })
        .unwrap_or_default();

    // Remove from visited so sibling branches can visit this document.
    visited.remove(doc_path);

    NavEntry {
        title,
        path: doc_path.to_string(),
        children,
    }
}

use std::collections::BTreeMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_nav_subtree_basic() {
        let mut toctrees = BTreeMap::new();
        toctrees.insert("index.rst".to_string(), vec!["child.rst".to_string()]);

        let mut titles = BTreeMap::new();
        titles.insert("index.rst".to_string(), "Index Title".to_string());
        titles.insert("child.rst".to_string(), "Child Title".to_string());

        let mut visited = std::collections::HashSet::new();
        let nav = build_nav_subtree("index.rst", &toctrees, &titles, &mut visited);

        assert_eq!(nav.title, "Index Title");
        assert_eq!(nav.path, "index.rst");
        assert_eq!(nav.children.len(), 1);
        assert_eq!(nav.children[0].title, "Child Title");
        assert_eq!(nav.children[0].path, "child.rst");
        assert!(nav.children[0].children.is_empty());
    }
    #[test]
    fn test_build_nav_subtree_missing_title() {
        let toctrees = BTreeMap::new();
        let titles = BTreeMap::new();
        let mut visited = std::collections::HashSet::new();

        let nav = build_nav_subtree("untitled.rst", &toctrees, &titles, &mut visited);

        assert_eq!(nav.title, "untitled");
        assert_eq!(nav.path, "untitled.rst");
        assert!(nav.children.is_empty());
    }
    #[test]
    fn test_build_nav_subtree_cycle_prevention() {
        let mut toctrees = BTreeMap::new();
        // index -> child -> index
        toctrees.insert("index.rst".to_string(), vec!["child.rst".to_string()]);
        toctrees.insert("child.rst".to_string(), vec!["index.rst".to_string()]);

        let titles = BTreeMap::new();
        let mut visited = std::collections::HashSet::new();

        let nav = build_nav_subtree("index.rst", &toctrees, &titles, &mut visited);

        assert_eq!(nav.title, "index");
        assert_eq!(nav.path, "index.rst");
        assert_eq!(nav.children.len(), 1);

        let child = &nav.children[0];
        assert_eq!(child.title, "child");
        assert_eq!(child.path, "child.rst");
        assert_eq!(child.children.len(), 1);

        // Cycle broken here
        let cycle_leaf = &child.children[0];
        assert_eq!(cycle_leaf.title, "index");
        assert_eq!(cycle_leaf.path, "index.rst");
        assert!(cycle_leaf.children.is_empty());
    }
    #[test]
    fn test_extract_toctree_paths_empty_document() {
        let doc = Document::new("index.rst".to_string(), vec![]);
        let paths = extract_toctree_paths(&doc);
        assert!(paths.is_empty());
    }
    #[test]
    fn test_extract_toctree_paths_single_entry_appends_rst() {
        let doc = Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["chapter1".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })],
        );
        let paths = extract_toctree_paths(&doc);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "chapter1.rst");
    }
    #[test]
    fn test_extract_toctree_paths_multiple_entries_mixed_extensions() {
        let doc = Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec![
                    "chapter1".to_string(),
                    "chapter2.rst".to_string(),
                    "chapter3.RST".to_string(),
                ],
                maxdepth: None,
                ignored_options: vec![],
            })],
        );
        let paths = extract_toctree_paths(&doc);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], "chapter1.rst");
        assert_eq!(paths[1], "chapter2.rst");
        assert_eq!(paths[2], "chapter3.RST");
    }
    #[test]
    fn test_extract_toctree_paths_resolves_relative_to_parent() {
        let doc = Document::new(
            "docs/index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec![
                    "chapter1".to_string(),
                    "../readme".to_string(),
                    "sub/chapter2.rst".to_string(),
                ],
                maxdepth: None,
                ignored_options: vec![],
            })],
        );
        let paths = extract_toctree_paths(&doc);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], "docs/chapter1.rst");
        assert_eq!(paths[1], "readme.rst");
        assert_eq!(paths[2], "docs/sub/chapter2.rst");
    }
}

//! The analyzer module represents the global indexing phase.
//!
//! It builds a `ProjectIndex` — a global symbol table containing cross-reference
//! targets, document titles, and a hierarchical navigation tree derived from
//! toctree directives.

use crate::ast::{Directive, Document, Node};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

/// One node in the navigation tree, matching Sphinx's sidebar nesting behavior.
///
/// Each entry corresponds to a document and may have children derived from
/// its toctree directive. The hierarchy mirrors how `.. toctree::` directives
/// link documents together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavEntry {
    /// The display title (from the document's H1 heading, or the path if untitled).
    pub title: String,
    /// The `.rst` path (used to compute relative HTML links).
    pub path: String,
    /// Child entries from this document's toctree directive.
    pub children: Vec<NavEntry>,
}

/// A global symbol table built from all documents in the project.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ProjectIndex {
    /// Maps target names to document paths.
    pub targets: HashMap<String, String>,
    /// Maps document paths to their top-level title.
    pub document_titles: HashMap<String, String>,
    /// Hierarchical navigation tree derived from toctree directives.
    #[serde(default)]
    pub nav_tree: Vec<NavEntry>,
}

impl ProjectIndex {
    /// Merge another `ProjectIndex` into this one.
    pub fn merge(&mut self, other: ProjectIndex) {
        self.targets.extend(other.targets);
        self.document_titles.extend(other.document_titles);
        // nav_tree is built globally, not merged per-document
    }
}

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
///
/// This extracts targets and document titles. The `nav_tree` is not populated
/// here — it is built globally by [`build_project_index()`].
#[must_use]
pub fn analyze(doc: &Document) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    let mut found_title = false;
    for node in &doc.nodes {
        if let Node::Target(name) = node {
            index.targets.insert(name.clone(), doc.path.clone());
        }
        if !found_title && let Node::Heading { level: 1, text } = node {
            index.document_titles.insert(doc.path.clone(), text.clone());
            found_title = true;
        }
    }
    index
}

/// Extracts toctree entries from a document, resolved to absolute paths.
fn extract_toctree_paths(doc: &Document) -> Vec<String> {
    let doc_parent = std::path::Path::new(&doc.path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    let mut paths = Vec::new();
    for node in &doc.nodes {
        if let Node::Directive(Directive::Toctree { paths: entries }) = node {
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
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            _ => normalized.push(component),
        }
    }
    normalized
}

/// Recursively builds a navigation tree for a given document.
fn build_nav_subtree(
    doc_path: &str,
    toctrees: &HashMap<String, Vec<String>>,
    titles: &HashMap<String, String>,
) -> NavEntry {
    let title = titles.get(doc_path).cloned().unwrap_or_else(|| {
        doc_path
            .strip_suffix(".rst")
            .unwrap_or(doc_path)
            .to_string()
    });

    let children = toctrees
        .get(doc_path)
        .map(|child_paths| {
            child_paths
                .iter()
                .map(|child| build_nav_subtree(child, toctrees, titles))
                .collect()
        })
        .unwrap_or_default();

    NavEntry {
        title,
        path: doc_path.to_string(),
        children,
    }
}

/// Analyzes a collection of `Document`s and builds a complete `ProjectIndex`
/// including the hierarchical navigation tree.
///
/// The nav tree is rooted at documents that are not referenced as children
/// by any other document's toctree — these are the top-level root documents.
#[must_use]
pub fn build_project_index(docs: &[Document]) -> ProjectIndex {
    // Step 1: Build per-document index (targets, titles)
    let mut index = ProjectIndex::default();
    for doc in docs {
        index.merge(analyze(doc));
    }

    // Step 2: Collect toctree relationships (parent path → child paths)
    let mut toctrees: HashMap<String, Vec<String>> = HashMap::new();
    for doc in docs {
        let children = extract_toctree_paths(doc);
        if !children.is_empty() {
            toctrees.insert(doc.path.clone(), children);
        }
    }

    // Step 3: Find root documents (those not referenced as a child by anyone)
    let all_children: std::collections::HashSet<&str> = toctrees
        .values()
        .flat_map(|children| children.iter().map(String::as_str))
        .collect();

    let mut roots: Vec<&str> = docs
        .iter()
        .map(|d| d.path.as_str())
        .filter(|p| !all_children.contains(p))
        .collect();
    roots.sort_unstable();

    // Step 4: Build nav tree recursively from roots
    index.nav_tree = roots
        .iter()
        .map(|root| build_nav_subtree(root, &toctrees, &index.document_titles))
        .collect();

    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{InlineNode, Node};

    #[test]
    fn test_analyze_returns_default_index_for_empty_document() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![]);

        // When
        let index = analyze(&doc);

        // Then
        let _ = format!("{index:?}"); // Ensures it doesn't panic
    }

    #[test]
    fn test_analyze_returns_default_index_for_populated_document() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: "Title".to_string(),
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        // Currently analyze does not populate anything, but it shouldn't panic
        let _ = format!("{index:?}");
    }

    #[test]
    fn test_build_project_index_returns_default_for_multiple_documents() {
        // Given
        let docs = vec![
            Document::new("test1.rst".to_string(), vec![]),
            Document::new("test2.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs);

        // Then
        let _ = format!("{index:?}");
    }

    #[test]
    fn test_merge_combines_indices_without_error() {
        // Given
        let mut idx1 = ProjectIndex::default();
        let idx2 = ProjectIndex::default();

        // When
        idx1.merge(idx2);

        // Then
        // Since we don't have fields to assert equality on right now,
        // we just ensure the execution path is hit without issues.
        let _ = format!("{idx1:?}");
    }
    #[test]
    fn test_analyze_populates_targets_for_target_nodes() {
        // Given
        let doc = Document::new(
            "docs/my-file.rst".to_string(),
            vec![
                Node::Target("section-1".to_string()),
                Node::Paragraph(vec![InlineNode::Text("some text".to_string())]),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.targets.len(), 1);
        assert_eq!(index.targets.get("section-1").unwrap(), "docs/my-file.rst");
    }

    #[test]
    fn test_analyze_extracts_h1_title() {
        // Given
        let doc = Document::new(
            "docs/my-file.rst".to_string(),
            vec![
                Node::Paragraph(vec![InlineNode::Text("some text".to_string())]),
                Node::Heading {
                    level: 1,
                    text: "My Title".to_string(),
                },
                Node::Heading {
                    level: 1,
                    text: "Ignored Second H1".to_string(),
                },
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.document_titles.len(), 1);
        assert_eq!(
            index.document_titles.get("docs/my-file.rst").unwrap(),
            "My Title"
        );
    }

    #[test]
    fn test_build_project_index_creates_flat_nav_for_single_document() {
        // Given — a single document with no toctree
        let docs = vec![Document::new(
            "index.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: "Root".to_string(),
            }],
        )];

        // When
        let index = build_project_index(&docs);

        // Then — one root entry, no children
        assert_eq!(index.nav_tree.len(), 1);
        assert_eq!(index.nav_tree[0].title, "Root");
        assert_eq!(index.nav_tree[0].path, "index.rst");
        assert!(index.nav_tree[0].children.is_empty());
    }

    #[test]
    fn test_build_project_index_creates_nested_nav_from_toctree() {
        // Given — root references two children via toctree
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![
                    Node::Heading {
                        level: 1,
                        text: "Home".to_string(),
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                    }),
                ],
            ),
            Document::new(
                "team_a/index.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: "Team A".to_string(),
                }],
            ),
            Document::new(
                "team_b/index.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: "Team B".to_string(),
                }],
            ),
        ];

        // When
        let index = build_project_index(&docs);

        // Then — one root with two children
        assert_eq!(index.nav_tree.len(), 1);
        let root = &index.nav_tree[0];
        assert_eq!(root.title, "Home");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].title, "Team A");
        assert_eq!(root.children[0].path, "team_a/index.rst");
        assert!(root.children[0].children.is_empty());
        assert_eq!(root.children[1].title, "Team B");
        assert_eq!(root.children[1].path, "team_b/index.rst");
    }

    #[test]
    fn test_build_project_index_uses_path_for_untitled_documents() {
        // Given — child has no H1 heading
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![
                    Node::Heading {
                        level: 1,
                        text: "Home".to_string(),
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["about".to_string()],
                    }),
                ],
            ),
            Document::new(
                "about.rst".to_string(),
                vec![Node::Paragraph(vec![InlineNode::Text(
                    "No heading here.".to_string(),
                )])],
            ),
        ];

        // When
        let index = build_project_index(&docs);

        // Then — child title falls back to path
        assert_eq!(index.nav_tree[0].children[0].title, "about");
    }

    #[test]
    fn test_build_project_index_builds_multi_level_hierarchy() {
        // Given — root → child → grandchild
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![
                    Node::Heading {
                        level: 1,
                        text: "Root".to_string(),
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["section/index".to_string()],
                    }),
                ],
            ),
            Document::new(
                "section/index.rst".to_string(),
                vec![
                    Node::Heading {
                        level: 1,
                        text: "Section".to_string(),
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["sub/page".to_string()],
                    }),
                ],
            ),
            Document::new(
                "section/sub/page.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: "Deep Page".to_string(),
                }],
            ),
        ];

        // When
        let index = build_project_index(&docs);

        // Then — three levels deep
        assert_eq!(index.nav_tree.len(), 1);
        let root = &index.nav_tree[0];
        assert_eq!(root.children.len(), 1);
        let section = &root.children[0];
        assert_eq!(section.title, "Section");
        assert_eq!(section.children.len(), 1);
        assert_eq!(section.children[0].title, "Deep Page");
        assert!(section.children[0].children.is_empty());
    }

    #[test]
    fn test_build_project_index_serializes_nav_tree() {
        // Given — build an index with nav_tree
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![
                    Node::Heading {
                        level: 1,
                        text: "Home".to_string(),
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["child".to_string()],
                    }),
                ],
            ),
            Document::new(
                "child.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: "Child".to_string(),
                }],
            ),
        ];
        let index = build_project_index(&docs);

        // When — serialize and deserialize
        let json = serde_json::to_string(&index).unwrap();
        let deserialized: ProjectIndex = serde_json::from_str(&json).unwrap();

        // Then — round-trips correctly
        assert_eq!(deserialized.nav_tree.len(), 1);
        assert_eq!(deserialized.nav_tree[0].children.len(), 1);
        assert_eq!(deserialized.nav_tree[0].children[0].title, "Child");
    }
}

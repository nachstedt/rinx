//! The analyzer module represents the global indexing phase.
//!
//! It builds a `ProjectIndex` — a global symbol table containing cross-reference
//! targets, document titles, and a hierarchical navigation tree derived from
//! toctree directives.

mod utils;

pub use utils::normalize_path;

use rusty_sphinx_ast::{Directive, Document, Node, TargetName};
use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

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
    pub children: Vec<Self>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetLocation {
    Internal(String), // doc_path
    External(String), // URL
}

/// A global symbol table built from all documents in the project.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIndex {
    /// Maps target names to document paths.
    pub targets: BTreeMap<TargetName, TargetLocation>,
    /// Maps document paths to their top-level title.
    pub document_titles: BTreeMap<String, String>,
    /// Hierarchical navigation tree derived from toctree directives.
    #[serde(default)]
    pub nav_tree: Vec<NavEntry>,
    /// Maps normalized glossary term names to the document path containing their definition.
    #[serde(default)]
    pub glossary_terms: BTreeMap<TargetName, String>,
    /// Maps domain-qualified object keys (e.g. `py:function:foo`) to the
    /// document path containing their `Directive::DomainObject` definition.
    #[serde(default)]
    pub domain_objects: BTreeMap<TargetName, String>,
}

impl ProjectIndex {
    /// Merge another `ProjectIndex` into this one.
    ///
    /// Emits a diagnostic string for each glossary term defined in both indices
    /// (case-insensitive duplicate detection). Last-writer-wins for the mapping value.
    pub fn merge(&mut self, other: Self) -> Vec<String> {
        self.targets.extend(other.targets);
        self.document_titles.extend(other.document_titles);
        self.domain_objects.extend(other.domain_objects);
        // nav_tree is built globally, not merged per-document
        let mut diagnostics = Vec::new();
        for (term, path) in other.glossary_terms {
            if let Some(existing) = self.glossary_terms.get(&term) {
                diagnostics.push(format!(
                    "Duplicate glossary term '{}': defined in '{}' and '{}'. The latter definition wins.",
                    term.as_str(),
                    existing,
                    path,
                ));
            }
            self.glossary_terms.insert(term, path);
        }
        diagnostics
    }
}

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
///
/// This extracts targets, document titles, and glossary terms. The `nav_tree` is not
/// populated here — it is built globally by [`build_project_index()`].
#[must_use]
pub fn analyze(doc: &Document) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    let mut found_title = false;
    for node in &doc.nodes {
        if !found_title && let Node::Heading { level: 1, text } = node {
            index
                .document_titles
                .insert(doc.path.clone(), rusty_sphinx_ast::inline_plain_text(text));
            found_title = true;
        }
    }
    index_nodes(&doc.nodes, &doc.path, &mut index);
    index
}

/// Recursively registers targets, glossary terms, and domain objects found
/// anywhere in `nodes`, including inside table cells, list items, and
/// directive bodies — not just at the document's top level. This mirrors
/// the recursion shape `render_nodes` (in the renderer crate) uses, since a
/// definition nested in a container still needs to be indexed for
/// cross-references to resolve, exactly like it's still rendered with a
/// working anchor.
fn index_nodes(nodes: &[Node], doc_path: &str, index: &mut ProjectIndex) {
    for node in nodes {
        match node {
            Node::Target { name, uri } => {
                let location = uri.as_ref().map_or_else(
                    || TargetLocation::Internal(doc_path.to_string()),
                    |url| TargetLocation::External(url.clone()),
                );
                index.targets.insert(name.clone(), location);
            }
            Node::Directive(Directive::Glossary { entries, .. }) => {
                for entry in entries {
                    for term in &entry.terms {
                        index
                            .glossary_terms
                            .insert(TargetName::new(term), doc_path.to_string());
                    }
                }
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                let key = rusty_sphinx_ast::build_domain_object_key(obj.object_type(), &obj.name());
                index.domain_objects.insert(key, doc_path.to_string());
                index_nodes(obj.body(), doc_path, index);
            }
            Node::Directive(
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                index_nodes(body, doc_path, index);
            }
            Node::BulletList { items, .. } => {
                for item in items {
                    index_nodes(&item.nodes, doc_path, index);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    index_nodes(&item.definition, doc_path, index);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter().chain(body_rows) {
                    for cell in &row.cells {
                        index_nodes(&cell.content, doc_path, index);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extracts toctree entries from a document, resolved to absolute paths.
fn extract_toctree_paths(doc: &Document) -> Vec<String> {
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
fn build_nav_subtree(
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
        let _ = index.merge(analyze(doc));
    }

    // Step 2: Collect toctree relationships (parent path → child paths)
    let mut toctrees: BTreeMap<String, Vec<String>> = BTreeMap::new();
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
    let mut visited = std::collections::HashSet::new();
    index.nav_tree = roots
        .iter()
        .map(|root| build_nav_subtree(root, &toctrees, &index.document_titles, &mut visited))
        .collect();

    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{InlineNode, Node, ObjectType, PyObjectType};

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
    fn test_normalize_path_basic() {
        // Given
        let path = std::path::Path::new("a/b/c");

        // When
        let normalized = normalize_path(path);

        // Then
        assert_eq!(normalized, std::path::PathBuf::from("a/b/c"));
    }

    #[test]
    fn test_normalize_path_current_dir() {
        // Given
        let path1 = std::path::Path::new("a/./c");
        let path2 = std::path::Path::new("./a/b");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a/c"));
        assert_eq!(normalized2, std::path::PathBuf::from("a/b"));
    }

    #[test]
    fn test_normalize_path_parent_dir() {
        // Given
        let path1 = std::path::Path::new("a/b/../c");
        let path2 = std::path::Path::new("a/b/../../c");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a/c"));
        assert_eq!(normalized2, std::path::PathBuf::from("c"));
    }

    #[test]
    fn test_normalize_path_complex() {
        // Given
        let path1 = std::path::Path::new("a/./b/../c/d/./../e");
        let path2 = std::path::Path::new("/a/b/../c");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a/c/e"));
        assert_eq!(normalized2, std::path::PathBuf::from("/a/c"));
    }

    #[test]
    fn test_normalize_path_above_root() {
        // Given
        let path1 = std::path::Path::new("../a");
        let path2 = std::path::Path::new("a/../../b");
        let path3 = std::path::Path::new("/../a");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);
        let normalized3 = normalize_path(path3);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a"));
        assert_eq!(normalized2, std::path::PathBuf::from("b"));
        assert_eq!(normalized3, std::path::PathBuf::from("/a"));
    }

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
                text: vec![InlineNode::Text("Title".to_string())],
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
                Node::Target {
                    name: TargetName::new("section-1"),
                    uri: None,
                },
                Node::Paragraph(vec![InlineNode::Text("some text".to_string())]),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.targets.len(), 1);
        assert_eq!(
            index.targets.get(&TargetName::new("section-1")).unwrap(),
            &TargetLocation::Internal("docs/my-file.rst".to_string())
        );
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
                    text: vec![InlineNode::Text("My Title".to_string())],
                },
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Ignored Second H1".to_string())],
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
    fn test_analyze_extracts_h1_title_as_plain_text_when_heading_has_domain_object_reference() {
        // Given — a heading containing a `~`-shortened domain-object reference
        let doc = Document::new(
            "docs/greetings.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![
                    InlineNode::Text("The ".to_string()),
                    InlineNode::DomainObjectReference {
                        object_type: ObjectType::Py(PyObjectType::Module),
                        name: "pkg.greetings".to_string(),
                        display: "greetings".to_string(),
                        link: true,
                    },
                    InlineNode::Text(" Module".to_string()),
                ],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then — the title is flattened to plain text, using the shortened display
        assert_eq!(
            index.document_titles.get("docs/greetings.rst").unwrap(),
            "The greetings Module"
        );
    }

    #[test]
    fn test_build_project_index_creates_flat_nav_for_single_document() {
        // Given — a single document with no toctree
        let docs = vec![Document::new(
            "index.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Root".to_string())],
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
                        text: vec![InlineNode::Text("Home".to_string())],
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                        maxdepth: None,
                        ignored_options: vec![],
                    }),
                ],
            ),
            Document::new(
                "team_a/index.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Team A".to_string())],
                }],
            ),
            Document::new(
                "team_b/index.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Team B".to_string())],
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
                        text: vec![InlineNode::Text("Home".to_string())],
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["about".to_string()],
                        maxdepth: None,
                        ignored_options: vec![],
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
                        text: vec![InlineNode::Text("Root".to_string())],
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["section/index".to_string()],
                        maxdepth: None,
                        ignored_options: vec![],
                    }),
                ],
            ),
            Document::new(
                "section/index.rst".to_string(),
                vec![
                    Node::Heading {
                        level: 1,
                        text: vec![InlineNode::Text("Section".to_string())],
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["sub/page".to_string()],
                        maxdepth: None,
                        ignored_options: vec![],
                    }),
                ],
            ),
            Document::new(
                "section/sub/page.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Deep Page".to_string())],
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
                        text: vec![InlineNode::Text("Home".to_string())],
                    },
                    Node::Directive(Directive::Toctree {
                        paths: vec!["child".to_string()],
                        maxdepth: None,
                        ignored_options: vec![],
                    }),
                ],
            ),
            Document::new(
                "child.rst".to_string(),
                vec![Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Child".to_string())],
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

    #[test]
    fn test_build_project_index_breaks_direct_cycle_in_toctree() {
        // Given — root references A, A references B, B references A (B→A creates a cycle)
        let docs = vec![
            Document::new(
                "root.rst".to_string(),
                vec![Node::Directive(Directive::Toctree {
                    paths: vec!["a".to_string()],
                    maxdepth: None,
                    ignored_options: vec![],
                })],
            ),
            Document::new(
                "a.rst".to_string(),
                vec![Node::Directive(Directive::Toctree {
                    paths: vec!["b".to_string()],
                    maxdepth: None,
                    ignored_options: vec![],
                })],
            ),
            Document::new(
                "b.rst".to_string(),
                vec![Node::Directive(Directive::Toctree {
                    paths: vec!["a".to_string()], // cycle back to a
                    maxdepth: None,
                    ignored_options: vec![],
                })],
            ),
        ];

        // When
        let index = build_project_index(&docs);

        // Then — root → a → b → a(leaf). The second occurrence of a.rst must be childless.
        assert_eq!(index.nav_tree.len(), 1);
        let root = &index.nav_tree[0];
        assert_eq!(root.path, "root.rst");
        assert_eq!(root.children.len(), 1);
        let a = &root.children[0];
        assert_eq!(a.path, "a.rst");
        assert_eq!(a.children.len(), 1);
        let b = &a.children[0];
        assert_eq!(b.path, "b.rst");
        // b references a, but a is already an ancestor — cycle must be broken
        assert_eq!(b.children.len(), 1);
        assert_eq!(b.children[0].path, "a.rst");
        assert!(
            b.children[0].children.is_empty(),
            "cycle must be broken: a.rst must appear as a leaf"
        );
    }

    #[test]
    fn test_build_project_index_allows_shared_node_in_multiple_branches() {
        // Given — root → left, root → right, both left and right reference shared.
        // shared appears in two branches but creates no cycle.
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![Node::Directive(Directive::Toctree {
                    paths: vec!["left".to_string(), "right".to_string()],
                    maxdepth: None,
                    ignored_options: vec![],
                })],
            ),
            Document::new(
                "left.rst".to_string(),
                vec![Node::Directive(Directive::Toctree {
                    paths: vec!["shared".to_string()],
                    maxdepth: None,
                    ignored_options: vec![],
                })],
            ),
            Document::new(
                "right.rst".to_string(),
                vec![Node::Directive(Directive::Toctree {
                    paths: vec!["shared".to_string()],
                    maxdepth: None,
                    ignored_options: vec![],
                })],
            ),
            Document::new("shared.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs);

        // Then — shared.rst appears as a child of both left and right
        let root = &index.nav_tree[0];
        assert_eq!(root.children.len(), 2);
        let left = &root.children[0];
        let right = &root.children[1];
        assert_eq!(left.children.len(), 1);
        assert_eq!(left.children[0].path, "shared.rst");
        assert_eq!(right.children.len(), 1);
        assert_eq!(
            right.children[0].path, "shared.rst",
            "shared.rst must appear in both branches, not be truncated as a false cycle"
        );
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

    // ── Glossary analyzer tests ───────────────────────────────────────────────

    #[test]
    fn test_analyze_registers_glossary_terms() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["environment".to_string()],
                    definition: vec![],
                }],
                sorted: false,
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.glossary_terms.len(), 1);
        assert_eq!(
            index.glossary_terms.get(&TargetName::new("environment")),
            Some(&"glossary.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_all_terms_in_multi_term_entry() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["term 1".to_string(), "term 2".to_string()],
                    definition: vec![],
                }],
                sorted: false,
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.glossary_terms.len(), 2);
        assert!(
            index
                .glossary_terms
                .contains_key(&TargetName::new("term 1"))
        );
        assert!(
            index
                .glossary_terms
                .contains_key(&TargetName::new("term 2"))
        );
    }

    #[test]
    fn test_merge_combines_glossary_terms_from_two_documents() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.glossary_terms
            .insert(TargetName::new("foo"), "glossary_a.rst".to_string());

        let mut idx2 = ProjectIndex::default();
        idx2.glossary_terms
            .insert(TargetName::new("bar"), "glossary_b.rst".to_string());

        // When
        let diagnostics = idx1.merge(idx2);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(idx1.glossary_terms.len(), 2);
        assert!(idx1.glossary_terms.contains_key(&TargetName::new("foo")));
        assert!(idx1.glossary_terms.contains_key(&TargetName::new("bar")));
    }

    #[test]
    fn test_merge_emits_diagnostic_for_duplicate_glossary_term() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        let mut idx2 = ProjectIndex::default();
        idx2.glossary_terms
            .insert(TargetName::new("environment"), "other.rst".to_string());

        // When
        let diagnostics = idx1.merge(idx2);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("Duplicate glossary term"));
        assert!(diagnostics[0].contains("environment"));
        // Last-writer-wins: idx2's path should be kept
        assert_eq!(
            idx1.glossary_terms.get(&TargetName::new("environment")),
            Some(&"other.rst".to_string())
        );
    }

    // ── Domain object analyzer tests ──────────────────────────────────────────

    #[test]
    fn test_analyze_registers_domain_object() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    signature: "greet(name)".to_string(),
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.domain_objects.len(), 1);
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("py:function:greet")),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_module_domain_object() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.domain_objects.len(), 1);
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("py:module:greetings")),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_data_domain_object() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    name: "DEFAULT_TIMEOUT".to_string(),
                    type_: Some("int".to_string()),
                    value: Some("30".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — registered under the canonical "py:data:..." key, the same
        // one both `:py:data:` and `:py:const:` roles resolve against.
        assert_eq!(index.domain_objects.len(), 1);
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("py:data:DEFAULT_TIMEOUT")),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_distinct_keys_for_same_name_in_different_domains() {
        // Given — same object name "add" declared under both py and c domains
        let doc = Document::new(
            "api.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signature: "add(a, b)".to_string(),
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::CFunction {
                        signature: "int add(int a, int b)".to_string(),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.domain_objects.len(), 2);
        assert!(
            index
                .domain_objects
                .contains_key(&TargetName::new("py:function:add"))
        );
        assert!(
            index
                .domain_objects
                .contains_key(&TargetName::new("c:function:add"))
        );
    }

    #[test]
    fn test_analyze_leaves_domain_objects_empty_for_plain_document() {
        // Given
        let doc = Document::new(
            "plain.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::Text(
                "No domain objects here.".to_string(),
            )])],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(index.domain_objects.is_empty());
    }

    #[test]
    fn test_merge_combines_domain_objects_from_two_documents() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.domain_objects
            .insert(TargetName::new("py:function:foo"), "a.rst".to_string());

        let mut idx2 = ProjectIndex::default();
        idx2.domain_objects
            .insert(TargetName::new("c:function:bar"), "b.rst".to_string());

        // When
        idx1.merge(idx2);

        // Then
        assert_eq!(idx1.domain_objects.len(), 2);
        assert!(
            idx1.domain_objects
                .contains_key(&TargetName::new("py:function:foo"))
        );
        assert!(
            idx1.domain_objects
                .contains_key(&TargetName::new("c:function:bar"))
        );
    }

    #[test]
    fn test_analyze_registers_domain_object_nested_in_table_cell() {
        // Given — a `.. data::` directive nested inside a grid-table cell,
        // mirroring CPython's `curses.rst` attribute table (`A_NORMAL` etc.)
        let doc = Document::new(
            "curses.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![],
                body_rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![rusty_sphinx_ast::TableCell {
                        colspan: 1,
                        rowspan: 1,
                        content: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyData {
                                name: "A_NORMAL".to_string(),
                                type_: None,
                                value: None,
                                body: vec![],
                            },
                        ))],
                    }],
                }],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("py:data:A_NORMAL")),
            Some(&"curses.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_target_nested_in_table_cell() {
        // Given — a target nested inside a grid-table cell
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![],
                body_rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![rusty_sphinx_ast::TableCell {
                        colspan: 1,
                        rowspan: 1,
                        content: vec![Node::Target {
                            name: TargetName::new("nested-target"),
                            uri: None,
                        }],
                    }],
                }],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index.targets.get(&TargetName::new("nested-target")),
            Some(&TargetLocation::Internal("test.rst".to_string()))
        );
    }

    #[test]
    fn test_analyze_registers_domain_object_nested_in_bullet_list() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '-',
                items: vec![rusty_sphinx_ast::BulletListItem {
                    nodes: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyFunction {
                            signature: "greet(name)".to_string(),
                            body: vec![],
                        },
                    ))],
                }],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("py:function:greet")),
            Some(&"test.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_domain_object_nested_in_definition_list() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::DefinitionList {
                items: vec![rusty_sphinx_ast::DefinitionListItem {
                    term: vec![InlineNode::Text("term".to_string())],
                    definition: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyFunction {
                            signature: "greet(name)".to_string(),
                            body: vec![],
                        },
                    ))],
                }],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("py:function:greet")),
            Some(&"test.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_domain_object_nested_in_admonition_body() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: rusty_sphinx_ast::AdmonitionKind::Note,
                title: None,
                collapsible: None,
                body: vec![Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signature: "greet(name)".to_string(),
                        body: vec![],
                    },
                ))],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("py:function:greet")),
            Some(&"test.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_domain_object_nested_in_another_domain_objects_body() {
        // Given — a `py:module` whose body contains a nested `py:function`
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyFunction {
                            signature: "greet(name)".to_string(),
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — both the outer module and the nested function are indexed
        assert_eq!(index.domain_objects.len(), 2);
        assert!(
            index
                .domain_objects
                .contains_key(&TargetName::new("py:module:greetings"))
        );
        assert!(
            index
                .domain_objects
                .contains_key(&TargetName::new("py:function:greet"))
        );
    }

    #[test]
    fn test_merge_duplicate_detection_is_case_insensitive() {
        // Given — "Environment" and "environment" should collide
        let mut idx1 = ProjectIndex::default();
        idx1.glossary_terms
            .insert(TargetName::new("Environment"), "a.rst".to_string());

        let mut idx2 = ProjectIndex::default();
        idx2.glossary_terms
            .insert(TargetName::new("environment"), "b.rst".to_string());

        // When
        let diagnostics = idx1.merge(idx2);

        // Then
        assert_eq!(diagnostics.len(), 1, "Expected duplicate diagnostic");
    }
}

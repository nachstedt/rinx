//! The analyzer module represents the global indexing phase.
//!
//! It builds a `ProjectIndex` — a global symbol table containing cross-reference
//! targets, document titles, and a hierarchical navigation tree derived from
//! toctree directives.

mod utils;

pub use utils::normalize_path;

use rusty_sphinx_ast::{Directive, Document, DomainObjectBody, IndexEntry, Node, TargetName};
use rusty_sphinx_index::{GenIndexEntry, NavEntry, ProjectIndex, TargetLocation};
use rusty_sphinx_scope::Scope;

use std::collections::BTreeMap;

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
    index_nodes(&doc.nodes, &doc.path, &mut index, &mut Scope::default());
    index
}

/// Recursively registers targets, glossary terms, and domain objects found
/// anywhere in `nodes`, including inside table cells, list items, and
/// directive bodies — not just at the document's top level. This mirrors
/// the recursion shape `render_nodes` (in the renderer crate) uses, since a
/// definition nested in a container still needs to be indexed for
/// cross-references to resolve, exactly like it's still rendered with a
/// working anchor.
///
/// `scope.python` carries the enclosing `py:class`/`py:exception` stack
/// (lexical, pushed/popped around a nested body — see
/// [`rusty_sphinx_ast::DomainObjectBody::deduce_local_scope`], shared with
/// the renderer so index keys and anchor `id`s can't drift apart) and the
/// most recently seen `py:module` (document-order state, not lexical
/// nesting — real Sphinx docs write `py:module` and the functions/classes it
/// documents as *siblings*, not nested underneath it, so it is never popped
/// when returning from a nested body; a module stays "current" for the rest
/// of the document until another `py:module`, or `py:currentmodule`,
/// changes it). `scope.c` is the same idea for the `c` domain's
/// `c:struct`/`c:union` nesting — a wholly separate stack (see
/// [`rusty_sphinx_scope::CScope`]'s doc comment for why it isn't a variant of
/// `PythonScope`); `c:function`/`c:macro` never touch it and keep qualifying
/// via `scope.python` exactly as before it existed.
fn index_nodes(nodes: &[Node], doc_path: &str, index: &mut ProjectIndex, scope: &mut Scope) {
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
            Node::Directive(Directive::Index { entries, id }) => {
                for entry in entries {
                    if let IndexEntry::Term {
                        primary,
                        subentry,
                        main,
                    } = entry
                    {
                        index.genindex_entries.push(GenIndexEntry {
                            primary: primary.clone(),
                            subentry: subentry.clone(),
                            main: *main,
                            doc_path: doc_path.to_string(),
                            anchor: id.clone(),
                        });
                    }
                    // IndexEntry::See/SeeAlso redirect rather than link to
                    // content and are not surfaced in genindex_entries yet
                    // (see spec_gaps.md).
                }
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                index_domain_object(obj, doc_path, index, scope);
            }
            Node::Directive(Directive::PyCurrentModule { module }) => match module {
                Some(name) => scope.python.set_module(name),
                None => scope.python.clear_module(),
            },
            Node::Directive(
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                index_nodes(body, doc_path, index, scope);
            }
            Node::BulletList { items, .. } => {
                for item in items {
                    index_nodes(&item.nodes, doc_path, index, scope);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    index_nodes(&item.definition, doc_path, index, scope);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter().chain(body_rows) {
                    for cell in &row.cells {
                        index_nodes(&cell.content, doc_path, index, scope);
                    }
                }
            }
            Node::Directive(Directive::ListTable { rows, name, .. }) => {
                if let Some(target_name) = name {
                    index.targets.insert(
                        target_name.clone(),
                        TargetLocation::Internal(doc_path.to_string()),
                    );
                }
                for row in rows {
                    for cell in &row.cells {
                        index_nodes(&cell.content, doc_path, index, scope);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Registers a single `Directive::DomainObject` (and recurses into its
/// body), handling both qualification and module-context updates. Split out
/// of [`index_nodes`] to keep that function's line count manageable.
fn index_domain_object(
    obj: &rusty_sphinx_ast::DomainObjectBody,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    // A `py:module`'s own name is never qualified against the *previous*
    // module: real Sphinx always writes it in full and sets it verbatim as
    // the new current module, it never nests it under whatever module was
    // current before.
    let is_module = matches!(obj, rusty_sphinx_ast::DomainObjectBody::PyModule { .. });
    // `c:struct`/`c:union`/`c:member`/`c:type` nest under `CScope` instead of
    // `PythonScope` — everything else, including `c:function`/`c:macro`,
    // keeps using `scope` exactly as before this type existed. `c:type`
    // joins this set because real Sphinx's C domain scopes nested
    // declarations generically off whatever declaration they're indented
    // under, not specifically off struct/union.
    let uses_c_scope = matches!(
        obj,
        DomainObjectBody::CStruct { .. }
            | DomainObjectBody::CUnion { .. }
            | DomainObjectBody::CMember { .. }
            | DomainObjectBody::CType { .. }
    );
    let own_names = obj.names();
    // Only the primary name qualifies the *scope*: it alone decides what this
    // object lends to its body and, for a module, what becomes current. The
    // remaining names are pure aliases — each independently resolvable, but
    // none of them contributes scope.
    let (qualified_primary, new_segments) = if is_module {
        (own_names.first().clone(), Vec::new())
    } else if uses_c_scope {
        let qualification = scope.c.qualify(own_names.first());
        (qualification.qualified_name, qualification.new_segments)
    } else {
        let qualification = scope
            .python
            .qualify(obj.object_type().domain(), own_names.first());
        (qualification.qualified_name, qualification.new_segments)
    };

    for (index_in_object, own_name) in own_names.as_slice().iter().enumerate() {
        let qualified_name = if index_in_object == 0 {
            qualified_primary.clone()
        } else if uses_c_scope {
            scope.c.qualify(own_name).qualified_name
        } else {
            scope
                .python
                .qualify(obj.object_type().domain(), own_name)
                .qualified_name
        };
        if obj.no_index() {
            continue;
        }
        index.insert_domain_object(obj.object_type(), &qualified_name, doc_path);
        if obj.no_index_entry() {
            continue;
        }
        let anchor = rusty_sphinx_ast::build_domain_object_key(obj.object_type(), &qualified_name);
        index.genindex_entries.push(GenIndexEntry {
            primary: format!("{qualified_name} ({})", obj.object_type().as_str()),
            subentry: None,
            main: false,
            doc_path: doc_path.to_string(),
            anchor: anchor.as_str().to_string(),
        });
    }

    if is_module {
        scope.python.set_module(&qualified_primary);
    }
    // The body is indexed exactly once, no matter how many names the object
    // declares — the aliases share it rather than each owning a copy.
    let lend = obj.deduce_local_scope(&new_segments);
    if uses_c_scope {
        let depth = scope.c.push_containers(&lend);
        index_nodes(obj.body(), doc_path, index, scope);
        scope.c.truncate_containers(depth);
    } else {
        let depth = scope.python.push_classes(&lend);
        index_nodes(obj.body(), doc_path, index, scope);
        scope.python.truncate_classes(depth);
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
    use rusty_sphinx_ast::{
        Domain, InlineNode, Node, NonEmptyVector, ObjectType, PyObjectType, TargetSearchOrder,
    };

    /// Looks up a domain object by the pre-refactor flat `"domain:objtype:name"`
    /// key shape (e.g. `"py:function:greet"`), so test expectations can stay
    /// expressed as a single string instead of repeating two-level map
    /// navigation at every call site below.
    fn lookup_domain_object<'a>(index: &'a ProjectIndex, flat_key: &str) -> Option<&'a String> {
        let mut parts = flat_key.splitn(3, ':');
        let domain: Domain = parts.next()?.parse().ok()?;
        let objtype_str = parts.next()?;
        let name = parts.next()?;
        let object_type = ObjectType::from_directive_name(domain, objtype_str)?;
        index
            .domain_objects
            .get(&TargetName::new(name))?
            .get(&object_type)
    }

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
                        search_order: TargetSearchOrder::LeastQualifiedFirst,
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

    // ── Domain object analyzer tests ──────────────────────────────────────────

    #[test]
    fn test_analyze_registers_domain_object() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    signatures: NonEmptyVector::single("greet(name)".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.domain_objects.len(), 1);
        assert_eq!(
            lookup_domain_object(&index, "py:function:greet"),
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
            lookup_domain_object(&index, "py:module:greetings"),
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
                    signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
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
            lookup_domain_object(&index, "py:data:DEFAULT_TIMEOUT"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_every_declared_name_of_a_multi_signature_object() {
        // Given — the confirmed `library/socket.rst` shape: one directive
        // declaring three aliases, which real Sphinx resolves individually.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    signatures: NonEmptyVector::new(
                        "AF_UNIX".to_string(),
                        vec!["AF_INET".to_string(), "AF_INET6".to_string()],
                    ),
                    type_: None,
                    value: None,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — each alias is an independently resolvable target.
        assert_eq!(index.domain_objects.len(), 3);
        for name in ["AF_UNIX", "AF_INET", "AF_INET6"] {
            assert_eq!(
                lookup_domain_object(&index, &format!("py:data:{name}")),
                Some(&"api.rst".to_string()),
                "{name} should resolve"
            );
        }
    }

    #[test]
    fn test_analyze_gives_every_declared_name_its_own_genindex_entry() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    signatures: NonEmptyVector::new("A".to_string(), vec!["ASCII".to_string()]),
                    type_: None,
                    value: None,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — one entry per alias, each anchored to its own name.
        assert_eq!(index.genindex_entries.len(), 2);
        assert_eq!(index.genindex_entries[0].anchor, "py:data:a");
        assert_eq!(index.genindex_entries[1].anchor, "py:data:ascii");
    }

    #[test]
    fn test_analyze_indexes_a_multi_signature_objects_body_only_once() {
        // Given — the aliases share one docstring; indexing it per alias
        // would register its contents several times over.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    signatures: NonEmptyVector::new(
                        "AF_UNIX".to_string(),
                        vec!["AF_INET".to_string()],
                    ),
                    type_: None,
                    value: None,
                    body: vec![Node::Target {
                        name: rusty_sphinx_ast::TargetName::new("address-families"),
                        uri: None,
                    }],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — the body's target is registered exactly once, even though
        // two names were.
        assert_eq!(index.domain_objects.len(), 2);
        assert_eq!(index.targets.len(), 1);
    }

    #[test]
    fn test_analyze_qualifies_every_alias_of_a_multi_signature_object_by_module() {
        // Given — a multi-signature object under a current module: every
        // alias, not just the primary, has to pick the module qualifier up.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "socket".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyData {
                        signatures: NonEmptyVector::new(
                            "AF_UNIX".to_string(),
                            vec!["AF_INET".to_string()],
                        ),
                        type_: None,
                        value: None,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "py:data:socket.AF_UNIX"),
            Some(&"api.rst".to_string())
        );
        assert_eq!(
            lookup_domain_object(&index, "py:data:socket.AF_INET"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_exception_domain_object() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signatures: NonEmptyVector::single("GreeterError".to_string()),
                    is_final: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.domain_objects.len(), 1);
        assert_eq!(
            lookup_domain_object(&index, "py:exception:GreeterError"),
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
                        signatures: NonEmptyVector::single("add(a, b)".to_string()),
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::CFunction {
                        signatures: NonEmptyVector::single("int add(int a, int b)".into()),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then — same qualified name ("add"), but two distinct object types
        // coexist under it, one entry per domain.
        assert_eq!(index.domain_objects.len(), 1);
        assert_eq!(
            index
                .domain_objects
                .get(&TargetName::new("add"))
                .map(std::collections::BTreeMap::len),
            Some(2)
        );
        assert!(lookup_domain_object(&index, "py:function:add").is_some());
        assert!(lookup_domain_object(&index, "c:function:add").is_some());
    }

    fn c_member(signature: &str) -> Node {
        Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
            signatures: NonEmptyVector::single(signature.into()),
            no_index: false,
            no_index_entry: false,
            no_contents_entry: false,
            body: vec![],
        }))
    }

    fn c_macro(signature: &str) -> Node {
        Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single(signature.into()),
            body: vec![],
        }))
    }

    #[test]
    fn test_analyze_qualifies_bare_c_member_nested_under_c_struct() {
        // Given — `.. c:member:: int count` nested inside `.. c:struct:: Data`.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CStruct {
                    signatures: NonEmptyVector::single("Data".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![c_member("int count")],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — the bare member name is auto-qualified against the
        // enclosing struct.
        assert_eq!(
            lookup_domain_object(&index, "c:member:Data.count"),
            Some(&"api.rst".to_string())
        );
        assert!(lookup_domain_object(&index, "c:member:count").is_none());
    }

    #[test]
    fn test_analyze_qualifies_bare_c_member_nested_under_c_union() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CUnion {
                    signatures: NonEmptyVector::single("Number".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![c_member("int as_int")],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:member:Number.as_int"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_flat_dotted_c_member_without_enclosing_struct() {
        // Given — the real CPython-docs shape: no `.. c:struct::` at all.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![c_member("PyObject *PyTypeObject.tp_bases")],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:member:PyTypeObject.tp_bases"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_c_function_nested_in_c_struct_is_not_qualified_by_it() {
        // Given — an (unrealistic) `c:function` written inside a `c:struct`
        // body: `deduce_local_scope` never lends its `CStruct` segments to
        // anything but `CScope`, and `c:function` doesn't consult `CScope` at
        // all, so it must be registered under its own bare name.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CStruct {
                    signatures: NonEmptyVector::single("Data".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        DomainObjectBody::CFunction {
                            signatures: NonEmptyVector::single("int helper(void)".into()),
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "c:function:helper").is_some());
        assert!(lookup_domain_object(&index, "c:function:Data.helper").is_none());
    }

    #[test]
    fn test_analyze_registers_flat_c_macro_nested_under_c_type_without_qualification() {
        // Given — the real CPython `c-api/memory.rst` shape (`known_bugs.md`):
        // enum-style `.. c:macro::` constants nested inside `.. c:type::`.
        // `c:type` joins `uses_c_scope` (it establishes a `CScope` container
        // for descendants that read it), but `c:macro` — like `c:function` —
        // never consults `CScope` regardless of what it's nested under (see
        // `test_analyze_c_function_nested_in_c_struct_is_not_qualified_by_it`
        // for the same, pre-existing behavior under `c:struct`), so the
        // nested macro registers under its own bare name. This happens to
        // match the actual bare-name constants CPython's docs render (real
        // Sphinx would also qualify-then-reset via `.. c:namespace:: NULL`,
        // which rusty-sphinx doesn't implement — the same bare-name result
        // falls out here for an unrelated, simpler reason).
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![c_macro("PYMEM_DOMAIN_RAW")],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:macro:PYMEM_DOMAIN_RAW"),
            Some(&"api.rst".to_string())
        );
        assert!(
            lookup_domain_object(&index, "c:macro:PyMemAllocatorDomain.PYMEM_DOMAIN_RAW").is_none()
        );
    }

    #[test]
    fn test_analyze_qualifies_bare_c_member_nested_under_c_type() {
        // Given — unlike `c:macro`/`c:function`, `c:member` does consult
        // `CScope`, so nesting it under `c:type` (rather than
        // `c:struct`/`c:union`) still qualifies it against the enclosing
        // type's name.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("Data".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![c_member("int count")],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:member:Data.count"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_bare_c_type_without_nesting() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:type:PyMemAllocatorDomain"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_registers_a_function_pointer_typedef_under_its_declared_name() {
        // Given — `Doc/c-api/init.rst`'s `Py_tracefunc`. Before the
        // declaration parser this registered as `c:type:int` (the return
        // type), leaving the eight `:c:type:` references to it in
        // `Doc/c-api/profiling.rst` unresolvable.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CType {
                    signatures: NonEmptyVector::single(
                        "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)"
                            .into(),
                    ),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:type:Py_tracefunc"),
            Some(&"api.rst".to_string())
        );
        assert_eq!(lookup_domain_object(&index, "c:type:int"), None);
    }

    #[test]
    fn test_analyze_registers_slot_typedefs_with_pointer_return_types() {
        // Given — `Doc/c-api/typeobj.rst` declares dozens of these, all of
        // the `RETTYPE *(*NAME)(ARGS)` shape.
        let doc = Document::new(
            "typeobj.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("PyObject *(*unaryfunc)(PyObject *)".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                })),
                Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
                    signatures: NonEmptyVector::single(
                        "int (*visitproc)(PyObject *object, void *arg)".into(),
                    ),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                })),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:type:unaryfunc"),
            Some(&"typeobj.rst".to_string())
        );
        assert_eq!(
            lookup_domain_object(&index, "c:type:visitproc"),
            Some(&"typeobj.rst".to_string())
        );
    }

    #[test]
    fn test_analyze_no_index_suppresses_target_and_genindex_entry_for_c_type() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("Hidden".into()),
                    no_index: true,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "c:type:Hidden").is_none());
        assert!(index.genindex_entries.is_empty());
    }

    #[test]
    fn test_analyze_no_index_suppresses_target_and_genindex_entry() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CMember {
                    signatures: NonEmptyVector::single("count".into()),
                    no_index: true,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "c:member:count").is_none());
        assert!(index.genindex_entries.is_empty());
    }

    #[test]
    fn test_analyze_no_index_entry_keeps_target_but_suppresses_genindex_entry() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                DomainObjectBody::CMember {
                    signatures: NonEmptyVector::single("count".into()),
                    no_index: false,
                    no_index_entry: true,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "c:member:count"),
            Some(&"api.rst".to_string())
        );
        assert!(index.genindex_entries.is_empty());
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
                                signatures: NonEmptyVector::single("A_NORMAL".to_string()),
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
            lookup_domain_object(&index, "py:data:A_NORMAL"),
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
    fn test_analyze_registers_list_table_name_as_target() {
        // Given — a `.. list-table::` with a `:name:` option
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: Some(TargetName::new("fruit-table")),
                rows: vec![],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index.targets.get(&TargetName::new("fruit-table")),
            Some(&TargetLocation::Internal("test.rst".to_string()))
        );
    }

    #[test]
    fn test_analyze_list_table_without_name_registers_no_target() {
        // Given — a `.. list-table::` with no `:name:` option
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(index.targets.is_empty());
    }

    #[test]
    fn test_analyze_registers_domain_object_nested_in_list_table_cell() {
        // Given — a `.. py:attribute::` nested inside a list-table cell,
        // mirroring the known_bugs.md `reference/datamodel.rst` scenario
        let doc = Document::new(
            "datamodel.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![rusty_sphinx_ast::TableCell {
                        colspan: 1,
                        rowspan: 1,
                        content: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                                signatures: NonEmptyVector::single("method.__self__".to_string()),
                                type_: None,
                                value: None,
                                canonical: None,
                                body: vec![],
                            },
                        ))],
                    }],
                }],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "py:attribute:method.__self__"),
            Some(&"datamodel.rst".to_string())
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
                            signatures: NonEmptyVector::single("greet(name)".to_string()),
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
            lookup_domain_object(&index, "py:function:greet"),
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
                            signatures: NonEmptyVector::single("greet(name)".to_string()),
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
            lookup_domain_object(&index, "py:function:greet"),
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
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![],
                    },
                ))],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "py:function:greet"),
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
                            signatures: NonEmptyVector::single("greet(name)".to_string()),
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — both the outer module and the nested function are indexed,
        // the function qualified by the enclosing module's name
        assert_eq!(index.domain_objects.len(), 2);
        assert!(lookup_domain_object(&index, "py:module:greetings").is_some());
        assert!(lookup_domain_object(&index, "py:function:greetings.greet").is_some());
    }

    #[test]
    fn test_analyze_qualifies_method_nested_in_class() {
        // Given — a `py:method` nested inside a `py:class` body
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("Greeter".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — both the class and its qualified method are indexed
        assert_eq!(index.domain_objects.len(), 2);
        assert!(lookup_domain_object(&index, "py:class:Greeter").is_some());
        assert!(lookup_domain_object(&index, "py:method:Greeter.greet").is_some());
    }

    #[test]
    fn test_analyze_qualifies_method_nested_in_exception() {
        // Given — a `py:method` nested inside a `py:exception` body, exactly
        // like the `py:class` nesting case, since exceptions are classes.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signatures: NonEmptyVector::single("GreeterError".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signatures: NonEmptyVector::single("reason(self)".to_string()),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — both the exception and its qualified method are indexed
        assert_eq!(index.domain_objects.len(), 2);
        assert!(lookup_domain_object(&index, "py:exception:GreeterError").is_some());
        assert!(lookup_domain_object(&index, "py:method:GreeterError.reason").is_some());
    }

    #[test]
    fn test_analyze_does_not_double_qualify_already_qualified_nested_attribute() {
        // Given — mirrors CPython's `Doc/library/exceptions.rst`, which
        // nests `.. attribute:: StopIteration.value` (already fully
        // qualified) inside `.. exception:: StopIteration`, rather than
        // writing the bare name `value`.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signatures: NonEmptyVector::single("StopIteration".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                            signatures: NonEmptyVector::single("StopIteration.value".to_string()),
                            type_: None,
                            value: None,
                            canonical: None,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — the attribute is indexed under its own already-qualified
        // name, not doubled to "StopIteration.StopIteration.value"
        assert_eq!(index.domain_objects.len(), 2);
        assert!(lookup_domain_object(&index, "py:exception:StopIteration").is_some());
        assert!(lookup_domain_object(&index, "py:attribute:StopIteration.value").is_some());
    }

    #[test]
    fn test_analyze_qualifies_nested_classes_two_levels_deep() {
        // Given — a class nested inside another class, both containing a method
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("Outer".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyClass {
                            signatures: NonEmptyVector::single("Inner".to_string()),
                            is_final: false,
                            body: vec![Node::Directive(Directive::DomainObject(
                                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                    signatures: NonEmptyVector::single("method(self)".to_string()),
                                    is_classmethod: false,
                                    is_staticmethod: false,
                                    is_abstractmethod: false,
                                    is_async: false,
                                    body: vec![],
                                },
                            ))],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:class:Outer.Inner").is_some());
        assert!(lookup_domain_object(&index, "py:method:Outer.Inner.method").is_some());
    }

    #[test]
    fn test_analyze_qualifies_non_method_object_nested_in_class() {
        // Given — a `py:data` (class attribute) nested inside a `py:class`
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("Greeter".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyData {
                            signatures: NonEmptyVector::single("DEFAULT_GREETING".to_string()),
                            type_: None,
                            value: None,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:data:Greeter.DEFAULT_GREETING").is_some());
    }

    #[test]
    fn test_analyze_qualifies_object_nested_in_module_domain_object() {
        // Given — a `py:function` nested inside a `py:module` body picks up
        // the module's name, matching real Sphinx.
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
                            signatures: NonEmptyVector::single("greet(name)".to_string()),
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:function:greetings.greet").is_some());
        assert!(lookup_domain_object(&index, "py:function:greet").is_none());
    }

    #[test]
    fn test_analyze_qualifies_sibling_object_after_module_domain_object() {
        // Given — the real-world CPython shape: `py:module` and the
        // `py:function` it documents are *siblings* at the document's top
        // level, not nested — this is what surfaced the `types.coroutine`
        // broken-domain-object bug.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "types".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signatures: NonEmptyVector::single("coroutine(gen_func)".to_string()),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:function:types.coroutine").is_some());
    }

    #[test]
    fn test_analyze_does_not_dedup_module_prefix_in_flat_sibling_signature() {
        // Given — the class/module conflation bug this change fixes: real
        // CPython's `datetime.rst` documents `.. classmethod::
        // datetime.strptime` as a column-0 sibling of `.. module::
        // datetime`, with no enclosing `.. class::`. The `datetime.` in the
        // signature is the *class* name (there is a separate `.. class::
        // datetime` elsewhere in the same file) — it only coincides with the
        // module name, and must not be mistaken for a repeat of it.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "datetime".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyMethod {
                        signatures: NonEmptyVector::single(
                            "datetime.strptime(date_string, format)".to_string(),
                        ),
                        is_classmethod: true,
                        is_staticmethod: false,
                        is_abstractmethod: false,
                        is_async: false,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then — not collapsed to "datetime.strptime".
        assert!(lookup_domain_object(&index, "py:method:datetime.datetime.strptime").is_some());
    }

    #[test]
    fn test_analyze_qualifies_object_after_current_module_directive() {
        // Given — the CPython `howto/enum.rst` shape: a document with no
        // `py:module` of its own, opening with `.. currentmodule:: enum`
        // before a class defined as if under that module.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::PyCurrentModule {
                    module: Some("enum".to_string()),
                }),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signatures: NonEmptyVector::single("Enum".to_string()),
                        is_final: false,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:class:enum.Enum").is_some());
    }

    #[test]
    fn test_analyze_current_module_directive_creates_no_index_entry_of_its_own() {
        // Given — real Sphinx's `currentmodule` documents nothing; unlike
        // `py:module`, it must not appear in `domain_objects` or
        // `genindex_entries`.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::PyCurrentModule {
                module: Some("enum".to_string()),
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(index.domain_objects.is_empty());
        assert!(index.genindex_entries.is_empty());
    }

    #[test]
    fn test_analyze_current_module_none_resets_qualification_to_module_free() {
        // Given — `.. currentmodule:: None` after a `py:module` restores
        // unqualified index keys, matching real Sphinx's reset form.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "pickle".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::PyCurrentModule { module: None }),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signatures: NonEmptyVector::single("example()".to_string()),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then — not qualified as "pickle.example".
        assert!(lookup_domain_object(&index, "py:function:example").is_some());
        assert!(lookup_domain_object(&index, "py:function:pickle.example").is_none());
    }

    #[test]
    fn test_analyze_object_before_any_module_directive_stays_unqualified() {
        // Given — a `py:function` appearing before any `py:module` in the
        // document has no current module to fall back to.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "greetings".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:function:greet").is_some());
    }

    #[test]
    fn test_analyze_switches_current_module_on_second_module_directive() {
        // Given — two sequential `py:module` directives in one document
        // (e.g. a package's docs covering a couple of submodules); the
        // second one becomes current for everything after it.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "email.mime".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "email.mime.text".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signatures: NonEmptyVector::single("MIMEText".to_string()),
                        is_final: false,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:class:email.mime.text.MIMEText").is_some());
    }

    #[test]
    fn test_analyze_composes_module_and_class_qualifiers() {
        // Given — a `py:class` documented as a sibling after `py:module`
        // (module-qualified), with a `py:method` nested inside the class
        // (class-qualified) — both qualifiers must compose.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "types".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signatures: NonEmptyVector::single("DynamicClassAttribute".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                signatures: NonEmptyVector::single(
                                    "__get__(self, instance, owner)".to_string(),
                                ),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:class:types.DynamicClassAttribute").is_some());
        assert!(
            lookup_domain_object(&index, "py:method:types.DynamicClassAttribute.__get__").is_some()
        );
    }

    #[test]
    fn test_analyze_dedups_class_name_repeated_in_flat_nested_signature() {
        // Given — the real CPython idiom from `random.rst`: `.. method::
        // Random.seed` indented inside `.. class:: Random`, itself a sibling
        // after `.. module:: random`. The method's own signature repeats the
        // class name; it must not double to "random.Random.Random.seed".
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "random".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signatures: NonEmptyVector::single("Random([seed])".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                signatures: NonEmptyVector::single(
                                    "Random.seed(a=None, version=2)".to_string(),
                                ),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:class:random.Random").is_some());
        assert!(lookup_domain_object(&index, "py:method:random.Random.seed").is_some());
    }

    // ── genindex analyzer tests ───────────────────────────────────────────────

    #[test]
    fn test_analyze_registers_genindex_entry_for_domain_object() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    signatures: NonEmptyVector::single("greet(name)".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.genindex_entries.len(), 1);
        let entry = &index.genindex_entries[0];
        assert_eq!(entry.primary, "greet (function)");
        assert_eq!(entry.subentry, None);
        assert!(!entry.main);
        assert_eq!(entry.doc_path, "api.rst");
        assert_eq!(entry.anchor, "py:function:greet");
    }

    #[test]
    fn test_analyze_qualifies_genindex_entry_for_method_nested_in_class() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("Greeter".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — one entry for the class, one for the qualified method
        assert_eq!(index.genindex_entries.len(), 2);
        assert!(
            index
                .genindex_entries
                .iter()
                .any(|e| e.primary == "Greeter (class)")
        );
        assert!(
            index
                .genindex_entries
                .iter()
                .any(|e| e.primary == "Greeter.greet (method)")
        );
    }

    #[test]
    fn test_analyze_registers_genindex_entry_for_index_directive_single() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Index {
                entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                    primary: "execution".to_string(),
                    subentry: None,
                    main: false,
                }],
                id: "index-0".to_string(),
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.genindex_entries.len(), 1);
        let entry = &index.genindex_entries[0];
        assert_eq!(entry.primary, "execution");
        assert_eq!(entry.doc_path, "guide.rst");
        assert_eq!(entry.anchor, "index-0");
    }

    #[test]
    fn test_analyze_registers_genindex_entry_with_subentry_and_main_flag() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Index {
                entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                    primary: "Python".to_string(),
                    subentry: Some("interpreter".to_string()),
                    main: true,
                }],
                id: "index-0".to_string(),
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        let entry = &index.genindex_entries[0];
        assert_eq!(entry.subentry, Some("interpreter".to_string()));
        assert!(entry.main);
    }

    #[test]
    fn test_analyze_skips_see_and_seealso_index_entries() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Index {
                entries: vec![
                    rusty_sphinx_ast::IndexEntry::See {
                        entry: "foo".to_string(),
                        target: "bar".to_string(),
                    },
                    rusty_sphinx_ast::IndexEntry::SeeAlso {
                        entry: "foo".to_string(),
                        target: "bar".to_string(),
                    },
                ],
                id: "index-0".to_string(),
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(index.genindex_entries.is_empty());
    }

    #[test]
    fn test_analyze_registers_genindex_entry_for_index_directive_nested_in_bullet_list() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::BulletList {
                bullet: '-',
                items: vec![rusty_sphinx_ast::BulletListItem {
                    nodes: vec![Node::Directive(Directive::Index {
                        entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                            primary: "execution".to_string(),
                            subentry: None,
                            main: false,
                        }],
                        id: "index-0".to_string(),
                    })],
                }],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.genindex_entries.len(), 1);
    }

    #[test]
    fn test_analyze_registers_genindex_entry_for_index_directive_nested_in_admonition() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: rusty_sphinx_ast::AdmonitionKind::Note,
                title: None,
                collapsible: None,
                body: vec![Node::Directive(Directive::Index {
                    entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                        primary: "execution".to_string(),
                        subentry: None,
                        main: false,
                    }],
                    id: "index-0".to_string(),
                })],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.genindex_entries.len(), 1);
    }
}

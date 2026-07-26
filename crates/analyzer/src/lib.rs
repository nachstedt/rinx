//! The analyzer module represents the global indexing phase.
//!
//! It builds a `ProjectIndex` — a global symbol table containing cross-reference
//! targets, document titles, and a hierarchical navigation tree derived from
//! toctree directives.

mod utils;

pub use utils::normalize_path;

use rusty_sphinx_ast::{Directive, Document, IndexEntry, Node, ObjectType, TargetName};
use rusty_sphinx_scope::PythonScope;
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

/// One entry in the site-wide general index (`genindex.html`), sourced
/// either from a `.. index::` directive or automatically from a domain
/// object definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenIndexEntry {
    /// The main, alphabetized term (e.g. `"execution"`, `"Greeter.greet (method)"`).
    pub primary: String,
    /// An optional nested sub-term (e.g. `"context"` in `single: execution; context`).
    pub subentry: Option<String>,
    /// Whether this occurrence should be emphasized as the entry's primary
    /// definition (from a leading `!` in a `.. index::` entry).
    pub main: bool,
    /// The document this entry's anchor lives on.
    pub doc_path: String,
    /// The HTML anchor `id` on `doc_path` this entry links to.
    pub anchor: String,
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
    /// Maps a domain object's qualified name (e.g. `xmlrpc.client.fault`) to
    /// every object type it's been defined under and the document path
    /// containing that `Directive::DomainObject` definition. Keyed by name
    /// first (rather than baking the object type into a single flat key)
    /// so a reference can be resolved against any of the object types real
    /// Sphinx treats as mutually aliasable for the same name (see
    /// [`ObjectType::role_alias_candidates`]) — e.g. `CPython` documents
    /// `Fault` via `.. class::` but references it via `:exc:`.
    #[serde(default)]
    pub domain_objects: BTreeMap<TargetName, BTreeMap<ObjectType, String>>,
    /// Entries for the site-wide general index page, accumulated (not
    /// deduplicated) across every document — the same term legitimately
    /// appearing from multiple locations is expected, not an error.
    #[serde(default)]
    pub genindex_entries: Vec<GenIndexEntry>,
}

impl ProjectIndex {
    /// Registers a domain object's definition under its qualified name,
    /// keyed further by its own object type — shared by [`index_domain_object`]
    /// and by tests, so both always agree on how a `domain_objects` entry is
    /// shaped. Last-writer-wins if the same `(qualified_name, object_type)`
    /// pair is inserted twice.
    pub fn insert_domain_object(
        &mut self,
        object_type: ObjectType,
        qualified_name: &str,
        doc_path: impl Into<String>,
    ) {
        self.domain_objects
            .entry(TargetName::new(qualified_name))
            .or_default()
            .insert(object_type, doc_path.into());
    }

    /// Merge another `ProjectIndex` into this one.
    ///
    /// Emits a diagnostic string for each glossary term defined in both indices
    /// (case-insensitive duplicate detection). Last-writer-wins for the mapping value.
    pub fn merge(&mut self, other: Self) -> Vec<String> {
        self.targets.extend(other.targets);
        self.document_titles.extend(other.document_titles);
        for (name, object_types) in other.domain_objects {
            self.domain_objects
                .entry(name)
                .or_default()
                .extend(object_types);
        }
        self.genindex_entries.extend(other.genindex_entries);
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
    index_nodes(
        &doc.nodes,
        &doc.path,
        &mut index,
        &mut PythonScope::default(),
    );
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
/// `scope` carries the enclosing `py:class`/`py:exception` stack (lexical,
/// pushed/popped around a nested body — see
/// [`rusty_sphinx_ast::DomainObjectBody::deduce_local_scope`], shared with
/// the renderer so index keys and anchor `id`s can't drift apart) and the
/// most recently seen `py:module` (document-order state, not lexical
/// nesting — real Sphinx docs write `py:module` and the functions/classes it
/// documents as *siblings*, not nested underneath it, so it is never popped
/// when returning from a nested body; a module stays "current" for the rest
/// of the document until another `py:module`, or once supported,
/// `py:currentmodule`, changes it).
fn index_nodes(nodes: &[Node], doc_path: &str, index: &mut ProjectIndex, scope: &mut PythonScope) {
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
    scope: &mut PythonScope,
) {
    // A `py:module`'s own name is never qualified against the *previous*
    // module: real Sphinx always writes it in full and sets it verbatim as
    // the new current module, it never nests it under whatever module was
    // current before.
    let is_module = matches!(obj, rusty_sphinx_ast::DomainObjectBody::PyModule { .. });
    let own_name = obj.name();
    let (qualified_name, new_segments) = if is_module {
        (own_name, Vec::new())
    } else {
        let qualification = scope.qualify(obj.object_type().domain(), &own_name);
        (qualification.qualified_name, qualification.new_segments)
    };

    index.insert_domain_object(obj.object_type(), &qualified_name, doc_path);
    let anchor = rusty_sphinx_ast::build_domain_object_key(obj.object_type(), &qualified_name);
    index.genindex_entries.push(GenIndexEntry {
        primary: format!("{qualified_name} ({})", obj.object_type().as_str()),
        subentry: None,
        main: false,
        doc_path: doc_path.to_string(),
        anchor: anchor.as_str().to_string(),
    });
    if is_module {
        scope.set_module(&qualified_name);
    }
    let lend = obj.deduce_local_scope(&new_segments);
    let depth = scope.push_classes(&lend);
    index_nodes(obj.body(), doc_path, index, scope);
    scope.truncate_classes(depth);
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
    use rusty_sphinx_ast::{Domain, InlineNode, Node, ObjectType, PyObjectType};

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
    fn test_lookup_domain_object_finds_inserted_entry() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "greet", "api.rst");

        // When / Then
        assert_eq!(
            lookup_domain_object(&index, "py:function:greet"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_lookup_domain_object_returns_none_for_missing_name() {
        // Given
        let index = ProjectIndex::default();

        // When / Then
        assert_eq!(lookup_domain_object(&index, "py:function:greet"), None);
    }

    #[test]
    fn test_lookup_domain_object_returns_none_when_name_present_under_different_objtype() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Class), "Fault", "xmlrpc.rst");

        // When / Then
        assert_eq!(lookup_domain_object(&index, "py:function:Fault"), None);
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
            lookup_domain_object(&index, "py:data:DEFAULT_TIMEOUT"),
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
                    signature: "GreeterError".to_string(),
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
        idx1.insert_domain_object(ObjectType::Py(PyObjectType::Function), "foo", "a.rst");

        let mut idx2 = ProjectIndex::default();
        idx2.insert_domain_object(
            ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
            "bar",
            "b.rst",
        );

        // When
        idx1.merge(idx2);

        // Then
        assert_eq!(idx1.domain_objects.len(), 2);
        assert!(lookup_domain_object(&idx1, "py:function:foo").is_some());
        assert!(lookup_domain_object(&idx1, "c:function:bar").is_some());
    }

    #[test]
    fn test_merge_combines_domain_objects_with_different_object_types_for_same_name() {
        // Given — mirrors CPython's `xmlrpc.client.rst`: one document defines
        // `Fault` via `.. class::`, another (hypothetically) documents it via
        // `.. exception::` — merge must keep both coexisting under the same
        // qualified name rather than one clobbering the other.
        let mut idx1 = ProjectIndex::default();
        idx1.insert_domain_object(ObjectType::Py(PyObjectType::Class), "Fault", "a.rst");

        let mut idx2 = ProjectIndex::default();
        idx2.insert_domain_object(ObjectType::Py(PyObjectType::Exception), "Fault", "b.rst");

        // When
        idx1.merge(idx2);

        // Then
        assert_eq!(idx1.domain_objects.len(), 1);
        assert_eq!(
            lookup_domain_object(&idx1, "py:class:Fault"),
            Some(&"a.rst".to_string())
        );
        assert_eq!(
            lookup_domain_object(&idx1, "py:exception:Fault"),
            Some(&"b.rst".to_string())
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
                            signature: "greet(name)".to_string(),
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
                    signature: "Greeter".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signature: "greet(self, name)".to_string(),
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
                    signature: "GreeterError".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signature: "reason(self)".to_string(),
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
                    signature: "StopIteration".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                            name: "StopIteration.value".to_string(),
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
                    signature: "Outer".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyClass {
                            signature: "Inner".to_string(),
                            is_final: false,
                            body: vec![Node::Directive(Directive::DomainObject(
                                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                    signature: "method(self)".to_string(),
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
                    signature: "Greeter".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyData {
                            name: "DEFAULT_GREETING".to_string(),
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
                            signature: "greet(name)".to_string(),
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
                        signature: "coroutine(gen_func)".to_string(),
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
                        signature: "datetime.strptime(date_string, format)".to_string(),
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
    fn test_analyze_object_before_any_module_directive_stays_unqualified() {
        // Given — a `py:function` appearing before any `py:module` in the
        // document has no current module to fall back to.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signature: "greet(name)".to_string(),
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
                        signature: "MIMEText".to_string(),
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
                        signature: "DynamicClassAttribute".to_string(),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                signature: "__get__(self, instance, owner)".to_string(),
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
                        signature: "Random([seed])".to_string(),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                signature: "Random.seed(a=None, version=2)".to_string(),
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
                    signature: "greet(name)".to_string(),
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
                    signature: "Greeter".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signature: "greet(self, name)".to_string(),
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

    #[test]
    fn test_merge_accumulates_genindex_entries_from_two_documents() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.genindex_entries.push(GenIndexEntry {
            primary: "foo".to_string(),
            subentry: None,
            main: false,
            doc_path: "a.rst".to_string(),
            anchor: "index-0".to_string(),
        });

        let mut idx2 = ProjectIndex::default();
        idx2.genindex_entries.push(GenIndexEntry {
            primary: "foo".to_string(),
            subentry: None,
            main: false,
            doc_path: "b.rst".to_string(),
            anchor: "index-0".to_string(),
        });

        // When
        let diagnostics = idx1.merge(idx2);

        // Then — both locations kept, no dedup/diagnostics
        assert!(diagnostics.is_empty());
        assert_eq!(idx1.genindex_entries.len(), 2);
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

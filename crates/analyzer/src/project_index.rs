use rusty_sphinx_ast::Document;
use rusty_sphinx_index::ProjectIndex;

use super::document_index::analyze;
use super::page_order::collect_page_order;
use super::section_numbering::assign_section_numbers;
use rusty_sphinx_toctree::expand_toctree;

use std::collections::BTreeSet;

/// Analyzes a collection of `Document`s and builds a complete `ProjectIndex`.
///
/// Written as a sequence of named phases rather than one body because the
/// phases have a real order dependency the names should make visible: roots
/// cannot be chosen until every document's toctrees are known, and section
/// numbering and page order cannot start until the roots are chosen.
///
/// `root_doc` is the configured root document, without its `.rst` extension
/// (`rusty_sphinx.toml`'s `root_doc`). It falls back to the inferred roots
/// when it names no document that exists, so a project that never configured
/// one keeps building.
#[must_use]
pub fn build_project_index(docs: &[Document], root_doc: &str) -> ProjectIndex {
    build_project_index_reporting(docs, root_doc).index
}

/// A built index, plus the problems only a project-wide view could see.
///
/// Mirrors the renderer's `RenderOutput`: the payload, and what the caller
/// should report about it. The diagnostics are *unfiltered* — suppression is
/// the reporter's job, never the detector's.
pub struct ProjectIndexBuild {
    pub index: ProjectIndex,
    pub diagnostics: Vec<super::DocumentDiagnostics>,
}

/// [`build_project_index`], also returning the diagnostics it found.
#[must_use]
pub fn build_project_index_reporting(docs: &[Document], root_doc: &str) -> ProjectIndexBuild {
    let universe: BTreeSet<String> = docs.iter().map(|doc| doc.path.clone()).collect();

    let mut index = merge_document_analyses(docs);
    index.root_documents = find_root_documents(docs, &index, root_doc);
    index.section_numbers = assign_section_numbers(&index, &universe);
    index.page_order = collect_page_order(&index, &universe);

    let diagnostics = super::nav_diagnostics::collect_nav_diagnostics(docs, &index);
    ProjectIndexBuild { index, diagnostics }
}

/// Merges every document's own analysis — targets, titles, outlines, toctrees,
/// glossary terms, equations — into one index.
fn merge_document_analyses(docs: &[Document]) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for doc in docs {
        let _ = index.merge(analyze(doc));
    }
    index
}

/// Chooses the documents navigation starts from.
///
/// The configured `root_doc` wins when it names a document that exists, which
/// is the only way to say which of several unreferenced documents is *the*
/// root. Otherwise roots are inferred as every document no toctree references
/// — the historical behaviour, which also means an orphaned document shows up
/// as its own top-level entry rather than disappearing.
fn find_root_documents(docs: &[Document], index: &ProjectIndex, root_doc: &str) -> Vec<String> {
    let configured = if std::path::Path::new(root_doc)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rst"))
    {
        root_doc.to_string()
    } else {
        format!("{root_doc}.rst")
    };
    if docs.iter().any(|doc| doc.path == configured) {
        return vec![configured];
    }

    let universe: BTreeSet<String> = docs.iter().map(|doc| doc.path.clone()).collect();
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for (owner, toctrees) in &index.toctrees {
        for toctree in toctrees {
            let (targets, _) = expand_toctree(&toctree.toctree, owner, &universe);
            for target in targets {
                // A `self` entry names its own document, which must not make
                // that document a non-root.
                if let rusty_sphinx_toctree::TocTarget::Document { docname, .. } = target
                    && docname != *owner
                {
                    referenced.insert(docname);
                }
            }
        }
    }

    universe
        .into_iter()
        .filter(|path| !referenced.contains(path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_root_documents_prefers_the_configured_root() {
        // Given — two documents neither of which references the other, so
        // inference alone cannot say which is the root.
        let docs = vec![
            Document::new("index.rst".to_string(), vec![]),
            Document::new("stray.rst".to_string(), vec![]),
        ];
        let index = merge_document_analyses(&docs);

        // When
        let roots = find_root_documents(&docs, &index, "index");

        // Then
        assert_eq!(roots, vec!["index.rst"]);
    }

    #[test]
    fn test_find_root_documents_accepts_a_configured_root_with_an_extension() {
        // Given
        let docs = vec![Document::new("index.rst".to_string(), vec![])];
        let index = merge_document_analyses(&docs);

        // When
        let roots = find_root_documents(&docs, &index, "index.rst");

        // Then
        assert_eq!(roots, vec!["index.rst"]);
    }

    #[test]
    fn test_find_root_documents_falls_back_to_inference_for_an_unknown_root() {
        // Given — a project whose configured root does not exist keeps
        // building rather than losing its navigation entirely.
        let docs = vec![
            Document::new(
                "start.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree_of(&["child"])))],
            ),
            Document::new("child.rst".to_string(), vec![]),
        ];
        let index = merge_document_analyses(&docs);

        // When
        let roots = find_root_documents(&docs, &index, "nonexistent");

        // Then — `child` is referenced, so only `start` is inferred a root.
        assert_eq!(roots, vec!["start.rst"]);
    }

    #[test]
    fn test_find_root_documents_does_not_treat_a_self_entry_as_a_reference() {
        // Given — a root listing itself with `self` is still a root.
        let mut toctree = toctree_of(&["child"]);
        toctree.entries.push(rusty_sphinx_ast::TocEntry::SelfRef {
            title: None,
            span: None,
        });
        let docs = vec![
            Document::new(
                "start.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree))],
            ),
            Document::new("child.rst".to_string(), vec![]),
        ];
        let index = merge_document_analyses(&docs);

        // When
        let roots = find_root_documents(&docs, &index, "nonexistent");

        // Then
        assert_eq!(roots, vec!["start.rst"]);
    }

    #[test]
    fn test_merge_document_analyses_records_each_document_toctrees() {
        // Given
        let docs = vec![Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree(toctree_of(&["child"])))],
        )];

        // When
        let index = merge_document_analyses(&docs);

        // Then — the graph is stored unexpanded, per toctree.
        assert_eq!(index.toctrees["index.rst"].len(), 1);
        assert_eq!(index.toctrees["index.rst"][0].toctree.entries.len(), 1);
    }

    #[test]
    fn test_build_project_index_registers_a_toctree_name_as_a_target() {
        // Given — `:name:` makes the toctree itself referenceable.
        let mut toctree = toctree_of(&["child"]);
        toctree.options.name = Some(rusty_sphinx_ast::TargetName::new("main-toc"));
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree))],
            ),
            Document::new("child.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs, "index");

        // Then
        assert_eq!(
            index
                .targets
                .get(&rusty_sphinx_ast::TargetName::new("main-toc")),
            Some(&rusty_sphinx_index::TargetLocation::Internal(
                "index.rst".to_string()
            ))
        );
    }

    /// A toctree of plain document entries, the shape every test here needs.
    /// Entry spans are irrelevant to these tests, so they are left unset
    /// rather than invented.
    fn toctree_of(docnames: &[&str]) -> rusty_sphinx_ast::Toctree {
        rusty_sphinx_ast::Toctree {
            entries: docnames
                .iter()
                .map(|docname| rusty_sphinx_ast::TocEntry::Document {
                    title: None,
                    docname: (*docname).to_string(),
                    span: None,
                })
                .collect(),
            options: rusty_sphinx_ast::ToctreeOptions::default(),
        }
    }
    use rusty_sphinx_ast::{Directive, Node};

    // The nesting, cycle-breaking and shared-branch cases this module used to
    // assert on a pre-flattened `nav_tree` now belong to the two phases that
    // actually walk the graph: `page_order` (reading order) and the renderer's
    // `nav::expand` (display). Both test them directly. What stays here is the
    // integration: that `build_project_index` wires the phases together.

    #[test]
    fn test_build_project_index_records_the_graph_and_reading_order() {
        // Given — index → guide → setup.
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree_of(&["guide"])))],
            ),
            Document::new(
                "guide.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree_of(&["setup"])))],
            ),
            Document::new("setup.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs, "index");

        // Then
        assert_eq!(index.root_documents, vec!["index.rst"]);
        assert_eq!(
            index.page_order,
            vec!["index.rst", "guide.rst", "setup.rst"]
        );
        assert!(index.toctrees.contains_key("index.rst"));
        assert!(index.toctrees.contains_key("guide.rst"));
    }

    #[test]
    fn test_build_project_index_numbers_a_numbered_toctree() {
        // Given
        let mut toctree = toctree_of(&["guide"]);
        toctree.options.numbered = Some(rusty_sphinx_ast::NumberedDepth::Unlimited);
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree))],
            ),
            Document::new("guide.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs, "index");

        // Then
        assert_eq!(
            index.section_numbers["guide.rst"].document(),
            Some([1].as_slice())
        );
    }

    #[test]
    fn test_build_project_index_leaves_an_orphan_out_of_the_reading_order() {
        // Given — `orphan` is in the project but no toctree names it.
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree_of(&["guide"])))],
            ),
            Document::new("guide.rst".to_string(), vec![]),
            Document::new("orphan.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs, "index");

        // Then — it gets no prev/next rather than an arbitrary position.
        assert_eq!(index.page_order, vec!["index.rst", "guide.rst"]);
    }

    #[test]
    fn test_build_project_index_returns_default_for_multiple_documents() {
        // Given
        let docs = vec![
            Document::new("test1.rst".to_string(), vec![]),
            Document::new("test2.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs, "index");

        // Then
        let _ = format!("{index:?}");
    }
}

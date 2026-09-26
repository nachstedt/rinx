use rinx_ast::Document;
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;

use super::document_index::analyze;
use super::entity_index::{
    collect_entity_diagnostics, collect_schema_mismatches, derive_entity_backlinks,
};
use super::entity_update_apply::apply_entity_updates;
use super::page_order::collect_page_order;
use super::section_numbering::assign_section_numbers;
use rinx_toctree::expand_toctree;

use std::collections::BTreeSet;

/// Analyzes a collection of `Document`s and builds a complete `ProjectIndex`.
///
/// Written as a sequence of named phases rather than one body because the
/// phases have a real order dependency the names should make visible: roots
/// cannot be chosen until every document's toctrees are known, and section
/// numbering and page order cannot start until the roots are chosen.
///
/// `root_doc` is the configured root document, without its `.rst` extension
/// (`rinx.toml`'s `root_doc`). It falls back to the inferred roots
/// when it names no document that exists, so a project that never configured
/// one keeps building.
#[must_use]
pub fn build_project_index(
    docs: &[Document],
    root_doc: &str,
    schema: &EntitySchema,
) -> ProjectIndex {
    build_project_index_reporting(docs, root_doc, schema).index
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
pub fn build_project_index_reporting(
    docs: &[Document],
    root_doc: &str,
    schema: &EntitySchema,
) -> ProjectIndexBuild {
    let universe: BTreeSet<String> = docs.iter().map(|doc| doc.path.clone()).collect();

    let (mut index, merge_diagnostics) = merge_document_analyses(docs);
    index.root_documents = find_root_documents(docs, &index, root_doc);
    index.section_numbers = assign_section_numbers(&index, &universe);
    index.page_order = collect_page_order(&index, &universe);
    // Applying every collected `.. entity-update::`/`.. needextend::` comes
    // next: it can change an entity's *effective* outgoing edges (never its
    // own record — see `apply_entity_updates`'s own doc comment), and
    // back-links must be derived from those, not from the as-authored ones.
    let update_diagnostics = apply_entity_updates(&mut index, schema);
    // Entity back-links come last among the index phases: they are a function
    // of the merged (and now updated) graph, exactly as page order is a
    // function of the merged toctrees.
    index.entity_backlinks = derive_entity_backlinks(&index, schema);

    let schema_hashes: Vec<(&str, Option<&str>)> = docs
        .iter()
        .map(|doc| (doc.path.as_str(), doc.entity_schema_hash.as_deref()))
        .collect();

    let mut diagnostics = super::nav_diagnostics::collect_nav_diagnostics(docs, &index);
    diagnostics.extend(merge_diagnostics);
    diagnostics.extend(collect_schema_mismatches(&schema_hashes, schema));
    diagnostics.extend(update_diagnostics);
    diagnostics.extend(collect_entity_diagnostics(&index, schema));
    ProjectIndexBuild { index, diagnostics }
}

/// Merges every document's own analysis — targets, titles, outlines, toctrees,
/// glossary terms, equations — into one index.
fn merge_document_analyses(docs: &[Document]) -> (ProjectIndex, Vec<super::DocumentDiagnostics>) {
    let mut index = ProjectIndex::default();
    let mut diagnostics = Vec::new();
    for doc in docs {
        let conflicts = index.merge(analyze(doc));
        // A duplicate id is only visible to whichever merge sees the second
        // definition, so it is collected here rather than rediscovered later.
        // Attributed to the document merged in, whose definition won and which
        // is therefore the one an author will want to look at.
        //
        // `duplicate_glossary_terms` is deliberately still dropped: there is no
        // diagnostic code for it yet, and inventing one here would start
        // emitting a warning on projects that never asked for this feature.
        if !conflicts.duplicate_entity_ids.is_empty() {
            diagnostics.push(super::DocumentDiagnostics {
                source_path: doc.path.clone(),
                diagnostics: conflicts
                    .duplicate_entity_ids
                    .iter()
                    .map(|clash| rinx_ast::Diagnostic {
                        code: rinx_ast::DiagnosticCode::EntityDuplicateId,
                        message: clash.message(),
                        span: None,
                    })
                    .collect(),
            });
        }
    }
    (index, diagnostics)
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
                if let rinx_toctree::TocTarget::Document { docname, .. } = target
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
        let (index, _) = merge_document_analyses(&docs);

        // When
        let roots = find_root_documents(&docs, &index, "index");

        // Then
        assert_eq!(roots, vec!["index.rst"]);
    }

    #[test]
    fn test_find_root_documents_accepts_a_configured_root_with_an_extension() {
        // Given
        let docs = vec![Document::new("index.rst".to_string(), vec![])];
        let (index, _) = merge_document_analyses(&docs);

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
        let (index, _) = merge_document_analyses(&docs);

        // When
        let roots = find_root_documents(&docs, &index, "nonexistent");

        // Then — `child` is referenced, so only `start` is inferred a root.
        assert_eq!(roots, vec!["start.rst"]);
    }

    #[test]
    fn test_find_root_documents_does_not_treat_a_self_entry_as_a_reference() {
        // Given — a root listing itself with `self` is still a root.
        let mut toctree = toctree_of(&["child"]);
        toctree.entries.push(rinx_ast::TocEntry::SelfRef {
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
        let (index, _) = merge_document_analyses(&docs);

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
        let (index, _) = merge_document_analyses(&docs);

        // Then — the graph is stored unexpanded, per toctree.
        assert_eq!(index.toctrees["index.rst"].len(), 1);
        assert_eq!(index.toctrees["index.rst"][0].toctree.entries.len(), 1);
    }

    #[test]
    fn test_build_project_index_registers_a_toctree_name_as_a_target() {
        // Given — `:name:` makes the toctree itself referenceable.
        let mut toctree = toctree_of(&["child"]);
        toctree.options.name = Some(rinx_ast::TargetName::new("main-toc"));
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree))],
            ),
            Document::new("child.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs, "index", &EntitySchema::empty());

        // Then
        assert_eq!(
            index.targets.get(&rinx_ast::TargetName::new("main-toc")),
            Some(&rinx_index::TargetLocation::Internal(
                "index.rst".to_string()
            ))
        );
    }

    /// A toctree of plain document entries, the shape every test here needs.
    /// Entry spans are irrelevant to these tests, so they are left unset
    /// rather than invented.
    fn toctree_of(docnames: &[&str]) -> rinx_ast::Toctree {
        rinx_ast::Toctree {
            entries: docnames
                .iter()
                .map(|docname| rinx_ast::TocEntry::Document {
                    title: None,
                    docname: (*docname).to_string(),
                    span: None,
                })
                .collect(),
            options: rinx_ast::ToctreeOptions::default(),
        }
    }
    use rinx_ast::{Directive, Node};

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
        let index = build_project_index(&docs, "index", &EntitySchema::empty());

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
        toctree.options.numbered = Some(rinx_ast::NumberedDepth::Unlimited);
        let docs = vec![
            Document::new(
                "index.rst".to_string(),
                vec![Node::Directive(Directive::Toctree(toctree))],
            ),
            Document::new("guide.rst".to_string(), vec![]),
        ];

        // When
        let index = build_project_index(&docs, "index", &EntitySchema::empty());

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
        let index = build_project_index(&docs, "index", &EntitySchema::empty());

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
        let index = build_project_index(&docs, "index", &EntitySchema::empty());

        // Then
        let _ = format!("{index:?}");
    }
}

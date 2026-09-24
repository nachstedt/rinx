//! The `index` subcommand: merges every document's local analysis into one
//! global `rusty_sphinx_index::ProjectIndex`.

use anyhow::{Context, Result};
use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_renderer::config;
use std::fs;

use super::cli_args::{flag_value, flag_value_opt, flag_values};
use super::diagnostics::{WarningOrigin, format_diagnostic};
use super::entity_schema::load_entity_schema;
use super::suppression::retain_reportable;

/// The serialized index, plus the warnings the caller should print.
pub(super) struct IndexedProject {
    pub json: String,
    /// Already filtered through each document's own `.. noqa:` suppressions —
    /// see [`super::suppression`] for why that happens here rather than where
    /// the diagnostics are found.
    pub warnings: Vec<String>,
}

/// `root_doc` is the configured root document (without its `.rst` extension);
/// it decides where navigation, page order and section numbering start.
pub(super) fn process_index(
    ast_jsons: &[String],
    root_doc: &str,
    schema: &rusty_sphinx_entity::EntitySchema,
) -> Result<IndexedProject> {
    let docs: Vec<ast::Document> = ast_jsons
        .iter()
        .map(|json| serde_json::from_str(json).context("Failed to deserialize AST"))
        .collect::<Result<_>>()?;

    let build = analyzer::build_project_index_reporting(&docs, root_doc, schema);

    // Each document's suppressions travel with its own AST, so a `.. noqa:`
    // written in the document holding the toctree silences the warning about
    // it even though the problem was only visible project-wide.
    let suppressions: std::collections::BTreeMap<&str, &[ast::Suppression]> = docs
        .iter()
        .map(|doc| (doc.path.as_str(), doc.suppressions.as_slice()))
        .collect();
    // …and so does its table of included files, without which a span from an
    // `.. include::` could not be resolved back to the fragment it names.
    let source_files: std::collections::BTreeMap<&str, &[String]> = docs
        .iter()
        .map(|doc| (doc.path.as_str(), doc.source_files.as_slice()))
        .collect();

    let mut warnings = Vec::new();
    for reported in &build.diagnostics {
        let empty: &[ast::Suppression] = &[];
        let doc_suppressions = suppressions
            .get(reported.source_path.as_str())
            .copied()
            .unwrap_or(empty);
        let empty_files: &[String] = &[];
        let origin = WarningOrigin::new(
            &reported.source_path,
            source_files
                .get(reported.source_path.as_str())
                .copied()
                .unwrap_or(empty_files),
        );
        for diagnostic in retain_reportable(&reported.diagnostics, doc_suppressions) {
            warnings.push(format_diagnostic(&origin, diagnostic));
        }
    }

    Ok(IndexedProject {
        json: serde_json::to_string(&build.index).context("Serialization error")?,
        warnings,
    })
}

pub(crate) fn cmd_index(args: &[String]) -> Result<()> {
    let output = flag_value(args, "--output")?;
    let inputs = flag_values(args, "--inputs")?;

    // `--config` is optional so that an invocation predating the root-document
    // option still works; without it the roots are inferred as before.
    let site_config = match flag_value_opt(args, "--config") {
        Some(path) => {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("Error reading config '{path}'"))?;
            toml::from_str(&text).with_context(|| format!("Error parsing config '{path}'"))?
        }
        None => config::SiteConfig::default(),
    };

    let files: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    let schema = load_entity_schema(args)?;
    let indexed = process_index(&files, &site_config.root_doc, &schema)?;
    for warning in &indexed.warnings {
        eprintln!("{warning}");
    }
    fs::write(&output, indexed.json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_entity::EntitySchema;

    /// One document's AST as the `index` subcommand receives it.
    fn ast_json(path: &str, nodes_json: &str) -> String {
        format!(r#"{{"path":"{path}","nodes":[{nodes_json}]}}"#)
    }

    /// A toctree naming one document, its entry spanning source line 3.
    fn toctree_json(docname: &str) -> String {
        format!(
            r#"{{"Directive":{{"Toctree":{{"entries":[{{"Document":{{"title":null,"docname":"{docname}","span":{{"start":{{"line":3,"column":4}},"end":{{"line":3,"column":9}}}}}}}}],"options":{{}}}}}}}}"#
        )
    }

    #[test]
    fn test_process_index_warns_about_an_entry_naming_no_document() {
        // Given — a project-wide problem no single parse could see.
        let docs = vec![ast_json("index.rst", &toctree_json("missing"))];

        // When
        let indexed = process_index(&docs, "index", &EntitySchema::empty()).unwrap();

        // Then
        assert_eq!(indexed.warnings.len(), 1, "{:?}", indexed.warnings);
        assert!(
            indexed.warnings[0].contains("toctree.missing-document"),
            "{:?}",
            indexed.warnings
        );
        assert!(indexed.warnings[0].starts_with("warning: index.rst:"));
    }

    #[test]
    fn test_process_index_honours_a_noqa_suppression() {
        // Given — the same document, with the code suppressed in it. The
        // problem is found project-wide but excused by a comment the parser
        // recorded, which is why suppressions travel on the AST.
        let docs = vec![format!(
            r#"{{"path":"index.rst","nodes":[{}],"suppressions":[{{"start_line":1,"end_line":9999,"codes":"All"}}]}}"#,
            toctree_json("missing")
        )];

        // When
        let indexed = process_index(&docs, "index", &EntitySchema::empty()).unwrap();

        // Then
        assert!(indexed.warnings.is_empty(), "{:?}", indexed.warnings);
    }

    #[test]
    fn test_process_index_warns_about_an_orphan_document() {
        // Given
        let docs = vec![
            ast_json("index.rst", &toctree_json("a")),
            ast_json("a.rst", ""),
            ast_json("stray.rst", ""),
        ];

        // When
        let indexed = process_index(&docs, "index", &EntitySchema::empty()).unwrap();

        // Then
        assert_eq!(indexed.warnings.len(), 1, "{:?}", indexed.warnings);
        assert!(indexed.warnings[0].contains("toctree.orphan-document"));
        assert!(indexed.warnings[0].contains("stray.rst"));
    }

    #[test]
    fn test_process_index_is_quiet_for_a_well_formed_project() {
        // Given
        let docs = vec![
            ast_json("index.rst", &toctree_json("a")),
            ast_json("a.rst", ""),
        ];

        // When
        let indexed = process_index(&docs, "index", &EntitySchema::empty()).unwrap();

        // Then
        assert!(indexed.warnings.is_empty(), "{:?}", indexed.warnings);
    }

    #[test]
    fn test_process_index_returns_serialized_project_index() {
        // Given
        let docs = vec![
            r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":[{"Text":"Title"}]}}]}"#
                .to_string(),
        ];

        // When
        let index = process_index(&docs, "index", &EntitySchema::empty())
            .unwrap()
            .json;

        // Then
        assert_eq!(
            index,
            r#"{"targets":{},"document_titles":{"test.rst":"Title"},"toctrees":{},"root_documents":["test.rst"],"page_order":["test.rst"],"section_numbers":{},"document_outlines":{},"glossary_terms":{},"domain_objects":{},"genindex_entries":[],"equations":{},"sectnum":{},"entities":{},"entity_backlinks":{},"entity_updates":[],"entity_update_history":{}}"#
        );
    }
}

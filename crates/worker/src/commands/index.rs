//! The `index` subcommand: merges every document's local analysis into one
//! global `rinx_index::ProjectIndex`.

use anyhow::{Context, Result, bail};
use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_renderer::config;
use std::collections::BTreeSet;
use std::fs;

use rinx_index::ExternalInventory;
use rinx_inventory::{InventoryName, MalformedLine, read_inventory};

use super::cli_args::{flag_groups, flag_value, flag_value_opt, flag_values};
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
    schema: &rinx_entity::EntitySchema,
    external_inventories: Vec<ExternalInventory>,
) -> Result<IndexedProject> {
    let docs: Vec<ast::Document> = ast_jsons
        .iter()
        .map(|json| serde_json::from_str(json).context("Failed to deserialize AST"))
        .collect::<Result<_>>()?;

    let mut build = analyzer::build_project_index_reporting(&docs, root_doc, schema);
    let written = written_reference_targets(ast_jsons)?;
    build.index.external_inventories = external_inventories
        .into_iter()
        .map(|mut inventory| {
            inventory.retain_referenced(&written);
            inventory
        })
        .collect();

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
    let declared = flag_groups(args, "--inventory", 3)?
        .into_iter()
        .map(|group| {
            let [name, base_url, path] = <[String; 3]>::try_from(group)
                .expect("flag_groups returns groups of the requested arity");
            let bytes =
                fs::read(&path).with_context(|| format!("Error reading inventory '{path}'"))?;
            Ok(DeclaredInventory {
                name,
                base_url,
                path,
                bytes,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let (external_inventories, inventory_warnings) = read_external_inventories(declared)?;
    for warning in &inventory_warnings {
        eprintln!("{warning}");
    }

    let indexed = process_index(&files, &site_config.root_doc, &schema, external_inventories)?;
    for warning in &indexed.warnings {
        eprintln!("{warning}");
    }
    fs::write(&output, indexed.json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

/// An inventory the build declared, read but not yet parsed.
pub(super) struct DeclaredInventory {
    pub name: String,
    pub base_url: String,
    /// Where it was read from, for naming it in a message.
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Parses every declared inventory, in declaration order, and returns them
/// with a warning for each body line Sphinx itself would have skipped.
///
/// Everything wrong with an inventory *as a whole* — an invalid or repeated
/// name, a file that is not an inventory — is an error rather than a
/// warning: it is a fault in the build's configuration, not in any document,
/// so no `.. noqa:` should be able to silence it, and every reference into
/// the inventory would otherwise break at once for a reason the page cannot
/// show.
pub(super) fn read_external_inventories(
    declared: Vec<DeclaredInventory>,
) -> Result<(Vec<ExternalInventory>, Vec<String>)> {
    let mut inventories: Vec<ExternalInventory> = Vec::new();
    let mut warnings = Vec::new();
    for DeclaredInventory {
        name,
        base_url,
        path,
        bytes,
    } in declared
    {
        let name = InventoryName::new(&name)
            .with_context(|| format!("Invalid name for inventory '{path}'"))?;
        if inventories.iter().any(|existing| existing.name == name) {
            bail!("Inventory name '{name}' is declared twice; every inventory needs its own name");
        }
        let read =
            read_inventory(&bytes).with_context(|| format!("Error reading inventory '{path}'"))?;
        warnings.extend(
            read.malformed_lines
                .iter()
                .map(|malformed| malformed_line_warning(&path, malformed)),
        );
        inventories.push(ExternalInventory::new(name, base_url, read.inventory));
    }
    Ok((inventories, warnings))
}

/// Every target a cross-reference role in any document names, as an external
/// lookup would search for it: as written, and — for a `name:`-prefixed one —
/// without the prefix too.
///
/// Read off each AST's JSON rather than its typed tree on purpose. Inline
/// content lives in paragraphs, headings, table cells, captions, list terms
/// and more, and no walker over all of them exists; a generic walk over the
/// serialized form reaches every one by construction, so a container added
/// later cannot silently hide its references from the inventory pruning.
fn written_reference_targets(ast_jsons: &[String]) -> Result<BTreeSet<String>> {
    fn collect(value: &serde_json::Value, written: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, inner) in fields {
                    let target_field = match key.as_str() {
                        "Reference" | "OptionReference" | "AnyReference" | "DocReference" => {
                            Some("target")
                        }
                        "TermReference" => Some("term"),
                        "DomainObjectReference" => Some("name"),
                        _ => None,
                    };
                    if let Some(target) = target_field
                        .and_then(|field| inner.get(field))
                        .and_then(serde_json::Value::as_str)
                    {
                        written.insert(target.to_string());
                        if let Some((_, rest)) = target.split_once(':') {
                            written.insert(rest.to_string());
                        }
                    }
                    collect(inner, written);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    collect(item, written);
                }
            }
            _ => {}
        }
    }
    let mut written = BTreeSet::new();
    for json in ast_jsons {
        let value: serde_json::Value =
            serde_json::from_str(json).context("Failed to deserialize AST")?;
        collect(&value, &mut written);
    }
    Ok(written)
}

/// The warning for one inventory line Sphinx would have skipped, in the
/// `warning: path:line: message` shape every other warning here takes.
fn malformed_line_warning(path: &str, malformed: &MalformedLine) -> String {
    format!(
        "warning: {path}:{}: {}: `{}`",
        malformed.line, malformed.reason, malformed.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_entity::EntitySchema;

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
        let indexed = process_index(&docs, "index", &EntitySchema::empty(), Vec::new()).unwrap();

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
        let indexed = process_index(&docs, "index", &EntitySchema::empty(), Vec::new()).unwrap();

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
        let indexed = process_index(&docs, "index", &EntitySchema::empty(), Vec::new()).unwrap();

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
        let indexed = process_index(&docs, "index", &EntitySchema::empty(), Vec::new()).unwrap();

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
        let index = process_index(&docs, "index", &EntitySchema::empty(), Vec::new())
            .unwrap()
            .json;

        // Then
        assert_eq!(
            index,
            r#"{"targets":{},"target_titles":{},"target_anchors":{},"document_titles":{"test.rst":"Title"},"documents":["test.rst"],"toctrees":{},"root_documents":["test.rst"],"page_order":["test.rst"],"section_numbers":{},"document_outlines":{},"glossary_terms":{},"domain_objects":{},"domain_object_spellings":{},"genindex_entries":[],"equations":{},"sectnum":{},"entities":{},"entity_backlinks":{},"entity_updates":[],"entity_update_history":{},"external_inventories":[]}"#
        );
    }

    /// An inventory file listing one Python class.
    fn declared(name: &str) -> DeclaredInventory {
        let inventory = rinx_inventory::Inventory {
            project: "Python".to_string(),
            version: "3.12".to_string(),
            entries: vec![rinx_inventory::InventoryEntry {
                name: "dict".to_string(),
                entry_type: rinx_inventory::EntryType::new("py:class").unwrap(),
                priority: 1,
                uri: "library/stdtypes.html#dict".to_string(),
                display_name: None,
            }],
        };
        DeclaredInventory {
            name: name.to_string(),
            base_url: "https://docs.python.org/3/".to_string(),
            path: format!("{name}.inv"),
            bytes: rinx_inventory::write_inventory(&inventory),
        }
    }

    #[test]
    fn test_read_external_inventories_keeps_declaration_order() {
        // Given
        let declared = vec![declared("python"), declared("numpy")];

        // When
        let (inventories, warnings) = read_external_inventories(declared).unwrap();

        // Then
        let names: Vec<&str> = inventories.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["python", "numpy"]);
        assert!(inventories[0].lookup("py:class", "dict").is_some());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_read_external_inventories_refuses_a_repeated_name() {
        // Given
        let declared = vec![declared("python"), declared("python")];

        // When
        let result = read_external_inventories(declared);

        // Then
        let message = format!("{:#}", result.unwrap_err());
        assert!(message.contains("'python' is declared twice"), "{message}");
    }

    #[test]
    fn test_read_external_inventories_refuses_an_invalid_name() {
        // Given
        let declared = vec![declared("py:thon")];

        // When
        let result = read_external_inventories(declared);

        // Then
        let message = format!("{:#}", result.unwrap_err());
        assert!(message.contains("py:thon.inv"), "{message}");
    }

    #[test]
    fn test_read_external_inventories_refuses_a_file_that_is_no_inventory() {
        // Given
        let mut not_an_inventory = declared("python");
        not_an_inventory.bytes = b"<html>404</html>\n".to_vec();

        // When
        let result = read_external_inventories(vec![not_an_inventory]);

        // Then
        let message = format!("{:#}", result.unwrap_err());
        assert!(message.contains("python.inv"), "{message}");
        assert!(message.contains("not a Sphinx inventory"), "{message}");
    }

    #[test]
    fn test_malformed_line_warning_names_file_line_and_text() {
        // Given
        let malformed = MalformedLine {
            line: 6,
            text: "garbage".to_string(),
            reason: "not five fields",
        };

        // When
        let warning = malformed_line_warning("python.inv", &malformed);

        // Then
        assert_eq!(warning, "warning: python.inv:6: not five fields: `garbage`");
    }

    #[test]
    fn test_process_index_stores_the_external_inventories() {
        // Given
        let (inventories, _) = read_external_inventories(vec![declared("python")]).unwrap();

        // When
        let indexed = process_index(&[], "index", &EntitySchema::empty(), inventories).unwrap();

        // Then
        let index: rinx_index::ProjectIndex = serde_json::from_str(&indexed.json).unwrap();
        assert_eq!(index.external_inventories.len(), 1);
    }

    #[test]
    fn test_written_reference_targets_finds_every_role_however_deeply_nested() {
        // Given — a `:ref:` inside a bullet list, a `:term:`, an `:option:`,
        // a domain role, an `:any:` and a `:doc:`, each as the parser
        // serializes it
        let doc = r#"{"path":"a.rst","nodes":[
            {"BulletList":{"bullet":"*","items":[{"nodes":[
                {"Paragraph":[{"Reference":{"target":"python:tut-intro"}}]}]}]}},
            {"Paragraph":[
                {"TermReference":{"display":"b","term":"bytecode"}},
                {"OptionReference":{"display":"-O","target":"-O"}},
                {"DomainObjectReference":{"object_type":"py:class","name":"dict","display":"dict","link":true}},
                {"AnyReference":{"target":"list","link":true}},
                {"DocReference":{"target":"tutorial/index","link":true}}
            ]}]}"#;

        // When
        let written = written_reference_targets(&[doc.to_string()]).unwrap();

        // Then — the prefixed target also yields its unprefixed part
        let expected: BTreeSet<String> = [
            "python:tut-intro",
            "tut-intro",
            "bytecode",
            "-O",
            "dict",
            "list",
            "tutorial/index",
        ]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
        assert_eq!(written, expected);
    }

    #[test]
    fn test_process_index_keeps_only_the_external_targets_a_document_names() {
        // Given — an inventory listing `dict`, and a document naming nothing
        let (inventories, _) = read_external_inventories(vec![declared("python")]).unwrap();
        let docs = vec![ast_json("a.rst", "")];

        // When
        let indexed = process_index(&docs, "index", &EntitySchema::empty(), inventories).unwrap();

        // Then
        let index: rinx_index::ProjectIndex = serde_json::from_str(&indexed.json).unwrap();
        assert!(
            index.external_inventories[0]
                .lookup("py:class", "dict")
                .is_none()
        );
    }
}

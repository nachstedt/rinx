//! The `inventory` subcommand: writes the site's `objects.inv`, so other
//! documentation sites — Sphinx or rusty-sphinx — can link into it.
//!
//! The inventory is a projection of the [`ProjectIndex`] and nothing more:
//! every entry's URI is built by the very function the renderer uses for a
//! link to the same target (`relative_doc_href`, `term_id`,
//! `build_domain_object_key`), so an entry cannot point somewhere a page's
//! own link would not.

use std::fs;

use anyhow::{Context, Result};
use rusty_sphinx_ast::{ObjectType, PyObjectType};
use rusty_sphinx_index::{ProjectIndex, TargetLocation, relative_doc_href};
use rusty_sphinx_inventory::{EntryType, Inventory, InventoryEntry, write_inventory};
use rusty_sphinx_renderer::config;

use super::cli_args::flag_value;

/// Sphinx's priority for an entry search should not list (every `std`
/// entry but an option).
const HIDDEN_FROM_SEARCH: i32 = -1;
/// Sphinx's priority for an important entry — it gives a module this.
const IMPORTANT: i32 = 0;
/// Sphinx's default priority.
const DEFAULT: i32 = 1;

pub(super) fn process_inventory(index_json: &str, config: &config::SiteConfig) -> Result<Vec<u8>> {
    let index: ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;
    Ok(write_inventory(&build_inventory(
        &index,
        &config.project,
        &config.version,
    )))
}

pub(crate) fn cmd_inventory(args: &[String]) -> Result<()> {
    let index_path = flag_value(args, "--index")?;
    let output = flag_value(args, "--output")?;
    let config_path = flag_value(args, "--config")?;

    let index_json =
        fs::read_to_string(&index_path).with_context(|| format!("Error reading '{index_path}'"))?;
    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;

    let bytes = process_inventory(&index_json, &site_config)?;
    fs::write(&output, bytes).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

/// Every target the site defines, as Sphinx would list it.
fn build_inventory(index: &ProjectIndex, project: &str, version: &str) -> Inventory {
    let mut entries = document_entries(index);
    entries.extend(label_entries(index));
    entries.extend(term_entries(index));
    entries.extend(domain_object_entries(index));
    entries.push(genindex_entry());
    Inventory {
        project: project.to_string(),
        version: version.to_string(),
        entries,
    }
}

/// One `std:doc` per page, named by its extensionless path and showing its
/// title — the entry an external `` :doc: `` resolves against.
fn document_entries(index: &ProjectIndex) -> Vec<InventoryEntry> {
    let mut paths: Vec<&String> = index
        .document_titles
        .keys()
        .chain(&index.page_order)
        .collect();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|doc_path| InventoryEntry {
            name: docname(doc_path),
            entry_type: entry_type("std:doc"),
            priority: HIDDEN_FROM_SEARCH,
            uri: page_uri(doc_path),
            display_name: index.document_titles.get(doc_path).cloned(),
        })
        .collect()
}

/// One `std:label` per internal target, showing the title a bare `:ref:`
/// shows. Unlike Sphinx, a label with no title is listed too (showing its
/// name): an entity or a `:name:`d directive is still worth linking to from
/// another site, and the author of that link can always give it a title.
/// A target with a URI names someone else's page, so it is not ours to list.
fn label_entries(index: &ProjectIndex) -> Vec<InventoryEntry> {
    index
        .targets
        .iter()
        .filter_map(|(name, location)| match location {
            TargetLocation::Internal(doc_path) => Some(InventoryEntry {
                name: name.as_str().to_string(),
                entry_type: entry_type("std:label"),
                priority: HIDDEN_FROM_SEARCH,
                uri: format!("{}#{}", page_uri(doc_path), index.target_anchor(name)),
                display_name: index.target_titles.get(name).cloned(),
            }),
            TargetLocation::External(_) => None,
        })
        .collect()
}

/// One `std:term` per glossary term.
fn term_entries(index: &ProjectIndex) -> Vec<InventoryEntry> {
    index
        .glossary_terms
        .iter()
        .map(|(term, doc_path)| InventoryEntry {
            name: term.as_str().to_string(),
            entry_type: entry_type("std:term"),
            priority: HIDDEN_FROM_SEARCH,
            uri: format!(
                "{}#{}",
                page_uri(doc_path),
                rusty_sphinx_ast::term_id(term.as_str())
            ),
            display_name: None,
        })
        .collect()
}

/// One entry per domain object and `.. option::`, named as its definition
/// spelled it — Sphinx resolves a Python name case-sensitively.
fn domain_object_entries(index: &ProjectIndex) -> Vec<InventoryEntry> {
    index
        .domain_objects
        .iter()
        .flat_map(|(key, object_types)| {
            let name = index.domain_object_spelling(key);
            object_types
                .iter()
                .map(move |(object_type, doc_path)| InventoryEntry {
                    name: name.to_string(),
                    entry_type: entry_type(&object_type.domain_qualified_str()),
                    priority: domain_object_priority(*object_type),
                    uri: format!(
                        "{}#{}",
                        page_uri(doc_path),
                        rusty_sphinx_ast::build_domain_object_key(*object_type, name).as_str()
                    ),
                    display_name: None,
                })
        })
        .collect()
}

/// Sphinx ranks a module above everything else a domain defines.
fn domain_object_priority(object_type: ObjectType) -> i32 {
    match object_type {
        ObjectType::Py(PyObjectType::Module) => IMPORTANT,
        _ => DEFAULT,
    }
}

/// The general index page, which every site has — Sphinx lists its own
/// special pages as labels the same way.
fn genindex_entry() -> InventoryEntry {
    InventoryEntry {
        name: "genindex".to_string(),
        entry_type: entry_type("std:label"),
        priority: HIDDEN_FROM_SEARCH,
        uri: "genindex.html".to_string(),
        display_name: Some("Index".to_string()),
    }
}

/// A document's page, relative to the site root.
fn page_uri(doc_path: &str) -> String {
    relative_doc_href(doc_path, "")
}

/// A document path without its source extension, which is what Sphinx calls
/// a docname.
fn docname(doc_path: &str) -> String {
    std::path::Path::new(doc_path)
        .with_extension("")
        .display()
        .to_string()
}

/// One of the fixed `domain:role` types this module writes.
fn entry_type(raw: &str) -> EntryType {
    EntryType::new(raw).expect("every entry type written here is a `domain:role` pair")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{CObjectType, StdObjectType, TargetName};
    use rusty_sphinx_inventory::read_inventory;

    fn find<'a>(inventory: &'a Inventory, entry_type: &str, name: &str) -> &'a InventoryEntry {
        inventory
            .entries
            .iter()
            .find(|entry| entry.entry_type.as_str() == entry_type && entry.name == name)
            .unwrap_or_else(|| panic!("no {entry_type} {name} in {inventory:#?}"))
    }

    #[test]
    fn test_build_inventory_lists_a_document_with_its_title() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("guide/install.rst".to_string(), "Installing".to_string());

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let entry = find(&inventory, "std:doc", "guide/install");
        assert_eq!(entry.uri, "guide/install.html");
        assert_eq!(entry.display_name.as_deref(), Some("Installing"));
        assert_eq!(entry.priority, -1);
    }

    #[test]
    fn test_build_inventory_lists_an_untitled_document_once() {
        // Given — in page order but without a title
        let index = ProjectIndex {
            page_order: vec!["notes.rst".to_string()],
            ..ProjectIndex::default()
        };

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let docs: Vec<_> = inventory
            .entries
            .iter()
            .filter(|entry| entry.entry_type.as_str() == "std:doc")
            .collect();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].display_name, None);
    }

    #[test]
    fn test_build_inventory_lists_a_label_with_its_section_title() {
        // Given
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("install"),
            TargetLocation::Internal("guide.rst".to_string()),
        );
        index
            .target_titles
            .insert(TargetName::new("install"), "Installing".to_string());

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let entry = find(&inventory, "std:label", "install");
        assert_eq!(entry.uri, "guide.html#install");
        assert_eq!(entry.display_name.as_deref(), Some("Installing"));
    }

    #[test]
    fn test_build_inventory_lists_a_label_at_its_recorded_anchor() {
        // Given — an entity, whose anchor is not its name
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("REQ_001"),
            TargetLocation::Internal("reqs.rst".to_string()),
        );
        index
            .target_anchors
            .insert(TargetName::new("REQ_001"), "entity-REQ_001".to_string());

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let entry = find(&inventory, "std:label", "req_001");
        assert_eq!(entry.uri, "reqs.html#entity-REQ_001");
    }

    #[test]
    fn test_build_inventory_skips_an_external_hyperlink_target() {
        // Given
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("python"),
            TargetLocation::External("https://python.org".to_string()),
        );

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        assert!(!inventory.entries.iter().any(|entry| entry.name == "python"));
    }

    #[test]
    fn test_build_inventory_lists_a_term_at_its_glossary_anchor() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("build step"), "glossary.rst".to_string());

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let entry = find(&inventory, "std:term", "build step");
        assert_eq!(entry.uri, "glossary.html#term-build-step");
    }

    #[test]
    fn test_build_inventory_lists_a_domain_object_under_its_written_spelling() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(PyObjectType::Class),
            "pkg.Greeter",
            "api.rst",
        );

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let entry = find(&inventory, "py:class", "pkg.Greeter");
        assert_eq!(entry.uri, "api.html#py:class:pkg.greeter");
        assert_eq!(entry.priority, 1);
    }

    #[test]
    fn test_build_inventory_lists_one_entry_per_object_type_of_a_name() {
        // Given — a C function and macro sharing a name
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::C(CObjectType::Function), "add", "c.rst");
        index.insert_domain_object(ObjectType::C(CObjectType::Macro), "add", "c.rst");

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        find(&inventory, "c:function", "add");
        find(&inventory, "c:macro", "add");
    }

    #[test]
    fn test_build_inventory_lists_an_option_as_a_cmdoption() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Std(StdObjectType::Cmdoption),
            "--verbose",
            "cli.rst",
        );

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let entry = find(&inventory, "std:cmdoption", "--verbose");
        assert_eq!(entry.uri, "cli.html#std:cmdoption:--verbose");
    }

    #[test]
    fn test_build_inventory_always_lists_the_general_index() {
        // Given
        let index = ProjectIndex::default();

        // When
        let inventory = build_inventory(&index, "Demo", "1.0");

        // Then
        let entry = find(&inventory, "std:label", "genindex");
        assert_eq!(entry.uri, "genindex.html");
    }

    #[test]
    fn test_domain_object_priority_ranks_a_module_first() {
        // Given / When / Then
        assert_eq!(
            domain_object_priority(ObjectType::Py(PyObjectType::Module)),
            0
        );
        assert_eq!(
            domain_object_priority(ObjectType::Py(PyObjectType::Function)),
            1
        );
    }

    #[test]
    fn test_docname_strips_the_source_extension() {
        // Given / When / Then
        assert_eq!(docname("guide/install.rst"), "guide/install");
    }

    #[test]
    fn test_page_uri_swaps_the_extension() {
        // Given / When / Then
        assert_eq!(page_uri("guide/install.rst"), "guide/install.html");
    }

    #[test]
    fn test_process_inventory_writes_a_readable_file_with_the_config_metadata() {
        // Given
        let index_json = r#"{"targets":{},"document_titles":{"index.rst":"Home"}}"#;
        let config: config::SiteConfig =
            toml::from_str("project = \"Demo\"\nversion = \"2.0\"\n").unwrap();

        // When
        let bytes = process_inventory(index_json, &config).unwrap();

        // Then
        let read = read_inventory(&bytes).unwrap();
        assert_eq!(read.inventory.project, "Demo");
        assert_eq!(read.inventory.version, "2.0");
        assert!(
            read.inventory
                .entries
                .iter()
                .any(|entry| entry.name == "index")
        );
    }

    #[test]
    fn test_process_inventory_refuses_a_malformed_index() {
        // Given
        let config: config::SiteConfig = toml::from_str("project = \"Demo\"\n").unwrap();

        // When
        let result = process_inventory("not json", &config);

        // Then
        assert!(result.is_err());
    }
}

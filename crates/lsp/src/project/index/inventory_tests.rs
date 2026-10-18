//! A project's intersphinx inventories: joining its index, and its findings.

use super::test_support::*;
use super::*;

/// A temporary file holding an inventory that lists the Python class
/// `dict`.
fn inventory_file(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rinx_lsp_project_{name}.inv"));
    let inventory = rinx_inventory::Inventory {
        project: "Python".to_string(),
        version: "3".to_string(),
        entries: vec![rinx_inventory::InventoryEntry {
            name: "dict".to_string(),
            entry_type: rinx_inventory::EntryType::new("py:class").unwrap(),
            priority: 1,
            uri: "library/stdtypes.html#$".to_string(),
            display_name: None,
        }],
    };
    std::fs::write(&path, rinx_inventory::write_inventory(&inventory)).unwrap();
    path
}

const PYTHON_CONF: &str = "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'python': ('https://docs.python.org/3/', None)}\n";

#[test]
fn test_a_downloaded_inventory_joins_the_index_and_leaves_the_strictness() {
    // Given
    let mut project = sphinx(PYTHON_CONF);
    project.advance_inventories(&|_: &str| None);
    assert_eq!(
        project.awaited_inventories(),
        ["https://docs.python.org/3/objects.inv"]
    );
    assert!(project.project_index().external_inventories.is_empty());
    let generation = project.generation();
    let path = inventory_file("downloaded");

    // When
    let changed = project.advance_inventories(&|_: &str| Some(Ok(path.clone())));

    // Then — and still after a document changes the fold
    assert!(changed);
    assert!(project.generation() > generation);
    let inventories = &project.project_index().external_inventories;
    assert_eq!(inventories.len(), 1);
    assert!(inventories[0].lookup("py:class", "dict").is_some());
    project.record("a.rst".to_string(), labelled("a.rst", "a"));
    assert_eq!(project.project_index().external_inventories.len(), 1);
    assert_eq!(
        project.strictness(),
        &project.model().settings.strictness.clone()
    );
    assert_eq!(project.inventory_cache_limit_days(), 5);
}

#[test]
fn test_an_unreadable_inventory_is_one_of_the_conf_findings() {
    // Given
    let mut project = sphinx(
        "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'a': ('https://a.org', 'missing.inv')}\n",
    );

    // When
    let changed = project.advance_inventories(&|_: &str| None);

    // Then
    assert!(changed);
    let (findings, _) = project.conf_findings();
    let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
    assert_eq!(codes, ["conf.inventory-unreadable"]);
    assert_ne!(
        project.strictness(),
        &project.model().settings.strictness.clone()
    );
}

#[test]
fn test_reload_inventory_file_reads_a_project_file_again() {
    // Given
    let mut project = sphinx(
        "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'a': ('https://a.org', 'a.inv')}\n",
    );
    project.advance_inventories(&|_: &str| None);

    // When / Then
    assert!(project.reload_inventory_file(Path::new("/work/docs/a.inv")));
    assert!(!project.reload_inventory_file(Path::new("/work/docs/b.inv")));
}

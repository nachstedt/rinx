//! Fixtures shared by the test modules of several sibling trees — the
//! resolvers under `resolution/` and the role renderers under `inline/` both
//! need a project index that links into another site.

use rusty_sphinx_index::{ExternalInventory, ProjectIndex};
use rusty_sphinx_inventory::{EntryType, Inventory, InventoryEntry, InventoryName};

/// An index with no documents of its own and one declared inventory,
/// `python`, published at `https://docs.python.org/3/` and listing one
/// target of each kind a role can reach.
pub(crate) fn index_linking_into_python() -> ProjectIndex {
    let entry = |name: &str, entry_type: &str, uri: &str, display: Option<&str>| InventoryEntry {
        name: name.to_string(),
        entry_type: EntryType::new(entry_type).unwrap(),
        priority: 1,
        uri: uri.to_string(),
        display_name: display.map(str::to_string),
    };
    ProjectIndex {
        external_inventories: vec![ExternalInventory::new(
            InventoryName::new("python").unwrap(),
            "https://docs.python.org/3/".to_string(),
            Inventory {
                project: "Python".to_string(),
                version: "3.12".to_string(),
                entries: vec![
                    entry("dict", "py:class", "library/stdtypes.html#dict", None),
                    entry(
                        "ValueError",
                        "py:exception",
                        "library/exceptions.html#ValueError",
                        None,
                    ),
                    entry(
                        "tut-intro",
                        "std:label",
                        "tutorial/introduction.html#tut-intro",
                        Some("An Informal Introduction to Python"),
                    ),
                    entry("bytecode", "std:term", "glossary.html#term-bytecode", None),
                    entry(
                        "-O",
                        "std:cmdoption",
                        "using/cmdline.html#cmdoption-O",
                        None,
                    ),
                ],
            },
        )],
        ..ProjectIndex::default()
    }
}

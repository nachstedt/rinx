//! An inventory and the entries it lists.

use serde::{Deserialize, Serialize};

use crate::EntryType;

/// One inventory: the project it describes and every target it lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    /// The `# Project:` header line.
    pub project: String,
    /// The `# Version:` header line.
    pub version: String,
    pub entries: Vec<InventoryEntry>,
}

/// One cross-reference target an inventory lists.
///
/// Held *expanded*: `uri` never carries the file's `$` shorthand for "the
/// name again", and `display_name` is `None` rather than the file's `-` for
/// "same as the name". Both abbreviations are the file format's business, so
/// only the reader and writer ever see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryEntry {
    /// The target's name, e.g. `pkg.Greeter` or `build step` — it may hold
    /// spaces.
    pub name: String,
    pub entry_type: EntryType,
    /// Sphinx's search priority: `-1` hides the entry from search, `0` is
    /// important, `1` the default and `2` unimportant.
    pub priority: i32,
    /// Where the target lives, relative to the site root, anchor included:
    /// `api.html#pkg.Greeter`.
    pub uri: String,
    /// The text a reference to the target shows when the author wrote none,
    /// or `None` when that is the name itself.
    pub display_name: Option<String>,
}

impl InventoryEntry {
    /// The text a reference to this entry shows when the author wrote none.
    #[must_use]
    pub fn display_text(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(display_name: Option<&str>) -> InventoryEntry {
        InventoryEntry {
            name: "intro".to_string(),
            entry_type: EntryType::new("std:label").unwrap(),
            priority: -1,
            uri: "index.html#intro".to_string(),
            display_name: display_name.map(str::to_string),
        }
    }

    #[test]
    fn test_display_text_prefers_the_display_name() {
        // Given
        let entry = entry(Some("Introduction"));

        // When / Then
        assert_eq!(entry.display_text(), "Introduction");
    }

    #[test]
    fn test_display_text_falls_back_to_the_name() {
        // Given
        let entry = entry(None);

        // When / Then
        assert_eq!(entry.display_text(), "intro");
    }
}

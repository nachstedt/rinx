//! Which inventories a cross-reference may resolve through.

use rinx_inventory::InventoryName;
use serde::{Deserialize, Serialize};

/// Where a cross-reference role may find its target, as its spelling says.
///
/// Recorded on the node rather than left in the role name, because the
/// `:external:` prefix is markup, not part of what is being referenced: the
/// parser strips it once and every later phase reads the intent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventorySelector {
    /// The ordinary role: this site's own documents first, then — if none
    /// defines the target — every declared inventory in order.
    #[default]
    Any,
    /// `:external:role:` — only the declared inventories, never this site.
    ExternalOnly,
    /// `:external+name:role:` — only the inventory declared as `name`.
    Named(InventoryName),
}

impl InventorySelector {
    /// Whether this is the ordinary role, which a `.ast` file leaves unsaid.
    #[must_use]
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Whether this site's own documents may define the target.
    #[must_use]
    pub fn allows_local(&self) -> bool {
        self.is_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_any() {
        // Given / When / Then
        assert!(InventorySelector::default().is_any());
    }

    #[test]
    fn test_allows_local_only_for_the_ordinary_role() {
        // Given / When / Then
        assert!(InventorySelector::Any.allows_local());
        assert!(!InventorySelector::ExternalOnly.allows_local());
        assert!(!InventorySelector::Named(InventoryName::new("python").unwrap()).allows_local());
    }

    #[test]
    fn test_serialization_roundtrip_of_a_named_inventory() {
        // Given
        let selector = InventorySelector::Named(InventoryName::new("python").unwrap());

        // When
        let json = serde_json::to_string(&selector).unwrap();
        let back: InventorySelector = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(json, r#"{"named":"python"}"#);
        assert_eq!(back, selector);
    }
}

use serde::{Deserialize, Serialize};

/// Which of Sphinx's two target-resolution orders a domain-object role asked
/// for, selected by whether its target was written with a leading `.`.
///
/// Sphinx documents the two orders at
/// <https://www.sphinx-doc.org/en/master/usage/domains/python.html#target-resolution>:
/// a plain target is searched "without any further qualification, then with
/// the current module name prepended, then with the current module and class
/// name (if any) prepended", and "if you prefix the name with a dot (`.`),
/// this order is reversed" — so `` :py:func:`open` `` in the `codecs` docs
/// means the builtin, while `` :py:func:`.open` `` means `codecs.open`.
///
/// Carried as parsed intent rather than as a leading `.` left in the target
/// text: the dot is markup, and a name that still contains it can never match
/// an index key (every qualified name is built by joining dot-free segments).
///
/// [`Self::LeastQualifiedFirst`] is the [`Default`] so that `.ast` files
/// written before this field existed deserialize to the unprefixed meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSearchOrder {
    /// No leading dot: try the target verbatim first, then progressively more
    /// of the enclosing scope. Global names win over nearby ones.
    #[default]
    LeastQualifiedFirst,
    /// A leading dot: try the full enclosing scope first and work outwards,
    /// then — if nothing matched exactly — fall back to searching the target
    /// as a dotted *suffix* of any indexed object name.
    MostQualifiedFirst,
}

impl TargetSearchOrder {
    /// Whether an exhausted exact search should fall back to Sphinx's "fuzzy"
    /// suffix search. Only dot-prefixed targets do: "if the name is prefixed
    /// with a dot, and no exact match is found, the target is taken as a
    /// suffix and all object names with that suffix are searched".
    #[must_use]
    pub const fn allows_suffix_search(self) -> bool {
        matches!(self, Self::MostQualifiedFirst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_search_order_defaults_to_least_qualified_first() {
        // Given / When
        let order = TargetSearchOrder::default();

        // Then — an absent field means "no leading dot was written".
        assert_eq!(order, TargetSearchOrder::LeastQualifiedFirst);
    }

    #[test]
    fn test_only_most_qualified_first_allows_suffix_search() {
        // Given / When / Then
        assert!(TargetSearchOrder::MostQualifiedFirst.allows_suffix_search());
        assert!(!TargetSearchOrder::LeastQualifiedFirst.allows_suffix_search());
    }

    #[test]
    fn test_target_search_order_serialization_roundtrip() {
        // Given
        let order = TargetSearchOrder::MostQualifiedFirst;

        // When
        let json = serde_json::to_string(&order).expect("Failed to serialize");
        let deserialized: TargetSearchOrder =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(json, "\"most_qualified_first\"");
        assert_eq!(order, deserialized);
    }
}

use serde::{Deserialize, Serialize};

/// A normalized reST target name.
///
/// reST target names are case-insensitive and all internal whitespace
/// is collapsed to a single space. This opaque type enforces that invariant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TargetName(String);

impl TargetName {
    /// Creates a new `TargetName`, applying whitespace collapse and lowercasing.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let normalized = raw
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        Self(normalized)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_name_normalization() {
        // Given
        let raw = "  My   Target  Name  ";

        // When
        let target = TargetName::new(raw);

        // Then
        assert_eq!(target.as_str(), "my target name");
    }

    #[test]
    fn test_target_name_equality() {
        // Given
        let raw1 = "My Target";
        let raw2 = "my   target";
        let raw3 = "  MY TARGET  ";
        let raw4 = "Different Target";

        // When
        let target1 = TargetName::new(raw1);
        let target2 = TargetName::new(raw2);
        let target3 = TargetName::new(raw3);
        let target4 = TargetName::new(raw4);

        // Then
        assert_eq!(target1, target2);
        assert_eq!(target1, target3);
        assert_eq!(target2, target3);

        assert_ne!(target1, target4);
    }
}

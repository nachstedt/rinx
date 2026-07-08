use serde::{Deserialize, Serialize};

/// Distinguishes the three Sphinx version-change directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionChangeKind {
    Added,
    Changed,
    Deprecated,
}

impl VersionChangeKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Added => "versionadded",
            Self::Changed => "versionchanged",
            Self::Deprecated => "deprecated",
        }
    }
}

impl std::str::FromStr for VersionChangeKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "versionadded" => Ok(Self::Added),
            "versionchanged" => Ok(Self::Changed),
            "deprecated" => Ok(Self::Deprecated),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for VersionChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_change_kind_serialization_roundtrip() {
        // Given
        let kind = VersionChangeKind::Deprecated;

        // When
        let json = serde_json::to_string(&kind).expect("Failed to serialize");
        let deserialized: VersionChangeKind =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(json, "\"deprecated\"");
        assert_eq!(kind, deserialized);
    }
}

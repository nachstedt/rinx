//! What a `:cwe:` role points at — `787`, `787#Demonstrative_Examples` — as
//! written between its backticks, or behind an explicit title's angle
//! brackets.
//!
//! The number-and-fragment shape `:pep:` has; Sphinx links the number
//! unpadded as `787.html` below a fixed address.

use serde::{Deserialize, Serialize};

use super::invalid::InvalidRegistryTarget;
use super::number::NumberedTarget;
use super::registry::Registry;

/// A CWE number with an optional fragment, as a `:cwe:` role wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CweTarget(NumberedTarget);

impl CweTarget {
    /// Reads a written target, splitting off a fragment at the first `#`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidRegistryTarget`] when the part before the `#` is not
    /// a number.
    pub fn parse(written: &str) -> Result<Self, InvalidRegistryTarget> {
        NumberedTarget::parse(Registry::Cwe, written).map(Self)
    }

    /// The target as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        self.0.as_written()
    }

    /// The weakness's number.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.0.number()
    }

    /// The weakness's page below the CWE definitions, as Sphinx builds it:
    /// the number unpadded, `.html`, then any fragment — `787.html`.
    #[must_use]
    pub fn page_path(&self) -> String {
        self.0.with_fragment(format!("{}.html", self.number()))
    }
}

impl TryFrom<String> for CweTarget {
    type Error = InvalidRegistryTarget;

    fn try_from(written: String) -> Result<Self, Self::Error> {
        Self::parse(&written)
    }
}

impl From<CweTarget> for String {
    fn from(target: CweTarget) -> Self {
        target.as_written().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry_target::InvalidRegistryTargetReason;

    #[test]
    fn test_parse_reads_a_number() {
        // Given / When
        let target = CweTarget::parse("0787").unwrap();

        // Then
        assert_eq!(target.number(), 787);
        assert_eq!(target.as_written(), "0787");
    }

    #[test]
    fn test_parse_refuses_a_prefixed_id() {
        // Given / When
        let error = CweTarget::parse("CWE-787").unwrap_err();

        // Then
        assert_eq!(error.registry(), Registry::Cwe);
        assert_eq!(error.reason(), InvalidRegistryTargetReason::NotANumber);
    }

    #[test]
    fn test_page_path_is_the_unpadded_number_with_the_fragment() {
        // Given / When / Then
        assert_eq!(CweTarget::parse("0787").unwrap().page_path(), "787.html");
        assert_eq!(CweTarget::parse("787#x").unwrap().page_path(), "787.html#x");
    }

    #[test]
    fn test_serde_round_trips_the_written_text() {
        // Given
        let target = CweTarget::parse("787#x").unwrap();

        // When
        let json = serde_json::to_string(&target).unwrap();

        // Then
        assert_eq!(json, "\"787#x\"");
        assert_eq!(serde_json::from_str::<CweTarget>(&json).unwrap(), target);
    }
}

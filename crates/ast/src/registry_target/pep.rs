//! What a `:pep:` role points at — `8`, `8#naming-conventions` — as written
//! between its backticks, or behind an explicit title's angle brackets.
//!
//! "Parse, don't validate": a [`PepTarget`] always holds a PEP number, so
//! nothing that builds a link from one meets a target it cannot link.

use serde::{Deserialize, Serialize};

use super::invalid::InvalidRegistryTarget;
use super::number::NumberedTarget;
use super::registry::Registry;

/// A PEP number with an optional fragment, as a `:pep:` role wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PepTarget(NumberedTarget);

impl PepTarget {
    /// Reads a written target, splitting off a fragment at the first `#`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidRegistryTarget`] when the part before the `#` is not
    /// a number.
    pub fn parse(written: &str) -> Result<Self, InvalidRegistryTarget> {
        NumberedTarget::parse(Registry::Pep, written).map(Self)
    }

    /// The target as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        self.0.as_written()
    }

    /// The PEP's number.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.0.number()
    }

    /// The anchor after the `#`, which may be empty; `None` without a `#`.
    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    /// The PEP's page below the PEP index, as Sphinx builds it: the number
    /// padded to four digits, a trailing slash, then any fragment —
    /// `pep-0008/`, `pep-0008/#naming`.
    #[must_use]
    pub fn page_path(&self) -> String {
        self.0.with_fragment(format!("pep-{:04}/", self.number()))
    }
}

impl TryFrom<String> for PepTarget {
    type Error = InvalidRegistryTarget;

    fn try_from(written: String) -> Result<Self, Self::Error> {
        Self::parse(&written)
    }
}

impl From<PepTarget> for String {
    fn from(target: PepTarget) -> Self {
        target.as_written().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry_target::InvalidRegistryTargetReason;

    #[test]
    fn test_parse_reads_a_bare_number() {
        // Given / When
        let target = PepTarget::parse("8").unwrap();

        // Then
        assert_eq!(target.number(), 8);
        assert_eq!(target.fragment(), None);
        assert_eq!(target.as_written(), "8");
    }

    #[test]
    fn test_parse_keeps_leading_zeros_as_written() {
        // Given / When
        let target = PepTarget::parse("0008").unwrap();

        // Then
        assert_eq!(target.number(), 8);
        assert_eq!(target.as_written(), "0008");
    }

    #[test]
    fn test_parse_keeps_an_empty_fragment() {
        // Given / When / Then
        assert_eq!(PepTarget::parse("8#").unwrap().fragment(), Some(""));
    }

    #[test]
    fn test_parse_refuses_what_is_not_digits() {
        // Given / When
        let error = PepTarget::parse("abc").unwrap_err();

        // Then
        assert_eq!(error.registry(), Registry::Pep);
        assert_eq!(error.reason(), InvalidRegistryTargetReason::NotANumber);
        assert!(
            error.to_string().contains("invalid PEP number 'abc'"),
            "{error}"
        );
    }

    #[test]
    fn test_page_path_pads_the_number_to_four_digits() {
        // Given / When / Then
        assert_eq!(PepTarget::parse("8").unwrap().page_path(), "pep-0008/");
        assert_eq!(PepTarget::parse("12345").unwrap().page_path(), "pep-12345/");
    }

    #[test]
    fn test_page_path_appends_the_fragment() {
        // Given / When / Then
        assert_eq!(
            PepTarget::parse("8#naming").unwrap().page_path(),
            "pep-0008/#naming"
        );
    }

    #[test]
    fn test_serde_round_trips_the_written_text() {
        // Given
        let target = PepTarget::parse("08#x").unwrap();

        // When
        let json = serde_json::to_string(&target).unwrap();
        let back: PepTarget = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(json, "\"08#x\"");
        assert_eq!(back, target);
    }

    #[test]
    fn test_deserialize_refuses_a_target_that_is_no_number() {
        // Given / When
        let error = serde_json::from_str::<PepTarget>("\"x\"").unwrap_err();

        // Then
        assert!(error.to_string().contains("invalid PEP number"), "{error}");
    }
}

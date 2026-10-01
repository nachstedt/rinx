//! What a `:cve:` role points at — `2024-3094`, `2024-3094#references` — as
//! written between its backticks, or behind an explicit title's angle
//! brackets.
//!
//! **A deliberate deviation.** Sphinx's `CVE` role reads no number: it
//! appends the target, whatever it is, to `…?id=CVE-`, so it never refuses
//! one. The likeliest mistake, writing the identifier in full as
//! `CVE-2024-3094`, then links `id=CVE-CVE-2024-3094` and shows "CVE
//! CVE-2024-3094" without a word of warning. Here a target is the
//! identifier's year and sequence number — four ASCII digits, a `-`, and at
//! least four more, as the CVE numbering scheme writes them — with an
//! optional `#` and anchor, and a repeated `CVE-` prefix is refused with a
//! message saying so.

use serde::{Deserialize, Serialize};

use super::invalid::{InvalidRegistryTarget, InvalidRegistryTargetReason};
use super::number::split_fragment;
use super::registry::Registry;

/// The fewest digits a CVE sequence number is written with.
const MIN_SEQUENCE_DIGITS: usize = 4;

/// A CVE identifier without its `CVE-` prefix, with an optional fragment, as
/// a `:cve:` role wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CveTarget {
    written: String,
}

impl CveTarget {
    /// Reads a written target: a year and a sequence number, then an
    /// optional `#` and anchor.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidRegistryTarget`] when the part before any `#` starts
    /// with a `CVE-` prefix, or is not a year and a sequence number.
    pub fn parse(written: &str) -> Result<Self, InvalidRegistryTarget> {
        let (id, _) = split_fragment(written);
        let refuse = |reason| Err(InvalidRegistryTarget::new(Registry::Cve, written, reason));
        if id
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("CVE-"))
        {
            return refuse(InvalidRegistryTargetReason::CvePrefix);
        }
        if !is_year_and_sequence(id) {
            return refuse(InvalidRegistryTargetReason::NotACveId);
        }
        Ok(Self {
            written: written.to_string(),
        })
    }

    /// The target as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        &self.written
    }

    /// The record's address below the CVE record lookup, as Sphinx builds
    /// it: the prefixed identifier, then any fragment —
    /// `CVE-2024-3094#references`.
    #[must_use]
    pub fn page_path(&self) -> String {
        format!("CVE-{}", self.written)
    }
}

/// Whether `id` is four ASCII digits, a `-`, and at least
/// [`MIN_SEQUENCE_DIGITS`] more.
fn is_year_and_sequence(id: &str) -> bool {
    let all_digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    id.split_once('-').is_some_and(|(year, sequence)| {
        year.len() == 4
            && all_digits(year)
            && sequence.len() >= MIN_SEQUENCE_DIGITS
            && all_digits(sequence)
    })
}

impl TryFrom<String> for CveTarget {
    type Error = InvalidRegistryTarget;

    fn try_from(written: String) -> Result<Self, Self::Error> {
        Self::parse(&written)
    }
}

impl From<CveTarget> for String {
    fn from(target: CveTarget) -> Self {
        target.written
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(written: &str) -> InvalidRegistryTargetReason {
        CveTarget::parse(written).unwrap_err().reason()
    }

    #[test]
    fn test_parse_reads_a_year_and_sequence_number() {
        // Given / When / Then
        for written in [
            "2024-3094",
            "2021-44228",
            "1999-0001",
            "2024-3094#refs",
            "2024-3094#",
        ] {
            assert_eq!(
                CveTarget::parse(written).unwrap().as_written(),
                written,
                "{written}"
            );
        }
    }

    #[test]
    fn test_parse_refuses_a_cve_prefix_in_any_case() {
        // Given / When / Then
        for written in ["CVE-2024-3094", "cve-2024-3094", "Cve-x"] {
            assert_eq!(
                reason(written),
                InvalidRegistryTargetReason::CvePrefix,
                "{written}"
            );
        }
    }

    #[test]
    fn test_parse_refuses_what_is_not_a_year_and_sequence_number() {
        // Given / When / Then
        for written in [
            "",
            "#refs",
            "2024",
            "2024-",
            "2024-123",
            "24-3094",
            "20244-3094",
            "2024-3094-1",
            "2024_3094",
            "２０２４-3094",
            "log4shell",
        ] {
            assert_eq!(
                reason(written),
                InvalidRegistryTargetReason::NotACveId,
                "{written:?}"
            );
        }
    }

    #[test]
    fn test_parse_error_names_the_registry() {
        // Given / When
        let error = CveTarget::parse("CVE-2024-3094").unwrap_err();

        // Then
        assert_eq!(error.registry(), Registry::Cve);
        assert!(
            error
                .to_string()
                .contains("invalid CVE number 'CVE-2024-3094'"),
            "{error}"
        );
    }

    #[test]
    fn test_page_path_prefixes_the_identifier_and_keeps_the_fragment() {
        // Given / When / Then
        assert_eq!(
            CveTarget::parse("2024-3094").unwrap().page_path(),
            "CVE-2024-3094"
        );
        assert_eq!(
            CveTarget::parse("2024-3094#refs").unwrap().page_path(),
            "CVE-2024-3094#refs"
        );
    }

    #[test]
    fn test_is_year_and_sequence_checks_both_parts() {
        // Given / When / Then
        assert!(is_year_and_sequence("2024-0001"));
        assert!(is_year_and_sequence("2024-1234567"));
        assert!(!is_year_and_sequence("2024-001"));
        assert!(!is_year_and_sequence("202-0001"));
        assert!(!is_year_and_sequence("20240001"));
    }

    #[test]
    fn test_serde_round_trips_the_written_text() {
        // Given
        let target = CveTarget::parse("2024-3094").unwrap();

        // When
        let json = serde_json::to_string(&target).unwrap();

        // Then
        assert_eq!(json, "\"2024-3094\"");
        assert_eq!(serde_json::from_str::<CveTarget>(&json).unwrap(), target);
        assert!(serde_json::from_str::<CveTarget>("\"CVE-1\"").is_err());
    }
}

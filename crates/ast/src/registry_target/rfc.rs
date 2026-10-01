//! What an `:rfc:` role points at — `2324`, `2324#section-2.3.2` — as written
//! between its backticks, or behind an explicit title's angle brackets.
//!
//! The same number-and-fragment shape `:pep:` has, with two differences
//! Sphinx makes: the page is `rfc2324.html`, unpadded, and the text a link
//! shows names a section, appendix or page anchor in words — `RFC 2324
//! Section 2.3.2` — which is also the role's general-index subentry.

use serde::{Deserialize, Serialize};

use super::invalid::InvalidRegistryTarget;
use super::number::NumberedTarget;
use super::registry::Registry;

/// The anchor kinds Sphinx's `_format_rfc_target` spells out in words, as
/// written in an anchor and as shown.
const NAMED_ANCHORS: [(&str, &str); 3] = [
    ("appendix", "Appendix"),
    ("page", "Page"),
    ("section", "Section"),
];

/// An RFC number with an optional fragment, as an `:rfc:` role wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RfcTarget(NumberedTarget);

impl RfcTarget {
    /// Reads a written target, splitting off a fragment at the first `#`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidRegistryTarget`] when the part before the `#` is not
    /// a number.
    pub fn parse(written: &str) -> Result<Self, InvalidRegistryTarget> {
        NumberedTarget::parse(Registry::Rfc, written).map(Self)
    }

    /// The target as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        self.0.as_written()
    }

    /// The RFC's number.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.0.number()
    }

    /// The anchor after the `#`, which may be empty; `None` without a `#`.
    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    /// What a link to the RFC shows without an explicit title, and what its
    /// general-index subentry reads — Sphinx's `_format_rfc_target`. An
    /// anchor beginning `section`, `appendix` or `page` is spelled out, its
    /// remainder after the first `-` following: `2324#section-2.3.2` reads
    /// "RFC 2324 Section 2.3.2". Any other target reads `RFC ` and the target
    /// as written, anchor and all.
    #[must_use]
    pub fn display_text(&self) -> String {
        let number = self.0.written_number();
        if let Some(anchor) = self.fragment() {
            let (first, remaining) = anchor.split_once('-').unwrap_or((anchor, ""));
            if let Some((_, word)) = NAMED_ANCHORS.iter().find(|(name, _)| *name == first) {
                return if remaining.is_empty() {
                    format!("RFC {number} {word}")
                } else {
                    format!("RFC {number} {word} {remaining}")
                };
            }
        }
        format!("RFC {}", self.as_written())
    }

    /// The RFC's page below the RFC index, as Sphinx builds it from
    /// docutils' `rfc%d.html`: the number unpadded, then any fragment —
    /// `rfc2324.html`, `rfc2324.html#section-2.3`.
    #[must_use]
    pub fn page_path(&self) -> String {
        self.0.with_fragment(format!("rfc{}.html", self.number()))
    }
}

impl TryFrom<String> for RfcTarget {
    type Error = InvalidRegistryTarget;

    fn try_from(written: String) -> Result<Self, Self::Error> {
        Self::parse(&written)
    }
}

impl From<RfcTarget> for String {
    fn from(target: RfcTarget) -> Self {
        target.as_written().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry_target::InvalidRegistryTargetReason;

    fn display(written: &str) -> String {
        RfcTarget::parse(written).unwrap().display_text()
    }

    #[test]
    fn test_parse_reads_a_number_and_a_fragment() {
        // Given / When
        let target = RfcTarget::parse("2324#section-2").unwrap();

        // Then
        assert_eq!(target.number(), 2324);
        assert_eq!(target.fragment(), Some("section-2"));
        assert_eq!(target.as_written(), "2324#section-2");
    }

    #[test]
    fn test_parse_refuses_what_is_not_a_number() {
        // Given / When
        let error = RfcTarget::parse("HTTP").unwrap_err();

        // Then
        assert_eq!(error.registry(), Registry::Rfc);
        assert_eq!(error.reason(), InvalidRegistryTargetReason::NotANumber);
        assert!(
            error.to_string().contains("invalid RFC number 'HTTP'"),
            "{error}"
        );
    }

    #[test]
    fn test_display_text_of_a_bare_number_keeps_it_as_written() {
        // Given / When / Then
        assert_eq!(display("2324"), "RFC 2324");
        assert_eq!(display("02324"), "RFC 02324");
    }

    #[test]
    fn test_display_text_spells_out_a_named_anchor() {
        // Given / When / Then
        assert_eq!(display("2324#section-2.3.2"), "RFC 2324 Section 2.3.2");
        assert_eq!(display("2324#appendix-A"), "RFC 2324 Appendix A");
        assert_eq!(display("2324#page-12"), "RFC 2324 Page 12");
    }

    #[test]
    fn test_display_text_of_a_named_anchor_without_remainder_is_the_word() {
        // Given / When / Then
        assert_eq!(display("2324#section"), "RFC 2324 Section");
        assert_eq!(display("2324#section-"), "RFC 2324 Section");
    }

    #[test]
    fn test_display_text_keeps_any_other_anchor_as_written() {
        // Given / When / Then
        assert_eq!(display("2324#intro"), "RFC 2324#intro");
        assert_eq!(display("2324#Section-1"), "RFC 2324#Section-1");
        assert_eq!(display("2324#"), "RFC 2324#");
    }

    #[test]
    fn test_page_path_is_unpadded_with_the_fragment() {
        // Given / When / Then
        assert_eq!(RfcTarget::parse("0022").unwrap().page_path(), "rfc22.html");
        assert_eq!(
            RfcTarget::parse("2324#section-2").unwrap().page_path(),
            "rfc2324.html#section-2"
        );
    }

    #[test]
    fn test_serde_round_trips_the_written_text() {
        // Given
        let target = RfcTarget::parse("2324#page-3").unwrap();

        // When
        let json = serde_json::to_string(&target).unwrap();

        // Then
        assert_eq!(json, "\"2324#page-3\"");
        assert_eq!(serde_json::from_str::<RfcTarget>(&json).unwrap(), target);
    }
}

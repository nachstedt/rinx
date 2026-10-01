//! What docutils' `:rfc-reference:` role points at — `2822`,
//! `2822#section-3` — as written between its backticks.
//!
//! "Parse, don't validate": a [`DocutilsRfcNumber`] always holds a number
//! docutils would link. docutils' `rfc_reference_role` splits the text at its
//! first `#` and hands the part before it to Python's `int()`, refusing
//! anything below 1. Unlike `:pep-reference:` a section is allowed, but
//! there is still no explicit title: `Title <2822>` is not a number.
//!
//! Only ASCII digits are a number, for the reason [`crate::PepTarget`] gives,
//! and through the same reader.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::registry_target::read_ascii_number;

/// An RFC number with an optional section, as an `:rfc-reference:` role
/// wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DocutilsRfcNumber {
    written: String,
    number: u32,
    /// Where the section starts in `written`, after its `#`.
    section_start: Option<usize>,
}

/// Why a written `:rfc-reference:` target was refused: its part before any
/// `#` is not a number of at least 1. Holds the target as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidDocutilsRfcNumber(pub String);

impl fmt::Display for InvalidDocutilsRfcNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // docutils' own wording, so both builds report the same thing.
        write!(
            f,
            "RFC number must be a number greater than or equal to 1; \"{}\" is invalid",
            self.0
        )
    }
}

impl std::error::Error for InvalidDocutilsRfcNumber {}

impl DocutilsRfcNumber {
    /// Reads a written target, splitting off a section at the first `#`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDocutilsRfcNumber`] when the part before the `#` is
    /// not a number of at least 1.
    pub fn parse(written: &str) -> Result<Self, InvalidDocutilsRfcNumber> {
        let (digits, section_start) = match written.split_once('#') {
            Some((digits, _)) => (digits, Some(digits.len() + 1)),
            None => (written, None),
        };
        match read_ascii_number(digits) {
            Ok(number) if number >= 1 => Ok(Self {
                written: written.to_string(),
                number,
                section_start,
            }),
            _ => Err(InvalidDocutilsRfcNumber(written.to_string())),
        }
    }

    /// The target as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        &self.written
    }

    /// The RFC's number.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }

    /// What the link shows: `RFC ` and the number as `int()` reads it, so
    /// leading zeros are dropped, and never the section — `RFC 2822`.
    #[must_use]
    pub fn display_text(&self) -> String {
        format!("RFC {}", self.number)
    }

    /// The RFC's page below the RFC index, as docutils builds it: the number
    /// unpadded as `rfc%d.html`, then any section — `rfc2822.html#section-3`.
    #[must_use]
    pub fn page_path(&self) -> String {
        let page = format!("rfc{}.html", self.number);
        match self.section_start {
            Some(start) => format!("{page}#{}", &self.written[start..]),
            None => page,
        }
    }
}

impl TryFrom<String> for DocutilsRfcNumber {
    type Error = InvalidDocutilsRfcNumber;

    fn try_from(written: String) -> Result<Self, Self::Error> {
        Self::parse(&written)
    }
}

impl From<DocutilsRfcNumber> for String {
    fn from(number: DocutilsRfcNumber) -> Self {
        number.written
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_a_number_and_a_section() {
        // Given / When
        let number = DocutilsRfcNumber::parse("02822#section-3").unwrap();

        // Then
        assert_eq!(number.number(), 2822);
        assert_eq!(number.as_written(), "02822#section-3");
    }

    #[test]
    fn test_parse_refuses_zero_and_what_is_not_digits() {
        // Given / When / Then — no explicit title either
        for written in [
            "0",
            "",
            "#x",
            "abc",
            "Title <2822>",
            "+1",
            " 1",
            "99999999999",
        ] {
            assert_eq!(
                DocutilsRfcNumber::parse(written),
                Err(InvalidDocutilsRfcNumber(written.to_string())),
                "{written:?}"
            );
        }
    }

    #[test]
    fn test_display_text_normalizes_the_number_and_drops_the_section() {
        // Given / When / Then
        assert_eq!(
            DocutilsRfcNumber::parse("02822#section-3")
                .unwrap()
                .display_text(),
            "RFC 2822"
        );
    }

    #[test]
    fn test_page_path_appends_the_section_even_an_empty_one() {
        // Given / When / Then
        assert_eq!(
            DocutilsRfcNumber::parse("0022").unwrap().page_path(),
            "rfc22.html"
        );
        assert_eq!(
            DocutilsRfcNumber::parse("22#s").unwrap().page_path(),
            "rfc22.html#s"
        );
        assert_eq!(
            DocutilsRfcNumber::parse("22#").unwrap().page_path(),
            "rfc22.html#"
        );
    }

    #[test]
    fn test_invalid_number_uses_docutils_wording() {
        // Given / When
        let message = InvalidDocutilsRfcNumber("abc".to_string()).to_string();

        // Then
        assert_eq!(
            message,
            "RFC number must be a number greater than or equal to 1; \"abc\" is invalid"
        );
    }

    #[test]
    fn test_serde_round_trips_the_written_text() {
        // Given
        let number = DocutilsRfcNumber::parse("22#s").unwrap();

        // When
        let json = serde_json::to_string(&number).unwrap();

        // Then
        assert_eq!(json, "\"22#s\"");
        assert_eq!(
            serde_json::from_str::<DocutilsRfcNumber>(&json).unwrap(),
            number
        );
        assert!(serde_json::from_str::<DocutilsRfcNumber>("\"0\"").is_err());
    }
}

//! What docutils' `:pep-reference:` role points at — `8`, `0008` — as written
//! between its backticks.
//!
//! "Parse, don't validate": a [`DocutilsPepNumber`] always holds a number
//! docutils would link, so nothing that builds a link from one meets a target
//! it cannot link. docutils' `pep_reference_role` hands the whole text to
//! Python's `int()` and refuses anything outside `0..=9999`, so unlike
//! [`crate::PepTarget`] there is no fragment and no explicit title: `8#x`
//! and `Style <8>` are not numbers.
//!
//! Only ASCII digits are a number, for the reason [`crate::PepTarget`] gives,
//! and through the same reader.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::registry_target::{NumberError, read_ascii_number};

/// The largest number docutils' `:pep-reference:` links.
const MAX_PEP_NUMBER: u32 = 9999;

/// A PEP number, as a `:pep-reference:` role wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocutilsPepNumber {
    /// The text as written, kept because docutils shows it: ``08`` reads
    /// "PEP 08".
    written: String,
    number: u32,
}

/// Why a written `:pep-reference:` target was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidDocutilsPepNumber {
    /// The text is not a run of ASCII digits.
    NotANumber(String),
    /// A number above 9999.
    OutOfRange(String),
}

impl fmt::Display for InvalidDocutilsPepNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Self::NotANumber(target) | Self::OutOfRange(target)) = self;
        // docutils' own wording, so both builds report the same thing.
        write!(
            f,
            "PEP number must be a number from 0 to {MAX_PEP_NUMBER}; \"{target}\" is invalid"
        )
    }
}

impl std::error::Error for InvalidDocutilsPepNumber {}

impl DocutilsPepNumber {
    /// Reads a written target.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDocutilsPepNumber`] when the text is not a number
    /// from 0 to 9999.
    pub fn parse(written: &str) -> Result<Self, InvalidDocutilsPepNumber> {
        let number = match read_ascii_number(written) {
            Ok(number) if number <= MAX_PEP_NUMBER => number,
            Ok(_) | Err(NumberError::TooLarge) => {
                return Err(InvalidDocutilsPepNumber::OutOfRange(written.to_string()));
            }
            Err(NumberError::NotDigits) => {
                return Err(InvalidDocutilsPepNumber::NotANumber(written.to_string()));
            }
        };
        Ok(Self {
            written: written.to_string(),
            number,
        })
    }

    /// The number as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        &self.written
    }

    /// The PEP's number.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }

    /// The PEP's page below the PEP index, as docutils' default
    /// `pep_file_url_template` builds it: the number padded to four digits,
    /// with no trailing slash — `pep-0008`.
    #[must_use]
    pub fn page_path(&self) -> String {
        format!("pep-{:04}", self.number)
    }
}

impl Serialize for DocutilsPepNumber {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.written)
    }
}

impl<'de> Deserialize<'de> for DocutilsPepNumber {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let written = String::deserialize(deserializer)?;
        Self::parse(&written).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_a_bare_number() {
        // Given / When
        let number = DocutilsPepNumber::parse("8").unwrap();

        // Then
        assert_eq!(number.number(), 8);
        assert_eq!(number.as_written(), "8");
    }

    #[test]
    fn test_parse_keeps_leading_zeros_as_written() {
        // Given / When
        let number = DocutilsPepNumber::parse("08").unwrap();

        // Then
        assert_eq!(number.number(), 8);
        assert_eq!(number.as_written(), "08");
    }

    #[test]
    fn test_parse_accepts_both_ends_of_the_range() {
        // Given / When / Then
        assert_eq!(DocutilsPepNumber::parse("0").unwrap().number(), 0);
        assert_eq!(DocutilsPepNumber::parse("9999").unwrap().number(), 9999);
    }

    #[test]
    fn test_parse_refuses_a_number_above_the_range() {
        // Given / When / Then
        for written in ["10000", "99999999999"] {
            assert_eq!(
                DocutilsPepNumber::parse(written),
                Err(InvalidDocutilsPepNumber::OutOfRange(written.to_string())),
                "{written:?}"
            );
        }
    }

    #[test]
    fn test_parse_refuses_what_is_not_digits() {
        // Given / When / Then — no fragment and no explicit title either
        for written in ["", "abc", "8#naming", "Style <8>", "+8", "-1", " 8"] {
            assert_eq!(
                DocutilsPepNumber::parse(written),
                Err(InvalidDocutilsPepNumber::NotANumber(written.to_string())),
                "{written:?}"
            );
        }
    }

    #[test]
    fn test_page_path_pads_to_four_digits_without_a_trailing_slash() {
        // Given / When / Then
        assert_eq!(
            DocutilsPepNumber::parse("8").unwrap().page_path(),
            "pep-0008"
        );
        assert_eq!(
            DocutilsPepNumber::parse("9999").unwrap().page_path(),
            "pep-9999"
        );
    }

    #[test]
    fn test_invalid_number_uses_docutils_wording() {
        // Given / When
        let message = InvalidDocutilsPepNumber::NotANumber("abc".to_string()).to_string();

        // Then
        assert_eq!(
            message,
            "PEP number must be a number from 0 to 9999; \"abc\" is invalid"
        );
    }

    #[test]
    fn test_serde_round_trips_the_written_text() {
        // Given
        let number = DocutilsPepNumber::parse("08").unwrap();

        // When
        let json = serde_json::to_string(&number).unwrap();
        let back: DocutilsPepNumber = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(json, "\"08\"");
        assert_eq!(back, number);
    }

    #[test]
    fn test_deserialize_refuses_an_invalid_number() {
        // Given / When / Then
        assert!(serde_json::from_str::<DocutilsPepNumber>("\"10000\"").is_err());
    }
}

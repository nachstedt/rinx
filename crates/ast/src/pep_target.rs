//! What a `:pep:` role points at — `8`, `8#naming-conventions` — as written
//! between its backticks, or behind an explicit title's angle brackets.
//!
//! "Parse, don't validate": a [`PepTarget`] always holds a PEP number, so
//! nothing that builds a link from one meets a target it cannot link. Sphinx's
//! `PEP` role splits the text at its first `#` and hands the part before it to
//! Python's `int()`, reporting "invalid PEP number" when that raises; this
//! type is the same split, and the same refusal, made where the text is read.
//!
//! Only ASCII digits are a number here. `int()` also accepts a sign, an
//! underscore between digits and non-ASCII digits — none of which names a
//! PEP anybody wrote — so those are refused rather than reproduced.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A PEP number with an optional fragment, as a `:pep:` role wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PepTarget {
    /// The text as written, kept because Sphinx shows it: ``:pep:`08` ``
    /// reads "PEP 08", fragment and all.
    written: String,
    number: u32,
    /// Where the fragment starts in `written`, after its `#`.
    fragment_start: Option<usize>,
}

/// Why a written `:pep:` target was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidPepTarget {
    /// The part before any `#` is not a run of ASCII digits.
    NotANumber(String),
    /// A run of digits too long to be any PEP's number.
    TooLarge(String),
}

impl fmt::Display for InvalidPepTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotANumber(target) => write!(
                f,
                "invalid PEP number '{target}': write the number in digits, optionally \
                 followed by '#' and an anchor"
            ),
            Self::TooLarge(target) => write!(f, "invalid PEP number '{target}': too large"),
        }
    }
}

impl std::error::Error for InvalidPepTarget {}

impl PepTarget {
    /// Reads a written target, splitting off a fragment at the first `#`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPepTarget`] when the part before the `#` is not a
    /// number.
    pub fn parse(written: &str) -> Result<Self, InvalidPepTarget> {
        let (digits, fragment_start) = match written.split_once('#') {
            Some((digits, _)) => (digits, Some(digits.len() + 1)),
            None => (written, None),
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvalidPepTarget::NotANumber(written.to_string()));
        }
        let number = digits
            .parse()
            .map_err(|_| InvalidPepTarget::TooLarge(written.to_string()))?;
        Ok(Self {
            written: written.to_string(),
            number,
            fragment_start,
        })
    }

    /// The target as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        &self.written
    }

    /// The PEP's number.
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }

    /// The anchor after the `#`, which may be empty; `None` without a `#`.
    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.fragment_start.map(|start| &self.written[start..])
    }

    /// The PEP's page below the PEP index, as Sphinx builds it: the number
    /// padded to four digits, a trailing slash, then any fragment —
    /// `pep-0008/`, `pep-0008/#naming`.
    #[must_use]
    pub fn page_path(&self) -> String {
        match self.fragment() {
            Some(fragment) => format!("pep-{:04}/#{fragment}", self.number),
            None => format!("pep-{:04}/", self.number),
        }
    }
}

impl Serialize for PepTarget {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.written)
    }
}

impl<'de> Deserialize<'de> for PepTarget {
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
    fn test_parse_splits_the_fragment_at_the_first_hash() {
        // Given / When
        let target = PepTarget::parse("8#a#b").unwrap();

        // Then
        assert_eq!(target.number(), 8);
        assert_eq!(target.fragment(), Some("a#b"));
    }

    #[test]
    fn test_parse_keeps_an_empty_fragment() {
        // Given / When / Then
        assert_eq!(PepTarget::parse("8#").unwrap().fragment(), Some(""));
    }

    #[test]
    fn test_parse_refuses_what_is_not_digits() {
        // Given / When / Then
        for written in ["", "#a", "abc", "+8", "-1", "8_0", " 8", "٨"] {
            assert_eq!(
                PepTarget::parse(written),
                Err(InvalidPepTarget::NotANumber(written.to_string())),
                "{written:?}"
            );
        }
    }

    #[test]
    fn test_parse_refuses_a_number_too_large() {
        // Given / When / Then
        assert_eq!(
            PepTarget::parse("99999999999"),
            Err(InvalidPepTarget::TooLarge("99999999999".to_string()))
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
    fn test_invalid_pep_target_names_the_target() {
        // Given / When
        let message = InvalidPepTarget::NotANumber("abc".to_string()).to_string();

        // Then
        assert!(message.contains("invalid PEP number 'abc'"), "{message}");
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
    fn test_deserialize_refuses_an_invalid_target() {
        // Given / When / Then
        assert!(serde_json::from_str::<PepTarget>("\"abc\"").is_err());
    }
}

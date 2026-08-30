use std::fmt;

use serde::{Deserialize, Serialize};

use crate::enumerator::EnumeratorFormat;
use crate::enumerator::EnumeratorSequence;

/// One enumerator of an enumerated list — the sequence it counts in, how it is
/// punctuated, and which position it denotes.
///
/// The invariant is that `ordinal` is expressible in `sequence`: there is no
/// 27th letter and no roman numeral for 5000. Construction goes through
/// [`Enumerator::new`], and deserialization re-runs the same check, so a value
/// of this type can always be rendered back to text — which is what lets
/// [`Enumerator::marker_text`] and [`std::fmt::Display`] be infallible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EnumeratorRaw")]
pub struct Enumerator {
    sequence: EnumeratorSequence,
    format: EnumeratorFormat,
    ordinal: u32,
}

impl Enumerator {
    /// Creates an enumerator, rejecting an ordinal the sequence cannot express.
    ///
    /// # Errors
    ///
    /// Returns [`EnumeratorError`] when `ordinal` is outside `sequence`'s range
    /// — past `z` for the alphabetic sequences, outside `1..=4999` for the
    /// roman ones. Arabic accepts any `u32`, including `0`, because docutils'
    /// `[0-9]+` pattern matches a literal `0.` enumerator.
    pub fn new(
        sequence: EnumeratorSequence,
        format: EnumeratorFormat,
        ordinal: u32,
    ) -> Result<Self, EnumeratorError> {
        if sequence.accepts_ordinal(ordinal) {
            Ok(Self {
                sequence,
                format,
                ordinal,
            })
        } else {
            Err(EnumeratorError { sequence, ordinal })
        }
    }

    /// The sequence this enumerator counts in.
    #[must_use]
    pub const fn sequence(&self) -> EnumeratorSequence {
        self.sequence
    }

    /// How this enumerator is punctuated.
    #[must_use]
    pub const fn format(&self) -> EnumeratorFormat {
        self.format
    }

    /// The position this enumerator denotes.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    /// The enumerator text without its punctuation — `"iv"` for `(iv)`.
    ///
    /// Note this is the *canonical* rendering of the ordinal, not the source
    /// text: a document writing `007.` yields `"7"` here, matching docutils'
    /// `str(ordinal)`, which is why `007.` and `8.` form one list.
    ///
    /// # Panics
    ///
    /// Never, for a value obtained through [`Enumerator::new`] or through
    /// deserialization: both reject an ordinal the sequence cannot express, so
    /// the rendering below always succeeds. The `expect` guards that invariant
    /// rather than describing a reachable state.
    #[must_use]
    pub fn marker_text(&self) -> String {
        self.sequence
            .render(self.ordinal)
            .expect("ordinal is representable in its sequence by construction")
    }

    /// The next enumerator in the same sequence and format, or `None` when the
    /// sequence has run out (past `z`, past roman 4999).
    ///
    /// docutils' `make_enumerator` returns `None` in exactly these cases, and
    /// its `is_enumerated_list_item` then declines the line — so a list cannot
    /// continue past its sequence's ceiling.
    #[must_use]
    pub fn successor(&self) -> Option<Self> {
        Self::new(self.sequence, self.format, self.ordinal.checked_add(1)?).ok()
    }
}

impl fmt::Display for Enumerator {
    /// Writes the enumerator as it appears in the source, punctuation included:
    /// `(iv)`, `iv)`, `iv.`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.format.prefix(),
            self.marker_text(),
            self.format.suffix()
        )
    }
}

/// The error [`Enumerator::new`] returns for an out-of-range ordinal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumeratorError {
    sequence: EnumeratorSequence,
    ordinal: u32,
}

impl fmt::Display for EnumeratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ordinal {} is not expressible in the {} enumeration sequence",
            self.ordinal,
            self.sequence.css_class()
        )
    }
}

impl std::error::Error for EnumeratorError {}

/// Private helper for validated deserialization of [`Enumerator`].
#[derive(Deserialize)]
struct EnumeratorRaw {
    sequence: EnumeratorSequence,
    format: EnumeratorFormat,
    ordinal: u32,
}

impl TryFrom<EnumeratorRaw> for Enumerator {
    type Error = String;

    fn try_from(raw: EnumeratorRaw) -> Result<Self, Self::Error> {
        Self::new(raw.sequence, raw.format, raw.ordinal).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_accepts_an_ordinal_its_sequence_can_express() {
        // Given an ordinal within the alphabet
        // When building an enumerator for it
        let enumerator =
            Enumerator::new(EnumeratorSequence::LowerAlpha, EnumeratorFormat::Period, 26);

        // Then it is accepted
        assert_eq!(enumerator.map(|e| e.ordinal()), Ok(26));
    }

    #[test]
    fn test_new_rejects_an_ordinal_past_the_alphabet() {
        // Given the first ordinal past `z`
        // When building an alphabetic enumerator for it
        let enumerator =
            Enumerator::new(EnumeratorSequence::LowerAlpha, EnumeratorFormat::Period, 27);

        // Then construction fails
        assert!(enumerator.is_err());
    }

    #[test]
    fn test_new_rejects_ordinals_outside_the_roman_range() {
        // Given ordinals either side of the roman range
        // When building roman enumerators for them
        // Then both are rejected
        for ordinal in [0, 5000] {
            assert!(
                Enumerator::new(
                    EnumeratorSequence::UpperRoman,
                    EnumeratorFormat::Period,
                    ordinal
                )
                .is_err(),
                "{ordinal}"
            );
        }
    }

    #[test]
    fn test_new_accepts_arabic_zero() {
        // Given ordinal zero, which docutils accepts from a literal `0.`
        // When building an arabic enumerator for it
        let enumerator = Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 0);

        // Then it is accepted
        assert!(enumerator.is_ok());
    }

    #[test]
    fn test_marker_text_renders_the_ordinal_canonically() {
        // Given an enumerator built from a source that wrote leading zeros
        let enumerator =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 7).unwrap();

        // When reading its marker text
        // Then the canonical rendering comes back, not the source spelling
        assert_eq!(enumerator.marker_text(), "7");
    }

    #[test]
    fn test_display_wraps_the_marker_in_its_formats_punctuation() {
        // Given the same ordinal in each of the three formats
        let cases = [
            (EnumeratorFormat::Parens, "(iv)"),
            (EnumeratorFormat::RightParen, "iv)"),
            (EnumeratorFormat::Period, "iv."),
        ];

        // When displaying each
        // Then the punctuation matches the format
        for (format, expected) in cases {
            let enumerator = Enumerator::new(EnumeratorSequence::LowerRoman, format, 4).unwrap();
            assert_eq!(enumerator.to_string(), expected);
        }
    }

    #[test]
    fn test_successor_advances_within_the_same_sequence_and_format() {
        // Given an enumerator partway through its sequence
        let enumerator =
            Enumerator::new(EnumeratorSequence::UpperAlpha, EnumeratorFormat::Parens, 3).unwrap();

        // When taking its successor
        let next = enumerator.successor().expect("D follows C");

        // Then only the ordinal advances
        assert_eq!(next.ordinal(), 4);
        assert_eq!(next.sequence(), EnumeratorSequence::UpperAlpha);
        assert_eq!(next.format(), EnumeratorFormat::Parens);
        assert_eq!(next.to_string(), "(D)");
    }

    #[test]
    fn test_successor_returns_none_at_the_end_of_a_bounded_sequence() {
        // Given the last enumerator each bounded sequence can express
        let last_alpha =
            Enumerator::new(EnumeratorSequence::LowerAlpha, EnumeratorFormat::Period, 26).unwrap();
        let last_roman = Enumerator::new(
            EnumeratorSequence::UpperRoman,
            EnumeratorFormat::Period,
            4999,
        )
        .unwrap();

        // When taking their successors
        // Then the sequence has run out
        assert_eq!(last_alpha.successor(), None);
        assert_eq!(last_roman.successor(), None);
    }

    #[test]
    fn test_successor_does_not_overflow_at_the_top_of_the_arabic_range() {
        // Given the largest arabic ordinal representable
        let enumerator = Enumerator::new(
            EnumeratorSequence::Arabic,
            EnumeratorFormat::Period,
            u32::MAX,
        )
        .unwrap();

        // When taking its successor
        // Then it declines rather than overflowing
        assert_eq!(enumerator.successor(), None);
    }

    #[test]
    fn test_enumerator_serialization_roundtrip() {
        // Given an enumerator
        let enumerator =
            Enumerator::new(EnumeratorSequence::LowerRoman, EnumeratorFormat::Parens, 9).unwrap();

        // When serializing and deserializing it
        let json = serde_json::to_string(&enumerator).expect("Failed to serialize");
        let deserialized: Enumerator = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then the value survives the round trip
        assert_eq!(enumerator, deserialized);
    }

    #[test]
    fn test_deserialization_rejects_an_ordinal_past_the_alphabet() {
        // Given serialized data claiming a 27th letter
        let json = r#"{"sequence":"LowerAlpha","format":"Period","ordinal":27}"#;

        // When deserializing it
        let result: Result<Enumerator, _> = serde_json::from_str(json);

        // Then the invariant is enforced on load rather than trusted
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialization_rejects_an_ordinal_past_the_roman_range() {
        // Given serialized data claiming roman 5000
        let json = r#"{"sequence":"UpperRoman","format":"Period","ordinal":5000}"#;

        // When deserializing it
        let result: Result<Enumerator, _> = serde_json::from_str(json);

        // Then it is rejected
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialization_rejects_roman_zero() {
        // Given serialized data claiming roman zero
        let json = r#"{"sequence":"LowerRoman","format":"Period","ordinal":0}"#;

        // When deserializing it
        let result: Result<Enumerator, _> = serde_json::from_str(json);

        // Then it is rejected
        assert!(result.is_err());
    }

    #[test]
    fn test_error_message_names_the_sequence_and_ordinal() {
        // Given a rejected construction
        let error = Enumerator::new(EnumeratorSequence::LowerAlpha, EnumeratorFormat::Period, 27)
            .expect_err("27 is past z");

        // When rendering the error
        let message = error.to_string();

        // Then it names both the offending ordinal and the sequence
        assert!(message.contains("27"), "{message}");
        assert!(message.contains("loweralpha"), "{message}");
    }
}

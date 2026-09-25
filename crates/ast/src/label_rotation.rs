//! How far a chart's text is turned.
//!
//! "Parse, don't validate": a [`LabelRotation`] only ever holds a whole number
//! of degrees below a full turn, so whatever draws the text never asks what a
//! written angle meant. It is read where the option line is — the only phase
//! that can point at what the author wrote.
//!
//! The accepted spelling is the one sphinx-needs' `needbar` accepts: a
//! non-negative integer. That implementation checks `isdigit()` and silently
//! draws unrotated text for anything else; here anything else is refused, so a
//! `:xlabels_rotation: -45` that would have done nothing says so.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// A full turn, the modulus every written angle is reduced by.
const FULL_TURN: u16 = 360;

/// A clockwise rotation in whole degrees, always below a full turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LabelRotation(u16);

/// Why a written rotation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidLabelRotation(String);

impl fmt::Display for InvalidLabelRotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' is not a rotation; write a whole number of degrees such as 45",
            self.0
        )
    }
}

impl LabelRotation {
    /// Reads a written angle, reducing it below a full turn.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLabelRotation`] for anything but a non-negative
    /// integer, naming what was written.
    pub fn parse(value: &str) -> Result<Self, InvalidLabelRotation> {
        let written = value.trim();
        if written.is_empty() || !written.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvalidLabelRotation(written.to_string()));
        }
        // Reduced digit by digit, so an absurdly long angle is still just an
        // angle rather than an overflow.
        let degrees = written.bytes().fold(0u16, |reduced, digit| {
            (reduced * 10 + u16::from(digit - b'0')) % FULL_TURN
        });
        Ok(Self(degrees))
    }

    /// The rotation in degrees, in `0..360`.
    #[must_use]
    pub const fn degrees(self) -> u16 {
        self.0
    }
}

impl<'de> Deserialize<'de> for LabelRotation {
    /// Re-checks the invariant on load, as every opaque type here does: a
    /// `.ast` is a file on disk, and nothing guarantees this build wrote it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let degrees = u16::deserialize(deserializer)?;
        if degrees >= FULL_TURN {
            return Err(serde::de::Error::custom(format!(
                "a rotation of {degrees} degrees is not below a full turn"
            )));
        }
        Ok(Self(degrees))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_written_angle_is_kept_in_degrees() {
        // Given
        let written = "45";

        // When
        let rotation = LabelRotation::parse(written).unwrap();

        // Then
        assert_eq!(rotation.degrees(), 45);
    }

    #[test]
    fn test_surrounding_whitespace_is_ignored() {
        // Given
        let written = "  90 ";

        // When
        let rotation = LabelRotation::parse(written).unwrap();

        // Then
        assert_eq!(rotation.degrees(), 90);
    }

    #[test]
    fn test_an_angle_past_a_full_turn_is_reduced() {
        // Given
        let written = "405";

        // When
        let rotation = LabelRotation::parse(written).unwrap();

        // Then
        assert_eq!(rotation.degrees(), 45);
    }

    #[test]
    fn test_an_overlong_angle_is_reduced_rather_than_overflowing() {
        // Given
        let written = "36000000000000000000045";

        // When
        let rotation = LabelRotation::parse(written).unwrap();

        // Then
        assert_eq!(rotation.degrees(), 45);
    }

    #[test]
    fn test_a_negative_angle_is_refused_naming_what_was_written() {
        // Given — sphinx-needs' `isdigit()` would silently ignore it
        let written = "-45";

        // When
        let error = LabelRotation::parse(written).unwrap_err();

        // Then
        assert!(error.to_string().contains("'-45'"), "{error}");
    }

    #[test]
    fn test_a_fractional_angle_is_refused() {
        // Given
        let written = "22.5";

        // When
        let parsed = LabelRotation::parse(written);

        // Then
        assert!(parsed.is_err());
    }

    #[test]
    fn test_an_empty_angle_is_refused() {
        // Given
        let written = " ";

        // When
        let parsed = LabelRotation::parse(written);

        // Then
        assert!(parsed.is_err());
    }

    #[test]
    fn test_a_rotation_survives_a_serialization_round_trip() {
        // Given
        let rotation = LabelRotation::parse("270").unwrap();

        // When
        let json = serde_json::to_string(&rotation).unwrap();
        let decoded: LabelRotation = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, rotation);
    }

    #[test]
    fn test_loading_a_full_turn_or_more_is_refused() {
        // Given — a `.ast` this build did not write
        let json = "360";

        // When
        let decoded = serde_json::from_str::<LabelRotation>(json);

        // Then
        assert!(decoded.is_err());
    }
}

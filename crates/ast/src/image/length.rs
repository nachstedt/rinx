//! The measurements an image directive's `:height:`, `:width:` and
//! `:figwidth:` options are written in.
//!
//! docutils validates these with `directives.length_or_unitless` and
//! `directives.length_or_percentage_or_unitless`, which accept a bare number
//! optionally followed by one of eight CSS units. All three types here are
//! opaque with smart constructors, because the text is re-emitted verbatim
//! into a `style` attribute: a value that reached the renderer unvalidated
//! would become malformed CSS that no phase could still report.
//!
//! The numeric part is kept as its already-validated *text* rather than as an
//! `f64` so the AST types stay `Eq`/`Hash` (a `Directive` must be comparable)
//! and so a value round-trips through a `.ast` file exactly as the author
//! wrote it. [`Length::scaled`] is the one place it is read back as a number.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a measurement could not be built. An enum rather than a message string
/// so the parser can word each case for the author without re-deriving which
/// one happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidLength {
    /// Nothing but whitespace was given.
    Empty,
    /// The leading numeric part is missing or malformed.
    NotANumber(String),
    /// The trailing unit is not one of docutils' eight.
    UnknownUnit(String),
}

impl fmt::Display for InvalidLength {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a measurement cannot be empty"),
            Self::NotANumber(text) => write!(formatter, "'{text}' is not a number"),
            Self::UnknownUnit(unit) => write!(
                formatter,
                "'{unit}' is not a length unit (expected one of {})",
                LengthUnit::ALL
                    .iter()
                    .map(|unit| unit.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// One of the eight CSS units docutils accepts on a length.
///
/// A closed enum rather than an open string because this set is fixed by the
/// docutils specification, not by a backend that might be upgraded — the
/// opposite of [`crate::LanguageName`]'s situation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LengthUnit {
    Em,
    Ex,
    Px,
    In,
    Cm,
    Mm,
    Pt,
    Pc,
}

impl LengthUnit {
    /// Every unit, in docutils' own declaration order.
    pub const ALL: &'static [Self] = &[
        Self::Em,
        Self::Ex,
        Self::Px,
        Self::In,
        Self::Cm,
        Self::Mm,
        Self::Pt,
        Self::Pc,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Em => "em",
            Self::Ex => "ex",
            Self::Px => "px",
            Self::In => "in",
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::Pt => "pt",
            Self::Pc => "pc",
        }
    }

    /// Reads a unit suffix, which must already be lowercased and trimmed.
    fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|unit| unit.as_str() == text)
    }
}

impl fmt::Display for LengthUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Validates the numeric part of a measurement, returning it normalized.
///
/// Mirrors docutils' `[0-9.]+` exactly: digits and dots only, so a sign or an
/// exponent is rejected rather than reaching a `style` attribute. The parse to
/// `f64` on top of that rejects the degenerate cases the character class alone
/// still admits (`.`, `1.2.3`).
fn validate_number(text: &str) -> Result<String, InvalidLength> {
    if text.is_empty() {
        return Err(InvalidLength::Empty);
    }
    let well_formed = text
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.')
        && text.parse::<f64>().is_ok_and(f64::is_finite);
    if well_formed {
        Ok(text.to_string())
    } else {
        Err(InvalidLength::NotANumber(text.to_string()))
    }
}

/// Multiplies a validated numeric text by `percent`, formatting the result
/// back into the same shape.
///
/// Trailing zeros are trimmed so a whole number stays whole (`50%` of `10` is
/// `5`, not `5.0000`), which is what keeps the emitted CSS readable.
fn scale_number(text: &str, percent: u32) -> String {
    let value = text.parse::<f64>().unwrap_or_default();
    let scaled = value * f64::from(percent) / 100.0;
    let formatted = format!("{scaled:.4}");
    let trimmed = if formatted.contains('.') {
        formatted.trim_end_matches('0').trim_end_matches('.')
    } else {
        formatted.as_str()
    };
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A CSS length, e.g. `1.5em`, `200px`, or the unitless `200`.
///
/// The unit is optional because docutils' `length_or_unitless` accepts a bare
/// number, which the HTML writer emits as a pixel-ish attribute value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Length {
    value: String,
    unit: Option<LengthUnit>,
}

impl Length {
    /// Reads a length from raw option text.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLength`] when the numeric part is missing or
    /// malformed, or when the trailing unit is not one of docutils' eight.
    pub fn new(raw: &str) -> Result<Self, InvalidLength> {
        let normalized = raw.trim().to_lowercase();
        if normalized.is_empty() {
            return Err(InvalidLength::Empty);
        }
        let split_at = normalized
            .find(|character: char| !character.is_ascii_digit() && character != '.')
            .unwrap_or(normalized.len());
        let (number, unit_text) = normalized.split_at(split_at);
        let value = validate_number(number)?;
        let unit = if unit_text.is_empty() {
            None
        } else {
            Some(
                LengthUnit::parse(unit_text)
                    .ok_or_else(|| InvalidLength::UnknownUnit(unit_text.to_string()))?,
            )
        };
        Ok(Self { value, unit })
    }

    /// The numeric part, without its unit.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The unit, or `None` for a bare number.
    #[must_use]
    pub const fn unit(&self) -> Option<LengthUnit> {
        self.unit
    }

    /// This length multiplied by `percent`, keeping the unit.
    ///
    /// This is how `:scale:` is applied: docutils scales whatever `:width:`
    /// and `:height:` were given rather than rewriting them into a new unit.
    #[must_use]
    pub fn scaled(&self, percent: u32) -> Self {
        Self {
            value: scale_number(&self.value, percent),
            unit: self.unit,
        }
    }
}

impl fmt::Display for Length {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.value)?;
        match self.unit {
            Some(unit) => formatter.write_str(unit.as_str()),
            None => Ok(()),
        }
    }
}

impl From<Length> for String {
    fn from(length: Length) -> Self {
        length.to_string()
    }
}

impl TryFrom<String> for Length {
    type Error = InvalidLength;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

/// A CSS percentage, e.g. `50%`.
///
/// Its own type rather than a [`Length`] with a ninth unit, because docutils
/// accepts it in only some of the places a length is accepted: `:width:` and
/// `:figwidth:` take one, `:height:` does not.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Percentage(String);

impl Percentage {
    /// Reads a percentage from raw option text, with or without its `%`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLength`] when the numeric part is missing or
    /// malformed.
    pub fn new(raw: &str) -> Result<Self, InvalidLength> {
        let normalized = raw.trim();
        let number = normalized.strip_suffix('%').unwrap_or(normalized).trim();
        Ok(Self(validate_number(number)?))
    }

    /// The numeric part, without its `%`.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }

    /// This percentage multiplied by `percent`.
    #[must_use]
    pub fn scaled(&self, percent: u32) -> Self {
        Self(scale_number(&self.0, percent))
    }
}

impl fmt::Display for Percentage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}%", self.0)
    }
}

impl From<Percentage> for String {
    fn from(percentage: Percentage) -> Self {
        percentage.to_string()
    }
}

impl TryFrom<String> for Percentage {
    type Error = InvalidLength;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

/// What `:width:` accepts: a length or a percentage of the available width.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LengthOrPercentage {
    Length(Length),
    Percentage(Percentage),
}

impl LengthOrPercentage {
    /// Reads either form, deciding on the trailing `%`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLength`] when neither form parses.
    pub fn new(raw: &str) -> Result<Self, InvalidLength> {
        if raw.trim().ends_with('%') {
            Percentage::new(raw).map(Self::Percentage)
        } else {
            Length::new(raw).map(Self::Length)
        }
    }

    /// This measurement multiplied by `percent`, keeping its form.
    #[must_use]
    pub fn scaled(&self, percent: u32) -> Self {
        match self {
            Self::Length(length) => Self::Length(length.scaled(percent)),
            Self::Percentage(percentage) => Self::Percentage(percentage.scaled(percent)),
        }
    }
}

impl fmt::Display for LengthOrPercentage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length(length) => length.fmt(formatter),
            Self::Percentage(percentage) => percentage.fmt(formatter),
        }
    }
}

/// What `.. figure::`'s `:figwidth:` accepts.
///
/// `image` is a variant rather than a name a [`Length`] might collide with,
/// because docutils gives it a meaning no measurement has: match the width of
/// the contained image, whatever that turns out to be.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FigureWidth {
    Length(Length),
    Percentage(Percentage),
    /// `:figwidth: image` — as wide as the image itself.
    MatchImage,
}

impl FigureWidth {
    /// Reads a `:figwidth:` value.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLength`] when the value is neither `image` nor a
    /// well-formed measurement.
    pub fn new(raw: &str) -> Result<Self, InvalidLength> {
        if raw.trim().eq_ignore_ascii_case("image") {
            return Ok(Self::MatchImage);
        }
        Ok(match LengthOrPercentage::new(raw)? {
            LengthOrPercentage::Length(length) => Self::Length(length),
            LengthOrPercentage::Percentage(percentage) => Self::Percentage(percentage),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_new_reads_a_value_with_a_unit() {
        // Given
        let raw = "1.5em";

        // When
        let length = Length::new(raw).expect("should parse");

        // Then
        assert_eq!(length.value(), "1.5");
        assert_eq!(length.unit(), Some(LengthUnit::Em));
    }

    #[test]
    fn test_length_new_reads_a_unitless_value() {
        // Given
        let raw = "200";

        // When
        let length = Length::new(raw).expect("should parse");

        // Then
        assert_eq!(length.value(), "200");
        assert_eq!(length.unit(), None);
    }

    #[test]
    fn test_length_new_trims_and_lowercases() {
        // Given
        let raw = "  10PX  ";

        // When
        let length = Length::new(raw).expect("should parse");

        // Then
        assert_eq!(length.to_string(), "10px");
    }

    #[test]
    fn test_length_new_rejects_an_unknown_unit() {
        // Given
        let raw = "10rem";

        // When
        let result = Length::new(raw);

        // Then
        assert_eq!(result, Err(InvalidLength::UnknownUnit("rem".to_string())));
    }

    #[test]
    fn test_length_new_rejects_an_empty_value() {
        // Given
        let raw = "   ";

        // When
        let result = Length::new(raw);

        // Then
        assert_eq!(result, Err(InvalidLength::Empty));
    }

    #[test]
    fn test_length_new_rejects_a_missing_number() {
        // Given
        let raw = "px";

        // When
        let result = Length::new(raw);

        // Then
        assert_eq!(result, Err(InvalidLength::Empty));
    }

    #[test]
    fn test_length_new_rejects_a_malformed_number() {
        // Given
        let raw = "1.2.3em";

        // When
        let result = Length::new(raw);

        // Then
        assert_eq!(result, Err(InvalidLength::NotANumber("1.2.3".to_string())));
    }

    #[test]
    fn test_length_new_rejects_a_signed_number() {
        // Given — docutils' own character class admits no sign
        let raw = "-10px";

        // When
        let result = Length::new(raw);

        // Then
        assert_eq!(result, Err(InvalidLength::Empty));
    }

    #[test]
    fn test_length_display_round_trips_every_unit() {
        for unit in LengthUnit::ALL {
            // Given
            let raw = format!("12{unit}");

            // When
            let length = Length::new(&raw).expect("should parse");

            // Then
            assert_eq!(length.to_string(), raw);
        }
    }

    #[test]
    fn test_length_scaled_multiplies_the_value_and_keeps_the_unit() {
        // Given
        let length = Length::new("200px").expect("should parse");

        // When
        let scaled = length.scaled(50);

        // Then
        assert_eq!(scaled.to_string(), "100px");
    }

    #[test]
    fn test_length_scaled_keeps_a_fractional_result() {
        // Given
        let length = Length::new("1.5em").expect("should parse");

        // When
        let scaled = length.scaled(50);

        // Then
        assert_eq!(scaled.to_string(), "0.75em");
    }

    #[test]
    fn test_length_scaled_by_zero_is_zero() {
        // Given
        let length = Length::new("200px").expect("should parse");

        // When
        let scaled = length.scaled(0);

        // Then
        assert_eq!(scaled.to_string(), "0px");
    }

    #[test]
    fn test_length_serialization_is_the_written_form() {
        // Given
        let length = Length::new("2.5cm").expect("should parse");

        // When
        let json = serde_json::to_string(&length).expect("should serialize");

        // Then
        assert_eq!(json, "\"2.5cm\"");
    }

    #[test]
    fn test_length_deserialization_rejects_an_invalid_value() {
        // Given
        let json = "\"10rem\"";

        // When
        let result: Result<Length, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_length_serialization_round_trips() {
        // Given
        let length = Length::new("12pt").expect("should parse");

        // When
        let json = serde_json::to_string(&length).expect("should serialize");
        let restored: Length = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, length);
    }

    #[test]
    fn test_percentage_new_accepts_the_percent_sign() {
        // Given
        let raw = "50%";

        // When
        let percentage = Percentage::new(raw).expect("should parse");

        // Then
        assert_eq!(percentage.value(), "50");
        assert_eq!(percentage.to_string(), "50%");
    }

    #[test]
    fn test_percentage_new_accepts_a_bare_number() {
        // Given
        let raw = "75";

        // When
        let percentage = Percentage::new(raw).expect("should parse");

        // Then
        assert_eq!(percentage.to_string(), "75%");
    }

    #[test]
    fn test_percentage_new_rejects_a_malformed_number() {
        // Given
        let raw = "half%";

        // When
        let result = Percentage::new(raw);

        // Then
        assert_eq!(result, Err(InvalidLength::NotANumber("half".to_string())));
    }

    #[test]
    fn test_percentage_scaled_multiplies_the_value() {
        // Given
        let percentage = Percentage::new("80%").expect("should parse");

        // When
        let scaled = percentage.scaled(50);

        // Then
        assert_eq!(scaled.to_string(), "40%");
    }

    #[test]
    fn test_percentage_serialization_round_trips() {
        // Given
        let percentage = Percentage::new("33.5%").expect("should parse");

        // When
        let json = serde_json::to_string(&percentage).expect("should serialize");
        let restored: Percentage = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(json, "\"33.5%\"");
        assert_eq!(restored, percentage);
    }

    #[test]
    fn test_length_or_percentage_new_reads_a_percentage() {
        // Given
        let raw = "50%";

        // When
        let measurement = LengthOrPercentage::new(raw).expect("should parse");

        // Then
        assert!(matches!(measurement, LengthOrPercentage::Percentage(_)));
        assert_eq!(measurement.to_string(), "50%");
    }

    #[test]
    fn test_length_or_percentage_new_reads_a_length() {
        // Given
        let raw = "3in";

        // When
        let measurement = LengthOrPercentage::new(raw).expect("should parse");

        // Then
        assert!(matches!(measurement, LengthOrPercentage::Length(_)));
        assert_eq!(measurement.to_string(), "3in");
    }

    #[test]
    fn test_length_or_percentage_new_rejects_a_bad_value() {
        // Given
        let raw = "wide";

        // When
        let result = LengthOrPercentage::new(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_length_or_percentage_scaled_keeps_its_form() {
        // Given
        let percentage = LengthOrPercentage::new("50%").expect("should parse");
        let length = LengthOrPercentage::new("10px").expect("should parse");

        // When
        let scaled_percentage = percentage.scaled(50);
        let scaled_length = length.scaled(50);

        // Then
        assert_eq!(scaled_percentage.to_string(), "25%");
        assert_eq!(scaled_length.to_string(), "5px");
    }

    #[test]
    fn test_figure_width_new_reads_the_image_keyword() {
        // Given
        let raw = "image";

        // When
        let width = FigureWidth::new(raw).expect("should parse");

        // Then
        assert_eq!(width, FigureWidth::MatchImage);
    }

    #[test]
    fn test_figure_width_new_reads_the_image_keyword_case_insensitively() {
        // Given
        let raw = "Image";

        // When
        let width = FigureWidth::new(raw).expect("should parse");

        // Then
        assert_eq!(width, FigureWidth::MatchImage);
    }

    #[test]
    fn test_figure_width_new_reads_a_percentage() {
        // Given
        let raw = "60%";

        // When
        let width = FigureWidth::new(raw).expect("should parse");

        // Then
        assert!(matches!(width, FigureWidth::Percentage(_)));
    }

    #[test]
    fn test_figure_width_new_reads_a_length() {
        // Given
        let raw = "8cm";

        // When
        let width = FigureWidth::new(raw).expect("should parse");

        // Then
        assert!(matches!(width, FigureWidth::Length(_)));
    }

    #[test]
    fn test_figure_width_new_rejects_a_bad_value() {
        // Given
        let raw = "picture";

        // When
        let result = FigureWidth::new(raw);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_length_messages_name_the_problem() {
        // Given / When / Then
        assert_eq!(
            InvalidLength::Empty.to_string(),
            "a measurement cannot be empty"
        );
        assert_eq!(
            InvalidLength::NotANumber("x".to_string()).to_string(),
            "'x' is not a number"
        );
        assert!(
            InvalidLength::UnknownUnit("rem".to_string())
                .to_string()
                .contains("'rem' is not a length unit")
        );
    }
}

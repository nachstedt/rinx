//! Where an image sits relative to the text around it.
//!
//! docutils names six alignments but never accepts all six at once: the three
//! vertical ones (`top`/`middle`/`bottom`) are valid *only* on an image inside
//! a substitution definition, where the image is inline and there is a text
//! baseline to align to, and the three horizontal ones only outside one. A
//! `.. figure::` is always a block, so it takes the horizontal three alone.
//!
//! rusty-sphinx has no substitution definitions, so every image it can parse
//! is a block-level one and the vertical three are unreachable. They are
//! therefore not variants of [`ImageAlign`] — a value that could never be
//! valid should not be representable — but [`is_vertical_name`] still
//! recognizes them, so an author who writes one is told *why* it was refused
//! rather than being handed the generic "not a valid value" list.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The horizontal alignment an `.. image::` or `.. figure::` may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageAlign {
    Left,
    Center,
    Right,
}

impl ImageAlign {
    /// Every alignment, in docutils' own declaration order.
    pub const ALL: &'static [Self] = &[Self::Left, Self::Center, Self::Right];

    /// The name this alignment is written as, which is also the suffix of the
    /// `align-*` CSS class docutils' HTML writer emits for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    /// Reads an `:align:` value, ignoring case and surrounding whitespace.
    ///
    /// Returns `None` for anything else — including the three vertical names,
    /// which [`is_vertical_name`] separates out for the caller's diagnostic.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|align| align.as_str() == normalized)
    }

    /// The `align-*` CSS class for this alignment.
    #[must_use]
    pub fn css_class(self) -> String {
        format!("align-{}", self.as_str())
    }
}

impl fmt::Display for ImageAlign {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Whether `raw` names one of docutils' three *vertical* alignments.
///
/// Not an [`ImageAlign`] variant, because rusty-sphinx parses no substitution
/// definitions and so has no context where one would be valid. Recognizing the
/// name anyway is what lets the parser explain the refusal instead of listing
/// the horizontal three and leaving the author to guess why theirs is missing.
#[must_use]
pub fn is_vertical_name(raw: &str) -> bool {
    matches!(
        raw.trim().to_lowercase().as_str(),
        "top" | "middle" | "bottom"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_every_horizontal_alignment() {
        for align in ImageAlign::ALL {
            // Given
            let raw = align.as_str();

            // When
            let parsed = ImageAlign::parse(raw);

            // Then
            assert_eq!(parsed, Some(*align));
        }
    }

    #[test]
    fn test_parse_ignores_case_and_whitespace() {
        // Given
        let raw = "  CENTER  ";

        // When
        let parsed = ImageAlign::parse(raw);

        // Then
        assert_eq!(parsed, Some(ImageAlign::Center));
    }

    #[test]
    fn test_parse_rejects_a_vertical_alignment() {
        // Given — valid in docutils only inside a substitution definition
        let raw = "middle";

        // When
        let parsed = ImageAlign::parse(raw);

        // Then
        assert_eq!(parsed, None);
    }

    #[test]
    fn test_parse_rejects_an_unknown_name() {
        // Given
        let raw = "sideways";

        // When
        let parsed = ImageAlign::parse(raw);

        // Then
        assert_eq!(parsed, None);
    }

    #[test]
    fn test_css_class_is_the_docutils_align_class() {
        // Given
        let align = ImageAlign::Right;

        // When
        let class = align.css_class();

        // Then
        assert_eq!(class, "align-right");
    }

    #[test]
    fn test_is_vertical_name_recognizes_all_three() {
        for raw in ["top", "middle", "bottom"] {
            // Given / When / Then
            assert!(is_vertical_name(raw), "{raw} should be vertical");
        }
    }

    #[test]
    fn test_is_vertical_name_ignores_case_and_whitespace() {
        // Given
        let raw = " Top ";

        // When / Then
        assert!(is_vertical_name(raw));
    }

    #[test]
    fn test_is_vertical_name_rejects_a_horizontal_alignment() {
        // Given / When / Then
        assert!(!is_vertical_name("left"));
    }

    #[test]
    fn test_display_matches_the_written_name() {
        // Given / When / Then
        assert_eq!(ImageAlign::Left.to_string(), "left");
    }

    #[test]
    fn test_serialization_round_trips() {
        // Given
        let align = ImageAlign::Center;

        // When
        let json = serde_json::to_string(&align).expect("should serialize");
        let restored: ImageAlign = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, align);
    }
}

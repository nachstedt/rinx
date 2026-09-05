//! Where an image sits relative to the text around it.
//!
//! docutils names six alignments: three horizontal (`left`/`center`/`right`),
//! valid on a block-level `.. image::`/`.. figure::`, and three vertical
//! (`top`/`middle`/`bottom`), valid only on an image inside a substitution
//! definition, where the image is inline and there is a text baseline to
//! align to. A `.. figure::` is always a block, so it takes the horizontal
//! three alone.
//!
//! All six are variants of one enum because [`Self::css_class`] treats them
//! identically — docutils' own HTML writer emits `align-<value>` for any of
//! the six and leaves the stylesheet to interpret `align-top` as a
//! `vertical-align`, not a float. What differs by context is which names
//! [`Self::parse`] (block context: horizontal only) versus [`Self::parse_any`]
//! (substitution context: all six) will accept — see [`is_vertical_name`] for
//! how a caller in the block context tells an author *why* a vertical name
//! was refused rather than just listing the three it does accept.
use std::fmt;

use serde::{Deserialize, Serialize};

/// The alignment an `.. image::`/`.. figure::` (horizontal) or a
/// substitution-definition image (all six) may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageAlign {
    Left,
    Center,
    Right,
    /// Valid only inside a substitution definition.
    Top,
    /// Valid only inside a substitution definition.
    Middle,
    /// Valid only inside a substitution definition.
    Bottom,
}

impl ImageAlign {
    /// The three alignments a block-level `.. image::`/`.. figure::` accepts,
    /// in docutils' own declaration order.
    pub const HORIZONTAL: &'static [Self] = &[Self::Left, Self::Center, Self::Right];

    /// The three alignments valid only inside a substitution definition, in
    /// docutils' own declaration order.
    pub const VERTICAL: &'static [Self] = &[Self::Top, Self::Middle, Self::Bottom];

    /// Every alignment docutils names, horizontal first.
    pub const ALL: &'static [Self] = &[
        Self::Left,
        Self::Center,
        Self::Right,
        Self::Top,
        Self::Middle,
        Self::Bottom,
    ];

    /// The name this alignment is written as, which is also the suffix of the
    /// `align-*` CSS class docutils' HTML writer emits for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Top => "top",
            Self::Middle => "middle",
            Self::Bottom => "bottom",
        }
    }

    /// Whether this is one of the three vertical alignments.
    #[must_use]
    pub const fn is_vertical(self) -> bool {
        matches!(self, Self::Top | Self::Middle | Self::Bottom)
    }

    /// Reads an `:align:` value for a block-level image, ignoring case and
    /// surrounding whitespace.
    ///
    /// Returns `None` for anything else — including the three vertical names,
    /// which [`is_vertical_name`] separates out for the caller's diagnostic.
    /// Use [`Self::parse_any`] for an image inside a substitution definition,
    /// where the vertical three are valid too.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::parse_from(raw, Self::HORIZONTAL)
    }

    /// Reads an `:align:` value for an image inside a substitution
    /// definition, where all six alignments are valid.
    #[must_use]
    pub fn parse_any(raw: &str) -> Option<Self> {
        Self::parse_from(raw, Self::ALL)
    }

    fn parse_from(raw: &str, candidates: &[Self]) -> Option<Self> {
        let normalized = raw.trim().to_lowercase();
        candidates
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
/// Used by the block-level `:align:` parser to explain *why* a vertical name
/// was refused — it is a real docutils rule, not a typo — rather than just
/// listing the three horizontal ones and leaving the author to guess.
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
        for align in ImageAlign::HORIZONTAL {
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
    fn test_parse_any_reads_every_horizontal_alignment() {
        for align in ImageAlign::HORIZONTAL {
            // Given / When
            let parsed = ImageAlign::parse_any(align.as_str());

            // Then
            assert_eq!(parsed, Some(*align));
        }
    }

    #[test]
    fn test_parse_any_reads_every_vertical_alignment() {
        for align in ImageAlign::VERTICAL {
            // Given — only valid inside a substitution definition
            let raw = align.as_str();

            // When
            let parsed = ImageAlign::parse_any(raw);

            // Then
            assert_eq!(parsed, Some(*align));
        }
    }

    #[test]
    fn test_parse_any_ignores_case_and_whitespace() {
        // Given
        let raw = "  Top  ";

        // When
        let parsed = ImageAlign::parse_any(raw);

        // Then
        assert_eq!(parsed, Some(ImageAlign::Top));
    }

    #[test]
    fn test_parse_any_rejects_an_unknown_name() {
        // Given
        let raw = "sideways";

        // When
        let parsed = ImageAlign::parse_any(raw);

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
    fn test_css_class_works_for_a_vertical_alignment_too() {
        // Given
        let align = ImageAlign::Top;

        // When
        let class = align.css_class();

        // Then
        assert_eq!(class, "align-top");
    }

    #[test]
    fn test_is_vertical_recognizes_all_three() {
        for align in ImageAlign::VERTICAL {
            // Given / When / Then
            assert!(align.is_vertical(), "{align} should be vertical");
        }
    }

    #[test]
    fn test_is_vertical_rejects_a_horizontal_alignment() {
        // Given / When / Then
        assert!(!ImageAlign::Left.is_vertical());
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

    #[test]
    fn test_vertical_alignment_serialization_round_trips() {
        // Given
        let align = ImageAlign::Bottom;

        // When
        let json = serde_json::to_string(&align).expect("should serialize");
        let restored: ImageAlign = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, align);
    }
}

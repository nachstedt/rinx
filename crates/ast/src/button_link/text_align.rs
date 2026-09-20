//! The alignment `:align:` places a button with.
//!
//! Four names, which are sphinx-design's `text_align` validator in its own
//! order. A closed enum rather than an open string for the reason
//! [`crate::SemanticColor`] is one: each name becomes a CSS class
//! (`sd-text-<name>`) this build's stylesheet has to define, so an unvalidated
//! value would render as an unaligned button rather than as an error.
//!
//! Kept inside `button_link/` rather than flat beside [`crate::Spacing`]
//! because this directive is its only writer today. A second one — a card, a
//! tab set — should lift it out rather than declare a parallel enum.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One of sphinx-design's four text alignments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

impl TextAlign {
    /// Every alignment, in sphinx-design's declaration order — which is the
    /// order a diagnostic lists them in, so the message reads like its docs.
    pub const ALL: &'static [Self] = &[Self::Left, Self::Right, Self::Center, Self::Justify];

    /// The name this alignment is written as, which is also the suffix of the
    /// CSS class it produces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Justify => "justify",
        }
    }

    /// The class sphinx-design's `text_align` puts on the containing element.
    #[must_use]
    pub fn css_class(self) -> String {
        format!("sd-text-{}", self.as_str())
    }

    /// Reads an `:align:` value, or `None` when it names no alignment.
    ///
    /// Trims and lowercases first, as [`crate::SemanticColor::parse`] does and
    /// for the same reason: docutils' own `choice` validator lowercases before
    /// matching.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|align| align.as_str() == normalized)
    }
}

impl fmt::Display for TextAlign {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_every_declared_alignment() {
        // Given — the names sphinx-design's `text_align` accepts
        let names = ["left", "right", "center", "justify"];

        // When / Then
        for name in names {
            assert_eq!(
                TextAlign::parse(name).map(TextAlign::as_str),
                Some(name),
                "`{name}` should be an alignment"
            );
        }
    }

    #[test]
    fn test_parse_normalizes_case_and_surrounding_space() {
        // Given
        let value = "  Center  ";

        // When
        let align = TextAlign::parse(value);

        // Then
        assert_eq!(align, Some(TextAlign::Center));
    }

    #[test]
    fn test_parse_rejects_a_name_sphinx_design_does_not_accept() {
        // Given — docutils' image `:align:` also has `top`/`middle`/`bottom`,
        // which this option does not
        let value = "middle";

        // When
        let align = TextAlign::parse(value);

        // Then
        assert_eq!(align, None);
    }

    #[test]
    fn test_css_class_is_the_name_behind_the_shared_prefix() {
        // Given / When / Then
        assert_eq!(TextAlign::Left.css_class(), "sd-text-left");
        assert_eq!(TextAlign::Justify.css_class(), "sd-text-justify");
    }

    #[test]
    fn test_all_lists_every_variant_once() {
        // Given / When
        let mut names: Vec<&str> = TextAlign::ALL.iter().map(|align| align.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();

        // Then
        assert_eq!(count, 4);
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn test_display_writes_the_written_name() {
        // Given / When / Then
        assert_eq!(TextAlign::Right.to_string(), "right");
    }
}

//! The semantic colour a `.. dropdown::` paints its summary bar with.
//!
//! Eleven names, which are sphinx-design's own `SEMANTIC_COLORS` tuple in its
//! own order. They are a closed enum rather than an open string because each
//! one becomes a *pair* of CSS classes (`sd-bg-<name>` and
//! `sd-bg-text-<name>`) that this build's stylesheet has to define: a colour
//! nobody styled would render as an unpainted bar rather than as an error.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One of sphinx-design's eleven semantic colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticColor {
    Primary,
    Secondary,
    Success,
    Info,
    Warning,
    Danger,
    Light,
    Muted,
    Dark,
    White,
    Black,
}

impl SemanticColor {
    /// Every colour, in sphinx-design's declaration order — which is the order
    /// a diagnostic lists them in, so the message reads like its docs.
    pub const ALL: &'static [Self] = &[
        Self::Primary,
        Self::Secondary,
        Self::Success,
        Self::Info,
        Self::Warning,
        Self::Danger,
        Self::Light,
        Self::Muted,
        Self::Dark,
        Self::White,
        Self::Black,
    ];

    /// The name this colour is written as, which is also the suffix of both
    /// CSS classes it produces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Success => "success",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Danger => "danger",
            Self::Light => "light",
            Self::Muted => "muted",
            Self::Dark => "dark",
            Self::White => "white",
            Self::Black => "black",
        }
    }

    /// Reads a `:color:` value, or `None` when it names no colour.
    ///
    /// Trims and lowercases first: every other option value in this build is
    /// normalized the same way, and docutils' own `choice` validator
    /// lowercases before matching.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|color| color.as_str() == normalized)
    }
}

impl fmt::Display for SemanticColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_every_declared_color() {
        // Given — the names sphinx-design's `SEMANTIC_COLORS` declares
        let names = [
            "primary",
            "secondary",
            "success",
            "info",
            "warning",
            "danger",
            "light",
            "muted",
            "dark",
            "white",
            "black",
        ];

        // When / Then
        for name in names {
            assert_eq!(
                SemanticColor::parse(name).map(SemanticColor::as_str),
                Some(name),
                "`{name}` should be a semantic colour"
            );
        }
    }

    #[test]
    fn test_all_lists_every_variant_once() {
        // Given / When
        let mut names: Vec<&str> = SemanticColor::ALL
            .iter()
            .map(|color| color.as_str())
            .collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();

        // Then — eleven distinct names, matching sphinx-design's tuple
        assert_eq!(count, 11);
        assert_eq!(names.len(), 11);
    }

    #[test]
    fn test_parse_normalizes_case_and_surrounding_space() {
        // Given
        let value = "  Success  ";

        // When
        let color = SemanticColor::parse(value);

        // Then
        assert_eq!(color, Some(SemanticColor::Success));
    }

    #[test]
    fn test_parse_rejects_an_unknown_name() {
        // Given
        let value = "chartreuse";

        // When
        let color = SemanticColor::parse(value);

        // Then
        assert_eq!(color, None);
    }

    #[test]
    fn test_display_writes_the_written_name() {
        // Given
        let color = SemanticColor::Muted;

        // When
        let rendered = color.to_string();

        // Then
        assert_eq!(rendered, "muted");
    }
}

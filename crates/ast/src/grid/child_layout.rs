//! How a `.. grid-item::` lays its own content out.
//!
//! Two independent closed choices, kept together because they describe one
//! thing — the flex box an item is — and because both are read by the same
//! `make_choice` validator in sphinx-design, which is why each is matched
//! against its set rather than parsed loosely.
//!
//! Note [`ChildDirection`] has a default and [`ChildAlign`] does not: an
//! omitted `:child-direction:` still emits `sd-d-flex-column`, while an
//! omitted `:child-align:` emits no class at all.

use std::fmt;

use serde::{Deserialize, Serialize};

/// `:child-direction:` — which way an item stacks its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ChildDirection {
    /// `column`, and what an omitted `:child-direction:` means.
    #[default]
    Column,
    /// `row`.
    Row,
}

impl ChildDirection {
    /// Both directions, in sphinx-design's declaration order.
    pub const ALL: &'static [Self] = &[Self::Column, Self::Row];

    /// The name this direction is written as.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Row => "row",
        }
    }

    /// The class the item carries, which is always present.
    #[must_use]
    pub fn css_class(self) -> String {
        format!("sd-d-flex-{}", self.as_str())
    }

    /// Reads a `:child-direction:` value, or `None` when it names neither.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|direction| direction.as_str() == normalized)
    }
}

impl fmt::Display for ChildDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// `:child-align:` — where an item puts its content along the major axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChildAlign {
    Start,
    End,
    Center,
    Justify,
    Spaced,
}

impl ChildAlign {
    /// Every alignment, in sphinx-design's declaration order.
    pub const ALL: &'static [Self] = &[
        Self::Start,
        Self::End,
        Self::Center,
        Self::Justify,
        Self::Spaced,
    ];

    /// The name this alignment is written as.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Justify => "justify",
            Self::Spaced => "spaced",
        }
    }

    /// The class the item carries when this alignment was written.
    #[must_use]
    pub fn css_class(self) -> String {
        format!("sd-align-major-{}", self.as_str())
    }

    /// Reads a `:child-align:` value, or `None` when it names no alignment.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|align| align.as_str() == normalized)
    }
}

impl fmt::Display for ChildAlign {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_defaults_to_column() {
        // Given / When
        let direction = ChildDirection::default();

        // Then — an omitted `:child-direction:` is a real choice, not an
        // absent one: the class is emitted either way
        assert_eq!(direction, ChildDirection::Column);
        assert_eq!(direction.css_class(), "sd-d-flex-column");
    }

    #[test]
    fn test_direction_parse_reads_both_names() {
        // Given / When / Then
        assert_eq!(ChildDirection::parse("row"), Some(ChildDirection::Row));
        assert_eq!(
            ChildDirection::parse("column"),
            Some(ChildDirection::Column)
        );
    }

    #[test]
    fn test_direction_parse_is_case_insensitive_and_trims() {
        // Given
        let written = "  Row  ";

        // When
        let direction = ChildDirection::parse(written);

        // Then
        assert_eq!(direction, Some(ChildDirection::Row));
    }

    #[test]
    fn test_direction_parse_refuses_an_unknown_name() {
        // Given / When / Then
        assert_eq!(ChildDirection::parse("diagonal"), None);
    }

    #[test]
    fn test_align_css_class_is_the_major_axis_family() {
        // Given
        let align = ChildAlign::Spaced;

        // When
        let class = align.css_class();

        // Then
        assert_eq!(class, "sd-align-major-spaced");
    }

    #[test]
    fn test_align_parse_reads_every_choice() {
        // Given / When / Then
        for align in ChildAlign::ALL {
            assert_eq!(ChildAlign::parse(align.as_str()), Some(*align));
        }
    }

    #[test]
    fn test_align_parse_refuses_an_unknown_name() {
        // Given / When / Then
        assert_eq!(ChildAlign::parse("middle"), None);
    }

    #[test]
    fn test_display_writes_the_name_back() {
        // Given / When / Then
        assert_eq!(ChildAlign::Justify.to_string(), "justify");
        assert_eq!(ChildDirection::Column.to_string(), "column");
    }
}

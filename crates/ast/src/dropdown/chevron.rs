//! Which chevron marks a `.. dropdown::`'s open/closed state.
//!
//! Two directions, named by the pair of arrows they move between:
//! `right-down` (the default) points right when closed and down when open;
//! `down-up` points down when closed and up when open. The name a value is
//! written as therefore does *not* name the icon drawn — [`Self::octicon`]
//! does, and it is the closed-state half of the pair, because a `<details>`
//! rotates its own marker with CSS from there.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A `:chevron:` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Chevron {
    /// `right-down` — sphinx-design's default, and what an omitted
    /// `:chevron:` means. Not merely the first variant: the transform reaches
    /// this case through an `else`, so every value but `down-up` lands here.
    #[default]
    RightDown,
    /// `down-up`.
    DownUp,
}

impl Chevron {
    /// Both directions, in sphinx-design's declaration order.
    pub const ALL: &'static [Self] = &[Self::RightDown, Self::DownUp];

    /// The name this direction is written as.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RightDown => "right-down",
            Self::DownUp => "down-up",
        }
    }

    /// The octicon drawn in the summary's state marker.
    ///
    /// The closed-state half of the pair: the stylesheet rotates it when the
    /// `<details>` opens, so only one icon is ever embedded.
    #[must_use]
    pub const fn octicon(self) -> &'static str {
        match self {
            Self::RightDown => "chevron-right",
            Self::DownUp => "chevron-down",
        }
    }

    /// The `sd-summary-<marker>` class suffix sphinx-design gives the state
    /// marker, which is its octicon name.
    #[must_use]
    pub const fn marker_class_suffix(self) -> &'static str {
        self.octicon()
    }

    /// Reads a `:chevron:` value, or `None` when it names no direction.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|chevron| chevron.as_str() == normalized)
    }
}

impl fmt::Display for Chevron {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_both_directions() {
        // Given / When / Then
        assert_eq!(Chevron::parse("right-down"), Some(Chevron::RightDown));
        assert_eq!(Chevron::parse("down-up"), Some(Chevron::DownUp));
    }

    #[test]
    fn test_parse_normalizes_case_and_surrounding_space() {
        // Given
        let value = "  Down-Up ";

        // When
        let chevron = Chevron::parse(value);

        // Then
        assert_eq!(chevron, Some(Chevron::DownUp));
    }

    #[test]
    fn test_parse_rejects_an_unknown_direction() {
        // Given
        let value = "left-right";

        // When
        let chevron = Chevron::parse(value);

        // Then
        assert_eq!(chevron, None);
    }

    #[test]
    fn test_default_is_right_down() {
        // Given / When
        let chevron = Chevron::default();

        // Then — an omitted `:chevron:` draws the right-pointing marker
        assert_eq!(chevron, Chevron::RightDown);
        assert_eq!(chevron.octicon(), "chevron-right");
    }

    #[test]
    fn test_octicon_names_the_closed_state_icon() {
        // Given / When / Then — the marker is the *closed* half of each pair
        assert_eq!(Chevron::RightDown.octicon(), "chevron-right");
        assert_eq!(Chevron::DownUp.octicon(), "chevron-down");
    }

    #[test]
    fn test_marker_class_suffix_matches_the_octicon() {
        // Given
        let chevron = Chevron::DownUp;

        // When / Then — sphinx-design names the class after the icon it drew
        assert_eq!(chevron.marker_class_suffix(), chevron.octicon());
    }

    #[test]
    fn test_display_writes_the_written_name() {
        // Given
        let chevron = Chevron::RightDown;

        // When
        let rendered = chevron.to_string();

        // Then
        assert_eq!(rendered, "right-down");
    }
}

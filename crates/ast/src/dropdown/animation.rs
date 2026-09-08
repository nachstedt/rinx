//! How a `.. dropdown::`'s body appears when it opens.
//!
//! Two animations, each of which is really just a CSS class
//! (`sd-<name>`) added to the container — the motion itself lives entirely in
//! the stylesheet. They are an enum rather than a class string because
//! sphinx-design validates the value at parse time, and a misspelled
//! animation would otherwise be indistinguishable from a deliberate custom
//! class, which `:class-container:` already exists for.

use std::fmt;

use serde::{Deserialize, Serialize};

/// An `:animate:` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Animation {
    FadeIn,
    FadeInSlideDown,
}

impl Animation {
    /// Both animations, in sphinx-design's declaration order.
    pub const ALL: &'static [Self] = &[Self::FadeIn, Self::FadeInSlideDown];

    /// The name this animation is written as, which is also the suffix of the
    /// `sd-` class it produces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FadeIn => "fade-in",
            Self::FadeInSlideDown => "fade-in-slide-down",
        }
    }

    /// The CSS class this animation adds to the dropdown's container.
    #[must_use]
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::FadeIn => "sd-fade-in",
            Self::FadeInSlideDown => "sd-fade-in-slide-down",
        }
    }

    /// Reads an `:animate:` value, or `None` when it names no animation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|animation| animation.as_str() == normalized)
    }
}

impl fmt::Display for Animation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_both_animations() {
        // Given / When / Then
        assert_eq!(Animation::parse("fade-in"), Some(Animation::FadeIn));
        assert_eq!(
            Animation::parse("fade-in-slide-down"),
            Some(Animation::FadeInSlideDown)
        );
    }

    #[test]
    fn test_parse_normalizes_case_and_surrounding_space() {
        // Given
        let value = " Fade-In ";

        // When
        let animation = Animation::parse(value);

        // Then
        assert_eq!(animation, Some(Animation::FadeIn));
    }

    #[test]
    fn test_parse_rejects_an_unknown_animation() {
        // Given
        let value = "slide-up";

        // When
        let animation = Animation::parse(value);

        // Then
        assert_eq!(animation, None);
    }

    #[test]
    fn test_css_class_prefixes_the_written_name() {
        // Given / When / Then — sphinx-design builds the class as `"sd-" + value`
        for animation in Animation::ALL {
            assert_eq!(
                animation.css_class(),
                format!("sd-{}", animation.as_str()),
                "{animation} should prefix its own name"
            );
        }
    }

    #[test]
    fn test_display_writes_the_written_name() {
        // Given
        let animation = Animation::FadeInSlideDown;

        // When
        let rendered = animation.to_string();

        // Then
        assert_eq!(rendered, "fade-in-slide-down");
    }
}

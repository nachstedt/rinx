//! The four value-less options a button is written with.
//!
//! A set of named flags rather than four `bool` fields, because that is what
//! they are: `directives.flag` accepts no value at all, so each one carries
//! exactly the fact that it was written. Modelling the set also keeps one
//! vocabulary for the parser to read names into, the node to answer questions
//! from and the renderer to draw — where four fields would have each of the
//! three spell the same four names again.
//!
//! [`crate::ButtonLink`] stores them in a `BTreeSet`, so a serialized button
//! writes them in one deterministic order however they were written.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One of sphinx-design's four button flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ButtonFlag {
    /// `:outline:` — draw the colour as a border rather than a fill. Produces
    /// nothing without a `:color:` to outline.
    Outline,
    /// `:expand:` — fill the width available.
    Expand,
    /// `:click-parent:` — make the enclosing block clickable.
    ClickParent,
    /// `:shadow:` — draw the button raised.
    Shadow,
}

impl ButtonFlag {
    /// Every flag, in sphinx-design's declaration order.
    pub const ALL: &'static [Self] =
        &[Self::Outline, Self::Expand, Self::ClickParent, Self::Shadow];

    /// The option name this flag is written as.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Outline => "outline",
            Self::Expand => "expand",
            Self::ClickParent => "click-parent",
            Self::Shadow => "shadow",
        }
    }

    /// Reads an option name as a flag, or `None` when it names no flag.
    ///
    /// Matched exactly rather than case-insensitively, unlike an option's
    /// *value*: docutils matches option names as written.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|flag| flag.as_str() == name)
    }
}

impl fmt::Display for ButtonFlag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_every_declared_flag() {
        // Given — the names sphinx-design's option spec declares as flags
        let names = ["outline", "expand", "click-parent", "shadow"];

        // When / Then
        for name in names {
            assert_eq!(
                ButtonFlag::parse(name).map(ButtonFlag::as_str),
                Some(name),
                "`{name}` should be a flag"
            );
        }
    }

    #[test]
    fn test_parse_rejects_an_option_that_carries_a_value() {
        // Given / When / Then — these are options, but not flags
        assert_eq!(ButtonFlag::parse("color"), None);
        assert_eq!(ButtonFlag::parse("tooltip"), None);
    }

    #[test]
    fn test_parse_matches_the_name_as_written() {
        // Given / When / Then
        assert_eq!(ButtonFlag::parse("Shadow"), None);
        assert_eq!(ButtonFlag::parse("click_parent"), None);
    }

    #[test]
    fn test_all_lists_every_variant_once() {
        // Given / When
        let mut names: Vec<&str> = ButtonFlag::ALL.iter().map(|flag| flag.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();

        // Then
        assert_eq!(count, 4);
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn test_display_writes_the_option_name() {
        // Given / When / Then
        assert_eq!(ButtonFlag::ClickParent.to_string(), "click-parent");
    }
}

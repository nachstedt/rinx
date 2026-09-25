//! How a bar chart lays its bars out, as types rather than flags.
//!
//! sphinx-needs spells each of these as a bare option — `:horizontal:`,
//! `:stacked:`, `:show_sum:`, `:show_top_sum:` — which read as booleans. Two
//! of them are really a choice between two layouts, and naming both sides
//! makes a `match` in the renderer say which layout each branch draws.

use serde::{Deserialize, Serialize};

/// Which way the bars run — `:horizontal:`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarOrientation {
    /// Bars rise from a horizontal category axis — sphinx-needs' default.
    #[default]
    Vertical,
    /// Bars run rightwards from a vertical category axis, the first category
    /// at the top.
    Horizontal,
}

impl BarOrientation {
    /// Whether this is the default, so a `.ast` need not record it.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// How a category's series share its place on the axis — `:stacked:`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarArrangement {
    /// Side by side, one bar per series — sphinx-needs' default.
    #[default]
    Grouped,
    /// On top of each other, one bar per category.
    Stacked,
}

impl BarArrangement {
    /// Whether this is the default, so a `.ast` need not record it.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Which values are written onto the chart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarValueLabels {
    /// `:show_sum:` — each bar's value in its middle.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inside: bool,
    /// `:show_top_sum:` — each bar's value past its end; stacked, the stack's
    /// total.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_end: bool,
}

impl BarValueLabels {
    /// Whether any value is written at all — and so whether `:sum_rotation:`
    /// has anything to turn.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.inside || self.at_end
    }

    /// Whether nothing is written, so a `.ast` need not record it.
    #[must_use]
    pub const fn is_default(&self) -> bool {
        !self.any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_defaults_are_sphinx_needs_defaults() {
        // Given / When
        let defaults = (
            BarOrientation::default(),
            BarArrangement::default(),
            BarValueLabels::default(),
        );

        // Then
        assert_eq!(
            defaults,
            (
                BarOrientation::Vertical,
                BarArrangement::Grouped,
                BarValueLabels {
                    inside: false,
                    at_end: false
                }
            )
        );
    }

    #[test]
    fn test_only_the_defaults_report_themselves_as_default() {
        // Given / When / Then
        assert!(BarOrientation::Vertical.is_default());
        assert!(!BarOrientation::Horizontal.is_default());
        assert!(BarArrangement::Grouped.is_default());
        assert!(!BarArrangement::Stacked.is_default());
    }

    #[test]
    fn test_value_labels_are_written_when_either_is_asked_for() {
        // Given
        let at_end = BarValueLabels {
            inside: false,
            at_end: true,
        };

        // When
        let any = at_end.any();

        // Then
        assert!(any);
        assert!(!at_end.is_default());
        assert!(!BarValueLabels::default().any());
    }
}

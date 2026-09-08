//! The space a `.. dropdown::` keeps around itself.
//!
//! sphinx-design's `margin_option` accepts either one value, applying to all
//! four sides, or exactly four, read as *top bottom left right* — note the
//! order, which is neither CSS's clockwise `top right bottom left` nor
//! alphabetical. Each value is `auto` or a step on a 0–5 spacing scale, and
//! the result is a list of `sd-m*` classes the stylesheet defines.
//!
//! Every value is its own variant rather than a validated integer, so an
//! out-of-range step cannot be constructed *or deserialized* in the first
//! place — the same reason the rest of this crate reaches for an enum before
//! a smart constructor.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One step on sphinx-design's spacing scale, or `auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarginValue {
    Auto,
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
}

impl MarginValue {
    /// Every value, in the order sphinx-design lists them in its own error
    /// message (`auto` first, then the scale).
    pub const ALL: &'static [Self] = &[
        Self::Auto,
        Self::Zero,
        Self::One,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
    ];

    /// The text this value is written as, which is also the suffix of the CSS
    /// class it produces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Zero => "0",
            Self::One => "1",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
        }
    }

    /// Reads one written value, or `None` when it is not on the scale.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|step| step.as_str() == value)
    }
}

impl fmt::Display for MarginValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A `:margin:` value: one step for every side, or one per side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Margin {
    /// One value, applying to all four sides — `sd-m-<value>`.
    All(MarginValue),
    /// Four values, written *top bottom left right*.
    Sides {
        top: MarginValue,
        bottom: MarginValue,
        left: MarginValue,
        right: MarginValue,
    },
}

/// Why a `:margin:` value could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidMargin {
    /// A written value that is not `auto` and not a step from 0 to 5.
    Value(String),
    /// Something other than one or four values was written. Carries how many
    /// there were, since that is what the author has to fix.
    Count(usize),
}

impl fmt::Display for InvalidMargin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(value) => write!(
                formatter,
                "'{value}' is not one of {}",
                MarginValue::ALL
                    .iter()
                    .map(|step| step.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Count(count) => write!(
                formatter,
                "expected one (all sides) or four (top bottom left right) values, found {count}"
            ),
        }
    }
}

impl Margin {
    /// The classes an *omitted* `:margin:` produces.
    ///
    /// sphinx-design defaults the container's margin classes to `sd-mb-3`
    /// rather than to nothing, so a dropdown with no `:margin:` still clears
    /// the content below it. Kept here, beside the vocabulary it belongs to,
    /// rather than as a literal in the renderer.
    pub const DEFAULT_CLASSES: &'static [&'static str] = &["sd-mb-3"];

    /// Reads a whole `:margin:` option value.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidMargin::Value`] for a step that is not on the scale,
    /// and [`InvalidMargin::Count`] when the number of values written is
    /// neither one nor four.
    pub fn parse(value: &str) -> Result<Self, InvalidMargin> {
        let mut steps = Vec::new();
        for written in value.split_whitespace() {
            let step = MarginValue::parse(written)
                .ok_or_else(|| InvalidMargin::Value(written.to_string()))?;
            steps.push(step);
        }
        match steps[..] {
            [all] => Ok(Self::All(all)),
            [top, bottom, left, right] => Ok(Self::Sides {
                top,
                bottom,
                left,
                right,
            }),
            _ => Err(InvalidMargin::Count(steps.len())),
        }
    }

    /// The CSS classes this margin adds to the dropdown's container.
    #[must_use]
    pub fn css_classes(self) -> Vec<String> {
        match self {
            Self::All(all) => vec![format!("sd-m-{}", all.as_str())],
            Self::Sides {
                top,
                bottom,
                left,
                right,
            } => [("t", top), ("b", bottom), ("l", left), ("r", right)]
                .into_iter()
                .map(|(side, step)| format!("sd-m{side}-{}", step.as_str()))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_a_single_value_as_every_side() {
        // Given
        let value = "3";

        // When
        let margin = Margin::parse(value);

        // Then
        assert_eq!(margin, Ok(Margin::All(MarginValue::Three)));
    }

    #[test]
    fn test_parse_reads_four_values_as_top_bottom_left_right() {
        // Given — sphinx-design's order, not CSS's clockwise one
        let value = "0 1 2 auto";

        // When
        let margin = Margin::parse(value);

        // Then
        assert_eq!(
            margin,
            Ok(Margin::Sides {
                top: MarginValue::Zero,
                bottom: MarginValue::One,
                left: MarginValue::Two,
                right: MarginValue::Auto,
            })
        );
    }

    #[test]
    fn test_parse_collapses_runs_of_whitespace() {
        // Given
        let value = "  1   2\t3 4 ";

        // When
        let margin = Margin::parse(value);

        // Then
        assert_eq!(
            margin,
            Ok(Margin::Sides {
                top: MarginValue::One,
                bottom: MarginValue::Two,
                left: MarginValue::Three,
                right: MarginValue::Four,
            })
        );
    }

    #[test]
    fn test_parse_rejects_a_step_off_the_scale() {
        // Given
        let value = "6";

        // When
        let margin = Margin::parse(value);

        // Then
        assert_eq!(margin, Err(InvalidMargin::Value("6".to_string())));
    }

    #[test]
    fn test_parse_rejects_two_values() {
        // Given — one or four, never two
        let value = "1 2";

        // When
        let margin = Margin::parse(value);

        // Then
        assert_eq!(margin, Err(InvalidMargin::Count(2)));
    }

    #[test]
    fn test_parse_rejects_an_empty_value() {
        // Given
        let value = "   ";

        // When
        let margin = Margin::parse(value);

        // Then — no values at all is a count problem, not a value problem
        assert_eq!(margin, Err(InvalidMargin::Count(0)));
    }

    #[test]
    fn test_css_classes_for_a_single_value() {
        // Given
        let margin = Margin::All(MarginValue::Auto);

        // When
        let classes = margin.css_classes();

        // Then
        assert_eq!(classes, vec!["sd-m-auto".to_string()]);
    }

    #[test]
    fn test_css_classes_for_four_values() {
        // Given
        let margin = Margin::Sides {
            top: MarginValue::Zero,
            bottom: MarginValue::Three,
            left: MarginValue::Auto,
            right: MarginValue::Five,
        };

        // When
        let classes = margin.css_classes();

        // Then — t, b, l, r, in that order
        assert_eq!(classes, vec!["sd-mt-0", "sd-mb-3", "sd-ml-auto", "sd-mr-5"]);
    }

    #[test]
    fn test_default_classes_match_the_bottom_margin_sphinx_design_applies() {
        // Given / When / Then
        assert_eq!(Margin::DEFAULT_CLASSES, &["sd-mb-3"]);
    }

    #[test]
    fn test_invalid_value_message_lists_the_scale() {
        // Given
        let error = InvalidMargin::Value("9".to_string());

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "'9' is not one of auto, 0, 1, 2, 3, 4, 5");
    }

    #[test]
    fn test_invalid_count_message_names_the_two_accepted_shapes() {
        // Given
        let error = InvalidMargin::Count(3);

        // When
        let message = error.to_string();

        // Then
        assert_eq!(
            message,
            "expected one (all sides) or four (top bottom left right) values, found 3"
        );
    }
}

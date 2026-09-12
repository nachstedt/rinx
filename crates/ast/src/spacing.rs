//! The space a sphinx-design container keeps around or inside itself.
//!
//! sphinx-design's `margin_option` and `padding_option` are one helper reading
//! one spec: either a single value applying to all four sides, or exactly
//! four, read as *top bottom left right* — note the order, which is neither
//! CSS's clockwise `top right bottom left` nor alphabetical. Each value is
//! `auto` or a step on a 0–5 spacing scale, and the result is a list of
//! `sd-m*` or `sd-p*` classes the stylesheet defines. Which of the two a
//! [`Spacing`] becomes is not a property of the value but of the option it was
//! written on, so it is asked for at [`Spacing::css_classes`] time via
//! [`SpacingKind`] rather than stored.
//!
//! Every value is its own variant rather than a validated integer, so an
//! out-of-range step cannot be constructed *or deserialized* in the first
//! place — the same reason the rest of this crate reaches for an enum before
//! a smart constructor.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One step on sphinx-design's spacing scale, or `auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpacingValue {
    Auto,
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
}

impl SpacingValue {
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

impl fmt::Display for SpacingValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Which side of the box a [`Spacing`] is spacing — outside or inside.
///
/// Two things differ between the pair, and both are asked of this type rather
/// than stored on the value: the CSS class infix (`sd-m*` against `sd-p*`),
/// and whether `auto` is on the scale — sphinx-design's `margin_option`
/// accepts it and `padding_option` does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpacingKind {
    /// `:margin:` — space outside the box, `sd-m*`.
    Margin,
    /// `:padding:` — space inside the box, `sd-p*`.
    Padding,
}

impl SpacingKind {
    /// The letter sphinx-design's class names use for this kind.
    #[must_use]
    pub const fn class_infix(self) -> &'static str {
        match self {
            Self::Margin => "m",
            Self::Padding => "p",
        }
    }

    /// Whether `auto` is on this kind's scale.
    #[must_use]
    pub const fn allows_auto(self) -> bool {
        matches!(self, Self::Margin)
    }

    /// The values this kind accepts, in the order a diagnostic lists them.
    #[must_use]
    pub fn accepted(self) -> Vec<SpacingValue> {
        SpacingValue::ALL
            .iter()
            .copied()
            .filter(|value| *value != SpacingValue::Auto || self.allows_auto())
            .collect()
    }
}

/// A `:margin:` or `:padding:` value: one step for every side, or one per side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Spacing {
    /// One value, applying to all four sides — `sd-m-<value>`.
    All(SpacingValue),
    /// Four values, written *top bottom left right*.
    Sides {
        top: SpacingValue,
        bottom: SpacingValue,
        left: SpacingValue,
        right: SpacingValue,
    },
}

/// Why a `:margin:` or `:padding:` value could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidSpacing {
    /// A written value that is not on the kind's scale. Carries the kind,
    /// since that is what decides whether `auto` was allowed.
    Value { written: String, kind: SpacingKind },
    /// Something other than one or four values was written. Carries how many
    /// there were, since that is what the author has to fix.
    Count(usize),
}

impl fmt::Display for InvalidSpacing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value { written, kind } => write!(
                formatter,
                "'{written}' is not one of {}",
                kind.accepted()
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

impl Spacing {
    /// Reads a whole `:margin:` or `:padding:` option value.
    ///
    /// `kind` decides whether `auto` is on the scale, so an `auto` padding is
    /// refused here rather than reaching the renderer as a class no
    /// stylesheet defines.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSpacing::Value`] for a step that is not on the scale,
    /// and [`InvalidSpacing::Count`] when the number of values written is
    /// neither one nor four.
    pub fn parse(value: &str, kind: SpacingKind) -> Result<Self, InvalidSpacing> {
        let mut steps = Vec::new();
        for written in value.split_whitespace() {
            let step = SpacingValue::parse(written)
                .filter(|step| *step != SpacingValue::Auto || kind.allows_auto())
                .ok_or_else(|| InvalidSpacing::Value {
                    written: written.to_string(),
                    kind,
                })?;
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
            _ => Err(InvalidSpacing::Count(steps.len())),
        }
    }

    /// The CSS classes this spacing adds, as a margin or as a padding.
    #[must_use]
    pub fn css_classes(self, kind: SpacingKind) -> Vec<String> {
        let infix = kind.class_infix();
        match self {
            Self::All(all) => vec![format!("sd-{infix}-{}", all.as_str())],
            Self::Sides {
                top,
                bottom,
                left,
                right,
            } => [("t", top), ("b", bottom), ("l", left), ("r", right)]
                .into_iter()
                .map(|(side, step)| format!("sd-{infix}{side}-{}", step.as_str()))
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
        let spacing = Spacing::parse(value, SpacingKind::Margin);

        // Then
        assert_eq!(spacing, Ok(Spacing::All(SpacingValue::Three)));
    }

    #[test]
    fn test_parse_reads_four_values_as_top_bottom_left_right() {
        // Given — sphinx-design's order, not CSS's clockwise one
        let value = "0 1 2 auto";

        // When
        let spacing = Spacing::parse(value, SpacingKind::Margin);

        // Then
        assert_eq!(
            spacing,
            Ok(Spacing::Sides {
                top: SpacingValue::Zero,
                bottom: SpacingValue::One,
                left: SpacingValue::Two,
                right: SpacingValue::Auto,
            })
        );
    }

    #[test]
    fn test_parse_collapses_runs_of_whitespace() {
        // Given
        let value = "  1   2\t3 4 ";

        // When
        let spacing = Spacing::parse(value, SpacingKind::Margin);

        // Then
        assert_eq!(
            spacing,
            Ok(Spacing::Sides {
                top: SpacingValue::One,
                bottom: SpacingValue::Two,
                left: SpacingValue::Three,
                right: SpacingValue::Four,
            })
        );
    }

    #[test]
    fn test_parse_rejects_a_step_off_the_scale() {
        // Given
        let value = "6";

        // When
        let spacing = Spacing::parse(value, SpacingKind::Margin);

        // Then
        assert_eq!(
            spacing,
            Err(InvalidSpacing::Value {
                written: "6".to_string(),
                kind: SpacingKind::Margin,
            })
        );
    }

    #[test]
    fn test_parse_rejects_two_values() {
        // Given — one or four, never two
        let value = "1 2";

        // When
        let spacing = Spacing::parse(value, SpacingKind::Margin);

        // Then
        assert_eq!(spacing, Err(InvalidSpacing::Count(2)));
    }

    #[test]
    fn test_parse_rejects_an_empty_value() {
        // Given
        let value = "   ";

        // When
        let spacing = Spacing::parse(value, SpacingKind::Margin);

        // Then — no values at all is a count problem, not a value problem
        assert_eq!(spacing, Err(InvalidSpacing::Count(0)));
    }

    #[test]
    fn test_css_classes_for_a_single_margin_value() {
        // Given
        let spacing = Spacing::All(SpacingValue::Auto);

        // When
        let classes = spacing.css_classes(SpacingKind::Margin);

        // Then
        assert_eq!(classes, vec!["sd-m-auto".to_string()]);
    }

    #[test]
    fn test_css_classes_for_four_margin_values() {
        // Given
        let spacing = Spacing::Sides {
            top: SpacingValue::Zero,
            bottom: SpacingValue::Three,
            left: SpacingValue::Auto,
            right: SpacingValue::Five,
        };

        // When
        let classes = spacing.css_classes(SpacingKind::Margin);

        // Then — t, b, l, r, in that order
        assert_eq!(classes, vec!["sd-mt-0", "sd-mb-3", "sd-ml-auto", "sd-mr-5"]);
    }

    #[test]
    fn test_css_classes_for_a_single_padding_value() {
        // Given — the same value, asked for as a padding
        let spacing = Spacing::All(SpacingValue::Two);

        // When
        let classes = spacing.css_classes(SpacingKind::Padding);

        // Then
        assert_eq!(classes, vec!["sd-p-2".to_string()]);
    }

    #[test]
    fn test_css_classes_for_four_padding_values() {
        // Given
        let spacing = Spacing::Sides {
            top: SpacingValue::One,
            bottom: SpacingValue::Two,
            left: SpacingValue::Three,
            right: SpacingValue::Four,
        };

        // When
        let classes = spacing.css_classes(SpacingKind::Padding);

        // Then
        assert_eq!(classes, vec!["sd-pt-1", "sd-pb-2", "sd-pl-3", "sd-pr-4"]);
    }

    #[test]
    fn test_parse_refuses_auto_for_a_padding() {
        // Given — `padding_option` leaves `auto` off the scale
        let value = "auto";

        // When
        let spacing = Spacing::parse(value, SpacingKind::Padding);

        // Then
        assert_eq!(
            spacing,
            Err(InvalidSpacing::Value {
                written: "auto".to_string(),
                kind: SpacingKind::Padding,
            })
        );
    }

    #[test]
    fn test_parse_accepts_auto_for_a_margin() {
        // Given / When / Then — the one place the two kinds disagree
        assert!(Spacing::parse("auto", SpacingKind::Margin).is_ok());
    }

    #[test]
    fn test_accepted_lists_auto_only_for_a_margin() {
        // Given / When / Then
        assert!(SpacingKind::Margin.accepted().contains(&SpacingValue::Auto));
        assert!(
            !SpacingKind::Padding
                .accepted()
                .contains(&SpacingValue::Auto)
        );
    }

    #[test]
    fn test_class_infix_distinguishes_the_two_kinds() {
        // Given / When / Then
        assert_eq!(SpacingKind::Margin.class_infix(), "m");
        assert_eq!(SpacingKind::Padding.class_infix(), "p");
    }

    #[test]
    fn test_invalid_value_message_lists_the_scale() {
        // Given
        let error = InvalidSpacing::Value {
            written: "9".to_string(),
            kind: SpacingKind::Margin,
        };

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "'9' is not one of auto, 0, 1, 2, 3, 4, 5");
    }

    #[test]
    fn test_invalid_padding_message_omits_auto() {
        // Given
        let error = InvalidSpacing::Value {
            written: "auto".to_string(),
            kind: SpacingKind::Padding,
        };

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "'auto' is not one of 0, 1, 2, 3, 4, 5");
    }

    #[test]
    fn test_invalid_count_message_names_the_two_accepted_shapes() {
        // Given
        let error = InvalidSpacing::Count(3);

        // When
        let message = error.to_string();

        // Then
        assert_eq!(
            message,
            "expected one (all sides) or four (top bottom left right) values, found 3"
        );
    }
}

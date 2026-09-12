//! The four-breakpoint value every sphinx-design grid option is written in.
//!
//! sphinx-design has one helper, `_media_option`, behind `.. grid::`'s
//! argument, a `.. grid-item::`'s `:columns:` and a grid's `:gutter:`: either
//! one value, applying at every breakpoint, or exactly four, read as
//! *xs sm md lg*. What differs between the three is only the range each value
//! must fall in and whether `auto` is one of them — that is [`MediaDomain`],
//! which is passed to the constructor rather than stored, because the option a
//! value was written on is what decides it.
//!
//! The class list is the other half worth spelling out. `_media_option`
//! returns *five* classes, not four: the first value again without a
//! breakpoint (the unqualified base), then one per breakpoint. So a written
//! `1 1 2 2` under the `sd-row-cols-` prefix becomes
//! `sd-row-cols-1 sd-row-cols-xs-1 sd-row-cols-sm-1 sd-row-cols-md-2
//! sd-row-cols-lg-2`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One breakpoint's value: a whole number of columns or steps, or `auto`.
///
/// The number is unvalidated on its own — which range it must fall in is a
/// property of the option, not of the value. [`MediaSpec::parse`] is the only
/// way to obtain one from text, and every public wrapper around a
/// [`MediaSpec`] re-validates on deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaValue {
    /// `auto` — accepted only where [`MediaDomain::allows_auto`] says so.
    Auto,
    /// A whole number, in the domain's inclusive range.
    Number(u8),
}

impl MediaValue {
    /// The text this value is written as, which is also the CSS class suffix.
    #[must_use]
    pub fn as_class_suffix(self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Number(number) => number.to_string(),
        }
    }
}

impl fmt::Display for MediaValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_class_suffix())
    }
}

/// Which option a [`MediaSpec`] is being read for, and therefore what counts
/// as a legal value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaDomain {
    /// A column count: 1 to 12, or `auto`. Both `.. grid::`'s argument and a
    /// `.. grid-item::`'s `:columns:`.
    Columns,
    /// A gutter step: 0 to 5, and never `auto`.
    Gutter,
}

impl MediaDomain {
    /// The inclusive range a written number must fall in.
    #[must_use]
    pub const fn range(self) -> (u8, u8) {
        match self {
            Self::Columns => (1, 12),
            Self::Gutter => (0, 5),
        }
    }

    /// Whether `auto` is one of this domain's values.
    #[must_use]
    pub const fn allows_auto(self) -> bool {
        matches!(self, Self::Columns)
    }

    /// Whether `value` is in this domain.
    #[must_use]
    pub fn accepts(self, value: MediaValue) -> bool {
        match value {
            MediaValue::Auto => self.allows_auto(),
            MediaValue::Number(number) => {
                let (low, high) = self.range();
                (low..=high).contains(&number)
            }
        }
    }
}

/// Why a media option value could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidMediaSpec {
    /// A written value outside the option's domain.
    Value {
        written: String,
        domain: MediaDomain,
    },
    /// Something other than one or four values was written.
    Count(usize),
}

impl fmt::Display for InvalidMediaSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value { written, domain } => {
                let (low, high) = domain.range();
                let auto = if domain.allows_auto() {
                    "either auto or "
                } else {
                    ""
                };
                write!(
                    formatter,
                    "'{written}' should be {auto}an integer from {low} to {high}"
                )
            }
            Self::Count(count) => write!(
                formatter,
                "expected 1 or 4 (xs sm md lg) values, found {count}"
            ),
        }
    }
}

/// One value per breakpoint: *xs sm md lg*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaSpec {
    pub xs: MediaValue,
    pub sm: MediaValue,
    pub md: MediaValue,
    pub lg: MediaValue,
}

impl MediaSpec {
    /// The breakpoint names, in the order sphinx-design writes them.
    pub const BREAKPOINTS: [&'static str; 4] = ["xs", "sm", "md", "lg"];

    /// Reads a written media option value against `domain`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidMediaSpec::Value`] for a value outside the domain, and
    /// [`InvalidMediaSpec::Count`] when the number of values written is
    /// neither one nor four.
    pub fn parse(value: &str, domain: MediaDomain) -> Result<Self, InvalidMediaSpec> {
        let mut values = Vec::new();
        for written in value.split_whitespace() {
            values.push(Self::parse_one(written, domain)?);
        }
        match values[..] {
            [all] => Ok(Self {
                xs: all,
                sm: all,
                md: all,
                lg: all,
            }),
            [xs, sm, md, lg] => Ok(Self { xs, sm, md, lg }),
            _ => Err(InvalidMediaSpec::Count(values.len())),
        }
    }

    /// Reads one written breakpoint value against `domain`.
    fn parse_one(written: &str, domain: MediaDomain) -> Result<MediaValue, InvalidMediaSpec> {
        let value = if written == "auto" {
            MediaValue::Auto
        } else {
            written
                .parse::<u8>()
                .map(MediaValue::Number)
                .map_err(|_| InvalidMediaSpec::Value {
                    written: written.to_string(),
                    domain,
                })?
        };
        if domain.accepts(value) {
            Ok(value)
        } else {
            Err(InvalidMediaSpec::Value {
                written: written.to_string(),
                domain,
            })
        }
    }

    /// Whether every breakpoint's value is in `domain`.
    ///
    /// The check a wrapper's `Deserialize` re-runs, so a hand-edited `.ast`
    /// cannot smuggle in a thirteenth column.
    #[must_use]
    pub fn is_within(&self, domain: MediaDomain) -> bool {
        self.values().into_iter().all(|value| domain.accepts(value))
    }

    /// The four values, in breakpoint order.
    #[must_use]
    pub fn values(&self) -> [MediaValue; 4] {
        [self.xs, self.sm, self.md, self.lg]
    }

    /// The classes this spec produces under `prefix`, which must end in `-`.
    ///
    /// Five of them: the unqualified base built from the first value, then one
    /// per breakpoint — see the module documentation.
    #[must_use]
    pub fn css_classes(&self, prefix: &str) -> Vec<String> {
        let mut classes = vec![format!("{prefix}{}", self.xs.as_class_suffix())];
        classes.extend(Self::BREAKPOINTS.into_iter().zip(self.values()).map(
            |(breakpoint, value)| format!("{prefix}{breakpoint}-{}", value.as_class_suffix()),
        ));
        classes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_a_single_value_as_every_breakpoint() {
        // Given
        let value = "2";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Columns);

        // Then
        assert_eq!(
            spec,
            Ok(MediaSpec {
                xs: MediaValue::Number(2),
                sm: MediaValue::Number(2),
                md: MediaValue::Number(2),
                lg: MediaValue::Number(2),
            })
        );
    }

    #[test]
    fn test_parse_reads_four_values_as_xs_sm_md_lg() {
        // Given — the corpus' own `.. grid:: 1 1 2 2`
        let value = "1 1 2 2";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Columns);

        // Then
        assert_eq!(
            spec,
            Ok(MediaSpec {
                xs: MediaValue::Number(1),
                sm: MediaValue::Number(1),
                md: MediaValue::Number(2),
                lg: MediaValue::Number(2),
            })
        );
    }

    #[test]
    fn test_parse_accepts_auto_for_columns() {
        // Given
        let value = "auto";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Columns);

        // Then
        assert_eq!(spec.map(|spec| spec.xs), Ok(MediaValue::Auto));
    }

    #[test]
    fn test_parse_refuses_auto_for_a_gutter() {
        // Given — `gutter_option` passes `allow_auto=False`
        let value = "auto";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Gutter);

        // Then
        assert_eq!(
            spec,
            Err(InvalidMediaSpec::Value {
                written: "auto".to_string(),
                domain: MediaDomain::Gutter,
            })
        );
    }

    #[test]
    fn test_parse_refuses_a_thirteenth_column() {
        // Given
        let value = "13";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Columns);

        // Then
        assert!(spec.is_err());
    }

    #[test]
    fn test_parse_refuses_a_zeroth_column_but_accepts_a_zero_gutter() {
        // Given / When / Then — the one place the two domains disagree on a
        // number rather than on `auto`
        assert!(MediaSpec::parse("0", MediaDomain::Columns).is_err());
        assert!(MediaSpec::parse("0", MediaDomain::Gutter).is_ok());
    }

    #[test]
    fn test_parse_refuses_two_values() {
        // Given — one or four, never two
        let value = "1 2";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Columns);

        // Then
        assert_eq!(spec, Err(InvalidMediaSpec::Count(2)));
    }

    #[test]
    fn test_parse_refuses_an_empty_value() {
        // Given
        let value = "   ";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Columns);

        // Then
        assert_eq!(spec, Err(InvalidMediaSpec::Count(0)));
    }

    #[test]
    fn test_parse_refuses_a_value_that_is_not_a_number() {
        // Given
        let value = "wide";

        // When
        let spec = MediaSpec::parse(value, MediaDomain::Columns);

        // Then
        assert_eq!(
            spec,
            Err(InvalidMediaSpec::Value {
                written: "wide".to_string(),
                domain: MediaDomain::Columns,
            })
        );
    }

    #[test]
    fn test_css_classes_repeat_the_first_value_without_a_breakpoint() {
        // Given
        let spec = MediaSpec::parse("1 1 2 2", MediaDomain::Columns).expect("valid");

        // When
        let classes = spec.css_classes("sd-row-cols-");

        // Then — five classes, the base built from the xs value
        assert_eq!(
            classes,
            vec![
                "sd-row-cols-1",
                "sd-row-cols-xs-1",
                "sd-row-cols-sm-1",
                "sd-row-cols-md-2",
                "sd-row-cols-lg-2",
            ]
        );
    }

    #[test]
    fn test_css_classes_spell_auto_out() {
        // Given
        let spec = MediaSpec::parse("auto", MediaDomain::Columns).expect("valid");

        // When
        let classes = spec.css_classes("sd-col-");

        // Then
        assert_eq!(classes[0], "sd-col-auto");
        assert_eq!(classes[4], "sd-col-lg-auto");
    }

    #[test]
    fn test_is_within_rejects_a_spec_built_for_another_domain() {
        // Given — a legal gutter step of 0, asked about as a column count
        let spec = MediaSpec::parse("0", MediaDomain::Gutter).expect("valid");

        // When / Then
        assert!(spec.is_within(MediaDomain::Gutter));
        assert!(!spec.is_within(MediaDomain::Columns));
    }

    #[test]
    fn test_invalid_value_message_names_the_domain_range() {
        // Given
        let error = InvalidMediaSpec::Value {
            written: "13".to_string(),
            domain: MediaDomain::Columns,
        };

        // When
        let message = error.to_string();

        // Then
        assert_eq!(
            message,
            "'13' should be either auto or an integer from 1 to 12"
        );
    }

    #[test]
    fn test_invalid_gutter_message_omits_auto() {
        // Given
        let error = InvalidMediaSpec::Value {
            written: "9".to_string(),
            domain: MediaDomain::Gutter,
        };

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "'9' should be an integer from 0 to 5");
    }

    #[test]
    fn test_invalid_count_message_names_the_two_accepted_shapes() {
        // Given
        let error = InvalidMediaSpec::Count(3);

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "expected 1 or 4 (xs sm md lg) values, found 3");
    }
}

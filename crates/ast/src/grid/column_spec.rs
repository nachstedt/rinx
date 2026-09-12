//! How many of twelve columns a grid row splits into, or a grid item spans.
//!
//! One [`MediaSpec`] in the [`MediaDomain::Columns`] domain, wrapped so that
//! the domain cannot be lost: a hand-edited `.ast` naming a thirteenth column
//! is refused on load rather than silently rendering a class no stylesheet
//! defines.
//!
//! Which prefix the classes take is not a property of the value but of where
//! it was written — a `.. grid::` argument counts the columns a *row* splits
//! into (`sd-row-cols-`), a `.. grid-item::`'s `:columns:` counts the ones an
//! *item* spans (`sd-col-`). That is [`ColumnPrefix`], asked for at class time.

use serde::{Deserialize, Deserializer, Serialize};

use super::media_spec::{InvalidMediaSpec, MediaDomain, MediaSpec};

/// Which of the two column class families a [`ColumnSpec`] renders as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnPrefix {
    /// A `.. grid::` argument — how many columns the row splits into.
    Row,
    /// A `.. grid-item::`'s `:columns:` — how many the item spans.
    Item,
}

impl ColumnPrefix {
    /// The class prefix, trailing `-` included.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Row => "sd-row-cols-",
            Self::Item => "sd-col-",
        }
    }
}

/// A column count per breakpoint: 1 to 12, or `auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ColumnSpec(MediaSpec);

impl ColumnSpec {
    /// Reads a written column count.
    ///
    /// # Errors
    ///
    /// Forwards [`MediaSpec::parse`]'s error in the column domain.
    pub fn parse(value: &str) -> Result<Self, InvalidMediaSpec> {
        MediaSpec::parse(value, MediaDomain::Columns).map(Self)
    }

    /// The classes this count produces in the given family.
    #[must_use]
    pub fn css_classes(&self, prefix: ColumnPrefix) -> Vec<String> {
        self.0.css_classes(prefix.as_str())
    }
}

impl<'de> Deserialize<'de> for ColumnSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spec = MediaSpec::deserialize(deserializer)?;
        if spec.is_within(MediaDomain::Columns) {
            Ok(Self(spec))
        } else {
            Err(serde::de::Error::custom(
                "column counts must be 1 to 12, or auto",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::media_spec::MediaValue;

    #[test]
    fn test_parse_reads_the_corpus_four_value_spelling() {
        // Given
        let value = "1 1 2 2";

        // When
        let spec = ColumnSpec::parse(value).expect("a valid column spec");

        // Then
        assert_eq!(spec.0.lg, MediaValue::Number(2));
    }

    #[test]
    fn test_parse_refuses_a_thirteenth_column() {
        // Given
        let value = "13";

        // When
        let spec = ColumnSpec::parse(value);

        // Then
        assert!(spec.is_err());
    }

    #[test]
    fn test_css_classes_use_the_row_family_for_a_grid_argument() {
        // Given
        let spec = ColumnSpec::parse("2").expect("a valid column spec");

        // When
        let classes = spec.css_classes(ColumnPrefix::Row);

        // Then
        assert_eq!(classes[0], "sd-row-cols-2");
        assert_eq!(classes[1], "sd-row-cols-xs-2");
    }

    #[test]
    fn test_css_classes_use_the_item_family_for_a_columns_option() {
        // Given
        let spec = ColumnSpec::parse("12 12 8 8").expect("a valid column spec");

        // When
        let classes = spec.css_classes(ColumnPrefix::Item);

        // Then
        assert_eq!(
            classes,
            vec![
                "sd-col-12",
                "sd-col-xs-12",
                "sd-col-sm-12",
                "sd-col-md-8",
                "sd-col-lg-8",
            ]
        );
    }

    #[test]
    fn test_round_trips_through_json() {
        // Given
        let spec = ColumnSpec::parse("1 1 2 2").expect("a valid column spec");

        // When
        let json = serde_json::to_string(&spec).expect("serializable");
        let restored: ColumnSpec = serde_json::from_str(&json).expect("deserializable");

        // Then
        assert_eq!(restored, spec);
    }

    #[test]
    fn test_deserialize_refuses_a_count_outside_the_domain() {
        // Given — a stored spec no `parse` could have produced
        let json = r#"{"xs":{"Number":99},"sm":{"Number":1},"md":{"Number":1},"lg":{"Number":1}}"#;

        // When
        let restored: Result<ColumnSpec, _> = serde_json::from_str(json);

        // Then
        assert!(restored.is_err());
    }
}

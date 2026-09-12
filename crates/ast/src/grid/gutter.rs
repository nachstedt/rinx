//! The space a `.. grid::` keeps between its items.
//!
//! One [`MediaSpec`] in the [`MediaDomain::Gutter`] domain — 0 to 5 and never
//! `auto`, which is the one way `:gutter:` differs from the column counts
//! beside it. Wrapped for the same reason [`super::ColumnSpec`] is: the domain
//! is re-checked on load.

use serde::{Deserialize, Deserializer, Serialize};

use super::media_spec::{InvalidMediaSpec, MediaDomain, MediaSpec};

/// A `:gutter:` value: a spacing step per breakpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Gutter(MediaSpec);

impl Gutter {
    /// The class prefix sphinx-design gives a gutter, trailing `-` included.
    pub const CLASS_PREFIX: &'static str = "sd-g-";

    /// Reads a written `:gutter:` value.
    ///
    /// # Errors
    ///
    /// Forwards [`MediaSpec::parse`]'s error in the gutter domain.
    pub fn parse(value: &str) -> Result<Self, InvalidMediaSpec> {
        MediaSpec::parse(value, MediaDomain::Gutter).map(Self)
    }

    /// The classes this gutter adds to the row.
    #[must_use]
    pub fn css_classes(&self) -> Vec<String> {
        self.0.css_classes(Self::CLASS_PREFIX)
    }
}

impl<'de> Deserialize<'de> for Gutter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spec = MediaSpec::deserialize(deserializer)?;
        if spec.is_within(MediaDomain::Gutter) {
            Ok(Self(spec))
        } else {
            Err(serde::de::Error::custom("gutter steps must be 0 to 5"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_a_single_step() {
        // Given
        let value = "3";

        // When
        let gutter = Gutter::parse(value).expect("a valid gutter");

        // Then
        assert_eq!(gutter.css_classes()[0], "sd-g-3");
    }

    #[test]
    fn test_parse_accepts_a_zero_step() {
        // Given — the gutter domain starts at 0, unlike a column count
        let value = "0";

        // When
        let gutter = Gutter::parse(value);

        // Then
        assert!(gutter.is_ok());
    }

    #[test]
    fn test_parse_refuses_auto() {
        // Given — `gutter_option` passes `allow_auto=False`
        let value = "auto";

        // When
        let gutter = Gutter::parse(value);

        // Then
        assert!(gutter.is_err());
    }

    #[test]
    fn test_parse_refuses_a_step_off_the_scale() {
        // Given
        let value = "6";

        // When
        let gutter = Gutter::parse(value);

        // Then
        assert!(gutter.is_err());
    }

    #[test]
    fn test_css_classes_cover_every_breakpoint() {
        // Given
        let gutter = Gutter::parse("1 2 3 4").expect("a valid gutter");

        // When
        let classes = gutter.css_classes();

        // Then
        assert_eq!(
            classes,
            vec!["sd-g-1", "sd-g-xs-1", "sd-g-sm-2", "sd-g-md-3", "sd-g-lg-4"]
        );
    }

    #[test]
    fn test_round_trips_through_json() {
        // Given
        let gutter = Gutter::parse("2").expect("a valid gutter");

        // When
        let json = serde_json::to_string(&gutter).expect("serializable");
        let restored: Gutter = serde_json::from_str(&json).expect("deserializable");

        // Then
        assert_eq!(restored, gutter);
    }

    #[test]
    fn test_deserialize_refuses_a_step_outside_the_domain() {
        // Given
        let json = r#"{"xs":"Auto","sm":"Auto","md":"Auto","lg":"Auto"}"#;

        // When
        let restored: Result<Gutter, _> = serde_json::from_str(json);

        // Then
        assert!(restored.is_err());
    }
}

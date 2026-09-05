use std::num::{NonZeroU32, NonZeroUsize};

use serde::{Deserialize, Serialize};

/// The options a `.. sectnum::` (docutils' other spelling:
/// `.. section-numbering::`) accepts, each already interpreted.
///
/// Unlike [`crate::ContentsOptions`] there is no `:local:`, `:backlinks:`,
/// `:class:` or `:name:` here — docutils' own `options_spec` for this
/// directive really is only these four, and it takes no argument either.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectnumOptions {
    /// `:depth:` — how many section levels to number. `None` is unlimited,
    /// matching an absent value and an explicit `:depth: 0` alike — the same
    /// convention [`crate::ContentsOptions::depth`] already uses, reused here
    /// rather than inventing different zero-semantics for a very similarly
    /// worded option.
    #[serde(default)]
    pub depth: Option<NonZeroUsize>,
    /// `:start:` — the number the document's first covered top-level section
    /// receives. `None` is docutils' own default of `1`. Unlike `:depth:`, a
    /// literal `0` has no sensible number to display, so it is rejected at
    /// parse time rather than given a meaning.
    #[serde(default)]
    pub start: Option<NonZeroU32>,
    /// `:prefix:` — literal text prepended to every number this directive
    /// renders.
    #[serde(default)]
    pub prefix: String,
    /// `:suffix:` — literal text appended to every number this directive
    /// renders. Docutils' own default is a non-breaking space; this build's
    /// renderer already appends its own separating space after a rendered
    /// section number (see `rusty_sphinx_renderer`'s heading rendering), so an
    /// unset `:suffix:` here is the empty string rather than replicating that
    /// default and doubling the gap.
    #[serde(default)]
    pub suffix: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options_are_all_unset() {
        // Given / When
        let options = SectnumOptions::default();

        // Then
        assert_eq!(options.depth, None);
        assert_eq!(options.start, None);
        assert_eq!(options.prefix, "");
        assert_eq!(options.suffix, "");
    }

    #[test]
    fn test_sectnum_options_round_trip_through_json() {
        // Given
        let options = SectnumOptions {
            depth: NonZeroUsize::new(2),
            start: NonZeroU32::new(5),
            prefix: "Appendix ".to_string(),
            suffix: ".".to_string(),
        };

        // When
        let json = serde_json::to_string(&options).expect("serializes");
        let restored: SectnumOptions = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, options);
    }

    #[test]
    fn test_sectnum_options_deserializes_from_an_empty_object() {
        // Given — every field is `#[serde(default)]`, so an index written by
        // an older build stays readable.
        let json = "{}";

        // When
        let restored: SectnumOptions = serde_json::from_str(json).expect("deserializes");

        // Then
        assert_eq!(restored, SectnumOptions::default());
    }
}

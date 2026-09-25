use serde::{Deserialize, Serialize};

/// How an enumerator is punctuated.
///
/// These are docutils' three `formats`, and the variant order is the order a
/// line is probed in: `(1)` has to be tried before `1)`, or the leading paren
/// would be left dangling in front of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnumeratorFormat {
    /// `(1)` — docutils' `parens`.
    Parens,
    /// `1)` — docutils' `rparen`.
    RightParen,
    /// `1.` — docutils' `period`.
    Period,
}

impl EnumeratorFormat {
    /// The order a line is probed in, matching docutils' `self.enum.formats`.
    pub const PROBE_ORDER: [Self; 3] = [Self::Parens, Self::RightParen, Self::Period];

    /// The text placed before the enumerator.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Parens => "(",
            Self::RightParen | Self::Period => "",
        }
    }

    /// The text placed after the enumerator.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Parens | Self::RightParen => ")",
            Self::Period => ".",
        }
    }

    /// The HTML class the renderer puts on the `<ol>` for this format.
    ///
    /// Sphinx has no such class — it drops the prefix and suffix, so `(a)` and
    /// `a.` render identically. rinx keeps them, and this class is what
    /// the stylesheet hangs the counter rules off.
    #[must_use]
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Parens => "parens",
            Self::RightParen => "rparen",
            Self::Period => "period",
        }
    }

    /// Whether the browser's own list marker already renders this format.
    ///
    /// Only [`Self::Period`] does — `list-style: decimal` produces `1.` — so it
    /// is the one format needing no counter rules.
    #[must_use]
    pub const fn is_native_marker(self) -> bool {
        matches!(self, Self::Period)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_and_suffix_match_docutils_format_info() {
        // Given each format
        // When reading its prefix and suffix
        // Then they spell the punctuation docutils records
        assert_eq!(
            (
                EnumeratorFormat::Parens.prefix(),
                EnumeratorFormat::Parens.suffix()
            ),
            ("(", ")")
        );
        assert_eq!(
            (
                EnumeratorFormat::RightParen.prefix(),
                EnumeratorFormat::RightParen.suffix()
            ),
            ("", ")")
        );
        assert_eq!(
            (
                EnumeratorFormat::Period.prefix(),
                EnumeratorFormat::Period.suffix()
            ),
            ("", ".")
        );
    }

    #[test]
    fn test_probe_order_tries_parens_before_right_paren() {
        // Given the probe order
        let order = EnumeratorFormat::PROBE_ORDER;

        // When looking at the first two entries
        // Then the parenthesised form comes first, so `(1)` is not read as `1)`
        assert_eq!(order[0], EnumeratorFormat::Parens);
        assert_eq!(order[1], EnumeratorFormat::RightParen);
    }

    #[test]
    fn test_probe_order_covers_every_format_exactly_once() {
        // Given the probe order
        let mut order = EnumeratorFormat::PROBE_ORDER.map(EnumeratorFormat::css_class);

        // When deduplicating it
        order.sort_unstable();
        let unique = {
            let mut v = order.to_vec();
            v.dedup();
            v
        };

        // Then no format is repeated or missing
        assert_eq!(unique.len(), EnumeratorFormat::PROBE_ORDER.len());
    }

    #[test]
    fn test_css_class_is_distinct_per_format() {
        // Given every format
        let classes = EnumeratorFormat::PROBE_ORDER.map(EnumeratorFormat::css_class);

        // When comparing the class names
        // Then all three differ
        assert_ne!(classes[0], classes[1]);
        assert_ne!(classes[1], classes[2]);
        assert_ne!(classes[0], classes[2]);
    }

    #[test]
    fn test_only_the_period_format_has_a_native_browser_marker() {
        // Given every format
        // When asking whether the browser renders it natively
        // Then only the period form does
        assert!(EnumeratorFormat::Period.is_native_marker());
        assert!(!EnumeratorFormat::Parens.is_native_marker());
        assert!(!EnumeratorFormat::RightParen.is_native_marker());
    }

    #[test]
    fn test_enum_format_serialization_roundtrip() {
        // Given every format
        for format in EnumeratorFormat::PROBE_ORDER {
            // When serializing and deserializing it
            let json = serde_json::to_string(&format).expect("Failed to serialize");
            let deserialized: EnumeratorFormat =
                serde_json::from_str(&json).expect("Failed to deserialize");

            // Then the value survives the round trip
            assert_eq!(format, deserialized);
        }
    }
}

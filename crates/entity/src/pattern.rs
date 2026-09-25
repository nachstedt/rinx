//! The regular expression a schema constrains an id or a text value with.

use std::fmt;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A regular expression a value must match, compiled once when the schema
/// loads.
///
/// Matching follows JSON Schema's `pattern` keyword, which is what a
/// sphinx-needs `schemas.json` rule is written in: the expression is
/// *searched for* rather than matched against the whole value, so an author
/// who means the whole value writes `^…$`. A project migrating its rules
/// therefore keeps their meaning without rewriting them.
///
/// The dialect is the `regex` crate's, not ECMA-262: lookaround and
/// backreferences do not compile. That is why the only way to build one is
/// [`ValuePattern::new`], which a schema loader turns into a load-time error —
/// a pattern that silently matched nothing, or everything, would be worse than
/// a refused schema.
#[derive(Clone)]
pub struct ValuePattern {
    source: String,
    regex: Regex,
}

impl ValuePattern {
    /// Compiles `source` into a pattern.
    ///
    /// # Errors
    ///
    /// Returns the compiler's error when `source` is not a regular expression
    /// the `regex` crate accepts.
    pub fn new(source: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            source: source.to_string(),
            regex: Regex::new(source)?,
        })
    }

    /// The expression as the schema wrote it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Reports whether the expression occurs anywhere in `text`.
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }
}

impl fmt::Debug for ValuePattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ValuePattern").field(&self.source).finish()
    }
}

impl fmt::Display for ValuePattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

// Equal when written identically: the compiled form is derived from the
// source, so two patterns spelled alike always behave alike.
impl PartialEq for ValuePattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for ValuePattern {}

impl Serialize for ValuePattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.source)
    }
}

impl<'de> Deserialize<'de> for ValuePattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Self::new(&source).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_compiles_a_valid_expression() {
        // Given
        let source = "^REQ_[0-9]+$";

        // When
        let pattern = ValuePattern::new(source).unwrap();

        // Then
        assert_eq!(pattern.as_str(), source);
        assert_eq!(pattern.to_string(), source);
    }

    #[test]
    fn test_new_refuses_an_expression_the_dialect_cannot_compile() {
        // Given — a lookahead, legal in ECMA-262 but not in the `regex` crate
        let source = "^(?=REQ)";

        // When
        let result = ValuePattern::new(source);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_is_match_searches_rather_than_matching_the_whole_value() {
        // Given — JSON Schema's semantics: unanchored unless written anchored
        let unanchored = ValuePattern::new("REQ_").unwrap();
        let anchored = ValuePattern::new("^REQ_").unwrap();

        // When / Then
        assert!(unanchored.is_match("X_REQ_1"));
        assert!(!anchored.is_match("X_REQ_1"));
        assert!(anchored.is_match("REQ_1"));
    }

    #[test]
    fn test_patterns_are_equal_when_written_alike() {
        // Given
        let first = ValuePattern::new("^A$").unwrap();
        let same = ValuePattern::new("^A$").unwrap();
        let other = ValuePattern::new("^B$").unwrap();

        // When / Then
        assert_eq!(first, same);
        assert_ne!(first, other);
    }

    #[test]
    fn test_pattern_round_trips_through_its_source_text() {
        // Given
        let pattern = ValuePattern::new("^SG_[A-Z0-9_]+$").unwrap();

        // When
        let json = serde_json::to_string(&pattern).unwrap();
        let back: ValuePattern = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(json, "\"^SG_[A-Z0-9_]+$\"");
        assert_eq!(back, pattern);
        assert!(back.is_match("SG_1"));
    }

    #[test]
    fn test_deserialize_refuses_an_expression_that_does_not_compile() {
        // Given — a stored pattern must re-validate on load, not be trusted
        let json = "\"(unclosed\"";

        // When
        let result: Result<ValuePattern, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_debug_shows_the_source_text() {
        // Given
        let pattern = ValuePattern::new("^A$").unwrap();

        // When
        let debug = format!("{pattern:?}");

        // Then
        assert_eq!(debug, "ValuePattern(\"^A$\")");
    }
}

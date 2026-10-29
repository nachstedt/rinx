//! A value together with where the author wrote it.

use serde::{Deserialize, Serialize};

use crate::span::Span;

/// A name the author wrote — a label, a directive's `:name:`, an equation's
/// `:label:`, an entity's `:id:` — with the span of exactly its characters.
///
/// The span is the editor's alone: a rename rewrites those characters and
/// nothing around them. It is never serialized, as an
/// [`IncludeSite`](crate::IncludeSite) is not, so a `.ast` file reads and
/// writes exactly the value, and a value read back from one has no span.
/// Equality ignores the span for the same reason: two values a round trip
/// through a `.ast` cannot tell apart are equal, and a definition moved down
/// a line still defines the same thing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Written<T> {
    value: T,
    #[serde(skip)]
    span: Option<Span>,
}

impl<T> Written<T> {
    /// `value`, written at `span` — `None` when it was read from no source
    /// line, such as a generated row.
    #[must_use]
    pub const fn new(value: T, span: Option<Span>) -> Self {
        Self { value, span }
    }

    /// `value`, written nowhere in particular: built by code, or read back
    /// from a `.ast` file.
    #[must_use]
    pub const fn unplaced(value: T) -> Self {
        Self::new(value, None)
    }

    /// The value.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// The value, without where it was written.
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Exactly the characters the value was written as, when known.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
}

impl<T> From<T> for Written<T> {
    fn from(value: T) -> Self {
        Self::unplaced(value)
    }
}

impl<T: PartialEq> PartialEq for Written<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Written<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Position;
    use crate::target_name::TargetName;

    fn span(line: u32) -> Span {
        Span::new(Position::new(line, 4), Position::new(line, 11))
    }

    #[test]
    fn test_written_serializes_as_its_value_alone() {
        // Given
        let written = Written::new(TargetName::new("install"), Some(span(3)));

        // When
        let json = serde_json::to_string(&written).expect("serializes");

        // Then
        assert_eq!(
            json,
            serde_json::to_string(&TargetName::new("install")).expect("serializes")
        );
    }

    #[test]
    fn test_written_reads_back_without_a_span() {
        // Given
        let json = serde_json::to_string(&Written::new(TargetName::new("install"), Some(span(3))))
            .expect("serializes");

        // When
        let read: Written<TargetName> = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(read.value(), &TargetName::new("install"));
        assert_eq!(read.span(), None);
    }

    #[test]
    fn test_written_values_are_equal_wherever_they_were_written() {
        // Given
        let here = Written::new(TargetName::new("install"), Some(span(3)));
        let there = Written::new(TargetName::new("install"), Some(span(9)));

        // When / Then
        assert_eq!(here, there);
        assert_eq!(here, Written::unplaced(TargetName::new("install")));
        assert_ne!(here, Written::unplaced(TargetName::new("setup")));
    }

    #[test]
    fn test_into_value_drops_the_span() {
        // Given
        let written = Written::new(TargetName::new("install"), Some(span(3)));

        // When / Then
        assert_eq!(written.into_value(), TargetName::new("install"));
    }
}

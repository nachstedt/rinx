//! Where something was read: lines and columns as an author counts them.
//!
//! Lines are 1-based, and so are columns, counted in characters rather than
//! bytes — the convention every other rinx crate reports positions in, so a
//! consumer converts to a protocol's encoding once, at its own boundary.

/// A point in the source, before the character at `column` on `line`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    /// The start of a source.
    pub const START: Self = Self { line: 1, column: 1 };
}

/// A range of the source, from `start` up to (not including) `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    /// The range from the start of `self` to the end of `other`.
    #[must_use]
    pub const fn to(self, other: Self) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(line: usize, column: usize) -> Position {
        Position { line, column }
    }

    #[test]
    fn test_to_spans_from_the_first_start_to_the_second_end() {
        // Given
        let first = Span {
            start: at(1, 1),
            end: at(1, 4),
        };
        let second = Span {
            start: at(2, 3),
            end: at(2, 9),
        };

        // When
        let joined = first.to(second);

        // Then
        assert_eq!(
            joined,
            Span {
                start: at(1, 1),
                end: at(2, 9)
            }
        );
    }

    #[test]
    fn test_positions_order_by_line_then_column() {
        // When / Then
        assert!(at(1, 9) < at(2, 1));
        assert!(at(2, 1) < at(2, 2));
        assert_eq!(Position::START, at(1, 1));
    }
}

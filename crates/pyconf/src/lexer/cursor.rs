//! Walking a source character by character, knowing where each one is.

use crate::position::{Position, Span};

/// A position in a source, advanced one character at a time.
///
/// `\r\n`, `\n` and a lone `\r` each end a line, as they do for Python.
pub(crate) struct Cursor {
    chars: Vec<char>,
    index: usize,
    position: Position,
}

impl Cursor {
    pub(crate) fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            index: 0,
            position: Position::START,
        }
    }

    /// The character under the cursor.
    pub(crate) fn peek(&self) -> Option<char> {
        self.peek_at(0)
    }

    /// The character `offset` characters past the cursor.
    pub(crate) fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.index + offset).copied()
    }

    /// Where the cursor is.
    pub(crate) const fn position(&self) -> Position {
        self.position
    }

    /// The range from `start` to the cursor.
    pub(crate) const fn span_from(&self, start: Position) -> Span {
        Span {
            start,
            end: self.position,
        }
    }

    /// Whether a line ends at the cursor.
    pub(crate) fn at_newline(&self) -> bool {
        matches!(self.peek(), Some('\n' | '\r'))
    }

    /// Moves past the character under the cursor and returns it. A line
    /// ending of two characters counts as one line.
    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.index += 1;
        match c {
            // The `\n` of a `\r\n` ends the line.
            '\r' if self.peek() == Some('\n') => {}
            '\n' | '\r' => {
                self.position.line += 1;
                self.position.column = 1;
            }
            _ => self.position.column += 1,
        }
        Some(c)
    }

    /// Moves past a line ending, if one is under the cursor.
    pub(crate) fn bump_newline(&mut self) -> bool {
        match self.peek() {
            Some('\r') => {
                self.bump();
                if self.peek() == Some('\n') {
                    self.bump();
                }
                true
            }
            Some('\n') => {
                self.bump();
                true
            }
            _ => false,
        }
    }

    /// Moves past the rest of the line, leaving its ending under the cursor.
    pub(crate) fn skip_to_newline(&mut self) {
        while self.peek().is_some() && !self.at_newline() {
            self.bump();
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
    fn test_bump_counts_columns_in_characters() {
        // Given
        let mut cursor = Cursor::new("äb");

        // When
        cursor.bump();

        // Then
        assert_eq!(cursor.position(), at(1, 2));
        assert_eq!(cursor.peek(), Some('b'));
    }

    #[test]
    fn test_bump_starts_a_line_after_each_kind_of_line_ending() {
        // Given
        let mut cursor = Cursor::new("a\nb\r\nc\rd");

        // When
        let mut positions = Vec::new();
        while let Some(c) = cursor.bump() {
            if c.is_alphabetic() {
                positions.push(cursor.position());
            }
        }

        // Then
        assert_eq!(positions, [at(1, 2), at(2, 2), at(3, 2), at(4, 2)]);
    }

    #[test]
    fn test_bump_newline_consumes_a_whole_crlf() {
        // Given
        let mut cursor = Cursor::new("\r\nx");

        // When
        let bumped = cursor.bump_newline();

        // Then
        assert!(bumped);
        assert_eq!(cursor.peek(), Some('x'));
        assert_eq!(cursor.position(), at(2, 1));
        assert!(!cursor.bump_newline());
    }

    #[test]
    fn test_skip_to_newline_stops_before_the_line_ending() {
        // Given
        let mut cursor = Cursor::new("# note\nx");

        // When
        cursor.skip_to_newline();

        // Then
        assert!(cursor.at_newline());
        assert_eq!(cursor.span_from(Position::START).end, at(1, 7));
    }

    #[test]
    fn test_peek_at_looks_ahead_without_moving() {
        // Given
        let cursor = Cursor::new("abc");

        // When / Then
        assert_eq!(cursor.peek_at(2), Some('c'));
        assert_eq!(cursor.peek_at(3), None);
        assert_eq!(cursor.position(), Position::START);
    }
}

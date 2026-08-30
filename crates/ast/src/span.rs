use serde::{Deserialize, Serialize};

/// A single place in a source `.rst` file, as a reader or an editor would
/// point at it: both components are **1-based**, so the very first character
/// of a document is `Position { line: 1, column: 1 }`.
///
/// # Encoding
///
/// `column` counts **Unicode scalar values** (Rust `char`s), not bytes and not
/// UTF-16 code units. Bytes would make a column meaningless the moment a line
/// contains a non-ASCII character, which real documentation constantly does
/// (`—`, `’`, `π`). Characters are what the parser can produce cheaply and
/// what a human reading `guide.rst:12:30` expects to count to.
///
/// The Language Server Protocol, by contrast, defaults to UTF-16 code units
/// (`PositionEncodingKind`), so the future `rusty_sphinx_lsp` crate must
/// convert at its boundary rather than either side being "fixed" to match the
/// other — see `docs/decisions/003-diagnostics.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column, counted in characters. See the encoding note above.
    pub column: u32,
}

impl Position {
    /// The position at `line`/`column`, both 1-based.
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    /// The first character of `line`.
    #[must_use]
    pub const fn line_start(line: u32) -> Self {
        Self::new(line, 1)
    }
}

/// The extent of the source text a diagnostic is about: `start` inclusive,
/// `end` exclusive, in the manner of a Rust range.
///
/// A range rather than a single point because the reason positions exist at
/// all is to be shown to an author, and the eventual language server has to
/// *underline* a mistake rather than put a caret before it (see
/// `architecture.md`'s diagnostics section). Producing the end costs nothing:
/// the inline scan already knows where every construct stops, and a
/// block-level parser has the offending line's text in hand.
///
/// A span never crosses documents, so it carries no path — the reporting
/// layer supplies that, since it is the phase that knows which file is being
/// processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    /// The exact extent of a construct whose both ends are known — the inline
    /// markup case, where the scan yields a start and an end offset.
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// The whole of one line, for a block-level diagnostic that can name the
    /// offending line but has no meaningful column within it.
    ///
    /// `text` is that line's content; its character count sets the end, so the
    /// range covers exactly what the author sees on the line.
    #[must_use]
    pub fn whole_line(line: u32, text: &str) -> Self {
        Self::new(
            Position::line_start(line),
            Position::new(line, char_column_after(text)),
        )
    }

    /// Every line from `first` through `last` inclusive, for a diagnostic
    /// about a multi-line construct as a whole (a malformed grid table, a
    /// transition and its neighbours).
    ///
    /// `last_text` is the content of the *last* line, which sets the end
    /// column the same way [`Self::whole_line`] does.
    #[must_use]
    pub fn lines(first: u32, last: u32, last_text: &str) -> Self {
        Self::new(
            Position::line_start(first),
            Position::new(last, char_column_after(last_text)),
        )
    }
}

/// The 1-based column just past the last character of `text` — i.e. the
/// exclusive end column of a range covering all of it.
///
/// A `u32` because [`Position`] is one; a source line long enough to overflow
/// `u32` cannot be read into memory in the first place, so the saturating cast
/// is unreachable rather than lossy.
fn char_column_after(text: &str) -> u32 {
    u32::try_from(text.chars().count() + 1).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_new_keeps_line_and_column() {
        // Given / When
        let position = Position::new(12, 30);

        // Then
        assert_eq!(position.line, 12);
        assert_eq!(position.column, 30);
    }

    #[test]
    fn test_line_start_is_column_one() {
        // Given / When
        let position = Position::line_start(7);

        // Then — columns are 1-based, so a line starts at 1, not 0.
        assert_eq!(position, Position::new(7, 1));
    }

    #[test]
    fn test_whole_line_covers_the_whole_line() {
        // Given a four-character line
        let text = "abcd";

        // When
        let span = Span::whole_line(3, text);

        // Then — start at the first character, end just past the last.
        assert_eq!(span.start, Position::new(3, 1));
        assert_eq!(span.end, Position::new(3, 5));
    }

    #[test]
    fn test_whole_line_of_empty_line_is_empty() {
        // Given
        let span = Span::whole_line(9, "");

        // When / Then — start and end coincide: nothing to underline.
        assert_eq!(span.start, span.end);
        assert_eq!(span.start, Position::new(9, 1));
    }

    #[test]
    fn test_whole_line_counts_characters_not_bytes() {
        // Given a line whose characters are multi-byte in UTF-8
        let text = "πππ";

        // When
        let span = Span::whole_line(1, text);

        // Then — three characters, so the end is column 4, not column 7.
        assert_eq!(span.end, Position::new(1, 4));
    }

    #[test]
    fn test_lines_spans_from_first_line_start_to_last_line_end() {
        // Given
        let span = Span::lines(10, 14, "+---+");

        // When / Then
        assert_eq!(span.start, Position::new(10, 1));
        assert_eq!(span.end, Position::new(14, 6));
    }

    #[test]
    fn test_new_keeps_both_ends() {
        // Given
        let start = Position::new(2, 5);
        let end = Position::new(2, 19);

        // When
        let span = Span::new(start, end);

        // Then
        assert_eq!(span.start, start);
        assert_eq!(span.end, end);
    }

    #[test]
    fn test_span_serialization_roundtrip() {
        // Given
        let span = Span::new(Position::new(4, 8), Position::new(5, 2));

        // When
        let json = serde_json::to_string(&span).expect("Failed to serialize");
        let deserialized: Span = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(span, deserialized);
    }

    #[test]
    fn test_positions_order_by_line_then_column() {
        // Given positions on the same and on different lines
        let earlier_line = Position::new(1, 99);
        let later_line = Position::new(2, 1);
        let earlier_column = Position::new(2, 1);
        let later_column = Position::new(2, 2);

        // When / Then — the derived ordering must be document order.
        assert!(earlier_line < later_line);
        assert!(earlier_column < later_column);
    }
}

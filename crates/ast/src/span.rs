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
/// (`PositionEncodingKind`), so the future `rinx_lsp` crate must
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

/// Which file a [`Span`]'s line numbers count in, when that is not the
/// document being processed.
///
/// An interned index into [`Document::source_files`](crate::Document), not a
/// path, for one reason above all: [`Span`] is [`Copy`] and is passed by value
/// through every reporting signature in the workspace. A `String` on it would
/// end that, and an `Arc<str>` would still end it. A `u32` costs four bytes,
/// keeps the wire form small when a document includes the same fragment
/// hundreds of times, and leaves the resolution — id to path — in the one
/// place that owns the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileId(u32);

impl FileId {
    /// The id for the file at `index` in a document's source-file table.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// This id as an index into that table.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
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
/// A span never crosses *documents*, but since `.. include::` it may name a
/// file other than the document being processed — see [`Self::file`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: Position,
    pub end: Position,
    /// The file [`Self::start`] and [`Self::end`] count lines in, when that is
    /// not the document itself.
    ///
    /// `None` — the overwhelming majority — means the document being
    /// processed, whose path every reporting layer already knows. It is
    /// `Some` only for content spliced in by `.. include::` or
    /// `.. literalinclude::`, whose line 1 is its own line 1 and not the
    /// document's.
    ///
    /// This lives on the span rather than on [`Diagnostic`](crate::Diagnostic)
    /// because a span outlives the diagnostic that quotes it: an
    /// `InlineNode`'s span sits in the `.ast` and is read again at *render*
    /// time, when a broken `:ref:` inside an included fragment must still be
    /// reported against the fragment. A file recorded only on parse-time
    /// diagnostics could not answer that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<FileId>,
}

impl Span {
    /// The exact extent of a construct whose both ends are known — the inline
    /// markup case, where the scan yields a start and an end offset.
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self {
            start,
            end,
            file: None,
        }
    }

    /// The same extent, measured in `file` rather than in the document — see
    /// [`Self::file`].
    ///
    /// A builder rather than a parameter on every constructor because the file
    /// is not known where a span is *built*: the parsers that produce spans
    /// were handed a slice of lines and cannot tell where those lines came
    /// from. `ParseCtx` applies this once, at the single point that does know.
    #[must_use]
    pub const fn with_file(mut self, file: Option<FileId>) -> Self {
        self.file = file;
        self
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
    fn test_file_id_round_trips_through_its_index() {
        // Given / When
        let id = FileId::new(3);

        // Then
        assert_eq!(id.index(), 3);
    }

    #[test]
    fn test_file_ids_of_different_files_differ() {
        // Given / When / Then — the table's indices are the identity
        assert_ne!(FileId::new(0), FileId::new(1));
    }

    #[test]
    fn test_a_span_belongs_to_the_document_by_default() {
        // Given / When — every constructor
        let explicit = Span::new(Position::new(1, 1), Position::new(1, 2));
        let whole_line = Span::whole_line(3, "abc");
        let multi_line = Span::lines(3, 4, "abc");

        // Then — no file means "the document being processed"
        assert_eq!(explicit.file, None);
        assert_eq!(whole_line.file, None);
        assert_eq!(multi_line.file, None);
    }

    #[test]
    fn test_with_file_attributes_the_span_to_that_file() {
        // Given a span built while parsing an included fragment
        let span = Span::whole_line(3, "abc");

        // When
        let attributed = span.with_file(Some(FileId::new(2)));

        // Then
        assert_eq!(attributed.file, Some(FileId::new(2)));
    }

    #[test]
    fn test_with_file_leaves_the_extent_alone() {
        // Given
        let span = Span::whole_line(3, "abc");

        // When
        let attributed = span.with_file(Some(FileId::new(2)));

        // Then — only the attribution moves
        assert_eq!(attributed.start, span.start);
        assert_eq!(attributed.end, span.end);
    }

    #[test]
    fn test_with_file_of_none_leaves_the_span_on_the_document() {
        // Given — the shape `ParseCtx` uses, which holds an `Option` already
        let span = Span::whole_line(3, "abc");

        // When
        let attributed = span.with_file(None);

        // Then
        assert_eq!(attributed.file, None);
    }

    #[test]
    fn test_spans_in_different_files_are_not_equal() {
        // Given the same extent in two different files
        let in_document = Span::whole_line(3, "abc");
        let in_fragment = in_document.with_file(Some(FileId::new(0)));

        // When / Then — line 3 of a fragment is not line 3 of its includer,
        // and equality must not pretend otherwise.
        assert_ne!(in_document, in_fragment);
    }

    #[test]
    fn test_serialization_omits_an_absent_file() {
        // Given a span in the document itself
        let span = Span::whole_line(3, "abc");

        // When
        let json = serde_json::to_string(&span).expect("Failed to serialize");

        // Then — the common case must not grow the `.ast` wire form, which
        // carries one span per inline node.
        assert!(!json.contains("file"), "{json}");
    }

    #[test]
    fn test_serialization_roundtrip_with_a_file() {
        // Given
        let span = Span::whole_line(3, "abc").with_file(Some(FileId::new(7)));

        // When
        let json = serde_json::to_string(&span).expect("Failed to serialize");
        let deserialized: Span = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(span, deserialized);
    }

    #[test]
    fn test_deserialization_defaults_a_missing_file_to_none() {
        // Given JSON written before this field existed
        let json = r#"{"start":{"line":1,"column":1},"end":{"line":1,"column":4}}"#;

        // When
        let span: Span = serde_json::from_str(json).expect("Failed to deserialize");

        // Then — an `.ast` from an older build still loads
        assert_eq!(span.file, None);
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

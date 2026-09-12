//! Mapping a byte offset in the text handed to the inline scan back to a
//! position in the `.rst` file.
//!
//! Block-level parsing reflows before it hands text to
//! [`crate::inline::parse_inline_text`]: a paragraph's lines are trimmed and
//! joined with `\n`, so by the time a role is matched, both its line and its
//! column have been lost. A [`SourceMap`] is built *while* that joining
//! happens — the one moment both forms are in hand — recording where each
//! appended run started on each side.
//!
//! Positions come out as characters, not bytes, because that is what
//! [`Position`] documents and what a reader counting along a line expects.

use rusty_sphinx_ast::Span;

use crate::context::{ParseCtx, SourcePoint};

/// One contiguous run of text that came from a single source line.
struct Segment {
    /// Byte offset at which this run begins in the joined text.
    text_offset: usize,
    /// The run's own text, so a byte offset inside it can be converted to a
    /// character count without consulting the joined string again.
    text: String,
    /// 0-based line index, relative to the [`ParseCtx`] the map is resolved
    /// against.
    source_line: usize,
    /// 0-based column at which this run begins on that source line — the
    /// leading whitespace the block parser trimmed off.
    source_column: usize,
}

/// Where each part of a reflowed string came from.
///
/// An *empty* map (`SourceMap::none()`) resolves everything to `None`, which
/// is what every caller that has no source to point at uses — the legacy
/// `parse_inline_text` entry point and every inline unit test.
pub(crate) struct SourceMap {
    segments: Vec<Segment>,
}

impl SourceMap {
    /// A map that knows nothing, so every lookup yields `None`.
    pub(crate) const fn none() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// A map for text that is exactly one source line, starting at
    /// `source_line`/`source_column`.
    ///
    /// Used for headings and definition-list terms, which are single lines
    /// and so need no accumulation.
    pub(crate) fn single_line(text: &str, source_line: usize, source_column: usize) -> Self {
        Self {
            segments: vec![Segment {
                text_offset: 0,
                text: text.to_string(),
                source_line,
                source_column,
            }],
        }
    }

    /// Records that the text now at byte offset `text_offset` in the joined
    /// string is `text`, taken from `source_line` starting at
    /// `source_column`.
    pub(crate) fn push(
        &mut self,
        text_offset: usize,
        text: &str,
        source_line: usize,
        source_column: usize,
    ) {
        self.segments.push(Segment {
            text_offset,
            text: text.to_string(),
            source_line,
            source_column,
        });
    }

    /// The source position of byte offset `offset` in the joined text,
    /// resolved against `ctx`.
    ///
    /// An offset past the end of the last segment clamps to that segment's
    /// end rather than yielding `None`: the inline scan's *end* offsets are
    /// exclusive, so the last role on a line legitimately points one past its
    /// final character.
    pub(crate) fn position(&self, offset: usize, ctx: &ParseCtx<'_>) -> Option<SourcePoint> {
        let segment = self
            .segments
            .iter()
            .rev()
            .find(|segment| segment.text_offset <= offset)?;
        let within = offset - segment.text_offset;
        // Characters, not bytes — and clamped, since `within` may land past
        // this segment's text or inside a multi-byte character.
        let characters = segment
            .text
            .char_indices()
            .take_while(|(index, _)| *index < within)
            .count();
        ctx.position(segment.source_line, segment.source_column + characters)
    }

    /// The span from byte offset `start` to `end` in the joined text.
    pub(crate) fn span(&self, start: usize, end: usize, ctx: &ParseCtx<'_>) -> Option<Span> {
        Some(self.position(start, ctx)?.to(self.position(end, ctx)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Domain, Position};

    fn ctx() -> ParseCtx<'static> {
        ParseCtx::with_domain(Domain::Py)
    }

    #[test]
    fn test_none_resolves_nothing() {
        // Given a map with no segments
        let map = SourceMap::none();

        // When / Then — nothing to point at, so no position is invented
        assert_eq!(map.position(0, &ctx()), None);
        assert_eq!(map.span(0, 3, &ctx()), None);
    }

    #[test]
    fn test_single_line_maps_an_offset_to_its_column() {
        // Given a one-line map for text indented four columns on line 3
        // (0-based), i.e. document line 4
        let map = SourceMap::single_line("hello world", 3, 4);

        // When
        let position = map.position(6, &ctx()).expect("within the segment");

        // Then — 1-based line 4, and column 1 + 4 stripped + 6 into the text
        assert_eq!(position.position, Position::new(4, 11));
    }

    #[test]
    fn test_span_covers_both_offsets() {
        // Given
        let map = SourceMap::single_line("see :ref:`x` here", 0, 0);

        // When — the extent of the role
        let span = map.span(4, 12, &ctx()).expect("within the segment");

        // Then
        assert_eq!(span.start, Position::new(1, 5));
        assert_eq!(span.end, Position::new(1, 13));
    }

    #[test]
    fn test_offset_resolves_against_the_segment_it_falls_in() {
        // Given two lines joined by a newline: "first\nsecond", where the
        // second line was indented three columns
        let mut map = SourceMap::none();
        map.push(0, "first", 0, 0);
        map.push(6, "second", 1, 3);

        // When an offset in each half is resolved
        let in_first = map.position(2, &ctx()).expect("in the first segment");
        let in_second = map.position(8, &ctx()).expect("in the second segment");

        // Then each lands on its own source line, with the indent restored
        assert_eq!(in_first.position, Position::new(1, 3));
        assert_eq!(in_second.position, Position::new(2, 6));
    }

    #[test]
    fn test_a_span_may_cross_the_line_join() {
        // Given a paragraph whose role is split across the reflowed join
        let mut map = SourceMap::none();
        map.push(0, "see :ref:`long", 0, 0);
        map.push(15, "target`", 1, 0);

        // When the role's whole extent is resolved
        let span = map.span(4, 22, &ctx()).expect("both ends are mapped");

        // Then start and end sit on different lines, which is the truth
        assert_eq!(span.start.line, 1);
        assert_eq!(span.end.line, 2);
    }

    #[test]
    fn test_columns_count_characters_not_bytes() {
        // Given a line whose prefix is multi-byte in UTF-8
        let map = SourceMap::single_line("ππ :ref:`x`", 0, 0);

        // When resolving the offset of the role, which is 4 *bytes* in
        let position = map.position(5, &ctx()).expect("within the segment");

        // Then it reports column 4 — the third character — not column 6
        assert_eq!(position.position, Position::new(1, 4));
    }

    #[test]
    fn test_an_offset_past_the_end_clamps_to_the_last_segment() {
        // Given a map whose text ends at offset 5
        let map = SourceMap::single_line("hello", 0, 0);

        // When an exclusive end offset one past the text is resolved
        let position = map.position(5, &ctx()).expect("clamped, not dropped");

        // Then — the position just past the last character
        assert_eq!(position.position, Position::new(1, 6));
    }

    #[test]
    fn test_a_nested_context_shifts_every_position() {
        // Given a map for a list item's body, whose context is rebased two
        // lines down and three columns in
        let map = SourceMap::single_line("text", 0, 0);
        let base = ctx();
        let nested = base.nested(2, 3);

        // When
        let position = map.position(0, &nested).expect("within the segment");

        // Then the offsets compose with the map's own
        assert_eq!(position.position, Position::new(3, 4));
    }
}

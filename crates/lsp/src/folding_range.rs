//! What of an open document folds, as `textDocument/foldingRange` answers
//! it: each section from its title to its last line, each directive, comment,
//! literal block and table over its own lines.
//!
//! Read from the blocks the parser recorded in the document's latest parse
//! (`rinx_ast::SyntaxTable`), so a block folds exactly as far as the parser
//! read it. Only the document's own lines fold: a block an `.. include::`
//! brought in lies in another file, and the include's own directive folds
//! nothing of it.

use lsp_types::{FoldingRange, FoldingRangeKind};
use rinx_ast::{BlockKind, Document, SyntaxBlock};

/// The folding ranges of `document`, whose text is `text`, at most `limit`
/// of them when the client sets one.
#[must_use]
pub fn folding_ranges(document: &Document, text: &str, limit: Option<usize>) -> Vec<FoldingRange> {
    let lines: Vec<&str> = text.lines().collect();
    let own: Vec<&SyntaxBlock> = document
        .syntax
        .blocks
        .iter()
        .filter(|block| block.lines.file.is_none())
        .collect();
    let mut ranges = section_ranges(&own, &lines);
    ranges.extend(own.iter().filter_map(|block| block_range(block)));
    ranges.retain(|range| range.end_line > range.start_line);
    ranges.sort_by_key(|range| (range.start_line, std::cmp::Reverse(range.end_line)));
    ranges.dedup_by_key(|range| (range.start_line, range.end_line));
    if let Some(limit) = limit {
        keep_longest(&mut ranges, limit);
    }
    ranges
}

/// Each section's range: from its title line to the last written line before
/// the next heading at its level or above, or before the document's end.
fn section_ranges(blocks: &[&SyntaxBlock], lines: &[&str]) -> Vec<FoldingRange> {
    let headings: Vec<(u8, u32, u32)> = blocks
        .iter()
        .filter_map(|block| match block.kind {
            // 0-based: where the heading's lines start, and its title line,
            // the one above its underline.
            BlockKind::Heading { level } => Some((
                level,
                block.lines.start.line.saturating_sub(1),
                block.lines.end.line.saturating_sub(2),
            )),
            _ => None,
        })
        .collect();
    let document_end = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    headings
        .iter()
        .enumerate()
        .map(|(at, &(level, _, title))| {
            let next = headings[at + 1..]
                .iter()
                .find(|(other, _, _)| *other <= level)
                .map_or(document_end, |&(_, start, _)| start);
            line_range(title, last_written_line(lines, title, next), None)
        })
        .collect()
}

/// The 0-based index of the last non-blank line from `first` up to, not
/// including, `end` — `first` itself when every later one is blank.
fn last_written_line(lines: &[&str], first: u32, end: u32) -> u32 {
    (first..end)
        .rev()
        .find(|at| {
            lines
                .get(*at as usize)
                .is_some_and(|line| !line.trim().is_empty())
        })
        .unwrap_or(first)
}

/// The range a block other than a heading folds over.
fn block_range(block: &SyntaxBlock) -> Option<FoldingRange> {
    let kind = match block.kind {
        BlockKind::Heading { .. } => return None,
        BlockKind::Comment => Some(FoldingRangeKind::Comment),
        BlockKind::Directive | BlockKind::LiteralBlock | BlockKind::Table => None,
    };
    Some(line_range(
        block.lines.start.line.saturating_sub(1),
        block.lines.end.line.saturating_sub(1),
        kind,
    ))
}

/// A range folding whole lines, `start` through `end`, both 0-based.
fn line_range(start: u32, end: u32, kind: Option<FoldingRangeKind>) -> FoldingRange {
    FoldingRange {
        start_line: start,
        start_character: None,
        end_line: end,
        end_character: None,
        kind,
        collapsed_text: None,
    }
}

/// Keeps the `limit` longest of `ranges`, in their order — those enclosing
/// the most, which a client with a limit loses last.
fn keep_longest(ranges: &mut Vec<FoldingRange>, limit: usize) {
    if ranges.len() <= limit {
        return;
    }
    let mut by_length: Vec<usize> = (0..ranges.len()).collect();
    by_length.sort_by_key(|at| {
        let range = &ranges[*at];
        (std::cmp::Reverse(range.end_line - range.start_line), *at)
    });
    let mut kept: Vec<usize> = by_length.into_iter().take(limit).collect();
    kept.sort_unstable();
    *ranges = kept.into_iter().map(|at| ranges[at].clone()).collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{FileId, Position, Span};

    /// The (start, end, kind) of each range folding `text`.
    fn folds(text: &str) -> Vec<(u32, u32, Option<FoldingRangeKind>)> {
        let document = rinx_parser::parse("guide.rst", text);
        folding_ranges(&document, text, None)
            .into_iter()
            .map(|range| (range.start_line, range.end_line, range.kind))
            .collect()
    }

    #[test]
    fn test_a_section_folds_from_its_title_to_its_last_written_line() {
        // Given a title with an overline, then a sibling section in the same
        // style
        let text = "=====\nGuide\n=====\n\nIntro.\n\nMore.\n\n====\nNext\n====\n\nEnd.\n";

        // When
        let found = folds(text);

        // Then — the first section from its title, not its overline
        assert_eq!(found, vec![(1, 6, None), (9, 12, None)]);
    }

    #[test]
    fn test_a_subsection_closes_at_its_parents_sibling() {
        // Given
        let text = "Top\n===\n\nSub\n---\n\nText.\n\nTop two\n=======\n\nText.\n";

        // When
        let found = folds(text);

        // Then — the first section holds its subsection, both closing before
        // the second top-level title
        assert_eq!(found, vec![(0, 6, None), (3, 6, None), (8, 11, None)]);
    }

    #[test]
    fn test_directives_comments_literal_blocks_and_tables_fold() {
        // Given
        let text = ".. note::\n\n   Text.\n\n.. A comment\n   continued.\n\n\
                    Code::\n\n   x = 1\n\n===  ===\na    b\n===  ===\n";

        // When
        let found = folds(text);

        // Then
        assert_eq!(
            found,
            vec![
                (0, 2, None),
                (4, 5, Some(FoldingRangeKind::Comment)),
                (7, 9, None),
                (11, 13, None),
            ]
        );
    }

    #[test]
    fn test_a_one_line_block_does_not_fold() {
        // When / Then
        assert_eq!(folds(".. image:: logo.png\n\nText.\n"), Vec::new());
    }

    #[test]
    fn test_a_block_in_another_file_does_not_fold() {
        // Given a document whose only block lies in an included fragment
        let mut document = rinx_parser::parse("guide.rst", "Text.\n");
        document.syntax.blocks.push(SyntaxBlock {
            kind: BlockKind::Directive,
            lines: Span::new(Position::new(1, 1), Position::new(5, 1))
                .with_file(Some(FileId::new(0))),
        });

        // When
        let found = folding_ranges(&document, "Text.\n", None);

        // Then
        assert_eq!(found, Vec::new());
    }

    #[test]
    fn test_a_limit_keeps_the_longest_ranges_in_order() {
        // Given
        let mut ranges = vec![
            line_range(0, 9, None),
            line_range(2, 3, None),
            line_range(5, 8, None),
        ];

        // When
        keep_longest(&mut ranges, 2);

        // Then
        let kept: Vec<(u32, u32)> = ranges.iter().map(|r| (r.start_line, r.end_line)).collect();
        assert_eq!(kept, vec![(0, 9), (5, 8)]);
    }

    #[test]
    fn test_last_written_line_skips_trailing_blank_lines() {
        // Given
        let lines = ["Title", "=====", "", "Text.", "", ""];

        // When / Then
        assert_eq!(last_written_line(&lines, 0, 6), 3);
        assert_eq!(last_written_line(&lines, 4, 6), 4);
    }

    #[test]
    fn test_block_range_folds_a_comment_as_a_comment() {
        // Given
        let block = SyntaxBlock {
            kind: BlockKind::Comment,
            lines: Span::new(Position::new(2, 1), Position::new(4, 3)),
        };

        // When
        let range = block_range(&block);

        // Then
        assert_eq!(
            range,
            Some(line_range(1, 3, Some(FoldingRangeKind::Comment)))
        );
    }
}

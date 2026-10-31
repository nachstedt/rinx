//! Collects a document's [`SourceMap`] while it is parsed: its
//! [`SyntaxTable`], and the characters of each name it was written with.
//!
//! Reached from all three parsing trees — `blocks/` records comments, literal
//! blocks, tables and headings, `directives/` each directive's block, name
//! and options, `inline/` each role, reference and substitution — so it sits
//! flat here. Like [`crate::document_roles::DocumentRoles`] it is state the
//! context borrows, filled in through a shared reference, because the
//! context is passed by shared reference everywhere it is read.

use std::cell::RefCell;

use rinx_ast::{
    BlockId, BlockKind, NameId, SourceMap, Span, SyntaxBlock, SyntaxToken, TokenKind,
    TokenModifiers,
};

/// One document's source map, as it fills.
#[derive(Debug, Default)]
pub(crate) struct SyntaxRecorder {
    map: RefCell<SourceMap>,
}

impl SyntaxRecorder {
    /// Records a block of `kind` written over `lines`.
    pub(crate) fn block(&self, kind: BlockKind, lines: Span) {
        self.map
            .borrow_mut()
            .syntax
            .blocks
            .push(SyntaxBlock { kind, lines });
    }

    /// Records a token of `kind`, with `modifiers`, written at `span`.
    pub(crate) fn token(&self, kind: TokenKind, modifiers: TokenModifiers, span: Span) {
        self.map.borrow_mut().syntax.tokens.push(SyntaxToken {
            kind,
            modifiers,
            span,
        });
    }

    /// Records a name written as exactly the characters of `span`, yielding
    /// the id its value is to carry.
    pub(crate) fn name(&self, span: Span) -> NameId {
        self.map.borrow_mut().record_name(span)
    }

    /// Records a block written over `lines`, yielding the id its node is to
    /// carry.
    pub(crate) fn block_lines(&self, lines: Span) -> BlockId {
        self.map.borrow_mut().record_block_lines(lines)
    }

    /// What was recorded.
    pub(crate) fn into_source_map(self) -> SourceMap {
        self.map.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Position, SyntaxTable, TokenModifier};

    #[test]
    fn test_recorder_hands_each_name_the_id_its_span_is_found_by() {
        // Given
        let recorder = SyntaxRecorder::default();

        // When
        let first = recorder.name(span(2));
        let second = recorder.name(span(7));

        // Then
        let map = recorder.into_source_map();
        assert_eq!(map.name_span(first), Some(span(2)));
        assert_eq!(map.name_span(second), Some(span(7)));
    }

    #[test]
    fn test_recorder_hands_each_block_the_id_its_lines_are_found_by() {
        // Given
        let recorder = SyntaxRecorder::default();

        // When
        let first = recorder.block_lines(span(1));
        let second = recorder.block_lines(span(4));

        // Then
        let map = recorder.into_source_map();
        assert_eq!(map.block_lines(first), Some(span(1)));
        assert_eq!(map.block_lines(second), Some(span(4)));
    }

    fn span(line: u32) -> Span {
        Span::new(Position::new(line, 1), Position::new(line, 5))
    }

    #[test]
    fn test_recorder_keeps_blocks_and_tokens_in_the_order_recorded() {
        // Given
        let recorder = SyntaxRecorder::default();

        // When
        recorder.block(BlockKind::Comment, span(2));
        recorder.token(TokenKind::Role, TokenModifiers::NONE, span(3));
        recorder.block(BlockKind::Table, span(1));
        let declared = TokenModifiers::NONE.with(TokenModifier::Declaration);
        recorder.token(TokenKind::Reference, declared, span(1));

        // Then
        let table = recorder.into_source_map().syntax;
        assert_eq!(
            table.blocks,
            vec![
                SyntaxBlock {
                    kind: BlockKind::Comment,
                    lines: span(2)
                },
                SyntaxBlock {
                    kind: BlockKind::Table,
                    lines: span(1)
                },
            ]
        );
        assert_eq!(
            table.tokens,
            vec![
                SyntaxToken {
                    kind: TokenKind::Role,
                    modifiers: TokenModifiers::NONE,
                    span: span(3)
                },
                SyntaxToken {
                    kind: TokenKind::Reference,
                    modifiers: declared,
                    span: span(1)
                },
            ]
        );
    }

    /// The blocks parsing `text` records, as (kind, first line, last line).
    fn blocks(text: &str) -> Vec<(BlockKind, u32, u32)> {
        crate::blocks::parse_source_map("test.rst", text)
            .syntax
            .blocks
            .iter()
            .map(|block| (block.kind, block.lines.start.line, block.lines.end.line))
            .collect()
    }

    #[test]
    fn test_parse_records_a_heading_with_its_overline() {
        // When
        let found = blocks("=====\nTitle\n=====\n\nText.\n");

        // Then
        assert_eq!(found, vec![(BlockKind::Heading { level: 1 }, 1, 3)]);
    }

    #[test]
    fn test_parse_records_a_comment_to_its_last_written_line() {
        // When
        let found = blocks(".. A comment\n   continued.\n\nText.\n");

        // Then
        assert_eq!(found, vec![(BlockKind::Comment, 1, 2)]);
    }

    #[test]
    fn test_parse_records_a_literal_block_from_its_introducing_line() {
        // When
        let found = blocks("Code::\n\n   one\n   two\n\nText.\n");

        // Then
        assert_eq!(found, vec![(BlockKind::LiteralBlock, 1, 4)]);
    }

    #[test]
    fn test_parse_records_a_doctest_block_as_a_literal_block() {
        // When
        let found = blocks(">>> 1 + 1\n2\n");

        // Then
        assert_eq!(found, vec![(BlockKind::LiteralBlock, 1, 2)]);
    }

    #[test]
    fn test_parse_records_a_table() {
        // When
        let found = blocks("===  ===\na    b\n===  ===\n");

        // Then
        assert_eq!(found, vec![(BlockKind::Table, 1, 3)]);
    }

    #[test]
    fn test_parse_records_a_nested_directive_at_its_own_lines() {
        // When
        let found = blocks(".. note::\n\n   .. warning::\n\n      Text.\n");

        // Then
        assert_eq!(
            found,
            vec![(BlockKind::Directive, 1, 5), (BlockKind::Directive, 3, 5)]
        );
    }

    #[test]
    fn test_parse_records_a_directives_name_and_option_names() {
        // When
        let map =
            crate::blocks::parse_source_map("test.rst", ".. image:: logo.png\n   :alt: Logo\n");

        // Then
        let found: Vec<(TokenKind, u32, u32, u32)> = map
            .syntax
            .tokens
            .iter()
            .map(|token| {
                (
                    token.kind,
                    token.span.start.line,
                    token.span.start.column,
                    token.span.end.column,
                )
            })
            .collect();
        assert_eq!(
            found,
            vec![
                (TokenKind::Directive, 1, 4, 11),
                (TokenKind::Option, 2, 4, 9)
            ]
        );
    }

    #[test]
    fn test_parse_records_an_unknown_directive() {
        // When
        let found = blocks(".. made-up::\n\n   Text.\n");

        // Then
        assert_eq!(found, vec![(BlockKind::Directive, 1, 3)]);
    }

    #[test]
    fn test_a_new_recorder_holds_nothing() {
        // Given / When
        let table = SyntaxRecorder::default().into_source_map().syntax;

        // Then
        assert_eq!(table, SyntaxTable::new());
    }
}

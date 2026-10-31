//! Collects a document's [`SyntaxTable`] while it is parsed.
//!
//! Reached from all three parsing trees — `blocks/` records comments, literal
//! blocks, tables and headings, `directives/` each directive's block, name
//! and options, `inline/` each role, reference and substitution — so it sits
//! flat here. Like [`crate::document_roles::DocumentRoles`] it is state the
//! context borrows, filled in through a shared reference, because the
//! context is passed by shared reference everywhere it is read.

use std::cell::RefCell;

use rinx_ast::{BlockKind, Span, SyntaxBlock, SyntaxTable, SyntaxToken, TokenKind, TokenModifiers};

/// One document's syntax table, as it fills.
#[derive(Debug, Default)]
pub(crate) struct SyntaxRecorder {
    table: RefCell<SyntaxTable>,
}

impl SyntaxRecorder {
    /// Records a block of `kind` written over `lines`.
    pub(crate) fn block(&self, kind: BlockKind, lines: Span) {
        self.table
            .borrow_mut()
            .blocks
            .push(SyntaxBlock { kind, lines });
    }

    /// Records a token of `kind`, with `modifiers`, written at `span`.
    pub(crate) fn token(&self, kind: TokenKind, modifiers: TokenModifiers, span: Span) {
        self.table.borrow_mut().tokens.push(SyntaxToken {
            kind,
            modifiers,
            span,
        });
    }

    /// What was recorded.
    pub(crate) fn into_table(self) -> SyntaxTable {
        self.table.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Position, TokenModifier};

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
        let table = recorder.into_table();
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
        crate::parse("test.rst", text)
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
        let document = crate::parse("test.rst", ".. image:: logo.png\n   :alt: Logo\n");

        // Then
        let found: Vec<(TokenKind, u32, u32, u32)> = document
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
        let table = SyntaxRecorder::default().into_table();

        // Then
        assert_eq!(table, SyntaxTable::new());
    }
}

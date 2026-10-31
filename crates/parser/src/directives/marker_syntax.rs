//! Recording a directive's marker and block for an editor: the block it
//! folds and the name it colours — see [`rinx_ast::SyntaxTable`].
//!
//! Recorded once, where a marker is recognized, so a directive the build
//! does not know folds and is coloured like any other.

use crate::context::ParseCtx;
use rinx_ast::{BlockKind, TokenKind, TokenModifier, TokenModifiers};

use super::body::DirectiveBody;
use super::substitution::split_substitution_marker;

/// Records the directive whose marker is `line`, at index `index` of the
/// slice `ctx` positions, with `body` below it.
pub(in crate::directives) fn record_marker_syntax(
    line: &str,
    index: usize,
    body: &DirectiveBody<'_>,
    ctx: &ParseCtx<'_>,
) {
    let last = match body.lines.last() {
        Some(text) => (index + body.first_line_offset + body.lines.len(), *text),
        None => (index, line),
    };
    ctx.record_block(BlockKind::Directive, ctx.lines_span(index, last.0, last.1));
    let Some(name) = marker_name(line) else {
        return;
    };
    let token = |kind, modifiers, start: usize, end: usize| {
        let span = ctx
            .position(index, start)
            .zip(ctx.position(index, end))
            .map(|(start, end)| start.to(end));
        ctx.record_token(kind, modifiers, span);
    };
    let declared = TokenModifiers::NONE.with(TokenModifier::Declaration);
    match substitution_split(name.text) {
        Some(inner_offset) => {
            // `|name|` is the substitution defined, the rest the directive
            // defining it.
            let bars = name.text[..inner_offset].trim_end().chars().count();
            token(
                TokenKind::Substitution,
                declared,
                name.column,
                name.column + bars,
            );
            let inner = name.column + name.text[..inner_offset].chars().count();
            token(TokenKind::Directive, TokenModifiers::NONE, inner, name.end);
        }
        None => token(
            TokenKind::Directive,
            TokenModifiers::NONE,
            name.column,
            name.end,
        ),
    }
}

/// A marker's name, `::` included, and where it lies on its line.
#[derive(Debug, PartialEq, Eq)]
struct MarkerName<'t> {
    /// The name as written, without its `::`.
    text: &'t str,
    /// The 0-based character column the name starts at.
    column: usize,
    /// The 0-based character column just past its `::`.
    end: usize,
}

/// The name a marker line writes between `.. ` and `::`, or `None` for a
/// line that writes none.
fn marker_name(line: &str) -> Option<MarkerName<'_>> {
    let dots = line.find(".. ")?;
    let after_dots = &line[dots + 3..];
    let name_start = dots + 3 + (after_dots.len() - after_dots.trim_start().len());
    let colons = name_start + line[name_start..].find("::")?;
    let text = line[name_start..colons].trim_end();
    if text.is_empty() {
        return None;
    }
    Some(MarkerName {
        text,
        column: line[..name_start].chars().count(),
        end: line[..colons + 2].chars().count(),
    })
}

/// Where the directive's name starts within a substitution definition's
/// `|name| directive`, or `None` when `name` is no such marker.
fn substitution_split(name: &str) -> Option<usize> {
    let (_, inner) = split_substitution_marker(name)?;
    Some(name.len() - inner.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax_recorder::SyntaxRecorder;
    use rinx_ast::{Domain, Position, Span, SyntaxBlock, SyntaxToken};

    fn span(line: u32, start: u32, end: u32) -> Span {
        Span::new(Position::new(line, start), Position::new(line, end))
    }

    fn recorded(lines: &[&str]) -> rinx_ast::SyntaxTable {
        let base = ParseCtx::with_domain(Domain::Py);
        let recorder = SyntaxRecorder::default();
        let ctx = base.with_syntax_recorder(&recorder);
        let body = super::super::body::collect_directive_body(lines, 1, 0);
        record_marker_syntax(lines[0], 0, &body, &ctx);
        recorder.into_table()
    }

    #[test]
    fn test_marker_name_finds_the_name_between_the_dots_and_the_colons() {
        // Given / When
        let name = marker_name("  ..  note:: Hello");

        // Then
        assert_eq!(
            name,
            Some(MarkerName {
                text: "note",
                column: 6,
                end: 12
            })
        );
    }

    #[test]
    fn test_marker_name_is_none_without_a_name() {
        // Given / When / Then
        assert_eq!(marker_name(".. ::"), None);
        assert_eq!(marker_name("plain text"), None);
    }

    #[test]
    fn test_substitution_split_finds_the_inner_directive() {
        // Given / When / Then
        assert_eq!(substitution_split("|name| replace"), Some(7));
        assert_eq!(substitution_split("note"), None);
    }

    #[test]
    fn test_record_marker_syntax_records_the_block_to_its_last_written_line() {
        // Given a directive whose body ends before a trailing blank line
        let lines = [".. note::", "", "   First.", "   Last.", "", "After."];

        // When
        let table = recorded(&lines);

        // Then
        assert_eq!(
            table.blocks,
            vec![SyntaxBlock {
                kind: BlockKind::Directive,
                lines: Span::new(Position::new(1, 1), Position::new(4, 9)),
            }]
        );
        assert_eq!(
            table.tokens,
            vec![SyntaxToken {
                kind: TokenKind::Directive,
                modifiers: TokenModifiers::NONE,
                span: span(1, 4, 10),
            }]
        );
    }

    #[test]
    fn test_record_marker_syntax_records_a_bodiless_directive_on_its_line() {
        // Given
        let lines = [".. image:: logo.png", "After."];

        // When
        let table = recorded(&lines);

        // Then
        assert_eq!(table.blocks[0].lines, span(1, 1, 20));
    }

    #[test]
    fn test_record_marker_syntax_splits_a_substitution_definition() {
        // Given
        let lines = [".. |name| replace:: text"];

        // When
        let table = recorded(&lines);

        // Then
        assert_eq!(
            table.tokens,
            vec![
                SyntaxToken {
                    kind: TokenKind::Substitution,
                    modifiers: TokenModifiers::NONE.with(TokenModifier::Declaration),
                    span: span(1, 4, 10),
                },
                SyntaxToken {
                    kind: TokenKind::Directive,
                    modifiers: TokenModifiers::NONE,
                    span: span(1, 11, 20),
                },
            ]
        );
    }
}

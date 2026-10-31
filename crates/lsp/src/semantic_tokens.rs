//! The names an open document's markup writes, coloured by kind, as
//! `textDocument/semanticTokens/full` and `…/range` answer them: each
//! directive's name, option name, role name, what a reference leads to, and
//! each substitution.
//!
//! Read from the tokens the parser recorded in the document's latest parse
//! (`rinx_ast::SyntaxTable`), so a name is coloured exactly when the parser
//! read it as one. Only the document's own text is coloured: a token an
//! `.. include::` brought in lies in another file.
//!
//! A token running over several lines is sent as one per line, since not
//! every client takes `multilineTokenSupport`, and two tokens claiming the
//! same characters — which a Jinja pass can make, mapping two rendered lines
//! onto one written — keep the first.

use lsp_types::{
    Position as LspPosition, Range, SemanticToken, SemanticTokenModifier, SemanticTokenType,
    SemanticTokens, SemanticTokensLegend,
};
use rinx_ast::{Position, SyntaxTable, TokenKind, TokenModifier};

use crate::position::{PositionEncoding, to_lsp_position};

/// The protocol's name for a token of `kind`.
const fn token_type_name(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Directive => "directive",
        TokenKind::Option => "option",
        TokenKind::Role => "role",
        TokenKind::Reference => "reference",
        TokenKind::Substitution => "substitution",
    }
}

/// The protocol's name for `modifier`.
const fn token_modifier_name(modifier: TokenModifier) -> &'static str {
    match modifier {
        TokenModifier::Declaration => "declaration",
    }
}

/// The legend every token this server sends is read by: types in
/// [`TokenKind::ALL`]'s order, modifiers in [`TokenModifier::ALL`]'s.
#[must_use]
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TokenKind::ALL
            .map(|kind| SemanticTokenType::new(token_type_name(kind)))
            .to_vec(),
        token_modifiers: TokenModifier::ALL
            .map(|modifier| SemanticTokenModifier::new(token_modifier_name(modifier)))
            .to_vec(),
    }
}

/// The tokens of a document whose text is `text` and whose parse recorded
/// `syntax`, in `encoding` — those starting within `range` when one is asked
/// for.
#[must_use]
pub fn semantic_tokens(
    syntax: &SyntaxTable,
    text: &str,
    encoding: PositionEncoding,
    range: Option<Range>,
) -> SemanticTokens {
    let lines: Vec<&str> = text.lines().collect();
    let mut pieces: Vec<Piece> = syntax
        .tokens
        .iter()
        .filter(|token| token.span.file.is_none())
        .flat_map(|token| {
            line_pieces(token.span.start, token.span.end, &lines, encoding).map(move |piece| {
                Piece {
                    kind: kind_index(token.kind),
                    modifiers: token.modifiers.bits(),
                    ..piece
                }
            })
        })
        .filter(|piece| range.is_none_or(|range| within(piece.start, range)))
        .collect();
    pieces.sort_by_key(|piece| (piece.start.line, piece.start.character));
    SemanticTokens {
        result_id: None,
        data: encode(&without_overlaps(pieces)),
    }
}

/// One token's part on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Piece {
    start: LspPosition,
    length: u32,
    kind: u32,
    modifiers: u32,
}

/// The parts of the token from `start` to `end` on each line it touches, of
/// kind 0 and no modifiers — a part of no length, or on no line of the text,
/// left out.
fn line_pieces<'a>(
    start: Position,
    end: Position,
    lines: &'a [&'a str],
    encoding: PositionEncoding,
) -> impl Iterator<Item = Piece> + 'a {
    (start.line..=end.line).filter_map(move |line| {
        let text = lines.get(line.checked_sub(1)? as usize)?;
        let from = if line == start.line { start.column } else { 1 };
        let to = if line == end.line {
            end.column
        } else {
            u32::try_from(text.chars().count() + 1).unwrap_or(u32::MAX)
        };
        let from = to_lsp_position(Position::new(line, from), text, encoding);
        let to = to_lsp_position(Position::new(line, to), text, encoding);
        let length = to.character.checked_sub(from.character)?;
        (length > 0).then_some(Piece {
            start: from,
            length,
            kind: 0,
            modifiers: 0,
        })
    })
}

/// `kind`'s index in the legend.
fn kind_index(kind: TokenKind) -> u32 {
    let at = TokenKind::ALL
        .iter()
        .position(|listed| *listed == kind)
        .unwrap_or_default();
    u32::try_from(at).unwrap_or_default()
}

/// Whether `position` lies in `range`, its end excluded.
fn within(position: LspPosition, range: Range) -> bool {
    range.start <= position && position < range.end
}

/// `pieces`, sorted by where they start, without any that begins before the
/// one kept ahead of it ends.
fn without_overlaps(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut kept: Vec<Piece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let overlaps = kept.last().is_some_and(|last| {
            last.start.line == piece.start.line
                && piece.start.character < last.start.character + last.length
        });
        if !overlaps {
            kept.push(piece);
        }
    }
    kept
}

/// `pieces`, sorted and apart, in the protocol's relative encoding: each
/// token's line and start relative to the one before it.
fn encode(pieces: &[Piece]) -> Vec<SemanticToken> {
    let mut previous = LspPosition::new(0, 0);
    pieces
        .iter()
        .map(|piece| {
            let delta_line = piece.start.line - previous.line;
            let delta_start = if delta_line == 0 {
                piece.start.character - previous.character
            } else {
                piece.start.character
            };
            previous = piece.start;
            SemanticToken {
                delta_line,
                delta_start,
                length: piece.length,
                token_type: piece.kind,
                token_modifiers_bitset: piece.modifiers,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Domain, FileId, Span, SyntaxToken, TokenModifiers};
    use rinx_parser::ParseCtx;

    /// The syntax table parsing `text` records.
    fn syntax_of(text: &str) -> SyntaxTable {
        rinx_parser::parse("guide.rst", text, &ParseCtx::with_domain(Domain::Py))
            .source_map
            .syntax
    }

    /// The (line, start, length, type name, modifiers) of each token of
    /// `text`, decoded back from the relative encoding.
    fn decoded(text: &str, encoding: PositionEncoding) -> Vec<(u32, u32, u32, String, u32)> {
        decode(&semantic_tokens(&syntax_of(text), text, encoding, None))
    }

    fn decode(tokens: &SemanticTokens) -> Vec<(u32, u32, u32, String, u32)> {
        let legend = legend();
        let (mut line, mut start) = (0, 0);
        tokens
            .data
            .iter()
            .map(|token| {
                if token.delta_line > 0 {
                    start = 0;
                }
                line += token.delta_line;
                start += token.delta_start;
                (
                    line,
                    start,
                    token.length,
                    legend.token_types[token.token_type as usize]
                        .as_str()
                        .to_string(),
                    token.token_modifiers_bitset,
                )
            })
            .collect()
    }

    fn token(kind: TokenKind, start: (u32, u32), end: (u32, u32)) -> SyntaxToken {
        SyntaxToken {
            kind,
            modifiers: TokenModifiers::NONE,
            span: Span::new(Position::new(start.0, start.1), Position::new(end.0, end.1)),
        }
    }

    #[test]
    fn test_legend_lists_every_kind_and_modifier_in_order() {
        // When
        let legend = legend();

        // Then
        let types: Vec<&str> = legend
            .token_types
            .iter()
            .map(SemanticTokenType::as_str)
            .collect();
        assert_eq!(
            types,
            ["directive", "option", "role", "reference", "substitution"]
        );
        let modifiers: Vec<&str> = legend
            .token_modifiers
            .iter()
            .map(SemanticTokenModifier::as_str)
            .collect();
        assert_eq!(modifiers, ["declaration"]);
        assert_eq!(kind_index(TokenKind::Substitution), 4);
    }

    #[test]
    fn test_every_kind_of_token_is_coloured_in_source_order() {
        // Given
        let text = ".. _install:\n\n.. image:: logo.png\n   :alt: |name|\n\n\
                    See :ref:`install`.\n\n.. |name| replace:: text\n";

        // When
        let found = decoded(text, PositionEncoding::Utf16);

        // Then
        let names: Vec<(u32, u32, u32, &str, u32)> = found
            .iter()
            .map(|(line, start, length, kind, modifiers)| {
                (*line, *start, *length, kind.as_str(), *modifiers)
            })
            .collect();
        assert_eq!(
            names,
            vec![
                (0, 4, 7, "reference", 1),
                (2, 3, 7, "directive", 0),
                (3, 3, 5, "option", 0),
                (5, 4, 5, "role", 0),
                (5, 10, 7, "reference", 0),
                (7, 3, 6, "substitution", 1),
                (7, 10, 9, "directive", 0),
            ]
        );
    }

    #[test]
    fn test_columns_count_in_the_negotiated_encoding() {
        // Given a role after a character outside the Basic Multilingual Plane
        let text = "𝄞 :ref:`x`\n";

        // When
        let utf16 = decoded(text, PositionEncoding::Utf16);
        let utf32 = decoded(text, PositionEncoding::Utf32);

        // Then
        assert_eq!((utf16[0].1, utf16[0].2), (3, 5));
        assert_eq!((utf32[0].1, utf32[0].2), (2, 5));
    }

    #[test]
    fn test_a_token_over_several_lines_is_sent_once_per_line() {
        // Given
        let lines = ["see :ref:`a", "long label`"];

        // When
        let pieces: Vec<(u32, u32, u32)> = line_pieces(
            Position::new(1, 11),
            Position::new(2, 11),
            &lines,
            PositionEncoding::Utf16,
        )
        .map(|piece| (piece.start.line, piece.start.character, piece.length))
        .collect();

        // Then
        assert_eq!(pieces, vec![(0, 10, 1), (1, 0, 10)]);
    }

    #[test]
    fn test_a_token_in_another_file_or_past_the_text_is_left_out() {
        // Given
        let mut syntax = syntax_of("Text.\n");
        let mut included = token(TokenKind::Role, (1, 1), (1, 3));
        included.span = included.span.with_file(Some(FileId::new(0)));
        syntax.tokens = vec![included, token(TokenKind::Role, (9, 1), (9, 3))];

        // When
        let tokens = semantic_tokens(&syntax, "Text.\n", PositionEncoding::Utf16, None);

        // Then
        assert_eq!(tokens.data, Vec::new());
    }

    #[test]
    fn test_a_range_keeps_only_the_tokens_starting_in_it() {
        // Given
        let text = ":ref:`a`\n:ref:`b`\n";
        let syntax = syntax_of(text);
        let second_line = Range::new(LspPosition::new(1, 0), LspPosition::new(2, 0));

        // When
        let found = decode(&semantic_tokens(
            &syntax,
            text,
            PositionEncoding::Utf16,
            Some(second_line),
        ));

        // Then
        assert!(found.iter().all(|(line, ..)| *line == 1));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_without_overlaps_keeps_the_first_of_two_claiming_one_character() {
        // Given
        let piece = |character, length| Piece {
            start: LspPosition::new(0, character),
            length,
            kind: 0,
            modifiers: 0,
        };

        // When
        let kept = without_overlaps(vec![piece(0, 4), piece(2, 4), piece(4, 1)]);

        // Then
        assert_eq!(kept, vec![piece(0, 4), piece(4, 1)]);
    }

    #[test]
    fn test_encode_makes_each_position_relative_to_the_one_before() {
        // Given
        let piece = |line, character| Piece {
            start: LspPosition::new(line, character),
            length: 2,
            kind: 1,
            modifiers: 0,
        };

        // When
        let data = encode(&[piece(1, 4), piece(1, 9), piece(3, 2)]);

        // Then
        let deltas: Vec<(u32, u32)> = data.iter().map(|t| (t.delta_line, t.delta_start)).collect();
        assert_eq!(deltas, vec![(1, 4), (0, 5), (2, 2)]);
    }

    #[test]
    fn test_within_excludes_the_ranges_end() {
        // Given
        let range = Range::new(LspPosition::new(1, 0), LspPosition::new(2, 0));

        // When / Then
        assert!(within(LspPosition::new(1, 5), range));
        assert!(!within(LspPosition::new(2, 0), range));
        assert!(!within(LspPosition::new(0, 9), range));
    }
}

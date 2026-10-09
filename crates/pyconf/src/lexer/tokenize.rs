//! Splitting a module into tokens, with Python's logical lines and
//! indentation.
//!
//! A line ends a statement only outside brackets and without a trailing
//! backslash; blank and comment-only lines never count, and each change of
//! indentation between logical lines becomes an `Indent` or `Dedent`, so a
//! reader can tell which statements belong to a block.

use super::cursor::Cursor;
use super::string::{Prefix, is_identifier_char, is_identifier_start, lex_string};
use crate::position::{Position, Span};
use crate::token::{SyntaxError, SyntaxErrorKind, Token, TokenKind};

/// Every operator and delimiter, longest first so the longest match wins.
const OPERATORS: &[&str] = &[
    "**=", "//=", ">>=", "<<=", "...", "->", ":=", "**", "//", "<<", ">>", "<=", ">=", "==", "!=",
    "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "@=", "+", "-", "*", "/", "%", "@", "&", "|",
    "^", "~", "<", ">", "(", ")", "[", "]", "{", "}", ",", ":", ";", ".", "=", "!",
];

/// A module's tokens, up to the first syntax error if there is one.
#[derive(Debug, Clone, PartialEq)]
pub struct Tokenized {
    pub tokens: Vec<Token>,
    pub error: Option<SyntaxError>,
}

/// Splits `source` into tokens. Never panics: on input Python would refuse,
/// it returns the tokens before the problem and the problem.
#[must_use]
pub fn tokenize(source: &str) -> Tokenized {
    let mut lexer = Lexer {
        cursor: Cursor::new(source),
        tokens: Vec::new(),
        indents: vec![0],
        brackets: Vec::new(),
        line_has_tokens: false,
    };
    let error = lexer.run().err();
    Tokenized {
        tokens: lexer.tokens,
        error,
    }
}

struct Lexer {
    cursor: Cursor,
    tokens: Vec<Token>,
    /// The indentation of each enclosing block, outermost first.
    indents: Vec<usize>,
    /// The open brackets, with where each was opened.
    brackets: Vec<(char, Span)>,
    /// Whether the current logical line holds a token yet.
    line_has_tokens: bool,
}

impl Lexer {
    fn run(&mut self) -> Result<(), SyntaxError> {
        let mut at_line_start = true;
        loop {
            if at_line_start {
                if !self.read_indentation()? {
                    if self.cursor.peek().is_none() {
                        break;
                    }
                    // A blank line: the next one starts a line too.
                    continue;
                }
                at_line_start = false;
            }
            self.skip_whitespace();
            let start = self.cursor.position();
            let Some(c) = self.cursor.peek() else {
                break;
            };
            match c {
                '#' => self.cursor.skip_to_newline(),
                '\\' => {
                    self.cursor.bump();
                    if !self.cursor.bump_newline() {
                        return Err(self.error(SyntaxErrorKind::UnexpectedCharacter('\\'), start));
                    }
                }
                '\n' | '\r' => {
                    if self.brackets.is_empty() {
                        self.end_logical_line();
                        at_line_start = true;
                    }
                    self.cursor.bump_newline();
                }
                '\'' | '"' => self.lex_string(Prefix::NONE, start)?,
                c if is_identifier_start(c) => self.lex_name(start)?,
                c if c.is_ascii_digit() => self.lex_number(start),
                '.' if self
                    .cursor
                    .peek_at(1)
                    .is_some_and(|next| next.is_ascii_digit()) =>
                {
                    self.lex_number(start);
                }
                _ => self.lex_operator(c, start)?,
            }
        }
        if let Some(&(bracket, span)) = self.brackets.first() {
            return Err(SyntaxError {
                kind: SyntaxErrorKind::UnclosedBracket(bracket),
                span,
            });
        }
        self.end_logical_line();
        let end = self.cursor.span_from(self.cursor.position());
        while self.indents.len() > 1 {
            self.indents.pop();
            self.tokens.push(Token {
                kind: TokenKind::Dedent,
                span: end,
            });
        }
        Ok(())
    }

    /// Reads the indentation of a new physical line, emitting the indents
    /// and dedents it implies. Returns `false` for a blank or comment-only
    /// line, which has been consumed and implies nothing.
    fn read_indentation(&mut self) -> Result<bool, SyntaxError> {
        let start = self.cursor.position();
        let mut column = 0usize;
        loop {
            match self.cursor.peek() {
                Some(' ') => column += 1,
                // A tab moves to the next multiple of eight, as Python's
                // tokenizer counts it.
                Some('\t') => column = (column / 8 + 1) * 8,
                Some('\u{0c}') => column = 0,
                _ => break,
            }
            self.cursor.bump();
        }
        match self.cursor.peek() {
            None => return Ok(false),
            Some('#') => {
                self.cursor.skip_to_newline();
                self.cursor.bump_newline();
                return Ok(false);
            }
            Some('\n' | '\r') => {
                self.cursor.bump_newline();
                return Ok(false);
            }
            _ => {}
        }
        let span = self.cursor.span_from(start);
        let current = self.indents.last().copied().unwrap_or(0);
        if column > current {
            self.indents.push(column);
            self.tokens.push(Token {
                kind: TokenKind::Indent,
                span,
            });
        } else {
            while self.indents.last().is_some_and(|&indent| indent > column) {
                self.indents.pop();
                self.tokens.push(Token {
                    kind: TokenKind::Dedent,
                    span,
                });
            }
            if self.indents.last() != Some(&column) {
                return Err(SyntaxError {
                    kind: SyntaxErrorKind::InconsistentDedent,
                    span,
                });
            }
        }
        Ok(true)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.cursor.peek(), Some(' ' | '\t' | '\u{0c}')) {
            self.cursor.bump();
        }
    }

    /// Ends the logical line, if it held a token.
    fn end_logical_line(&mut self) {
        if self.line_has_tokens {
            let at = self.cursor.position();
            self.push(TokenKind::Newline, at);
            self.line_has_tokens = false;
        }
    }

    fn push(&mut self, kind: TokenKind, start: Position) {
        if !matches!(kind, TokenKind::Newline) {
            self.line_has_tokens = true;
        }
        let span = self.cursor.span_from(start);
        self.tokens.push(Token { kind, span });
    }

    fn error(&self, kind: SyntaxErrorKind, start: Position) -> SyntaxError {
        SyntaxError {
            kind,
            span: self.cursor.span_from(start),
        }
    }

    fn lex_string(&mut self, prefix: Prefix, start: Position) -> Result<(), SyntaxError> {
        let kind = lex_string(&mut self.cursor, prefix).map_err(|kind| self.error(kind, start))?;
        self.push(kind, start);
        Ok(())
    }

    /// A name, or the prefix of the string it turns out to start.
    fn lex_name(&mut self, start: Position) -> Result<(), SyntaxError> {
        let mut name = String::new();
        while let Some(c) = self.cursor.peek().filter(|&c| is_identifier_char(c)) {
            name.push(c);
            self.cursor.bump();
        }
        if let (Some('\'' | '"'), Some(prefix)) = (self.cursor.peek(), Prefix::parse(&name)) {
            return self.lex_string(prefix, start);
        }
        self.push(TokenKind::Name(name), start);
        Ok(())
    }

    /// A number in any of Python's spellings, kept as written: what it is
    /// worth is the literal reader's question.
    fn lex_number(&mut self, start: Position) {
        let mut text = String::new();
        let mut previous = '\0';
        while let Some(c) = self.cursor.peek() {
            let exponent_sign = matches!(c, '+' | '-')
                && matches!(previous, 'e' | 'E')
                && !text.starts_with("0x")
                && !text.starts_with("0X");
            if !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || exponent_sign) {
                break;
            }
            text.push(c);
            previous = c;
            self.cursor.bump();
        }
        self.push(TokenKind::Number(text), start);
    }

    fn lex_operator(&mut self, c: char, start: Position) -> Result<(), SyntaxError> {
        let Some(&operator) = OPERATORS.iter().find(|operator| {
            operator
                .chars()
                .enumerate()
                .all(|(offset, expected)| self.cursor.peek_at(offset) == Some(expected))
        }) else {
            self.cursor.bump();
            return Err(self.error(SyntaxErrorKind::UnexpectedCharacter(c), start));
        };
        for _ in operator.chars() {
            self.cursor.bump();
        }
        let span = self.cursor.span_from(start);
        match operator {
            "(" | "[" | "{" => self.brackets.push((c, span)),
            ")" | "]" | "}" => {
                let expected = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if self.brackets.pop().map(|(open, _)| open) != Some(expected) {
                    return Err(SyntaxError {
                        kind: SyntaxErrorKind::UnmatchedBracket(c),
                        span,
                    });
                }
            }
            _ => {}
        }
        self.push(TokenKind::Op(operator), start);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        let tokenized = tokenize(source);
        assert_eq!(tokenized.error, None, "{source:?}");
        tokenized
            .tokens
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    fn name(text: &str) -> TokenKind {
        TokenKind::Name(text.to_string())
    }

    fn number(text: &str) -> TokenKind {
        TokenKind::Number(text.to_string())
    }

    fn string(text: &str) -> TokenKind {
        TokenKind::Str(text.to_string())
    }

    use TokenKind::{Dedent, Indent, Newline, Op, Opaque};

    #[test]
    fn test_tokenize_reads_an_assignment() {
        // When / Then
        assert_eq!(
            kinds("root_doc = 'contents'\n"),
            [name("root_doc"), Op("="), string("contents"), Newline]
        );
    }

    #[test]
    fn test_tokenize_ends_the_last_line_without_a_line_ending() {
        // When / Then
        assert_eq!(kinds("x = 1"), [name("x"), Op("="), number("1"), Newline]);
    }

    #[test]
    fn test_tokenize_ignores_blank_and_comment_lines() {
        // When / Then
        assert_eq!(
            kinds("# header\n\n  # indented note\nx = 1  # trailing\n\n"),
            [name("x"), Op("="), number("1"), Newline]
        );
    }

    #[test]
    fn test_tokenize_joins_lines_inside_brackets() {
        // When / Then
        assert_eq!(
            kinds("x = [\n    'a',  # first\n\n    'b',\n]\n"),
            [
                name("x"),
                Op("="),
                Op("["),
                string("a"),
                Op(","),
                string("b"),
                Op(","),
                Op("]"),
                Newline
            ]
        );
    }

    #[test]
    fn test_tokenize_joins_lines_after_a_backslash() {
        // When / Then
        assert_eq!(
            kinds("x = 'a' \\\n    'b'\n"),
            [name("x"), Op("="), string("a"), string("b"), Newline]
        );
    }

    #[test]
    fn test_tokenize_marks_blocks_with_indents_and_dedents() {
        // Given
        let source = "if x:\n    a = 1\n    if y:\n\tb = 2\nc = 3\n";

        // When
        let kinds = kinds(source);

        // Then
        assert_eq!(
            kinds,
            [
                name("if"),
                name("x"),
                Op(":"),
                Newline,
                Indent,
                name("a"),
                Op("="),
                number("1"),
                Newline,
                name("if"),
                name("y"),
                Op(":"),
                Newline,
                Indent,
                name("b"),
                Op("="),
                number("2"),
                Newline,
                Dedent,
                Dedent,
                name("c"),
                Op("="),
                number("3"),
                Newline
            ]
        );
    }

    #[test]
    fn test_tokenize_dedents_after_a_blank_or_comment_line() {
        // When / Then
        assert_eq!(
            kinds("if x:\n    a\n\n  # note\nb\n"),
            [
                name("if"),
                name("x"),
                Op(":"),
                Newline,
                Indent,
                name("a"),
                Newline,
                Dedent,
                name("b"),
                Newline
            ]
        );
    }

    #[test]
    fn test_tokenize_closes_open_blocks_at_the_end() {
        // When / Then
        assert_eq!(
            kinds("if x:\n    a = 1"),
            [
                name("if"),
                name("x"),
                Op(":"),
                Newline,
                Indent,
                name("a"),
                Op("="),
                number("1"),
                Newline,
                Dedent
            ]
        );
    }

    #[test]
    fn test_tokenize_prefers_the_longest_operator() {
        // When / Then
        assert_eq!(
            kinds("a **= b // c := d -> e != f"),
            [
                name("a"),
                Op("**="),
                name("b"),
                Op("//"),
                name("c"),
                Op(":="),
                name("d"),
                Op("->"),
                name("e"),
                Op("!="),
                name("f"),
                Newline
            ]
        );
    }

    #[test]
    fn test_tokenize_reads_every_number_spelling() {
        // When / Then
        assert_eq!(
            kinds("1_000 0x_ff 0o17 0b10 1.5 .5 1e-3 2E+4 3j 1."),
            [
                number("1_000"),
                number("0x_ff"),
                number("0o17"),
                number("0b10"),
                number("1.5"),
                number(".5"),
                number("1e-3"),
                number("2E+4"),
                number("3j"),
                number("1."),
                Newline
            ]
        );
    }

    #[test]
    fn test_tokenize_reads_a_minus_after_a_hex_digit_e_as_an_operator() {
        // When / Then
        assert_eq!(
            kinds("0xe-1"),
            [number("0xe"), Op("-"), number("1"), Newline]
        );
    }

    #[test]
    fn test_tokenize_tells_prefixed_strings_from_names() {
        // When / Then
        assert_eq!(
            kinds("r'\\d' f'{x}' b'x' rb'x' t'{x}' fx 'a'"),
            [
                string("\\d"),
                Opaque,
                Opaque,
                Opaque,
                Opaque,
                name("fx"),
                string("a"),
                Newline
            ]
        );
    }

    #[test]
    fn test_tokenize_reads_unicode_names() {
        // When / Then
        assert_eq!(
            kinds("größe = 1"),
            [name("größe"), Op("="), number("1"), Newline]
        );
    }

    #[test]
    fn test_tokenize_records_spans_in_characters() {
        // Given
        let tokenized = tokenize("é = 'ü'\n");

        // When
        let spans: Vec<(usize, usize)> = tokenized
            .tokens
            .iter()
            .map(|token| (token.span.start.column, token.span.end.column))
            .collect();

        // Then
        assert_eq!(spans, [(1, 2), (3, 4), (5, 8), (8, 8)]);
    }

    fn error(source: &str) -> (SyntaxErrorKind, usize, usize) {
        let error = tokenize(source).error.expect("a syntax error");
        (error.kind, error.span.start.line, error.span.start.column)
    }

    #[test]
    fn test_tokenize_reports_an_unexpected_character() {
        // When / Then
        assert_eq!(
            error("x = 1\ny = $\n"),
            (SyntaxErrorKind::UnexpectedCharacter('$'), 2, 5)
        );
        assert_eq!(
            error("x = \\ 1"),
            (SyntaxErrorKind::UnexpectedCharacter('\\'), 1, 5)
        );
    }

    #[test]
    fn test_tokenize_reports_bracket_mismatches() {
        // When / Then
        assert_eq!(
            error("x = (]"),
            (SyntaxErrorKind::UnmatchedBracket(']'), 1, 6)
        );
        assert_eq!(
            error("x = )"),
            (SyntaxErrorKind::UnmatchedBracket(')'), 1, 5)
        );
        assert_eq!(
            error("x = [1,\n  2\n"),
            (SyntaxErrorKind::UnclosedBracket('['), 1, 5)
        );
    }

    #[test]
    fn test_tokenize_reports_an_inconsistent_dedent() {
        // When / Then
        assert_eq!(
            error("if x:\n    a = 1\n  b = 2\n"),
            (SyntaxErrorKind::InconsistentDedent, 3, 1)
        );
    }

    #[test]
    fn test_tokenize_reports_an_unterminated_string_where_it_starts() {
        // When / Then
        assert_eq!(
            error("x = 1\ny = 'open\n"),
            (SyntaxErrorKind::UnterminatedString, 2, 5)
        );
    }

    #[test]
    fn test_tokenize_keeps_the_tokens_before_an_error() {
        // When
        let tokenized = tokenize("x = 1\ny = $");

        // Then
        assert_eq!(tokenized.tokens.len(), 6);
    }

    #[test]
    fn test_tokenize_ignores_a_form_feed_in_indentation() {
        // When / Then
        assert_eq!(
            kinds("\u{0c}x = 1"),
            [name("x"), Op("="), number("1"), Newline]
        );
    }
}

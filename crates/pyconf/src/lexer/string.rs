//! Lexing Python's string literals: every prefix, both quote styles, single
//! or triple, and f-strings with replacement fields nested to any depth.
//!
//! Only a plain `str` literal yields a value. The others — f-strings,
//! template strings (`t"…"`), bytes, and a string naming a character with
//! `\N{…}`, whose names this crate does not carry — are scanned exactly as
//! carefully, since the module only reads on correctly from where a string
//! really ends, but yield [`TokenKind::Opaque`].
//!
//! An f-string's replacement field is skipped by following its brackets and
//! the strings inside it. Since Python 3.12 (PEP 701) those may reuse the
//! enclosing quote — `f"{d["key"]}"` — so a quote inside a field always
//! begins a nested string rather than ending the f-string.

use super::cursor::Cursor;
use crate::token::{SyntaxErrorKind, TokenKind};

/// How deeply strings may nest inside f-strings before the lexer gives up,
/// so a hostile input cannot exhaust the stack.
const MAX_NESTING: usize = 64;

/// What a string's prefix says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prefix {
    raw: bool,
    bytes: bool,
    /// An f-string or a template string, whose fields are code.
    formatted: bool,
}

impl Prefix {
    /// A string written with no prefix.
    pub(crate) const NONE: Self = Self {
        raw: false,
        bytes: false,
        formatted: false,
    };

    /// The prefix `letters` spells, or `None` when they are no string prefix
    /// at all (and so an ordinary name).
    pub(crate) fn parse(letters: &str) -> Option<Self> {
        let lower = letters.to_ascii_lowercase();
        let valid = matches!(
            lower.as_str(),
            "r" | "u" | "b" | "f" | "t" | "br" | "rb" | "fr" | "rf" | "tr" | "rt"
        );
        valid.then(|| Self {
            raw: lower.contains('r'),
            bytes: lower.contains('b'),
            formatted: lower.contains('f') || lower.contains('t'),
        })
    }
}

/// Lexes the string whose opening quote is under the cursor.
///
/// # Errors
///
/// The error that stops the module being read from this string on.
pub(crate) fn lex_string(
    cursor: &mut Cursor,
    prefix: Prefix,
) -> Result<TokenKind, SyntaxErrorKind> {
    lex_string_at(cursor, prefix, 0)
}

fn lex_string_at(
    cursor: &mut Cursor,
    prefix: Prefix,
    nesting: usize,
) -> Result<TokenKind, SyntaxErrorKind> {
    if nesting > MAX_NESTING {
        return Err(SyntaxErrorKind::TooDeeplyNested);
    }
    let quote = Quote::open(cursor);
    if prefix.formatted {
        scan_formatted_body(cursor, quote, prefix.raw, nesting)?;
        return Ok(TokenKind::Opaque);
    }
    let value = scan_plain_body(cursor, quote, prefix)?;
    Ok(match value {
        Some(value) if !prefix.bytes => TokenKind::Str(value),
        _ => TokenKind::Opaque,
    })
}

/// A string's quotes: which character, and whether there are three.
#[derive(Debug, Clone, Copy)]
struct Quote {
    character: char,
    triple: bool,
}

impl Quote {
    /// Consumes the opening quote under the cursor.
    fn open(cursor: &mut Cursor) -> Self {
        let character = cursor.bump().unwrap_or('"');
        let triple = cursor.peek() == Some(character) && cursor.peek_at(1) == Some(character);
        if triple {
            cursor.bump();
            cursor.bump();
        }
        Self { character, triple }
    }

    /// Whether `c`, just consumed, closes the string; consumes the rest of a
    /// triple quote when it does.
    fn closes(self, c: char, cursor: &mut Cursor) -> bool {
        if c != self.character {
            return false;
        }
        if !self.triple {
            return true;
        }
        if cursor.peek() == Some(self.character) && cursor.peek_at(1) == Some(self.character) {
            cursor.bump();
            cursor.bump();
            return true;
        }
        false
    }
}

/// Scans a plain (or bytes) string's body up to and including its closing
/// quote. Returns the decoded value, or `None` when it cannot be decoded.
fn scan_plain_body(
    cursor: &mut Cursor,
    quote: Quote,
    prefix: Prefix,
) -> Result<Option<String>, SyntaxErrorKind> {
    let mut value = Some(String::new());
    loop {
        if cursor.at_newline() && !quote.triple {
            return Err(SyntaxErrorKind::UnterminatedString);
        }
        let Some(c) = cursor.bump() else {
            return Err(SyntaxErrorKind::UnterminatedString);
        };
        if quote.closes(c, cursor) {
            return Ok(value);
        }
        if c == '\\' {
            if prefix.raw || prefix.bytes {
                // A raw string keeps the backslash and the character after
                // it, which therefore cannot close the string; a bytes string
                // has no value anyway.
                let next = cursor.bump().ok_or(SyntaxErrorKind::UnterminatedString)?;
                if let Some(value) = value.as_mut() {
                    value.push('\\');
                    value.push(next);
                }
            } else {
                match decode_escape(cursor)? {
                    Escape::Char(decoded) => {
                        if let Some(value) = value.as_mut() {
                            value.push(decoded);
                        }
                    }
                    Escape::Kept(kept) => {
                        if let Some(value) = value.as_mut() {
                            value.push('\\');
                            value.push(kept);
                        }
                    }
                    Escape::Continuation => {}
                    Escape::Named => value = None,
                }
            }
        } else if c == '\r' {
            // A line ending inside a triple-quoted string reads as `\n`.
            if let Some(value) = value.as_mut() {
                value.push('\n');
            }
            if cursor.peek() == Some('\n') {
                cursor.bump();
            }
        } else if let Some(value) = value.as_mut() {
            value.push(c);
        }
    }
}

/// What one escape sequence decodes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escape {
    Char(char),
    /// No escape at all: the backslash stays, followed by this character.
    Kept(char),
    /// A backslash ending the line, which continues the string.
    Continuation,
    /// `\N{…}`, a character by its Unicode name.
    Named,
}

/// Decodes the escape whose backslash was just consumed.
fn decode_escape(cursor: &mut Cursor) -> Result<Escape, SyntaxErrorKind> {
    if cursor.bump_newline() {
        return Ok(Escape::Continuation);
    }
    let Some(c) = cursor.bump() else {
        return Err(SyntaxErrorKind::UnterminatedString);
    };
    let decoded = match c {
        '\\' | '\'' | '"' => c,
        'a' => '\u{07}',
        'b' => '\u{08}',
        'f' => '\u{0c}',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'v' => '\u{0b}',
        '0'..='7' => {
            let mut code = c.to_digit(8).unwrap_or(0);
            for _ in 0..2 {
                match cursor.peek().and_then(|next| next.to_digit(8)) {
                    Some(digit) => {
                        cursor.bump();
                        code = code * 8 + digit;
                    }
                    None => break,
                }
            }
            char::from_u32(code).ok_or(SyntaxErrorKind::InvalidEscape)?
        }
        'x' => hex_escape(cursor, 2)?,
        'u' => hex_escape(cursor, 4)?,
        'U' => hex_escape(cursor, 8)?,
        'N' => {
            if cursor.peek() != Some('{') {
                return Err(SyntaxErrorKind::InvalidEscape);
            }
            while let Some(next) = cursor.peek() {
                if cursor.at_newline() {
                    break;
                }
                cursor.bump();
                if next == '}' {
                    return Ok(Escape::Named);
                }
            }
            return Err(SyntaxErrorKind::InvalidEscape);
        }
        other => return Ok(Escape::Kept(other)),
    };
    Ok(Escape::Char(decoded))
}

/// The character spelled by exactly `digits` hexadecimal digits.
fn hex_escape(cursor: &mut Cursor, digits: usize) -> Result<char, SyntaxErrorKind> {
    let mut code: u32 = 0;
    for _ in 0..digits {
        let digit = cursor
            .peek()
            .and_then(|c| c.to_digit(16))
            .ok_or(SyntaxErrorKind::InvalidEscape)?;
        cursor.bump();
        code = code * 16 + digit;
    }
    char::from_u32(code).ok_or(SyntaxErrorKind::InvalidEscape)
}

/// Scans an f-string's (or a template string's) body up to and including
/// its closing quote.
fn scan_formatted_body(
    cursor: &mut Cursor,
    quote: Quote,
    raw: bool,
    nesting: usize,
) -> Result<(), SyntaxErrorKind> {
    loop {
        if cursor.at_newline() && !quote.triple {
            return Err(SyntaxErrorKind::UnterminatedString);
        }
        let Some(c) = cursor.bump() else {
            return Err(SyntaxErrorKind::UnterminatedString);
        };
        if quote.closes(c, cursor) {
            return Ok(());
        }
        match c {
            '\\' => skip_formatted_escape(cursor, raw)?,
            '{' if cursor.peek() == Some('{') => {
                cursor.bump();
            }
            '{' => scan_replacement_field(cursor, quote, raw, nesting + 1)?,
            '}' if cursor.peek() == Some('}') => {
                cursor.bump();
            }
            _ => {}
        }
    }
}

/// Skips the escape whose backslash was just consumed in an f-string's
/// literal text, where `\N{…}`'s braces are no replacement field.
fn skip_formatted_escape(cursor: &mut Cursor, raw: bool) -> Result<(), SyntaxErrorKind> {
    if !raw && cursor.peek() == Some('N') && cursor.peek_at(1) == Some('{') {
        while let Some(c) = cursor.bump() {
            if c == '}' {
                return Ok(());
            }
        }
        return Err(SyntaxErrorKind::UnterminatedString);
    }
    if cursor.bump_newline() {
        return Ok(());
    }
    cursor
        .bump()
        .map(|_| ())
        .ok_or(SyntaxErrorKind::UnterminatedString)
}

/// Skips a replacement field whose `{` was just consumed, through its
/// closing `}`: the expression, a `!r`-style conversion, and a format spec,
/// which may hold fields of its own.
fn scan_replacement_field(
    cursor: &mut Cursor,
    quote: Quote,
    raw: bool,
    nesting: usize,
) -> Result<(), SyntaxErrorKind> {
    if nesting > MAX_NESTING {
        return Err(SyntaxErrorKind::TooDeeplyNested);
    }
    let mut depth = 0usize;
    loop {
        let Some(c) = cursor.peek() else {
            return Err(SyntaxErrorKind::UnterminatedString);
        };
        match c {
            '#' => cursor.skip_to_newline(),
            '\'' | '"' => {
                lex_string_at(cursor, Prefix::NONE, nesting + 1)?;
            }
            c if is_identifier_start(c) => {
                let mut letters = String::new();
                while let Some(next) = cursor.peek().filter(|&next| is_identifier_char(next)) {
                    letters.push(next);
                    cursor.bump();
                }
                if let (Some('\'' | '"'), Some(prefix)) = (cursor.peek(), Prefix::parse(&letters)) {
                    lex_string_at(cursor, prefix, nesting + 1)?;
                }
            }
            '(' | '[' | '{' => {
                cursor.bump();
                depth += 1;
            }
            '}' if depth == 0 => {
                cursor.bump();
                return Ok(());
            }
            ')' | ']' | '}' => {
                cursor.bump();
                depth = depth.saturating_sub(1);
            }
            ':' if depth == 0 => {
                cursor.bump();
                return scan_format_spec(cursor, quote, raw, nesting);
            }
            _ => {
                cursor.bump();
            }
        }
    }
}

/// Skips a format spec whose `:` was just consumed, through the `}` closing
/// its field.
fn scan_format_spec(
    cursor: &mut Cursor,
    quote: Quote,
    raw: bool,
    nesting: usize,
) -> Result<(), SyntaxErrorKind> {
    loop {
        if cursor.at_newline() && !quote.triple {
            return Err(SyntaxErrorKind::UnterminatedString);
        }
        let Some(c) = cursor.bump() else {
            return Err(SyntaxErrorKind::UnterminatedString);
        };
        match c {
            '}' => return Ok(()),
            '{' => scan_replacement_field(cursor, quote, raw, nesting + 1)?,
            '\\' => skip_formatted_escape(cursor, raw)?,
            c if quote.closes(c, cursor) => return Err(SyntaxErrorKind::UnterminatedString),
            _ => {}
        }
    }
}

/// Whether `c` may begin a name.
pub(crate) fn is_identifier_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

/// Whether `c` may continue a name.
pub(crate) fn is_identifier_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lexes the string at the start of `source` (after `prefix`'s letters,
    /// which the caller has already consumed), and what is left after it.
    fn lex(prefix: &str, source: &str) -> (Result<TokenKind, SyntaxErrorKind>, String) {
        let mut cursor = Cursor::new(source);
        let prefix = Prefix::parse(prefix).unwrap_or(Prefix::NONE);
        let kind = lex_string(&mut cursor, prefix);
        let mut rest = String::new();
        while let Some(c) = cursor.bump() {
            rest.push(c);
        }
        (kind, rest)
    }

    fn string(value: &str) -> TokenKind {
        TokenKind::Str(value.to_string())
    }

    #[test]
    fn test_prefix_parse_accepts_every_python_prefix_in_any_case() {
        // When / Then
        for letters in ["r", "U", "b", "F", "t", "Rb", "bR", "fr", "RF", "tr", "rT"] {
            assert!(Prefix::parse(letters).is_some(), "{letters}");
        }
        for letters in ["ur", "bf", "x", "rr", "rbf", ""] {
            assert!(Prefix::parse(letters).is_none(), "{letters}");
        }
    }

    #[test]
    fn test_lex_string_reads_both_quote_styles() {
        // When / Then
        assert_eq!(lex("", "'a' rest").0, Ok(string("a")));
        assert_eq!(lex("", "\"it's\"").0, Ok(string("it's")));
    }

    #[test]
    fn test_lex_string_stops_after_the_closing_quote() {
        // When
        let (_, rest) = lex("", "'a' + 'b'");

        // Then
        assert_eq!(rest, " + 'b'");
    }

    #[test]
    fn test_lex_string_reads_a_triple_quoted_string_across_lines() {
        // When
        let (kind, rest) = lex("", "'''one\n'two'\r\nthree''' x");

        // Then
        assert_eq!(kind, Ok(string("one\n'two'\nthree")));
        assert_eq!(rest, " x");
    }

    #[test]
    fn test_lex_string_decodes_escapes() {
        // When / Then
        assert_eq!(
            lex("", r#""\t\n\\\'\"\x41é\U0001F600\101\0""#).0,
            Ok(string("\t\n\\'\"Aé😀A\0"))
        );
        assert_eq!(
            lex("", "'\\a\\b\\f\\v\\r'").0,
            Ok(string("\u{7}\u{8}\u{c}\u{b}\r"))
        );
    }

    #[test]
    fn test_lex_string_keeps_an_unknown_escape_as_written() {
        // When / Then
        assert_eq!(lex("", r"'\d+'").0, Ok(string(r"\d+")));
    }

    #[test]
    fn test_lex_string_continues_a_line_after_a_backslash() {
        // When / Then
        assert_eq!(lex("", "'one \\\ntwo'").0, Ok(string("one two")));
    }

    #[test]
    fn test_lex_string_keeps_backslashes_in_a_raw_string() {
        // When / Then
        assert_eq!(lex("r", r"'\d\'+'").0, Ok(string(r"\d\'+")));
    }

    #[test]
    fn test_lex_string_gives_no_value_to_bytes_or_named_characters() {
        // When / Then
        assert_eq!(lex("b", r"'\xff'").0, Ok(TokenKind::Opaque));
        assert_eq!(lex("", r"'\N{BULLET}'").0, Ok(TokenKind::Opaque));
    }

    #[test]
    fn test_lex_string_reports_a_truncated_escape() {
        // When / Then
        assert_eq!(lex("", r"'\x4'").0, Err(SyntaxErrorKind::InvalidEscape));
        assert_eq!(lex("", r"'\N'").0, Err(SyntaxErrorKind::InvalidEscape));
        assert_eq!(lex("", r"'\N{x").0, Err(SyntaxErrorKind::InvalidEscape));
        assert_eq!(
            lex("", r"'\U00110000'").0,
            Err(SyntaxErrorKind::InvalidEscape)
        );
    }

    #[test]
    fn test_lex_string_reports_a_single_quoted_string_ending_with_its_line() {
        // When / Then
        assert_eq!(
            lex("", "'one\ntwo'").0,
            Err(SyntaxErrorKind::UnterminatedString)
        );
        assert_eq!(lex("", "'one").0, Err(SyntaxErrorKind::UnterminatedString));
        assert_eq!(
            lex("", "'''one").0,
            Err(SyntaxErrorKind::UnterminatedString)
        );
        assert_eq!(lex("", "'\\").0, Err(SyntaxErrorKind::UnterminatedString));
    }

    #[test]
    fn test_lex_string_skips_an_f_string_with_fields() {
        // When
        let (kind, rest) = lex("f", "'{a!r:>{width}} {{literal}} {b[1:2]}' x");

        // Then
        assert_eq!(kind, Ok(TokenKind::Opaque));
        assert_eq!(rest, " x");
    }

    #[test]
    fn test_lex_string_skips_a_field_reusing_the_enclosing_quote() {
        // Given — PEP 701, Python 3.12
        let source = r#""{d["key"]} and {f"{x}"}" x"#;

        // When
        let (kind, rest) = lex("f", source);

        // Then
        assert_eq!(kind, Ok(TokenKind::Opaque));
        assert_eq!(rest, " x");
    }

    #[test]
    fn test_lex_string_skips_a_triple_quoted_f_string_with_a_comment() {
        // Given
        let source = "\"\"\"\n{release # the version\n}\n.. |x| replace:: y\n\"\"\" x";

        // When
        let (kind, rest) = lex("f", source);

        // Then
        assert_eq!(kind, Ok(TokenKind::Opaque));
        assert_eq!(rest, " x");
    }

    #[test]
    fn test_lex_string_skips_escapes_in_an_f_string() {
        // When / Then
        assert_eq!(lex("f", r"'\N{BULLET} \' {x}' ").1, " ");
        assert_eq!(lex("rf", r"'\{x}' ").1, " ");
        assert_eq!(lex("f", "'a\\\nb' ").1, " ");
    }

    #[test]
    fn test_lex_string_reports_an_unterminated_f_string() {
        // When / Then
        assert_eq!(lex("f", "'{x").0, Err(SyntaxErrorKind::UnterminatedString));
        assert_eq!(
            lex("f", "'{x:>").0,
            Err(SyntaxErrorKind::UnterminatedString)
        );
        assert_eq!(
            lex("f", "'{x:>'").0,
            Err(SyntaxErrorKind::UnterminatedString)
        );
        assert_eq!(
            lex("f", "'x\n'").0,
            Err(SyntaxErrorKind::UnterminatedString)
        );
        assert_eq!(
            lex("f", r"'\N{x").0,
            Err(SyntaxErrorKind::UnterminatedString)
        );
    }

    #[test]
    fn test_lex_string_refuses_fields_nested_without_end() {
        // Given
        let source = format!("'{}'", "{f'".repeat(200));

        // When
        let (kind, _) = lex("f", &source);

        // Then
        assert_eq!(kind, Err(SyntaxErrorKind::TooDeeplyNested));
    }

    #[test]
    fn test_identifier_classes_accept_unicode_letters() {
        // When / Then
        assert!(is_identifier_start('é'));
        assert!(is_identifier_start('_'));
        assert!(!is_identifier_start('1'));
        assert!(is_identifier_char('1'));
        assert!(!is_identifier_char('-'));
    }
}

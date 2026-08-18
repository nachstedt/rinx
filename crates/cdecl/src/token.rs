/// The lexical class of a [`Token`].
///
/// Deliberately coarse: the tokenizer never decides whether an identifier is
/// a keyword, a type name or a declarator name — that is the parser's job,
/// and in C it cannot be decided lexically anyway (see the crate docs on why
/// no typedef table is needed here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// An identifier or keyword: `[A-Za-z_][A-Za-z0-9_]*`.
    Identifier,
    /// A numeric literal, including suffixes and radix prefixes (`10`, `0xFF`,
    /// `1.5`, `10UL`) — kept as one opaque token because array sizes and
    /// initializers are captured verbatim rather than evaluated.
    Number,
    /// Anything else, one character at a time, plus the multi-character `...`.
    Punctuation,
}

/// A single lexical token, carrying its byte offset into the original input so
/// parse errors can point at the right place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub offset: usize,
}

impl Token {
    /// Whether this token is punctuation with exactly the given text.
    #[must_use]
    pub fn is_punctuation(&self, text: &str) -> bool {
        self.kind == TokenKind::Punctuation && self.text == text
    }

    /// Whether this token is an identifier with exactly the given text.
    #[must_use]
    pub fn is_identifier(&self, text: &str) -> bool {
        self.kind == TokenKind::Identifier && self.text == text
    }
}

/// Splits a C declaration into tokens.
///
/// Infallible by design: whitespace is dropped and every remaining character
/// becomes a token, with unrecognized characters surviving as single-character
/// [`TokenKind::Punctuation`]. Nothing is ever silently discarded, so deciding
/// what counts as malformed is left entirely to the parser — which is what
/// lets the parser produce one meaningful error rather than the tokenizer
/// failing on input the parser might otherwise have recovered from.
#[must_use]
pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some(&(offset, character)) = chars.peek() {
        if character.is_whitespace() {
            chars.next();
        } else if is_identifier_start(character) {
            tokens.push(take_identifier(&mut chars, offset));
        } else if character.is_ascii_digit() {
            tokens.push(take_number(&mut chars, offset));
        } else {
            tokens.push(take_punctuation(&mut chars, offset, input));
        }
    }

    tokens
}

/// Whether `character` may begin an identifier.
fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

/// Whether `character` may continue an identifier.
fn is_identifier_continuation(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// Whether `character` may continue a numeric literal — alphanumerics (radix
/// prefixes and type suffixes) plus `.` for floating-point literals.
fn is_number_continuation(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '.'
}

/// Consumes an identifier token starting at `offset`.
fn take_identifier(chars: &mut std::iter::Peekable<std::str::CharIndices>, offset: usize) -> Token {
    Token {
        kind: TokenKind::Identifier,
        text: take_while(chars, is_identifier_continuation),
        offset,
    }
}

/// Consumes a numeric literal token starting at `offset`.
fn take_number(chars: &mut std::iter::Peekable<std::str::CharIndices>, offset: usize) -> Token {
    Token {
        kind: TokenKind::Number,
        text: take_while(chars, is_number_continuation),
        offset,
    }
}

/// Consumes a punctuation token starting at `offset`: the multi-character
/// ellipsis if it is present, otherwise exactly one character.
fn take_punctuation(
    chars: &mut std::iter::Peekable<std::str::CharIndices>,
    offset: usize,
    input: &str,
) -> Token {
    if input[offset..].starts_with("...") {
        for _ in 0..3 {
            chars.next();
        }
        return Token {
            kind: TokenKind::Punctuation,
            text: "...".to_string(),
            offset,
        };
    }
    let (_, character) = chars.next().expect("peeked character must exist");
    Token {
        kind: TokenKind::Punctuation,
        text: character.to_string(),
        offset,
    }
}

/// Consumes characters while `accept` holds, returning them as a string.
fn take_while(
    chars: &mut std::iter::Peekable<std::str::CharIndices>,
    accept: impl Fn(char) -> bool,
) -> String {
    let mut text = String::new();
    while let Some(&(_, character)) = chars.peek() {
        if !accept(character) {
            break;
        }
        text.push(character);
        chars.next();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reduces tokens to `(kind, text)` pairs, which is what most tests care
    /// about; offsets are asserted separately.
    fn kinds_and_texts(tokens: &[Token]) -> Vec<(TokenKind, &str)> {
        tokens
            .iter()
            .map(|token| (token.kind, token.text.as_str()))
            .collect()
    }

    #[test]
    fn test_tokenize_returns_no_tokens_for_empty_input() {
        // Given
        let input = "";

        // When
        let tokens = tokenize(input);

        // Then
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_returns_no_tokens_for_whitespace_only_input() {
        // Given
        let input = "  \t \n  ";

        // When
        let tokens = tokenize(input);

        // Then
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_splits_identifiers_on_whitespace() {
        // Given
        let input = "unsigned long ulong";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(
            kinds_and_texts(&tokens),
            vec![
                (TokenKind::Identifier, "unsigned"),
                (TokenKind::Identifier, "long"),
                (TokenKind::Identifier, "ulong"),
            ]
        );
    }

    #[test]
    fn test_tokenize_accepts_underscores_and_digits_inside_identifiers() {
        // Given — the shapes C API names actually take.
        let input = "_Py_ssize_t2 PY_SSIZE_T_MAX";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(
            kinds_and_texts(&tokens),
            vec![
                (TokenKind::Identifier, "_Py_ssize_t2"),
                (TokenKind::Identifier, "PY_SSIZE_T_MAX"),
            ]
        );
    }

    #[test]
    fn test_tokenize_does_not_start_an_identifier_with_a_digit() {
        // Given — `2x` is a number token, not an identifier.
        let input = "2x";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(kinds_and_texts(&tokens), vec![(TokenKind::Number, "2x")]);
    }

    #[test]
    fn test_tokenize_keeps_numeric_literals_with_prefixes_and_suffixes_whole() {
        // Given
        let input = "10 0xFF 1.5 10UL";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(
            kinds_and_texts(&tokens),
            vec![
                (TokenKind::Number, "10"),
                (TokenKind::Number, "0xFF"),
                (TokenKind::Number, "1.5"),
                (TokenKind::Number, "10UL"),
            ]
        );
    }

    #[test]
    fn test_tokenize_emits_each_punctuation_character_separately() {
        // Given
        let input = "*()[],:=";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(
            kinds_and_texts(&tokens),
            vec![
                (TokenKind::Punctuation, "*"),
                (TokenKind::Punctuation, "("),
                (TokenKind::Punctuation, ")"),
                (TokenKind::Punctuation, "["),
                (TokenKind::Punctuation, "]"),
                (TokenKind::Punctuation, ","),
                (TokenKind::Punctuation, ":"),
                (TokenKind::Punctuation, "="),
            ]
        );
    }

    #[test]
    fn test_tokenize_treats_ellipsis_as_one_token() {
        // Given — varargs must not become three separate `.` tokens.
        let input = "int f(int a, ...)";

        // When
        let tokens = tokenize(input);

        // Then
        assert!(tokens.iter().any(|token| token.is_punctuation("...")));
        assert!(!tokens.iter().any(|token| token.is_punctuation(".")));
    }

    #[test]
    fn test_tokenize_splits_a_lone_dot_pair_into_single_characters() {
        // Given — only a full `...` collapses; a shorter run does not.
        let input = "..";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(
            kinds_and_texts(&tokens),
            vec![(TokenKind::Punctuation, "."), (TokenKind::Punctuation, ".")]
        );
    }

    #[test]
    fn test_tokenize_records_byte_offsets_for_error_reporting() {
        // Given
        let input = "int *x";

        // When
        let tokens = tokenize(input);

        // Then
        let offsets: Vec<usize> = tokens.iter().map(|token| token.offset).collect();
        assert_eq!(offsets, vec![0, 4, 5]);
    }

    #[test]
    fn test_tokenize_records_offsets_after_multi_byte_characters() {
        // Given — offsets are byte offsets, so a multi-byte char shifts them.
        let input = "é x";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(tokens[0].offset, 0);
        assert_eq!(tokens[1].offset, 3);
    }

    #[test]
    fn test_tokenize_preserves_unrecognized_characters_as_punctuation() {
        // Given — nothing is silently dropped, so the parser (not the
        // tokenizer) gets to decide this input is malformed.
        let input = "int é x";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(
            kinds_and_texts(&tokens),
            vec![
                (TokenKind::Identifier, "int"),
                (TokenKind::Punctuation, "é"),
                (TokenKind::Identifier, "x"),
            ]
        );
    }

    #[test]
    fn test_tokenize_handles_a_full_function_pointer_declaration() {
        // Given — the real-world shape that motivated this crate.
        let input = "int (*Py_tracefunc)(PyObject *obj)";

        // When
        let tokens = tokenize(input);

        // Then
        assert_eq!(
            kinds_and_texts(&tokens),
            vec![
                (TokenKind::Identifier, "int"),
                (TokenKind::Punctuation, "("),
                (TokenKind::Punctuation, "*"),
                (TokenKind::Identifier, "Py_tracefunc"),
                (TokenKind::Punctuation, ")"),
                (TokenKind::Punctuation, "("),
                (TokenKind::Identifier, "PyObject"),
                (TokenKind::Punctuation, "*"),
                (TokenKind::Identifier, "obj"),
                (TokenKind::Punctuation, ")"),
            ]
        );
    }

    #[test]
    fn test_is_punctuation_matches_only_matching_punctuation() {
        // Given
        let star = Token {
            kind: TokenKind::Punctuation,
            text: "*".to_string(),
            offset: 0,
        };
        let name = Token {
            kind: TokenKind::Identifier,
            text: "*".to_string(),
            offset: 0,
        };

        // When / Then
        assert!(star.is_punctuation("*"));
        assert!(!star.is_punctuation("("));
        assert!(!name.is_punctuation("*"));
    }

    #[test]
    fn test_is_identifier_matches_only_matching_identifiers() {
        // Given
        let keyword = Token {
            kind: TokenKind::Identifier,
            text: "const".to_string(),
            offset: 0,
        };
        let punctuation = Token {
            kind: TokenKind::Punctuation,
            text: "const".to_string(),
            offset: 0,
        };

        // When / Then
        assert!(keyword.is_identifier("const"));
        assert!(!keyword.is_identifier("static"));
        assert!(!punctuation.is_identifier("const"));
    }

    #[test]
    fn test_is_identifier_start_accepts_letters_and_underscore_only() {
        // Given / When / Then
        assert!(is_identifier_start('a'));
        assert!(is_identifier_start('Z'));
        assert!(is_identifier_start('_'));
        assert!(!is_identifier_start('1'));
        assert!(!is_identifier_start('*'));
    }

    #[test]
    fn test_is_identifier_continuation_additionally_accepts_digits() {
        // Given / When / Then
        assert!(is_identifier_continuation('a'));
        assert!(is_identifier_continuation('_'));
        assert!(is_identifier_continuation('9'));
        assert!(!is_identifier_continuation('*'));
        assert!(!is_identifier_continuation('.'));
    }

    #[test]
    fn test_is_number_continuation_accepts_alphanumerics_and_dot() {
        // Given / When / Then
        assert!(is_number_continuation('0'));
        assert!(is_number_continuation('x'));
        assert!(is_number_continuation('.'));
        assert!(!is_number_continuation('_'));
        assert!(!is_number_continuation('*'));
    }

    #[test]
    fn test_take_while_stops_at_the_first_rejected_character() {
        // Given
        let input = "abc*def";
        let mut chars = input.char_indices().peekable();

        // When
        let taken = take_while(&mut chars, is_identifier_continuation);

        // Then
        assert_eq!(taken, "abc");
        assert_eq!(chars.peek().map(|&(_, c)| c), Some('*'));
    }

    #[test]
    fn test_take_while_returns_empty_when_nothing_matches() {
        // Given
        let input = "*abc";
        let mut chars = input.char_indices().peekable();

        // When
        let taken = take_while(&mut chars, is_identifier_continuation);

        // Then
        assert_eq!(taken, "");
        assert_eq!(chars.peek().map(|&(_, c)| c), Some('*'));
    }

    #[test]
    fn test_take_identifier_consumes_the_whole_identifier() {
        // Given
        let input = "Py_tracefunc)";
        let mut chars = input.char_indices().peekable();

        // When
        let token = take_identifier(&mut chars, 0);

        // Then
        assert_eq!(token.kind, TokenKind::Identifier);
        assert_eq!(token.text, "Py_tracefunc");
        assert_eq!(token.offset, 0);
    }

    #[test]
    fn test_take_number_consumes_the_whole_literal() {
        // Given
        let input = "0xFF]";
        let mut chars = input.char_indices().peekable();

        // When
        let token = take_number(&mut chars, 0);

        // Then
        assert_eq!(token.kind, TokenKind::Number);
        assert_eq!(token.text, "0xFF");
    }

    #[test]
    fn test_take_punctuation_consumes_an_ellipsis_whole() {
        // Given
        let input = "...)";
        let mut chars = input.char_indices().peekable();

        // When
        let token = take_punctuation(&mut chars, 0, input);

        // Then
        assert_eq!(token.text, "...");
        assert_eq!(chars.peek().map(|&(_, c)| c), Some(')'));
    }

    #[test]
    fn test_take_punctuation_consumes_a_single_character_otherwise() {
        // Given
        let input = "*x";
        let mut chars = input.char_indices().peekable();

        // When
        let token = take_punctuation(&mut chars, 0, input);

        // Then
        assert_eq!(token.text, "*");
        assert_eq!(chars.peek().map(|&(_, c)| c), Some('x'));
    }
}

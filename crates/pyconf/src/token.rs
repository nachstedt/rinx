//! The tokens a Python module is split into, and why splitting can fail.

use std::fmt;

use crate::position::Span;

/// One token of a Python module.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// What a token is.
///
/// Only what reading a literal needs is kept: a string's decoded value, a
/// number's text. Everything a literal cannot be — an f-string, a template
/// string, a bytes string, a string spelling a character by its Unicode
/// name — is [`TokenKind::Opaque`]: the lexer still finds where it ends, so
/// the rest of the module is read, but nothing claims to know its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// An identifier or a keyword; `True`, `False` and `None` among them.
    Name(String),
    /// A number, as written.
    Number(String),
    /// A string literal, with its escapes decoded.
    Str(String),
    /// A string-like token whose value is not a plain `str` literal.
    Opaque,
    /// An operator or delimiter.
    Op(&'static str),
    /// The end of a logical line.
    Newline,
    /// A deeper indentation than the line before.
    Indent,
    /// A return to an enclosing indentation.
    Dedent,
}

impl TokenKind {
    /// Whether this is the operator `op`.
    #[must_use]
    pub fn is_op(&self, op: &str) -> bool {
        matches!(self, Self::Op(found) if *found == op)
    }

    /// Whether this is the name or keyword `name`.
    #[must_use]
    pub fn is_name(&self, name: &str) -> bool {
        matches!(self, Self::Name(found) if found == name)
    }
}

/// Why the module could not be read past some point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub kind: SyntaxErrorKind,
    pub span: Span,
}

/// What made a module unreadable from some point on: each of these is a
/// syntax error to Python too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxErrorKind {
    /// A string with no closing quote.
    UnterminatedString,
    /// A character that begins no token.
    UnexpectedCharacter(char),
    /// A closing bracket that closes nothing, or the wrong bracket.
    UnmatchedBracket(char),
    /// A bracket still open at the end of the module.
    UnclosedBracket(char),
    /// A dedent to a column no enclosing line is indented to.
    InconsistentDedent,
    /// A `\x`, `\u` or `\U` escape without its hexadecimal digits.
    InvalidEscape,
    /// Strings nested in f-strings deeper than the reader follows.
    TooDeeplyNested,
}

impl fmt::Display for SyntaxErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedString => write!(f, "unterminated string"),
            Self::UnexpectedCharacter(c) => write!(f, "unexpected character {c:?}"),
            Self::UnmatchedBracket(c) => write!(f, "unmatched {c:?}"),
            Self::UnclosedBracket(c) => write!(f, "{c:?} was never closed"),
            Self::InconsistentDedent => {
                write!(f, "unindent does not match any outer indentation level")
            }
            Self::InvalidEscape => write!(f, "truncated escape sequence"),
            Self::TooDeeplyNested => write!(f, "strings nested too deeply"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_op_matches_only_that_operator() {
        // Given
        let kind = TokenKind::Op("+=");

        // When / Then
        assert!(kind.is_op("+="));
        assert!(!kind.is_op("+"));
        assert!(!TokenKind::Name("x".to_string()).is_op("+="));
    }

    #[test]
    fn test_is_name_matches_only_that_name() {
        // Given
        let kind = TokenKind::Name("import".to_string());

        // When / Then
        assert!(kind.is_name("import"));
        assert!(!kind.is_name("from"));
        assert!(!TokenKind::Str("import".to_string()).is_name("import"));
    }

    #[test]
    fn test_syntax_error_kinds_read_as_python_words_them() {
        // When / Then
        assert_eq!(
            SyntaxErrorKind::UnterminatedString.to_string(),
            "unterminated string"
        );
        assert_eq!(
            SyntaxErrorKind::UnexpectedCharacter('$').to_string(),
            "unexpected character '$'"
        );
        assert_eq!(
            SyntaxErrorKind::UnmatchedBracket(')').to_string(),
            "unmatched ')'"
        );
        assert_eq!(
            SyntaxErrorKind::UnclosedBracket('[').to_string(),
            "'[' was never closed"
        );
        assert_eq!(
            SyntaxErrorKind::InconsistentDedent.to_string(),
            "unindent does not match any outer indentation level"
        );
        assert_eq!(
            SyntaxErrorKind::InvalidEscape.to_string(),
            "truncated escape sequence"
        );
        assert_eq!(
            SyntaxErrorKind::TooDeeplyNested.to_string(),
            "strings nested too deeply"
        );
    }
}

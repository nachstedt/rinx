use crate::error::{FilterError, FilterErrorKind};

/// One lexical token of a filter expression.
///
/// Keywords are recognised here rather than left to the parser: unlike C, this
/// grammar has a closed keyword set and no context in which `and` could be a
/// name, so deciding it lexically keeps the parser to one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// A field name, already checked against [`FieldName`](crate::FieldName)'s
    /// character set.
    Ident(String),
    Str(String),
    Int(i64),
    True,
    False,
    NoneLit,
    And,
    Or,
    Not,
    In,
    Is,
    EqEq,
    NotEq,
    LParen,
    RParen,
}

impl TokenKind {
    /// How this token is named in a diagnostic.
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Ident(name) => format!("`{name}`"),
            Self::Str(_) => "a string".to_string(),
            Self::Int(number) => format!("`{number}`"),
            Self::True => "`True`".to_string(),
            Self::False => "`False`".to_string(),
            Self::NoneLit => "`None`".to_string(),
            Self::And => "`and`".to_string(),
            Self::Or => "`or`".to_string(),
            Self::Not => "`not`".to_string(),
            Self::In => "`in`".to_string(),
            Self::Is => "`is`".to_string(),
            Self::EqEq => "`==`".to_string(),
            Self::NotEq => "`!=`".to_string(),
            Self::LParen => "`(`".to_string(),
            Self::RParen => "`)`".to_string(),
        }
    }
}

/// A token and where it was written, in characters from the start of the
/// filter text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub offset: usize,
    pub length: usize,
}

/// Splits a filter expression into tokens.
///
/// Unlike `rusty_sphinx_cdecl`'s tokenizer, this one is fallible, and
/// deliberately so: sphinx-needs filters are Python, so the input frequently
/// contains constructs that are perfectly good Python and outside this
/// language. Recognising them *here* is what lets `len(x) > 0` be reported as
/// "function calls are not supported" pointing at `len`, instead of surfacing
/// as a parse failure at whatever token happened to break first.
///
/// # Errors
///
/// Returns [`FilterError`] for an unterminated string, an out-of-range number,
/// an illegal field name, or any of the Python constructs listed above.
pub(crate) fn tokenize(input: &str) -> Result<Vec<Token>, FilterError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;

    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            at += 1;
        } else if c == '"' || c == '\'' {
            let token = scan_string(&chars, at)?;
            at = token.offset + token.length;
            tokens.push(token);
        } else if c.is_ascii_digit() || (c == '-' && next_is_digit(&chars, at)) {
            let token = scan_number(&chars, at)?;
            at = token.offset + token.length;
            tokens.push(token);
        } else if is_name_start(c) {
            let token = scan_word(&chars, at)?;
            at = token.offset + token.length;
            tokens.push(token);
        } else {
            let token = scan_operator(&chars, at)?;
            at = token.offset + token.length;
            tokens.push(token);
        }
    }

    Ok(tokens)
}

/// Reads a quoted string, honouring the backslash escapes Python shares.
fn scan_string(chars: &[char], start: usize) -> Result<Token, FilterError> {
    let quote = chars[start];
    let mut text = String::new();
    let mut at = start + 1;

    while at < chars.len() {
        match chars[at] {
            '\\' if at + 1 < chars.len() => {
                text.push(unescape(chars[at + 1]));
                at += 2;
            }
            c if c == quote => {
                return Ok(Token {
                    kind: TokenKind::Str(text),
                    offset: start,
                    length: at + 1 - start,
                });
            }
            c => {
                text.push(c);
                at += 1;
            }
        }
    }

    Err(FilterError::new(
        FilterErrorKind::UnterminatedString,
        start,
        chars.len() - start,
    ))
}

/// The character a backslash escape stands for.
///
/// Only the escapes that mean something in a one-line filter; anything else
/// stands for itself, which is what Python does for an unknown escape too.
fn unescape(escaped: char) -> char {
    match escaped {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        other => other,
    }
}

/// Reads an integer literal, including a leading `-`.
fn scan_number(chars: &[char], start: usize) -> Result<Token, FilterError> {
    let mut at = start;
    if chars[at] == '-' {
        at += 1;
    }
    while at < chars.len() && chars[at].is_ascii_digit() {
        at += 1;
    }

    let text: String = chars[start..at].iter().collect();
    if at < chars.len() && (chars[at] == '.' || chars[at].is_alphabetic()) {
        // `1.5` or `10L`: a number this language has no type for, rather than
        // an integer that silently loses its tail.
        return Err(FilterError::new(
            FilterErrorKind::unsupported_but(
                "non-integer numbers",
                "compare whole numbers or text",
            ),
            start,
            at + 1 - start,
        ));
    }

    let value = text.parse::<i64>().map_err(|_| {
        FilterError::new(
            FilterErrorKind::NumberOutOfRange(text.clone()),
            start,
            at - start,
        )
    })?;

    Ok(Token {
        kind: TokenKind::Int(value),
        offset: start,
        length: at - start,
    })
}

/// Reads a keyword or a field name.
fn scan_word(chars: &[char], start: usize) -> Result<Token, FilterError> {
    let mut at = start;
    while at < chars.len() && is_name_char(chars[at]) {
        at += 1;
    }
    let word: String = chars[start..at].iter().collect();
    let length = at - start;

    // `f"..."`: a Python string prefix, which would otherwise read as a field
    // name followed by an unrelated string.
    if at < chars.len() && (chars[at] == '"' || chars[at] == '\'') {
        return Err(FilterError::new(
            FilterErrorKind::unsupported_but(
                "string prefixes such as f-strings",
                "write a plain string",
            ),
            start,
            length + 1,
        ));
    }

    let kind = match word.as_str() {
        "and" => TokenKind::And,
        "or" => TokenKind::Or,
        "not" => TokenKind::Not,
        "in" => TokenKind::In,
        "is" => TokenKind::Is,
        "True" => TokenKind::True,
        "False" => TokenKind::False,
        "None" => TokenKind::NoneLit,
        "for" | "if" | "else" | "lambda" => {
            return Err(FilterError::new(
                FilterErrorKind::unsupported("comprehensions and conditional expressions"),
                start,
                length,
            ));
        }
        name => TokenKind::Ident(name.to_string()),
    };

    Ok(Token {
        kind,
        offset: start,
        length,
    })
}

/// Reads an operator, rejecting by name every Python spelling this grammar
/// deliberately lacks.
fn scan_operator(chars: &[char], start: usize) -> Result<Token, FilterError> {
    let c = chars[start];
    let next = chars.get(start + 1).copied();

    let kind = match (c, next) {
        ('=', Some('=')) => TokenKind::EqEq,
        ('!', Some('=')) => TokenKind::NotEq,
        ('(', _) => TokenKind::LParen,
        (')', _) => TokenKind::RParen,
        _ => return Err(rejected_operator(c, next, start)),
    };

    let length = usize::from(matches!(kind, TokenKind::EqEq | TokenKind::NotEq)) + 1;
    Ok(Token {
        kind,
        offset: start,
        length,
    })
}

/// The diagnostic for an operator character this grammar does not accept.
fn rejected_operator(c: char, next: Option<char>, start: usize) -> FilterError {
    let (kind, length) = match (c, next) {
        ('&', Some('&')) => (FilterErrorKind::unsupported_but("`&&`", "use `and`"), 2),
        ('&', _) => (FilterErrorKind::unsupported_but("`&`", "use `and`"), 1),
        ('|', Some('|')) => (FilterErrorKind::unsupported_but("`||`", "use `or`"), 2),
        ('|', _) => (FilterErrorKind::unsupported_but("`|`", "use `or`"), 1),
        ('!', _) => (FilterErrorKind::unsupported_but("`!`", "use `not`"), 1),
        ('=', _) => (
            FilterErrorKind::unsupported_but("assignment", "use `==` to compare"),
            1,
        ),
        ('<' | '>', Some('=')) => (
            FilterErrorKind::unsupported_but("ordering comparisons", "compare with `==` or `in`"),
            2,
        ),
        ('<' | '>', _) => (
            FilterErrorKind::unsupported_but("ordering comparisons", "compare with `==` or `in`"),
            1,
        ),
        ('[', Some('[')) => (FilterErrorKind::unsupported("dynamic functions"), 2),
        ('[', _) => (
            FilterErrorKind::unsupported("list literals and comprehensions"),
            1,
        ),
        ('{', _) => (FilterErrorKind::unsupported("dict and set literals"), 1),
        (',', _) => (
            FilterErrorKind::unsupported_but("tuples", "combine conditions with `and` or `or`"),
            1,
        ),
        ('.', _) => (
            FilterErrorKind::unsupported_but("attribute access", "name the field on its own"),
            1,
        ),
        ('+' | '-' | '*' | '/' | '%', _) => (FilterErrorKind::unsupported("arithmetic"), 1),
        _ => (FilterErrorKind::UnexpectedToken(format!("`{c}`")), 1),
    };
    FilterError::new(kind, start, length)
}

/// Whether `c` may open a field name — [`FieldName`](crate::FieldName)'s rule,
/// applied while scanning.
fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Whether `c` may continue a field name.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-')
}

/// Whether the character after `at` is a digit, which is what makes a `-` the
/// sign of a literal rather than an arithmetic operator.
fn next_is_digit(chars: &[char], at: usize) -> bool {
    chars.get(at + 1).is_some_and(char::is_ascii_digit)
}

#[cfg(test)]
mod tests;

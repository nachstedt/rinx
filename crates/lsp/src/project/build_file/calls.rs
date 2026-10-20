//! Reading a build file's calls and string lists from its tokens.
//!
//! Only what the edits need is read: calls written at the top level, their
//! keyword arguments, and list literals of strings. Every position is turned
//! into the AST's, which the language server converts once.

use rinx_ast::{Position, Span};
use rinx_pyconf::{Token, TokenKind};

/// One call written at the top level of a build file.
pub(super) struct Call<'t> {
    /// The name called: `rinx_library`, or whatever a `load` bound it to.
    pub(super) function: &'t Token,
    pub(super) arguments: Vec<Argument<'t>>,
    pub(super) open: &'t Token,
    pub(super) close: &'t Token,
}

/// One argument of a [`Call`]: its keyword, if it has one, its value's
/// tokens, and the comma after it.
pub(super) struct Argument<'t> {
    pub(super) keyword: Option<&'t Token>,
    pub(super) value: &'t [Token],
    pub(super) comma: Option<&'t Token>,
}

impl Argument<'_> {
    /// From the keyword to the end of the value.
    pub(super) fn keyword_span(&self) -> Span {
        let start = self
            .keyword
            .map_or(self.value[0].span, |keyword| keyword.span);
        let end = self.value.last().map_or(start, |token| token.span);
        span_of(start.start, end.end)
    }
}

impl<'t> Call<'t> {
    /// The value of its `name` argument, when that is one written string.
    pub(super) fn name(&self) -> Option<&'t str> {
        match self.argument("name")?.value {
            [
                Token {
                    kind: TokenKind::Str(name),
                    ..
                },
            ] => Some(name),
            _ => None,
        }
    }

    pub(super) fn argument(&self, keyword: &str) -> Option<&Argument<'t>> {
        self.arguments.iter().find(|argument| {
            argument
                .keyword
                .is_some_and(|token| token.kind.is_name(keyword))
        })
    }
}

/// A `[…]` of string literals, as written.
pub(super) struct StringList<'t> {
    pub(super) open: &'t Token,
    pub(super) close: &'t Token,
    /// Each item's value and token, with the comma after it.
    pub(super) items: Vec<(String, &'t Token, Option<&'t Token>)>,
}

impl<'t> StringList<'t> {
    /// `value` as a list of strings, or `None` for any other expression.
    pub(super) fn read(value: &'t [Token]) -> Option<Self> {
        let (open, rest) = value.split_first()?;
        let (close, inner) = rest.split_last()?;
        if !open.kind.is_op("[") || !close.kind.is_op("]") {
            return None;
        }
        let mut items = Vec::new();
        let mut tokens = inner.iter().peekable();
        while let Some(token) = tokens.next() {
            let TokenKind::Str(value) = &token.kind else {
                return None;
            };
            let comma = tokens.next_if(|next| next.kind.is_op(","));
            if comma.is_none() && tokens.peek().is_some() {
                return None;
            }
            items.push((value.clone(), token, comma));
        }
        Some(Self { open, close, items })
    }
}

/// Every call written outside brackets: `name(…)`.
pub(super) fn top_level_calls(tokens: &[Token]) -> Vec<Call<'_>> {
    let mut calls = Vec::new();
    let mut depth = 0usize;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if depth == 0
            && matches!(token.kind, TokenKind::Name(_))
            && tokens
                .get(index + 1)
                .is_some_and(|next| next.kind.is_op("("))
            && let Some((call, end)) = read_call(tokens, index + 1)
        {
            calls.push(call);
            index = end + 1;
            continue;
        }
        if is_open(&token.kind) {
            depth += 1;
        } else if is_close(&token.kind) {
            depth = depth.saturating_sub(1);
        }
        index += 1;
    }
    calls
}

/// The argument written as `range`, followed by `comma`.
pub(super) fn argument<'t>(range: &'t [Token], comma: Option<&'t Token>) -> Argument<'t> {
    let keyword = match range {
        [keyword, equals, ..]
            if matches!(keyword.kind, TokenKind::Name(_)) && equals.kind.is_op("=") =>
        {
            Some(keyword)
        }
        _ => None,
    };
    let value = if keyword.is_some() {
        &range[2..]
    } else {
        range
    };
    Argument {
        keyword,
        value,
        comma,
    }
}

/// The call whose `(` is `tokens[open]`, after the name called, and the
/// index of its `)` — `None` when the file ends first.
pub(super) fn read_call(tokens: &[Token], open: usize) -> Option<(Call<'_>, usize)> {
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = open + 1;
    for (index, token) in tokens.iter().enumerate().skip(open + 1) {
        if depth == 0 && (token.kind.is_op(",") || token.kind.is_op(")")) {
            let range = &tokens[start..index];
            let comma = token.kind.is_op(",").then_some(token);
            if !range.is_empty() {
                arguments.push(argument(range, comma));
            }
            if token.kind.is_op(")") {
                return Some((
                    Call {
                        function: &tokens[open - 1],
                        arguments,
                        open: &tokens[open],
                        close: token,
                    },
                    index,
                ));
            }
            start = index + 1;
        } else if is_open(&token.kind) {
            depth += 1;
        } else if is_close(&token.kind) {
            depth = depth.saturating_sub(1);
        }
    }
    None
}

pub(super) fn is_open(kind: &TokenKind) -> bool {
    kind.is_op("(") || kind.is_op("[") || kind.is_op("{")
}

pub(super) fn is_close(kind: &TokenKind) -> bool {
    kind.is_op(")") || kind.is_op("]") || kind.is_op("}")
}

/// A reading position as an AST one.
pub(super) fn position(position: rinx_pyconf::Position) -> Position {
    Position::new(line_number(position.line), line_number(position.column))
}

pub(super) fn line_number(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub(super) fn span_of(start: rinx_pyconf::Position, end: rinx_pyconf::Position) -> Span {
    Span::new(position(start), position(end))
}

/// The empty span at `at`, where an insertion goes.
pub(super) fn point(at: rinx_pyconf::Position) -> Span {
    Span::new(position(at), position(at))
}

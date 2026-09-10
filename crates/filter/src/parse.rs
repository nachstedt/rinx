use crate::error::{FilterError, FilterErrorKind};
use crate::expr::{CompareOp, Expr, Operand};
use crate::field_name::FieldName;
use crate::token::{Token, TokenKind, tokenize};
use crate::value::Literal;

/// How deeply parentheses may nest before the parser gives up.
///
/// Recursion is the only unbounded resource here, and a filter is one line of
/// an option block: real ones nest two levels at most. The bound turns what
/// would be a stack overflow on pathological input into an ordinary
/// [`FilterError`], which matters because this parser runs on every keystroke
/// of the live-preview path.
const MAX_DEPTH: usize = 32;

/// Parses a filter expression.
///
/// # Errors
///
/// Returns [`FilterError`], carrying the character range of the offending
/// token, for anything this language does not evaluate — including Python it
/// deliberately does not support, which is reported by construct rather than
/// as a generic syntax error.
///
/// # Panics
///
/// Never. Every fallible step returns an error instead, because the caller is
/// a parser that must degrade rather than abort.
pub fn parse_filter(input: &str) -> Result<Expr, FilterError> {
    let tokens = tokenize(input)?;
    let end = input.chars().count();
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        end,
    };
    let expr = parser.parse_or(0)?;
    if let Some(token) = parser.peek() {
        return Err(unexpected(token));
    }
    Ok(expr)
}

/// An error at `token`, naming it.
fn unexpected(token: &Token) -> FilterError {
    FilterError::new(
        FilterErrorKind::UnexpectedToken(token.kind.describe()),
        token.offset,
        token.length,
    )
}

/// Recursive-descent state: the token stream and how far into it we are.
struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    /// Characters in the whole input, so an error at the end has somewhere to
    /// point.
    end: usize,
}

impl Parser<'_> {
    /// `or := and ("or" and)*`
    fn parse_or(&mut self, depth: usize) -> Result<Expr, FilterError> {
        let mut left = self.parse_and(depth)?;
        while self.eat(&TokenKind::Or) {
            let right = self.parse_and(depth)?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `and := unary ("and" unary)*`
    fn parse_and(&mut self, depth: usize) -> Result<Expr, FilterError> {
        let mut left = self.parse_unary(depth)?;
        while self.eat(&TokenKind::And) {
            let right = self.parse_unary(depth)?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `unary := "not" unary | comparison`
    fn parse_unary(&mut self, depth: usize) -> Result<Expr, FilterError> {
        if self.eat(&TokenKind::Not) {
            let inner = self.parse_unary(depth)?;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.parse_comparison(depth)
    }

    /// `comparison := operand [("==" | "!=") operand
    ///                        | ["not"] "in" operand
    ///                        | "is" ["not"] "None"]`
    fn parse_comparison(&mut self, depth: usize) -> Result<Expr, FilterError> {
        if self.check(&TokenKind::LParen) {
            return self.parse_group(depth);
        }

        let left = self.parse_operand()?;

        if self.eat(&TokenKind::EqEq) {
            return self.finish_equality(left, CompareOp::Eq);
        }
        if self.eat(&TokenKind::NotEq) {
            return self.finish_equality(left, CompareOp::Ne);
        }
        if self.eat(&TokenKind::In) {
            let haystack = self.parse_operand()?;
            return Ok(Expr::Contains {
                needle: left,
                haystack,
                negated: false,
            });
        }
        if self.check(&TokenKind::Not) && self.check_at(1, &TokenKind::In) {
            self.at += 2;
            let haystack = self.parse_operand()?;
            return Ok(Expr::Contains {
                needle: left,
                haystack,
                negated: true,
            });
        }
        if self.eat(&TokenKind::Is) {
            return self.finish_is_none(left);
        }

        Ok(Expr::Truthy(left))
    }

    /// `"(" or ")"`, guarding the recursion depth.
    fn parse_group(&mut self, depth: usize) -> Result<Expr, FilterError> {
        if depth >= MAX_DEPTH {
            let (offset, length) = self.here();
            return Err(FilterError::new(
                FilterErrorKind::unsupported_but("expressions nested this deeply", "simplify it"),
                offset,
                length,
            ));
        }
        self.at += 1;
        let inner = self.parse_or(depth + 1)?;
        if !self.eat(&TokenKind::RParen) {
            let (offset, length) = self.here();
            return Err(FilterError::new(
                FilterErrorKind::UnexpectedToken("the end of a parenthesised group".to_string()),
                offset,
                length,
            ));
        }
        Ok(inner)
    }

    /// The right-hand side of `==`/`!=`, where `None` is refused in favour of
    /// Python's own `is None` spelling.
    fn finish_equality(&mut self, left: Operand, op: CompareOp) -> Result<Expr, FilterError> {
        if self.check(&TokenKind::NoneLit) {
            let (offset, length) = self.here();
            let hint = match op {
                CompareOp::Eq => "use `is None`",
                CompareOp::Ne => "use `is not None`",
            };
            return Err(FilterError::new(
                FilterErrorKind::unsupported_but("comparing to `None`", hint),
                offset,
                length,
            ));
        }
        let right = self.parse_operand()?;
        Ok(Expr::Compare { left, op, right })
    }

    /// `"is" ["not"] "None"`, the only thing `is` may be followed by here.
    fn finish_is_none(&mut self, left: Operand) -> Result<Expr, FilterError> {
        let negated = self.eat(&TokenKind::Not);
        if !self.eat(&TokenKind::NoneLit) {
            let (offset, length) = self.here();
            return Err(FilterError::new(
                FilterErrorKind::unsupported_but(
                    "`is` comparisons other than to `None`",
                    "use `==`",
                ),
                offset,
                length,
            ));
        }
        let Operand::Field(field) = left else {
            let (offset, length) = self.here();
            return Err(FilterError::new(
                FilterErrorKind::UnexpectedToken("a constant before `is None`".to_string()),
                offset,
                length,
            ));
        };
        Ok(Expr::IsNone { field, negated })
    }

    /// `operand := field | string | int | "True" | "False"`
    fn parse_operand(&mut self) -> Result<Operand, FilterError> {
        let Some(token) = self.peek() else {
            return Err(self.at_end(FilterErrorKind::UnexpectedEnd));
        };

        let operand = match &token.kind {
            TokenKind::Ident(name) => {
                if self.check_at(1, &TokenKind::LParen) {
                    return Err(FilterError::new(
                        FilterErrorKind::unsupported("function calls"),
                        token.offset,
                        token.length,
                    ));
                }
                let field = FieldName::new(name).map_err(|error| {
                    FilterError::new(
                        FilterErrorKind::IllegalFieldName(error.to_string()),
                        token.offset,
                        token.length,
                    )
                })?;
                Operand::Field(field)
            }
            TokenKind::Str(text) => Operand::Literal(Literal::Text(text.clone())),
            TokenKind::Int(number) => Operand::Literal(Literal::Int(*number)),
            TokenKind::True => Operand::Literal(Literal::Bool(true)),
            TokenKind::False => Operand::Literal(Literal::Bool(false)),
            _ => return Err(unexpected(token)),
        };

        self.at += 1;
        Ok(operand)
    }

    /// The token about to be read, if any.
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    /// Whether the next token is `kind`.
    fn check(&self, kind: &TokenKind) -> bool {
        self.check_at(0, kind)
    }

    /// Whether the token `ahead` positions on is `kind`.
    fn check_at(&self, ahead: usize, kind: &TokenKind) -> bool {
        self.tokens
            .get(self.at + ahead)
            .is_some_and(|token| &token.kind == kind)
    }

    /// Consumes the next token if it is `kind`, reporting whether it did.
    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    /// Where the next token sits, or the end of the input when there is none.
    fn here(&self) -> (usize, usize) {
        self.peek()
            .map_or((self.end.saturating_sub(1), 1), |token| {
                (token.offset, token.length)
            })
    }

    /// An error positioned at the end of the input.
    fn at_end(&self, kind: FilterErrorKind) -> FilterError {
        FilterError::new(kind, self.end.saturating_sub(1), 1)
    }
}

#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod grammar_tests;

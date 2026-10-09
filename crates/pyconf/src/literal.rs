//! Reading the value of an expression that is a literal — and refusing every
//! expression that is not.
//!
//! The grammar is the literal subset of Python a configuration file is
//! written in: strings (adjacent ones concatenated), numbers, `True`,
//! `False`, `None`, lists, tuples, dicts and sets of literals, a unary sign
//! on a number, and `+` joining two strings, two lists or two tuples. Any
//! other expression — a name, a call, an f-string, arithmetic — is not a
//! literal, and [`read_literal`] returns `None` rather than a guess.

use crate::position::Span;
use crate::token::{Token, TokenKind};
use crate::value::{Value, ValueKind};

/// How deeply containers may nest before the reader gives up, so a hostile
/// input cannot exhaust the stack.
const MAX_DEPTH: usize = 100;

/// The literal `tokens` spell — all of them, nothing left over — or `None`
/// when they are no literal.
#[must_use]
pub fn read_literal(tokens: &[Token]) -> Option<Value> {
    let mut reader = Reader {
        tokens,
        position: 0,
        depth: 0,
    };
    let value = reader.expression_list()?;
    (reader.position == tokens.len()).then_some(value)
}

struct Reader<'a> {
    tokens: &'a [Token],
    position: usize,
    depth: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<&TokenKind> {
        self.tokens.get(self.position).map(|token| &token.kind)
    }

    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.position)?;
        self.position += 1;
        Some(token)
    }

    /// Consumes the operator `op` if it is next.
    fn eat(&mut self, op: &str) -> Option<Span> {
        let token = self.tokens.get(self.position)?;
        token.kind.is_op(op).then(|| {
            self.position += 1;
            token.span
        })
    }

    /// The span of the token just consumed.
    fn last_span(&self) -> Span {
        self.tokens[self.position - 1].span
    }

    /// An expression, or a tuple of them written without parentheses
    /// (`x = 1, 2`).
    fn expression_list(&mut self) -> Option<Value> {
        let first = self.sum()?;
        if !self.peek().is_some_and(|kind| kind.is_op(",")) {
            return Some(first);
        }
        let start = first.span;
        let mut items = vec![first];
        while self.eat(",").is_some() {
            if self.peek().is_none() {
                break;
            }
            items.push(self.sum()?);
        }
        Some(Value {
            kind: ValueKind::Tuple(items),
            span: start.to(self.last_span()),
        })
    }

    /// Operands joined by `+`, which only joins two strings, two lists or
    /// two tuples.
    fn sum(&mut self) -> Option<Value> {
        let mut value = self.unary()?;
        while self.eat("+").is_some() {
            let right = self.unary()?;
            let span = value.span.to(right.span);
            let kind = match (value.kind, right.kind) {
                (ValueKind::Str(mut a), ValueKind::Str(b)) => {
                    a.push_str(&b);
                    ValueKind::Str(a)
                }
                (ValueKind::List(mut a), ValueKind::List(b)) => {
                    a.extend(b);
                    ValueKind::List(a)
                }
                (ValueKind::Tuple(mut a), ValueKind::Tuple(b)) => {
                    a.extend(b);
                    ValueKind::Tuple(a)
                }
                _ => return None,
            };
            value = Value { kind, span };
        }
        Some(value)
    }

    /// A signed number, or an atom.
    fn unary(&mut self) -> Option<Value> {
        let Some(sign) = self.eat("-").or_else(|| self.eat("+")) else {
            return self.atom();
        };
        let negate = self.tokens[self.position - 1].kind.is_op("-");
        let operand = self.nested(Self::unary)?;
        let kind = match (operand.kind, negate) {
            (kind @ (ValueKind::Int(_) | ValueKind::Float(_)), false) => kind,
            (ValueKind::Int(value), true) => ValueKind::Int(value.checked_neg()?),
            (ValueKind::Float(value), true) => ValueKind::Float(-value),
            _ => return None,
        };
        Some(Value {
            kind,
            span: sign.to(operand.span),
        })
    }

    fn atom(&mut self) -> Option<Value> {
        let token = self.next()?.clone();
        let kind = match &token.kind {
            TokenKind::Str(text) => return Some(self.strings(text.clone(), token.span)),
            TokenKind::Number(text) => number(text)?,
            TokenKind::Name(name) => match name.as_str() {
                "True" => ValueKind::Bool(true),
                "False" => ValueKind::Bool(false),
                "None" => ValueKind::None,
                _ => return None,
            },
            TokenKind::Op("(") => return self.nested(|reader| reader.parenthesized(token.span)),
            TokenKind::Op("[") => {
                return self.nested(|reader| {
                    let items = reader.items("]")?;
                    Some(Value {
                        kind: ValueKind::List(items),
                        span: token.span.to(reader.last_span()),
                    })
                });
            }
            TokenKind::Op("{") => return self.nested(|reader| reader.braced(token.span)),
            _ => return None,
        };
        Some(Value {
            kind,
            span: token.span,
        })
    }

    /// Runs `read` one container deeper, refusing past [`MAX_DEPTH`].
    fn nested(&mut self, read: impl FnOnce(&mut Self) -> Option<Value>) -> Option<Value> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        let value = read(self);
        self.depth -= 1;
        value
    }

    /// A string followed by any adjacent strings, which Python joins.
    fn strings(&mut self, mut text: String, start: Span) -> Value {
        let mut span = start;
        while let Some(TokenKind::Str(next)) = self.peek() {
            text.push_str(next);
            span = start.to(self.tokens[self.position].span);
            self.position += 1;
        }
        Value {
            kind: ValueKind::Str(text),
            span,
        }
    }

    /// What follows a `(`: an empty tuple, a parenthesized expression, or a
    /// tuple.
    fn parenthesized(&mut self, open: Span) -> Option<Value> {
        if let Some(close) = self.eat(")") {
            return Some(Value {
                kind: ValueKind::Tuple(Vec::new()),
                span: open.to(close),
            });
        }
        let first = self.sum()?;
        if let Some(close) = self.eat(")") {
            // A parenthesized expression is the expression itself, written
            // over the parentheses too.
            return Some(Value {
                kind: first.kind,
                span: open.to(close),
            });
        }
        self.eat(",")?;
        let mut items = vec![first];
        items.extend(self.items(")")?);
        Some(Value {
            kind: ValueKind::Tuple(items),
            span: open.to(self.last_span()),
        })
    }

    /// Comma-separated expressions up to and including `close`, a trailing
    /// comma allowed.
    fn items(&mut self, close: &str) -> Option<Vec<Value>> {
        let mut items = Vec::new();
        loop {
            if self.eat(close).is_some() {
                return Some(items);
            }
            items.push(self.sum()?);
            if self.eat(",").is_none() {
                self.eat(close)?;
                return Some(items);
            }
        }
    }

    /// What follows a `{`: a dict, or a set.
    fn braced(&mut self, open: Span) -> Option<Value> {
        if let Some(close) = self.eat("}") {
            return Some(Value {
                kind: ValueKind::Dict(Vec::new()),
                span: open.to(close),
            });
        }
        let first = self.sum()?;
        if self.eat(":").is_none() {
            let mut elements = vec![first];
            if self.eat(",").is_some() {
                elements.extend(self.items("}")?);
            } else {
                self.eat("}")?;
            }
            let mut unique: Vec<Value> = Vec::new();
            for element in elements {
                if !unique.iter().any(|seen| seen.same_value(&element)) {
                    unique.push(element);
                }
            }
            return Some(Value {
                kind: ValueKind::Set(unique),
                span: open.to(self.last_span()),
            });
        }
        let mut entries: Vec<(Value, Value)> = Vec::new();
        let mut key = first;
        loop {
            let value = self.sum()?;
            match entries.iter_mut().find(|(seen, _)| seen.same_value(&key)) {
                Some(entry) => entry.1 = value,
                None => entries.push((key, value)),
            }
            if self.eat(",").is_none() {
                self.eat("}")?;
                break;
            }
            if self.eat("}").is_some() {
                break;
            }
            key = self.sum()?;
            self.eat(":")?;
        }
        Some(Value {
            kind: ValueKind::Dict(entries),
            span: open.to(self.last_span()),
        })
    }
}

/// The value of the number written `text`, or `None` for one this does not
/// read: an imaginary number, or an integer past 64 bits.
fn number(text: &str) -> Option<ValueKind> {
    let digits = text.replace('_', "");
    let lower = digits.to_ascii_lowercase();
    let radix = match lower.get(..2) {
        Some("0x") => Some(16),
        Some("0o") => Some(8),
        Some("0b") => Some(2),
        _ => None,
    };
    if let Some(radix) = radix {
        return i64::from_str_radix(&lower[2..], radix)
            .ok()
            .map(ValueKind::Int);
    }
    if lower.ends_with('j') {
        return None;
    }
    if lower.contains(['.', 'e']) {
        return lower.parse::<f64>().ok().map(ValueKind::Float);
    }
    lower.parse::<i64>().ok().map(ValueKind::Int)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::position::Position;

    /// The literal `source` spells, read as the right-hand side of an
    /// assignment is: its tokens without the final newline.
    fn read(source: &str) -> Option<Value> {
        let mut tokens = tokenize(source).tokens;
        if tokens
            .last()
            .is_some_and(|token| token.kind == TokenKind::Newline)
        {
            tokens.pop();
        }
        read_literal(&tokens)
    }

    fn kind(source: &str) -> Option<ValueKind> {
        read(source).map(|value| value.kind)
    }

    fn strings(values: &[&str]) -> Vec<ValueKind> {
        values
            .iter()
            .map(|value| ValueKind::Str((*value).to_string()))
            .collect()
    }

    fn kinds(value: &ValueKind) -> Vec<ValueKind> {
        match value {
            ValueKind::List(items) | ValueKind::Tuple(items) | ValueKind::Set(items) => {
                items.iter().map(|item| item.kind.clone()).collect()
            }
            _ => panic!("not a sequence: {value:?}"),
        }
    }

    #[test]
    fn test_read_literal_reads_scalars() {
        // When / Then
        assert_eq!(kind("'a'"), Some(ValueKind::Str("a".into())));
        assert_eq!(kind("42"), Some(ValueKind::Int(42)));
        assert_eq!(kind("1.5"), Some(ValueKind::Float(1.5)));
        assert_eq!(kind("True"), Some(ValueKind::Bool(true)));
        assert_eq!(kind("False"), Some(ValueKind::Bool(false)));
        assert_eq!(kind("None"), Some(ValueKind::None));
    }

    #[test]
    fn test_read_literal_reads_every_integer_spelling() {
        // When / Then
        assert_eq!(kind("1_000"), Some(ValueKind::Int(1000)));
        assert_eq!(kind("0xFF"), Some(ValueKind::Int(255)));
        assert_eq!(kind("0o17"), Some(ValueKind::Int(15)));
        assert_eq!(kind("0b101"), Some(ValueKind::Int(5)));
        assert_eq!(kind("1e3"), Some(ValueKind::Float(1000.0)));
        assert_eq!(kind(".5"), Some(ValueKind::Float(0.5)));
    }

    #[test]
    fn test_read_literal_refuses_numbers_it_cannot_hold() {
        // When / Then
        assert_eq!(kind("3j"), None);
        assert_eq!(kind("99999999999999999999"), None);
        assert_eq!(kind("0xZZ"), None);
    }

    #[test]
    fn test_read_literal_applies_a_sign_to_a_number_only() {
        // When / Then
        assert_eq!(kind("-1"), Some(ValueKind::Int(-1)));
        assert_eq!(kind("- -2.5"), Some(ValueKind::Float(2.5)));
        assert_eq!(kind("+3"), Some(ValueKind::Int(3)));
        assert_eq!(kind("-'a'"), None);
        assert_eq!(kind("+'a'"), None);
    }

    #[test]
    fn test_read_literal_joins_adjacent_strings_and_added_ones() {
        // When / Then
        assert_eq!(kind("'a' 'b' \"c\""), Some(ValueKind::Str("abc".into())));
        assert_eq!(kind("'a' + 'b'"), Some(ValueKind::Str("ab".into())));
        assert_eq!(kind("('a'\n 'b')"), Some(ValueKind::Str("ab".into())));
    }

    #[test]
    fn test_read_literal_joins_added_lists_and_tuples() {
        // When
        let list = kind("['a'] + ['b']").expect("a literal");
        let tuple = kind("('a',) + ('b',)").expect("a literal");

        // Then
        assert_eq!(kinds(&list), strings(&["a", "b"]));
        assert_eq!(kinds(&tuple), strings(&["a", "b"]));
        assert_eq!(kind("['a'] + ('b',)"), None);
        assert_eq!(kind("1 + 2"), None);
    }

    #[test]
    fn test_read_literal_reads_lists_with_or_without_a_trailing_comma() {
        // When / Then
        assert_eq!(kinds(&kind("[]").expect("a literal")), []);
        assert_eq!(
            kinds(&kind("['a', 'b']").expect("a literal")),
            strings(&["a", "b"])
        );
        assert_eq!(
            kinds(&kind("['a', 'b',]").expect("a literal")),
            strings(&["a", "b"])
        );
    }

    #[test]
    fn test_read_literal_tells_tuples_from_parentheses() {
        // When / Then
        assert_eq!(kinds(&kind("()").expect("a literal")), []);
        assert_eq!(kind("('a')"), Some(ValueKind::Str("a".into())));
        assert_eq!(kinds(&kind("('a',)").expect("a literal")), strings(&["a"]));
        assert_eq!(
            kinds(&kind("('a', 'b')").expect("a literal")),
            strings(&["a", "b"])
        );
        assert_eq!(
            kinds(&kind("'a', 'b'").expect("a literal")),
            strings(&["a", "b"])
        );
        assert_eq!(kinds(&kind("'a',").expect("a literal")), strings(&["a"]));
    }

    #[test]
    fn test_read_literal_reads_a_dict_keeping_a_repeated_keys_last_value() {
        // When
        let dict = kind("{'a': 1, 'b': [2], 'a': 3,}").expect("a literal");

        // Then
        let ValueKind::Dict(entries) = dict else {
            panic!("not a dict: {dict:?}");
        };
        let entries: Vec<(ValueKind, ValueKind)> = entries
            .into_iter()
            .map(|(key, value)| (key.kind, value.kind))
            .collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], (ValueKind::Str("a".into()), ValueKind::Int(3)));
        assert_eq!(entries[1].0, ValueKind::Str("b".into()));
        assert_eq!(kind("{}"), Some(ValueKind::Dict(Vec::new())));
    }

    #[test]
    fn test_read_literal_reads_a_set_keeping_each_element_once() {
        // When / Then
        assert_eq!(
            kinds(&kind("{'a', 'b', 'a'}").expect("a literal")),
            strings(&["a", "b"])
        );
        assert_eq!(kinds(&kind("{'a'}").expect("a literal")), strings(&["a"]));
    }

    #[test]
    fn test_read_literal_reads_nested_containers() {
        // When
        let value = kind("[('sphinx', 'https://x', None), {'k': {'n': 1}}]");

        // Then
        assert!(matches!(value, Some(ValueKind::List(items)) if items.len() == 2));
    }

    #[test]
    fn test_read_literal_refuses_every_non_literal() {
        // When / Then
        for source in [
            "version",
            "f'{x}'",
            "b'x'",
            "os.getenv('X')",
            "[x for x in y]",
            "'a' * 2",
            "'a' if x else 'b'",
            "{'a': x}",
            "{**base}",
            "['a', *rest]",
            "lambda: 1",
            "'a'.upper()",
            "1 2",
            "[1, 2",
            "{'a': 1",
            "{'a' 1}",
            "{'a', 1",
            "(1, 2",
            "(1 2)",
            "",
        ] {
            assert_eq!(kind(source), None, "{source}");
        }
    }

    #[test]
    fn test_read_literal_refuses_containers_nested_without_end() {
        // Given
        let source = format!("{}{}", "[".repeat(500), "]".repeat(500));

        // When / Then
        assert_eq!(kind(&source), None);
    }

    #[test]
    fn test_read_literal_records_where_each_value_was_written() {
        // When
        let value = read("[\n  'a',\n  'b' 'c',\n]").expect("a literal");

        // Then
        let at = |line, column| Position { line, column };
        assert_eq!(value.span.start, at(1, 1));
        assert_eq!(value.span.end, at(4, 2));
        let items = value.as_sequence().expect("a list");
        assert_eq!(
            (items[0].span.start, items[0].span.end),
            (at(2, 3), at(2, 6))
        );
        assert_eq!(
            (items[1].span.start, items[1].span.end),
            (at(3, 3), at(3, 10))
        );
    }

    #[test]
    fn test_read_literal_spans_a_parenthesized_value_and_a_signed_number() {
        // When
        let parenthesized = read("('a')").expect("a literal");
        let signed = read("-1").expect("a literal");

        // Then
        assert_eq!(parenthesized.span.end.column, 6);
        assert_eq!((signed.span.start.column, signed.span.end.column), (1, 3));
    }

    #[test]
    fn test_read_literal_negates_the_largest_integers() {
        // When / Then
        assert_eq!(
            kind("-9223372036854775807"),
            Some(ValueKind::Int(-9_223_372_036_854_775_807))
        );
        assert_eq!(
            kind("--9223372036854775807"),
            Some(ValueKind::Int(9_223_372_036_854_775_807))
        );
    }
}

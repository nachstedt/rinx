//! The values a literal can spell, each with where it was written.

use crate::position::Span;

/// A literal value and the source range it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub kind: ValueKind,
    pub span: Span,
}

/// What a literal is: the subset of Python's `ast.literal_eval` a
/// configuration file is written in.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    None,
    List(Vec<Value>),
    Tuple(Vec<Value>),
    /// The entries in the order written, a repeated key keeping its last
    /// value at its first key's place, as a Python `dict` does.
    Dict(Vec<(Value, Value)>),
    /// The elements in the order written, each once.
    Set(Vec<Value>),
}

impl Value {
    /// The string this value is, if it is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            ValueKind::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The elements of a list or a tuple — the two sequences a setting
    /// documented as a list is written as.
    #[must_use]
    pub fn as_sequence(&self) -> Option<&[Self]> {
        match &self.kind {
            ValueKind::List(items) | ValueKind::Tuple(items) => Some(items),
            _ => None,
        }
    }

    /// Whether `self` and `other` are the same Python value, wherever each
    /// was written. `True` and `1` are, since Python's dictionaries and sets
    /// treat them as one key; `1` and `1.0` are not told apart from two
    /// different keys, which no configuration this reads relies on.
    #[must_use]
    pub fn same_value(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (ValueKind::Str(a), ValueKind::Str(b)) => a == b,
            (ValueKind::None, ValueKind::None) => true,
            (ValueKind::List(a), ValueKind::List(b))
            | (ValueKind::Tuple(a), ValueKind::Tuple(b)) => same_values(a, b),
            (ValueKind::Dict(a), ValueKind::Dict(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((ka, va), (kb, vb))| ka.same_value(kb) && va.same_value(vb))
            }
            (ValueKind::Set(a), ValueKind::Set(b)) => {
                a.len() == b.len() && a.iter().all(|x| b.iter().any(|y| x.same_value(y)))
            }
            (ValueKind::Float(a), ValueKind::Float(b)) => a.total_cmp(b).is_eq(),
            (a, b) => match (integer(a), integer(b)) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            },
        }
    }
}

fn same_values(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same_value(y))
}

/// The integer a boolean or an integer is, for comparing them.
fn integer(kind: &ValueKind) -> Option<i64> {
    match kind {
        ValueKind::Int(value) => Some(*value),
        ValueKind::Bool(value) => Some(i64::from(*value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::Position;

    fn value(kind: ValueKind, column: usize) -> Value {
        Value {
            kind,
            span: Span {
                start: Position { line: 1, column },
                end: Position {
                    line: 1,
                    column: column + 1,
                },
            },
        }
    }

    #[test]
    fn test_as_str_reads_only_a_string() {
        // When / Then
        assert_eq!(value(ValueKind::Str("a".into()), 1).as_str(), Some("a"));
        assert_eq!(value(ValueKind::Int(1), 1).as_str(), None);
    }

    #[test]
    fn test_as_sequence_reads_a_list_or_a_tuple() {
        // Given
        let items = vec![value(ValueKind::Int(1), 2)];

        // When / Then
        assert_eq!(
            value(ValueKind::List(items.clone()), 1).as_sequence(),
            Some(items.as_slice())
        );
        assert_eq!(
            value(ValueKind::Tuple(items.clone()), 1).as_sequence(),
            Some(items.as_slice())
        );
        assert_eq!(value(ValueKind::Set(items), 1).as_sequence(), None);
    }

    #[test]
    fn test_same_value_ignores_where_a_value_was_written() {
        // When / Then
        assert!(
            value(ValueKind::Str("a".into()), 1).same_value(&value(ValueKind::Str("a".into()), 9))
        );
        assert!(
            !value(ValueKind::Str("a".into()), 1).same_value(&value(ValueKind::Str("b".into()), 1))
        );
    }

    #[test]
    fn test_same_value_treats_equal_numbers_as_one_key() {
        // When / Then
        assert!(value(ValueKind::Float(0.5), 1).same_value(&value(ValueKind::Float(0.5), 1)));
        assert!(value(ValueKind::Bool(true), 1).same_value(&value(ValueKind::Int(1), 1)));
        assert!(!value(ValueKind::Int(1), 1).same_value(&value(ValueKind::Str("1".into()), 1)));
        assert!(value(ValueKind::None, 1).same_value(&value(ValueKind::None, 2)));
    }

    #[test]
    fn test_same_value_compares_containers_element_by_element() {
        // Given
        let one = || value(ValueKind::Int(1), 1);
        let two = || value(ValueKind::Int(2), 1);

        // When / Then
        assert!(
            value(ValueKind::Tuple(vec![one(), two()]), 1)
                .same_value(&value(ValueKind::Tuple(vec![one(), two()]), 5))
        );
        assert!(
            !value(ValueKind::List(vec![one()]), 1)
                .same_value(&value(ValueKind::Tuple(vec![one()]), 1))
        );
        assert!(
            value(ValueKind::Set(vec![one(), two()]), 1)
                .same_value(&value(ValueKind::Set(vec![two(), one()]), 1))
        );
        assert!(
            value(ValueKind::Dict(vec![(one(), two())]), 1)
                .same_value(&value(ValueKind::Dict(vec![(one(), two())]), 1))
        );
        assert!(
            !value(ValueKind::Dict(vec![(one(), two())]), 1)
                .same_value(&value(ValueKind::Dict(vec![(two(), one())]), 1))
        );
    }
}

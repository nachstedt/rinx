use serde::{Deserialize, Serialize};

use crate::field_name::FieldName;
use crate::value::Literal;

/// One side of a comparison: either a field of the subject, or a constant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operand {
    Field(FieldName),
    Literal(Literal),
}

/// The two equality operators this grammar has.
///
/// Ordering (`<`, `>=`) is deliberately absent: no real filter in the corpus
/// orders, and admitting it would mean deciding how text compares to a number,
/// which is a question this language has no reason to answer. Writing one is
/// diagnosed by name rather than silently misparsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Eq,
    Ne,
}

/// A parsed filter expression.
///
/// Serialized into the `.ast`, because the filter is parsed while the document
/// is — that is the only phase holding the source position of the option line
/// it was written on, so a syntax error is reported at the column it breaks at
/// rather than against the document as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Compare {
        left: Operand,
        op: CompareOp,
        right: Operand,
    },
    /// `needle in haystack`, or `needle not in haystack` when negated.
    Contains {
        needle: Operand,
        haystack: Operand,
        negated: bool,
    },
    /// `field is None`, or `field is not None` when negated.
    ///
    /// The left side is a [`FieldName`] rather than an [`Operand`] because a
    /// constant is never `None`: `"x" is None` is answerable but meaningless,
    /// so it is unrepresentable instead.
    IsNone {
        field: FieldName,
        negated: bool,
    },
    /// A bare operand, true when its value is truthy.
    Truthy(Operand),
}

impl Expr {
    /// Every field name this expression reads, in the order encountered.
    ///
    /// What the caller validates against its own vocabulary — the parser
    /// checks each one against the entity schema, so an unknown field is
    /// reported where it was written rather than silently matching nothing.
    /// Names may repeat; the caller decides whether that matters.
    #[must_use]
    pub fn field_names(&self) -> Vec<&FieldName> {
        let mut names = Vec::new();
        self.collect_field_names(&mut names);
        names
    }

    /// Appends this expression's field names to `names`, depth first.
    fn collect_field_names<'a>(&'a self, names: &mut Vec<&'a FieldName>) {
        match self {
            Self::And(left, right) | Self::Or(left, right) => {
                left.collect_field_names(names);
                right.collect_field_names(names);
            }
            Self::Not(inner) => inner.collect_field_names(names),
            Self::Compare { left, right, .. } => {
                push_operand_name(left, names);
                push_operand_name(right, names);
            }
            Self::Contains {
                needle, haystack, ..
            } => {
                push_operand_name(needle, names);
                push_operand_name(haystack, names);
            }
            Self::IsNone { field, .. } => names.push(field),
            Self::Truthy(operand) => push_operand_name(operand, names),
        }
    }
}

/// Appends `operand`'s field name to `names`, if it names one at all.
fn push_operand_name<'a>(operand: &'a Operand, names: &mut Vec<&'a FieldName>) {
    if let Operand::Field(name) = operand {
        names.push(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str) -> Operand {
        Operand::Field(FieldName::new(name).unwrap())
    }

    fn text(value: &str) -> Operand {
        Operand::Literal(Literal::Text(value.to_string()))
    }

    #[test]
    fn test_field_names_finds_both_sides_of_a_comparison() {
        // Given
        let expr = Expr::Compare {
            left: field("status"),
            op: CompareOp::Eq,
            right: field("owner"),
        };

        // When
        let names = expr.field_names();

        // Then
        assert_eq!(names_as_strings(&names), ["status", "owner"]);
    }

    #[test]
    fn test_field_names_ignores_literals() {
        // Given
        let expr = Expr::Compare {
            left: field("status"),
            op: CompareOp::Eq,
            right: text("open"),
        };

        // When
        let names = expr.field_names();

        // Then
        assert_eq!(names_as_strings(&names), ["status"]);
    }

    #[test]
    fn test_field_names_descends_through_and_or_and_not() {
        // Given — the shape of the longest filter in the benchmark corpus
        let expr = Expr::And(
            Box::new(Expr::Compare {
                left: field("type"),
                op: CompareOp::Eq,
                right: text("fsr"),
            }),
            Box::new(Expr::Not(Box::new(Expr::Or(
                Box::new(Expr::Contains {
                    needle: text("Process"),
                    haystack: field("title"),
                    negated: false,
                }),
                Box::new(Expr::Truthy(field("asil"))),
            )))),
        );

        // When
        let names = expr.field_names();

        // Then
        assert_eq!(names_as_strings(&names), ["type", "title", "asil"]);
    }

    #[test]
    fn test_field_names_includes_an_is_none_subject() {
        // Given
        let expr = Expr::IsNone {
            field: FieldName::new("docname").unwrap(),
            negated: true,
        };

        // When
        let names = expr.field_names();

        // Then
        assert_eq!(names_as_strings(&names), ["docname"]);
    }

    #[test]
    fn test_field_names_keeps_repeats() {
        // Given — the caller decides whether a repeat matters
        let expr = Expr::Or(
            Box::new(Expr::Truthy(field("tags"))),
            Box::new(Expr::Truthy(field("tags"))),
        );

        // When
        let names = expr.field_names();

        // Then
        assert_eq!(names_as_strings(&names), ["tags", "tags"]);
    }

    #[test]
    fn test_an_expression_survives_a_serialization_round_trip() {
        // Given — an `Expr` is stored in the `.ast` and read back to render
        let expr = Expr::Contains {
            needle: text("safety"),
            haystack: field("docname"),
            negated: true,
        };

        // When
        let json = serde_json::to_string(&expr).unwrap();
        let decoded: Expr = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, expr);
    }

    fn names_as_strings(names: &[&FieldName]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }
}

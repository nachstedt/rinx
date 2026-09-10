use crate::expr::{CompareOp, Expr, Operand};
use crate::field_name::FieldName;
use crate::value::{FieldValue, Literal};

/// Whatever a filter is being evaluated against.
///
/// The seam that keeps this crate independent of the index it filters: the
/// renderer implements this over an entity record, and nothing here learns
/// what an entity is. A field the subject does not have is
/// [`FieldValue::Missing`], never an error — one table may span types that
/// declare different attributes, so a filter naming one of them must simply
/// not match the others.
pub trait FilterSubject {
    /// What `name` is worth on this subject.
    fn field(&self, name: &FieldName) -> FieldValue;
}

impl Expr {
    /// Whether `subject` satisfies this expression.
    ///
    /// Total: every operator has an answer for every value, including
    /// [`FieldValue::Missing`], so evaluation cannot fail once parsing has
    /// succeeded.
    pub fn matches(&self, subject: &impl FilterSubject) -> bool {
        match self {
            Self::And(left, right) => left.matches(subject) && right.matches(subject),
            Self::Or(left, right) => left.matches(subject) || right.matches(subject),
            Self::Not(inner) => !inner.matches(subject),
            Self::Compare { left, op, right } => {
                let equal = operands_are_equal(left, right, subject);
                match op {
                    CompareOp::Eq => equal,
                    CompareOp::Ne => !equal,
                }
            }
            Self::Contains {
                needle,
                haystack,
                negated,
            } => {
                let found = haystack_contains(needle, haystack, subject);
                found != *negated
            }
            Self::IsNone { field, negated } => {
                let missing = subject.field(field) == FieldValue::Missing;
                missing != *negated
            }
            Self::Truthy(operand) => resolve(operand, subject).is_truthy(),
        }
    }
}

/// An operand's value: a field's, or the constant itself lifted into one.
fn resolve(operand: &Operand, subject: &impl FilterSubject) -> FieldValue {
    match operand {
        Operand::Field(name) => subject.field(name),
        Operand::Literal(Literal::Text(text)) => FieldValue::Text(text.clone()),
        Operand::Literal(Literal::Int(number)) => FieldValue::Int(*number),
        Operand::Literal(Literal::Bool(flag)) => FieldValue::Bool(*flag),
    }
}

/// Whether two operands are equal.
///
/// A missing field equals nothing, including another missing field: two
/// entities that both lack a `status` are not thereby "the same status". That
/// is Python's rule for `None == None` inverted deliberately — there, `None`
/// is a value; here it is an absence, and `status == owner` should not match
/// every entity that has neither.
fn operands_are_equal(left: &Operand, right: &Operand, subject: &impl FilterSubject) -> bool {
    let left_value = resolve(left, subject);
    let right_value = resolve(right, subject);
    match (&left_value, &right_value) {
        (FieldValue::Missing, _) | (_, FieldValue::Missing) => false,
        (_, FieldValue::Text(text)) => left_value.equals(&Literal::Text(text.clone())),
        (_, FieldValue::Int(number)) => left_value.equals(&Literal::Int(*number)),
        (_, FieldValue::Bool(flag)) => left_value.equals(&Literal::Bool(*flag)),
        (_, FieldValue::List(items)) => {
            matches!(&left_value, FieldValue::List(left_items) if left_items == items)
        }
    }
}

/// Whether `haystack` contains `needle`.
fn haystack_contains(needle: &Operand, haystack: &Operand, subject: &impl FilterSubject) -> bool {
    let Some(text) = resolve(needle, subject).as_text() else {
        return false;
    };
    resolve(haystack, subject).contains(&text)
}

#[cfg(test)]
mod tests;

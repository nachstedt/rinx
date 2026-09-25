use serde::{Deserialize, Serialize};

use crate::span::Span;

/// The four operations an `.. entity-update::` option line spells with a
/// `+`/`-` prefix. Kept as one enum with the value folded in (rather than a
/// 3-mode enum plus a separate `Option<String>`) so `-field:` (clear) and
/// `-field: value` (remove one) are distinct variants an exhaustive match
/// must handle, instead of one operation with an optional argument that
/// silently means two different things.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldMutationMode {
    /// `field: value` — overwrite the effective value.
    Set(String),
    /// `+field: value` — append to a list-valued attribute or a relation's
    /// target list. An error at apply time on a scalar field.
    Append(String),
    /// `-field: value` — remove that value from a list-valued field or
    /// relation. An error at apply time on a scalar field.
    Remove(String),
    /// `-field:`, written with no value — clear the field entirely, whatever
    /// its type.
    Clear,
}

/// One field mutation an `.. entity-update::` directive applies to every
/// entity its target resolves to.
///
/// The value is kept as raw, unconverted text (folded into [`FieldMutationMode`]):
/// the matched entity's declared type — and so the [`rinx_entity::AttributeType`]
/// the text must fit — is only known once a specific entity is matched, at
/// apply time, and a filter target may match several types at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMutation {
    /// The option spelling with any `+`/`-` prefix already stripped.
    pub field: String,
    pub mode: FieldMutationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_mutation_survives_a_serialization_round_trip() {
        // Given
        let mutation = FieldMutation {
            field: "tags".to_string(),
            mode: FieldMutationMode::Append("safety-critical".to_string()),
            span: None,
        };

        // When
        let json = serde_json::to_string(&mutation).unwrap();
        let decoded: FieldMutation = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, mutation);
    }

    #[test]
    fn test_clear_and_remove_are_distinct_variants() {
        // Given / When / Then — `-field:` and `-field: value` must never
        // collapse into "remove with an optional value"
        assert_ne!(
            FieldMutationMode::Clear,
            FieldMutationMode::Remove(String::new())
        );
    }
}

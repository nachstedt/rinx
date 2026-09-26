use rinx_filter::Expr;
use serde::{Deserialize, Serialize};

use crate::entity::EntityId;

/// What `.. entity-update::`'s argument resolves to, once the whole project's
/// entities are known — never here, since one document's parser never sees
/// them.
///
/// Both fields may be populated at once (an argument that is both a legal id
/// spelling and a legal bare-field filter, e.g. `REQ_001`, which also parses
/// as `Truthy(Field("REQ_001"))`). Apply time always prefers `candidate_id`
/// when it names an entity that actually exists, exactly as upstream
/// sphinx-needs disambiguates — see `docs/decisions/019-entity-update.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateTarget {
    /// `Some` when the argument text is a legal [`EntityId`] spelling — *not*
    /// whether an entity with that id exists; that can only be checked once
    /// the project is merged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<EntityId>,
    /// `Some` when the argument parsed as a filter expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Expr>,
    /// The argument exactly as written, trimmed. Kept even when both fields
    /// above are `Some`, and even when both are `None`, so a diagnostic can
    /// always quote what the author wrote.
    pub raw: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_target_survives_a_serialization_round_trip() {
        // Given
        let target = UpdateTarget {
            candidate_id: Some(EntityId::new("REQ_001").unwrap()),
            filter: Some(rinx_filter::parse_filter("REQ_001").unwrap()),
            raw: "REQ_001".to_string(),
        };

        // When
        let json = serde_json::to_string(&target).unwrap();
        let decoded: UpdateTarget = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, target);
    }

    #[test]
    fn test_absent_readings_are_left_out_of_the_serialized_form() {
        // Given — a target whose argument parsed as neither reading
        let target = UpdateTarget {
            candidate_id: None,
            filter: None,
            raw: "???".to_string(),
        };

        // When
        let json = serde_json::to_string(&target).unwrap();

        // Then
        assert!(!json.contains("candidate_id"));
        assert!(!json.contains("filter"));
    }
}

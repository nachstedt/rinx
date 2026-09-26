use std::collections::BTreeMap;

use rinx_ast::{AttributeValue, EntityId, FieldMutationMode};
use serde::{Deserialize, Serialize};

/// One entity's full update history — present only for an entity at least
/// one `.. entity-update::`/`.. needextend::` touched. Absence means "never
/// touched": the entity's own [`crate::EntityRecord`] is already the
/// effective value, which is exactly what
/// [`crate::ProjectIndex::effective_attribute`]/
/// [`crate::ProjectIndex::effective_relation_targets`] fall back to.
///
/// Never merged, always recomputed globally from the whole graph by
/// `rinx_analyzer::apply_entity_updates` — exactly like
/// [`crate::ProjectIndex::entity_backlinks`] — and, crucially,
/// [`crate::EntityRecord`] itself is never mutated: this is a derived overlay
/// beside the as-authored graph, not a rewrite of it. See
/// `docs/decisions/019-entity-update.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityFieldHistory {
    #[serde(default)]
    pub attributes: BTreeMap<String, AttributeFieldHistory>,
    #[serde(default)]
    pub relations: BTreeMap<String, RelationFieldHistory>,
}

/// One attribute's history on one entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeFieldHistory {
    /// The entity's own value the first time any update touched this field —
    /// snapshotted once, never re-derived, so it survives however many
    /// updates follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<AttributeValue>,
    /// Every applied change, oldest first.
    pub applied: Vec<AppliedFieldUpdate>,
    /// The value after every applied change. `None` means cleared, distinct
    /// from "untouched" (represented by this struct's absence from
    /// [`EntityFieldHistory::attributes`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<AttributeValue>,
}

/// One relation's history on one entity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationFieldHistory {
    #[serde(default)]
    pub original: Vec<EntityId>,
    pub applied: Vec<AppliedRelationUpdate>,
    #[serde(default)]
    pub current: Vec<EntityId>,
}

/// One applied change to one attribute — the audit-trail entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedFieldUpdate {
    /// Position in the (now-sorted) [`crate::ProjectIndex::entity_updates`],
    /// so the full directive — including its justification body — is a plain
    /// index away rather than a second lookup key.
    pub update_index: usize,
    /// The document that wrote the directive — where a `.. noqa:` for a
    /// diagnostic about this entry would go.
    pub doc_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<rinx_ast::Span>,
    pub mode: FieldMutationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resulting_value: Option<AttributeValue>,
    /// `Some(earlier_update_index)` when this entry is a `Set`/`Clear` that
    /// overwrote a *different* value a different directive had already
    /// established. Never set for `Append`/`Remove`, and never set when the
    /// overwritten value agreed. Bookkeeping only — a side effect of the
    /// deterministic application order, not a precedence claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflicts_with: Option<usize>,
}

/// One applied change to one relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedRelationUpdate {
    pub update_index: usize,
    pub doc_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<rinx_ast::Span>,
    pub mode: FieldMutationMode,
    pub resulting_targets: Vec<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflicts_with: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_field_history_survives_a_serialization_round_trip() {
        // Given
        let mut history = EntityFieldHistory::default();
        history.attributes.insert(
            "status".to_string(),
            AttributeFieldHistory {
                original: Some(AttributeValue::String("open".to_string())),
                applied: vec![AppliedFieldUpdate {
                    update_index: 0,
                    doc_path: "specs/boot.rst".to_string(),
                    span: None,
                    mode: FieldMutationMode::Set("closed".to_string()),
                    resulting_value: Some(AttributeValue::String("closed".to_string())),
                    conflicts_with: None,
                }],
                current: Some(AttributeValue::String("closed".to_string())),
            },
        );

        // When
        let json = serde_json::to_string(&history).unwrap();
        let restored: EntityFieldHistory = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(restored, history);
    }

    #[test]
    fn test_a_default_history_has_nothing_in_either_map() {
        // Given / When
        let history = EntityFieldHistory::default();

        // Then
        assert!(history.attributes.is_empty());
        assert!(history.relations.is_empty());
    }
}

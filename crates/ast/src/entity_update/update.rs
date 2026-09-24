use serde::{Deserialize, Serialize};

use crate::entity_update::mutation::FieldMutation;
use crate::entity_update::source::EntityUpdateSource;
use crate::entity_update::target::UpdateTarget;
use crate::node::Node;
use crate::span::Span;

/// `.. entity-update::`, and its sphinx-needs spelling `.. needextend::` — a
/// project-wide mutation of one or many entities' fields.
///
/// The node carries a *question with intended effects*, not authored data:
/// which entities to touch and how, resolved and applied against the merged
/// project by `rusty_sphinx_analyzer::apply_entity_updates`, never here — the
/// entities this directive touches may be declared in documents this one has
/// never heard of, and only the project index knows them all. Applying it
/// never mutates an entity's own record: it builds a separate, traceable
/// history beside it — see `docs/decisions/019-entity-update.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUpdate {
    /// Which of the directive's two names was written.
    pub source: EntityUpdateSource,
    /// The argument, both its possible readings.
    pub target: UpdateTarget,
    /// One entry per option line other than `:strict:`, in the order written
    /// — the order later updates from other directives, and later fields of
    /// this same directive, are applied in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldMutation>,
    /// `:strict:` — whether a target matching no entity is reported. Defaults
    /// to `true`, matching sphinx-needs' own default.
    pub strict: bool,
    /// The directive's content: ordinary RST prose justifying the change.
    /// Parsed like any container directive's body — see
    /// `docs/decisions/019-entity-update.md` for why sections (unlike this
    /// body) are explicitly out of scope for what a mutation may touch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<Node>,
    /// Where the directive was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl EntityUpdate {
    /// A bare update with no fields, no body and `:strict:` at its default.
    #[must_use]
    pub fn new(source: EntityUpdateSource, target: UpdateTarget) -> Self {
        Self {
            source,
            target,
            fields: Vec::new(),
            strict: true,
            body: Vec::new(),
            span: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityId;
    use crate::entity_update::mutation::FieldMutationMode;

    fn target() -> UpdateTarget {
        UpdateTarget {
            candidate_id: Some(EntityId::new("REQ_001").unwrap()),
            filter: None,
            raw: "REQ_001".to_string(),
        }
    }

    #[test]
    fn test_new_defaults_strict_to_true_and_carries_nothing_else() {
        // Given
        let update = EntityUpdate::new(EntityUpdateSource::EntityUpdate, target());

        // When / Then
        assert!(update.strict);
        assert!(update.fields.is_empty());
        assert!(update.body.is_empty());
        assert_eq!(update.span, None);
    }

    #[test]
    fn test_an_update_survives_a_serialization_round_trip() {
        // Given
        let mut update = EntityUpdate::new(EntityUpdateSource::NeedExtend, target());
        update.fields.push(FieldMutation {
            field: "status".to_string(),
            mode: FieldMutationMode::Set("closed".to_string()),
            span: None,
        });
        update.strict = false;
        update.body = vec![crate::node::Node::Paragraph(vec![
            crate::inline_node::InlineNode::Text("Closed after review.".to_string()),
        ])];

        // When
        let json = serde_json::to_string(&update).unwrap();
        let decoded: EntityUpdate = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, update);
    }

    #[test]
    fn test_absent_fields_and_body_are_left_out_of_the_serialized_form() {
        // Given
        let update = EntityUpdate::new(EntityUpdateSource::EntityUpdate, target());

        // When
        let json = serde_json::to_string(&update).unwrap();

        // Then
        assert!(!json.contains("\"fields\""));
        assert!(!json.contains("\"body\""));
        assert!(!json.contains("\"span\""));
    }
}

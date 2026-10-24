use std::collections::BTreeMap;

use rinx_ast::{AttributeValue, EntityId};
use serde::{Deserialize, Serialize};

/// One entity as the project index knows it.
///
/// Carries what a *reader of the index* needs — the type, where it lives, its
/// title, its attribute values, and the edges it points along — and
/// deliberately not the entity's prose. A listing directive needs fields, not
/// paragraphs, and keeping section bodies out is what bounds the index's size
/// on a project with thousands of requirements.
///
/// Per-document data, so it merges like `document_titles` does. The *incoming*
/// side is not here: back-links are a project-wide product and are recomputed
/// from these records, the same split `page_order` makes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRecord {
    /// The declaring type's name.
    pub type_name: String,
    /// The document the entity was written in.
    pub doc_path: String,
    /// The entity's title, when its type maps one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Parsed attribute values.
    #[serde(default)]
    pub attributes: BTreeMap<String, AttributeValue>,
    /// Outgoing edges, keyed by the relation's option spelling.
    #[serde(default)]
    pub outgoing: BTreeMap<String, Vec<EntityId>>,
    /// The diagram templates written inside this entity, by `:key:` — the
    /// empty key being the one written without.
    ///
    /// The one place this record knowingly relaxes its "no prose" rule, and
    /// the trade is deliberate: a diagram's `uml('REQ_001')` pulls in another
    /// entity's template, and that entity is declared in a document the
    /// importing one has never seen. Keeping the templates here makes the
    /// import an index lookup instead of a second, diagram-shaped index. What
    /// bounds the growth is that only a diagram an author actually wrote
    /// inside an entity lands here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub uml: BTreeMap<String, String>,
    /// The line each outgoing relation was written on, by its option
    /// spelling — the parse's `EntityBody::relation_spans`, carried into the
    /// index so a target found missing only project-wide is reported on the
    /// line that names it. A toctree entry's span is kept for the same reason.
    ///
    /// Read only by the phase reporting that, in the process that folds the
    /// index, so never written with it: a line moves with every edit above
    /// it, and every page renders from the written index
    /// (`docs/decisions/045-definition-spans.md`).
    #[serde(skip)]
    pub relation_spans: BTreeMap<String, rinx_ast::Span>,
}

impl EntityRecord {
    /// The text to show when linking to this entity.
    ///
    /// The title when it has one, otherwise the id — so a type that maps no
    /// title (an `audit-event`, say) still produces a readable link rather
    /// than an empty one.
    #[must_use]
    pub fn display_text<'a>(&'a self, id: &'a EntityId) -> &'a str {
        self.title.as_deref().unwrap_or_else(|| id.as_str())
    }

    /// The targets of one outgoing relation, empty when it was not written.
    #[must_use]
    pub fn targets(&self, relation: &str) -> &[EntityId] {
        self.outgoing
            .get(relation)
            .map_or(&[], |targets| targets.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(title: Option<&str>) -> EntityRecord {
        EntityRecord {
            relation_spans: BTreeMap::new(),
            type_name: "req".to_string(),
            doc_path: "specs/boot".to_string(),
            title: title.map(ToString::to_string),
            attributes: BTreeMap::from([(
                "status".to_string(),
                AttributeValue::String("open".to_string()),
            )]),
            outgoing: BTreeMap::from([(
                "links".to_string(),
                vec![EntityId::new("SPEC_003").unwrap()],
            )]),
            uml: BTreeMap::new(),
        }
    }

    #[test]
    fn test_a_record_is_written_without_the_lines_its_relations_stand_on() {
        // Given — a relation written on line 7
        let mut written = record(Some("Boot quickly"));
        written.relation_spans.insert(
            "links".to_string(),
            rinx_ast::Span::new(
                rinx_ast::Position::new(7, 1),
                rinx_ast::Position::new(7, 20),
            ),
        );

        // When
        let json = serde_json::to_string(&written).expect("serializes");
        let read: EntityRecord = serde_json::from_str(&json).expect("deserializes");

        // Then — the line moves with every edit above it, so the written
        // index must not hold it (docs/decisions/045-definition-spans.md)
        assert!(!json.contains("relation_spans"), "{json}");
        assert_eq!(read, record(Some("Boot quickly")));
    }

    #[test]
    fn test_display_text_prefers_the_title() {
        // Given
        let entity = record(Some("Boot quickly"));
        let id = EntityId::new("REQ_001").unwrap();

        // When
        let shown = entity.display_text(&id);

        // Then
        assert_eq!(shown, "Boot quickly");
    }

    #[test]
    fn test_display_text_falls_back_to_the_id() {
        // Given — an audit-event maps no title at all
        let entity = record(None);
        let id = EntityId::new("os.system").unwrap();

        // When
        let shown = entity.display_text(&id);

        // Then
        assert_eq!(shown, "os.system");
    }

    #[test]
    fn test_targets_returns_the_recorded_edges() {
        // Given
        let entity = record(None);

        // When
        let targets = entity.targets("links");

        // Then
        assert_eq!(targets, [EntityId::new("SPEC_003").unwrap()]);
    }

    #[test]
    fn test_targets_is_empty_for_a_relation_that_was_not_written() {
        // Given
        let entity = record(None);

        // When
        let targets = entity.targets("blocks");

        // Then
        assert!(targets.is_empty());
    }

    #[test]
    fn test_entity_record_survives_a_serialization_round_trip() {
        // Given
        let original = record(Some("Boot quickly"));

        // When
        let json = serde_json::to_string(&original).unwrap();
        let restored: EntityRecord = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(original, restored);
    }
}

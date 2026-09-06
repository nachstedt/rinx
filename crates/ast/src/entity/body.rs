use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entity::attribute_value::AttributeValue;
use crate::entity::id::EntityId;
use crate::entity::section::EntitySection;
use crate::node::Node;
use crate::span::Span;

/// One entity as written in a document.
///
/// Carries only what survived validation against the entity's type: the
/// attribute map holds parsed values, the relation map holds ids that were
/// legal ids, and the sections are the ones the type declares. Whatever the
/// author got wrong became a diagnostic during parsing and is absent here, so
/// no later phase has to re-check the schema.
///
/// The type is named rather than typed, because a config-declared type cannot
/// be a Rust enum. That is the one deliberate deviation from how the built-in
/// py/c/std domain objects are modelled, and the schema is what buys it back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityBody {
    /// The declaring type's name, e.g. `req`.
    pub type_name: String,
    /// The determined id — explicit, derived or generated.
    pub id: EntityId,
    /// Parsed attribute values, including any filled from the directive's
    /// argument and any supplied by a schema default.
    pub attributes: BTreeMap<String, AttributeValue>,
    /// Outgoing relations, keyed by the relation's option spelling. Targets
    /// are recorded as written and resolved project-wide later, exactly as
    /// cross-references are — an entity may point at one in another document
    /// that this process never sees.
    pub relations: BTreeMap<String, Vec<EntityId>>,
    /// The body's prose, in document order, the unnamed content among them.
    pub sections: Vec<EntitySection>,
    /// Where the directive was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl EntityBody {
    /// The unnamed content section's nodes, empty when the entity has none.
    ///
    /// The named accessor the uniform `sections` collection is designed
    /// around: content is one of the sections, not a field beside them, so
    /// document order survives — but the one section that is special stays
    /// reachable without every caller scanning for it.
    #[must_use]
    pub fn content(&self) -> &[Node] {
        self.sections
            .iter()
            .find(|section| section.name().is_none())
            .map_or(&[], |section| section.body.as_slice())
    }

    /// Every occurrence of the named section, in document order.
    ///
    /// A `Vec` rather than an `Option` because a section may be declared
    /// `multiple`, and a caller that renders one should not have to know
    /// which kind it is looking at.
    #[must_use]
    pub fn named_sections(&self, name: &str) -> Vec<&EntitySection> {
        self.sections
            .iter()
            .filter(|section| section.name() == Some(name))
            .collect()
    }

    /// The entity's title, when its type maps one out of the argument.
    #[must_use]
    pub fn title(&self) -> Option<String> {
        self.attributes.get("title").map(ToString::to_string)
    }

    /// The targets of one outgoing relation, empty when it was not written.
    #[must_use]
    pub fn relation_targets(&self, relation: &str) -> &[EntityId] {
        self.relations
            .get(relation)
            .map_or(&[], |targets| targets.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    fn paragraph(text: &str) -> Vec<Node> {
        vec![Node::Paragraph(vec![InlineNode::Text(text.to_string())])]
    }

    fn requirement() -> EntityBody {
        EntityBody {
            type_name: "req".to_string(),
            id: EntityId::new("REQ_001").unwrap(),
            attributes: BTreeMap::from([(
                "title".to_string(),
                AttributeValue::String("Boot quickly".to_string()),
            )]),
            relations: BTreeMap::from([(
                "links".to_string(),
                vec![EntityId::new("SPEC_003").unwrap()],
            )]),
            sections: vec![
                EntitySection::content(paragraph("prose")),
                EntitySection::named(
                    "verification-criteria".to_string(),
                    paragraph("measured"),
                    None,
                ),
            ],
            span: None,
        }
    }

    #[test]
    fn test_content_returns_the_unnamed_section() {
        // Given
        let entity = requirement();

        // When
        let content = entity.content();

        // Then
        assert_eq!(content, paragraph("prose").as_slice());
    }

    #[test]
    fn test_content_is_empty_when_the_entity_has_only_named_sections() {
        // Given
        let mut entity = requirement();
        entity.sections.retain(|s| s.name().is_some());

        // When
        let content = entity.content();

        // Then
        assert!(content.is_empty());
    }

    #[test]
    fn test_named_sections_returns_each_occurrence_in_document_order() {
        // Given — a `multiple` section written twice
        let mut entity = requirement();
        entity.sections.push(EntitySection::named(
            "verification-criteria".to_string(),
            paragraph("also measured"),
            None,
        ));

        // When
        let sections = entity.named_sections("verification-criteria");

        // Then
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].body, paragraph("measured"));
        assert_eq!(sections[1].body, paragraph("also measured"));
    }

    #[test]
    fn test_named_sections_is_empty_for_a_section_that_was_not_written() {
        // Given
        let entity = requirement();

        // When
        let sections = entity.named_sections("safety-comment");

        // Then
        assert!(sections.is_empty());
    }

    #[test]
    fn test_title_reads_the_title_attribute() {
        // Given
        let entity = requirement();

        // When
        let title = entity.title();

        // Then
        assert_eq!(title.as_deref(), Some("Boot quickly"));
    }

    #[test]
    fn test_title_is_absent_for_a_type_that_maps_no_title() {
        // Given — an audit-event maps `name`/`args`/`version` instead
        let mut entity = requirement();
        entity.attributes.remove("title");

        // When
        let title = entity.title();

        // Then
        assert_eq!(title, None);
    }

    #[test]
    fn test_relation_targets_returns_the_written_ids() {
        // Given
        let entity = requirement();

        // When
        let targets = entity.relation_targets("links");

        // Then
        assert_eq!(targets, [EntityId::new("SPEC_003").unwrap()]);
    }

    #[test]
    fn test_relation_targets_is_empty_for_a_relation_that_was_not_written() {
        // Given
        let entity = requirement();

        // When
        let targets = entity.relation_targets("blocks");

        // Then
        assert!(targets.is_empty());
    }

    #[test]
    fn test_entity_body_survives_a_serialization_round_trip() {
        // Given
        let original = requirement();

        // When
        let json = serde_json::to_string(&original).unwrap();
        let restored: EntityBody = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(original, restored);
    }
}

use serde::{Deserialize, Serialize};

use crate::argument::ArgumentSpec;
use crate::attribute::AttributeSchema;
use crate::id::IdSpec;
use crate::relation::RelationSpec;
use crate::section::SectionSpec;

/// The option every entity type accepts, whatever else it declares.
pub const ID_OPTION: &str = "id";

/// One entity type: a directive name plus everything an instance may carry.
///
/// The four kinds of declaration answer four different questions, and the
/// distinction is the heart of the model:
///
/// - **attributes** are values — typed, validated, indexed, filterable, never
///   parsed as RST;
/// - **sections** are documents — fully-parsed RST, not indexed;
/// - **relations** are edges to other entities;
/// - roles (declared on the schema, not here) are how prose points at one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityType {
    /// The directive name, e.g. `req` for `.. req::`.
    pub name: String,
    /// Human-readable label for the default rendering; falls back to `name`.
    pub label: Option<String>,
    /// How the directive's argument maps onto attributes.
    pub argument: ArgumentSpec,
    /// How instances of this type are identified.
    pub id: IdSpec,
    /// Template name, resolved against the site's template directory. Absent
    /// means the built-in rendering.
    pub template: Option<String>,
    pub attributes: Vec<AttributeSchema>,
    pub sections: Vec<SectionSpec>,
    pub relations: Vec<RelationSpec>,
}

impl EntityType {
    /// The label to display, falling back to the directive name.
    #[must_use]
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    /// Looks up a declared attribute by its option spelling.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&AttributeSchema> {
        self.attributes.iter().find(|a| a.name == name)
    }

    /// Looks up a declared section by its sub-directive name.
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&SectionSpec> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// Looks up a declared relation by its option spelling.
    #[must_use]
    pub fn relation(&self, name: &str) -> Option<&RelationSpec> {
        self.relations.iter().find(|r| r.name == name)
    }

    /// Every option spelling an instance of this type may use.
    ///
    /// The unknown-option diagnostic is built from this, so `id`, attributes
    /// and relations are listed together — an author who misspells `:links:`
    /// should be offered the whole vocabulary, not the half of it that happens
    /// to be attributes.
    #[must_use]
    pub fn option_names(&self) -> Vec<&str> {
        std::iter::once(ID_OPTION)
            .chain(self.attributes.iter().map(|a| a.name.as_str()))
            .chain(self.relations.iter().map(|r| r.name.as_str()))
            .collect()
    }

    /// Reports whether `name` is an option this type accepts.
    #[must_use]
    pub fn accepts_option(&self, name: &str) -> bool {
        name == ID_OPTION || self.attribute(name).is_some() || self.relation(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute::AttributeType;

    pub(crate) fn requirement_type() -> EntityType {
        EntityType {
            name: "req".to_string(),
            label: Some("Requirement".to_string()),
            argument: ArgumentSpec {
                fields: vec!["title".to_string()],
                split: crate::argument::ArgumentSplit::Whole,
            },
            id: IdSpec::default(),
            template: None,
            attributes: vec![AttributeSchema {
                name: "status".to_string(),
                label: None,
                value_type: AttributeType::String,
                required: false,
                default: None,
            }],
            sections: vec![SectionSpec {
                name: "verification-criteria".to_string(),
                label: None,
                required: false,
                multiple: false,
            }],
            relations: vec![RelationSpec {
                name: "links".to_string(),
                label: None,
                to: None,
                required: false,
                multiple: true,
                incoming: Some("linked_by".to_string()),
                incoming_label: None,
            }],
        }
    }

    #[test]
    fn test_entity_type_falls_back_to_the_directive_name_as_its_label() {
        // Given
        let mut unlabelled = requirement_type();
        unlabelled.label = None;

        // When / Then
        assert_eq!(unlabelled.display_label(), "req");
        assert_eq!(requirement_type().display_label(), "Requirement");
    }

    #[test]
    fn test_entity_type_looks_up_each_kind_of_declaration_by_name() {
        // Given
        let entity_type = requirement_type();

        // When / Then
        assert!(entity_type.attribute("status").is_some());
        assert!(entity_type.section("verification-criteria").is_some());
        assert!(entity_type.relation("links").is_some());
    }

    #[test]
    fn test_entity_type_lookups_miss_a_name_of_the_wrong_kind() {
        // Given — the three namespaces are distinct
        let entity_type = requirement_type();

        // When / Then
        assert!(entity_type.attribute("links").is_none());
        assert!(entity_type.relation("status").is_none());
        assert!(entity_type.section("status").is_none());
    }

    #[test]
    fn test_option_names_lists_id_attributes_and_relations_together() {
        // Given
        let entity_type = requirement_type();

        // When
        let names = entity_type.option_names();

        // Then
        assert_eq!(names, vec!["id", "status", "links"]);
    }

    #[test]
    fn test_accepts_option_covers_every_declared_spelling() {
        // Given
        let entity_type = requirement_type();

        // When / Then
        assert!(entity_type.accepts_option("id"));
        assert!(entity_type.accepts_option("status"));
        assert!(entity_type.accepts_option("links"));
        assert!(!entity_type.accepts_option("verification-criteria"));
        assert!(!entity_type.accepts_option("nonsense"));
    }
}

//! An entity type's [`DirectiveSpec`], built from the schema declaring it.
//!
//! A built-in directive's spec is a constant; an entity type's names, purpose
//! and options are the schema's, known only once it loads. So its spec
//! borrows the [`EntityType`]: [`EntitySpec`] holds the options built from
//! it, and [`EntitySpec::spec`] lends a [`DirectiveSpec`] over them that the
//! reader splits a block by and completion lists, as it does a built-in's.
//!
//! The shape is sphinx-needs' `NeedDirective`'s: one optional argument
//! keeping the rest of its lines, options — `:id:`, then the type's
//! attributes and relations — and content. A type's sections are written as
//! directives inside that content, not as options, so they are not listed.

use rinx_entity::{AttributeType, EntityType, ID_OPTION};

use super::spec::{Arguments, DirectiveSpec, OptionSet, OptionSpec, OptionSupport, OptionValue};

/// The spec of one entity type the schema declares.
#[derive(Debug)]
pub struct EntitySpec<'s> {
    /// The type's directive name.
    name: &'s str,
    /// What the type is, as the schema labels it.
    doc: &'s str,
    /// `:id:`, then each attribute, then each relation.
    options: Vec<OptionSpec<'s>>,
}

impl<'s> EntitySpec<'s> {
    /// The spec of `entity_type`.
    #[must_use]
    pub fn new(entity_type: &'s EntityType) -> Self {
        let id = OptionSpec {
            name: ID_OPTION,
            aliases: &[],
            value: OptionValue::Text,
            doc: if entity_type.id.required {
                "The entity's id."
            } else {
                "The entity's id, generated when left out."
            },
            support: OptionSupport::Supported,
        };
        let attributes = entity_type.attributes.iter().map(|attribute| OptionSpec {
            name: &attribute.name,
            aliases: &[],
            value: attribute_value(&attribute.value_type),
            doc: attribute.display_label(),
            support: OptionSupport::Supported,
        });
        let relations = entity_type.relations.iter().map(|relation| OptionSpec {
            name: &relation.name,
            aliases: &[],
            value: OptionValue::EntityIds,
            doc: relation.display_label(),
            support: OptionSupport::Supported,
        });
        Self {
            name: &entity_type.name,
            doc: entity_type.display_label(),
            options: std::iter::once(id)
                .chain(attributes)
                .chain(relations)
                .collect(),
        }
    }

    /// The type's spec, over the options built from it.
    #[must_use]
    pub fn spec(&self) -> DirectiveSpec<'_> {
        DirectiveSpec {
            names: std::slice::from_ref(&self.name),
            doc: self.doc,
            arguments: Arguments::OPTIONAL_TEXT,
            takes_content: true,
            content_hint: None,
            options: OptionSet {
                own: &self.options,
                shared: &[],
            },
            accepts_further: false,
        }
    }
}

/// What an attribute of `value_type` is written as.
fn attribute_value(value_type: &AttributeType) -> OptionValue<'_> {
    match value_type {
        AttributeType::String { .. }
        | AttributeType::Text { .. }
        | AttributeType::StringList { .. } => OptionValue::Text,
        AttributeType::Int => OptionValue::Integer,
        AttributeType::Bool => OptionValue::Flag,
        AttributeType::Enum { values } => OptionValue::Declared(values),
        AttributeType::EnumList { values } => OptionValue::DeclaredList(values),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_entity::{AttributeSchema, IdSpec, RelationSpec};

    /// A `req` type labelled `Requirement`, with a `status` choice, a
    /// `tags` choice list and a `links` relation.
    fn requirement() -> EntityType {
        let attribute = |name: &str, value_type| AttributeSchema {
            name: name.to_string(),
            label: Some(format!("The {name}")),
            value_type,
            required: false,
            default: None,
        };
        EntityType {
            name: "req".to_string(),
            label: Some("Requirement".to_string()),
            argument: rinx_entity::ArgumentSpec::default(),
            id: IdSpec::default(),
            template: None,
            attributes: vec![
                attribute(
                    "status",
                    AttributeType::Enum {
                        values: vec!["open".to_string(), "done".to_string()],
                    },
                ),
                attribute(
                    "tags",
                    AttributeType::EnumList {
                        values: vec!["ui".to_string()],
                    },
                ),
            ],
            sections: Vec::new(),
            relations: vec![RelationSpec {
                name: "links".to_string(),
                label: None,
                to: None,
                required: false,
                multiple: true,
                incoming: None,
                incoming_label: None,
            }],
        }
    }

    #[test]
    fn test_an_entity_spec_is_named_and_described_by_its_type() {
        // Given
        let entity_type = requirement();

        // When
        let entity_spec = EntitySpec::new(&entity_type);
        let spec = entity_spec.spec();

        // Then
        assert_eq!(spec.names, ["req"]);
        assert_eq!(spec.doc, "Requirement");
        assert_eq!(spec.arguments, Arguments::OPTIONAL_TEXT);
        assert!(spec.takes_content);
        assert!(spec.takes_options());
    }

    #[test]
    fn test_an_entity_spec_lists_the_id_then_attributes_then_relations() {
        // Given
        let entity_type = requirement();

        // When
        let entity_spec = EntitySpec::new(&entity_type);
        let names: Vec<&str> = entity_spec
            .spec()
            .option_specs()
            .map(|option| option.name)
            .collect();

        // Then
        assert_eq!(names, ["id", "status", "tags", "links"]);
    }

    #[test]
    fn test_an_entity_spec_offers_the_declared_values_of_a_choice() {
        // Given
        let entity_type = requirement();

        // When
        let entity_spec = EntitySpec::new(&entity_type);
        let options: Vec<OptionValue> = entity_spec
            .spec()
            .option_specs()
            .map(|option| option.value)
            .collect();

        // Then
        assert!(matches!(options[1], OptionValue::Declared(values) if values == ["open", "done"]));
        assert!(matches!(options[2], OptionValue::DeclaredList(values) if values == ["ui"]));
        assert!(matches!(options[3], OptionValue::EntityIds));
    }

    #[test]
    fn test_an_entity_spec_says_when_the_id_is_generated() {
        // Given
        let mut entity_type = requirement();
        let optional = EntitySpec::new(&entity_type).options[0].doc.to_string();
        entity_type.id.required = true;

        // When
        let required = EntitySpec::new(&entity_type).options[0].doc;

        // Then
        assert_eq!(optional, "The entity's id, generated when left out.");
        assert_eq!(required, "The entity's id.");
    }

    #[test]
    fn test_attribute_value_names_what_each_attribute_type_is_written_as() {
        // Given / When / Then
        assert!(matches!(
            attribute_value(&AttributeType::string()),
            OptionValue::Text
        ));
        assert!(matches!(
            attribute_value(&AttributeType::string_list()),
            OptionValue::Text
        ));
        assert!(matches!(
            attribute_value(&AttributeType::Int),
            OptionValue::Integer
        ));
        assert!(matches!(
            attribute_value(&AttributeType::Bool),
            OptionValue::Flag
        ));
    }
}

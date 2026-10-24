//! An entity's fields as a reader is shown them: its attributes, its outgoing
//! relations and the back-links derived onto it, each under its declared
//! label and in its declared order, with the values every
//! `.. entity-update::`/`.. needextend::` left.
//!
//! In this crate rather than in the renderer that drew them first, because a
//! second reader now shows the same thing: the language server's hover. A
//! field the page lists and the hover leaves out, or one labelled differently
//! in each, would be a bug neither crate's tests could see — the same reason
//! [`crate::EntitySubject`] lives here.

use std::collections::BTreeMap;

use rinx_ast::{AttributeValue, EntityId};
use rinx_entity::EntitySchema;

use crate::ProjectIndex;

/// What an entity holds as written: the values its own directive gave it.
///
/// An entity's effective values come from the index, but not every entity
/// a page shows is in it: one whose id two documents declare is moved to
/// [`crate::AmbiguousDefinitions`] and defines nothing, yet each of its
/// declarations is still drawn. Those fall back to what was written.
#[derive(Debug, Clone, Copy)]
pub struct OwnFields<'a> {
    pub attributes: &'a BTreeMap<String, AttributeValue>,
    pub relations: &'a BTreeMap<String, Vec<EntityId>>,
}

impl<'a> OwnFields<'a> {
    /// The value to show for the attribute `name` of the entity `id`: the
    /// index's effective value when the index holds the entity — a `None`
    /// there meaning an update cleared it — else the value written.
    #[must_use]
    pub fn effective_attribute(
        self,
        id: &EntityId,
        name: &str,
        index: &'a ProjectIndex,
    ) -> Option<&'a AttributeValue> {
        if index.entities.contains_key(id) {
            return index.effective_attribute(id, name);
        }
        self.attributes.get(name)
    }

    /// The targets to show for the relation `name` of the entity `id`, by
    /// the rule of [`Self::effective_attribute`].
    #[must_use]
    pub fn effective_relation_targets(
        self,
        id: &EntityId,
        name: &str,
        index: &'a ProjectIndex,
    ) -> &'a [EntityId] {
        if index.entities.contains_key(id) {
            return index.effective_relation_targets(id, name);
        }
        self.relations.get(name).map_or(&[], Vec::as_slice)
    }
}

/// One attribute value, as shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownAttribute<'a> {
    /// The attribute's option name.
    pub name: &'a str,
    /// What it is shown as: its declared label, else its name.
    pub label: &'a str,
    pub value: &'a AttributeValue,
    /// Whether the value is one side of conflicting updates.
    pub conflict: bool,
}

/// One labelled list of entity ids, as shown: an outgoing relation's
/// targets, or the entities a back-link derives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownLinks<'a> {
    pub label: &'a str,
    pub targets: &'a [EntityId],
    /// Whether the list is one side of conflicting updates. Never set on a
    /// back-link, which is derived and so no update's target.
    pub conflict: bool,
}

/// Everything shown about one entity besides its header and its prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityFields<'a> {
    /// The type's label, else its directive name.
    pub type_label: &'a str,
    /// The attributes holding a value, the title left out: the schema's in
    /// its declared order, then any the schema does not declare.
    pub attributes: Vec<ShownAttribute<'a>>,
    /// The type's relations holding a target, in declared order.
    pub relations: Vec<ShownLinks<'a>>,
    /// The back-links holding a source, in the schema's order.
    pub backlinks: Vec<ShownLinks<'a>>,
}

impl<'a> EntityFields<'a> {
    /// The fields of the entity `id` of type `type_name`, which holds `own`
    /// as written, against `index` and `schema`.
    #[must_use]
    pub fn of(
        id: &EntityId,
        type_name: &'a str,
        own: OwnFields<'a>,
        index: &'a ProjectIndex,
        schema: &'a EntitySchema,
    ) -> Self {
        let entity_type = schema.entity_type(type_name);
        let type_label = entity_type.map_or(type_name, |declared| declared.display_label());

        // Declared order, not the map's key order, so two entities of one type
        // put the same field in the same place. An attribute the schema does
        // not declare is appended rather than dropped, for the same reason a
        // section is.
        let declared: Vec<&'a str> = entity_type
            .map(|declared| {
                declared
                    .attributes
                    .iter()
                    .map(|attribute| attribute.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let undeclared = own
            .attributes
            .keys()
            .map(String::as_str)
            .filter(|name| !declared.contains(name));
        let attributes = declared
            .iter()
            .copied()
            .chain(undeclared)
            .filter(|name| *name != "title")
            .filter_map(|name| {
                Some(ShownAttribute {
                    name,
                    label: entity_type
                        .and_then(|declared| declared.attribute(name))
                        .map_or(name, |attribute| attribute.display_label()),
                    value: own.effective_attribute(id, name, index)?,
                    conflict: attribute_conflict(index, id, name),
                })
            })
            .collect();

        let relations = entity_type
            .map(|declared| {
                declared
                    .relations
                    .iter()
                    .filter_map(|relation| {
                        let targets = own.effective_relation_targets(id, &relation.name, index);
                        (!targets.is_empty()).then(|| ShownLinks {
                            label: relation.display_label(),
                            targets,
                            conflict: relation_conflict(index, id, &relation.name),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Incoming links come from the index: they are a fact about the whole
        // project, which is exactly why they are derived there.
        let backlinks = index
            .entity_backlinks
            .get(id)
            .map(|incoming| {
                schema
                    .backlinks_for(type_name)
                    .iter()
                    .filter_map(|backlink| {
                        Some(ShownLinks {
                            label: &backlink.label,
                            targets: incoming.get(&backlink.name)?,
                            conflict: false,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            type_label,
            attributes,
            relations,
            backlinks,
        }
    }
}

/// Whether the current value of the attribute `name` of the entity `id` was
/// set over a different value another update had set — either side is
/// marked, since neither directive is more "right". The visible counterpart
/// to the `entity-update.conflicting-update` diagnostic; see
/// `docs/decisions/019-entity-update.md`.
fn attribute_conflict(index: &ProjectIndex, id: &EntityId, name: &str) -> bool {
    index
        .entity_update_history
        .get(id)
        .and_then(|history| history.attributes.get(name))
        .and_then(|history| history.applied.last())
        .is_some_and(|entry| entry.conflicts_with.is_some())
}

/// As [`attribute_conflict`], for a relation's current targets.
fn relation_conflict(index: &ProjectIndex, id: &EntityId, name: &str) -> bool {
    index
        .entity_update_history
        .get(id)
        .and_then(|history| history.relations.get(name))
        .and_then(|history| history.applied.last())
        .is_some_and(|entry| entry.conflicts_with.is_some())
}

#[cfg(test)]
mod tests {
    use rinx_ast::FieldMutationMode;
    use rinx_entity::{NoReservedNames, load_schema};

    use super::*;
    use crate::{
        AppliedFieldUpdate, AppliedRelationUpdate, AttributeFieldHistory, EntityFieldHistory,
        EntityRecord, RelationFieldHistory,
    };

    /// `req`, with a labelled `status` and an unlabelled `priority`, and
    /// `test`, verifying requirements under a derived `verified_by`.
    fn schema() -> EntitySchema {
        load_schema(
            r#"
            [[entity_type]]
            name = "req"
            label = "Requirement"
              [[entity_type.attribute]]
              name = "title"
              type = "string"
              [[entity_type.attribute]]
              name = "status"
              label = "Current status"
              type = "string"
              [[entity_type.attribute]]
              name = "priority"
              type = "int"
              [[entity_type.relation]]
              name = "links"
              label = "Links to"

            [[entity_type]]
            name = "test"
              [[entity_type.relation]]
              name = "verifies"
              to = ["req"]
              incoming = "verified_by"
              incoming_label = "Verified by"
            "#,
            &NoReservedNames,
        )
        .expect("the test schema should load")
    }

    fn id(raw: &str) -> EntityId {
        EntityId::new(raw).unwrap()
    }

    fn text(value: &str) -> AttributeValue {
        AttributeValue::String(value.to_string())
    }

    /// `REQ_1` as written: a title, a status, a priority, one link.
    fn written() -> (
        BTreeMap<String, AttributeValue>,
        BTreeMap<String, Vec<EntityId>>,
    ) {
        (
            BTreeMap::from([
                ("title".to_string(), text("Boot quickly")),
                ("priority".to_string(), AttributeValue::Int(2)),
                ("status".to_string(), text("open")),
            ]),
            BTreeMap::from([("links".to_string(), vec![id("SPEC_1")])]),
        )
    }

    /// An index holding `REQ_1` as [`written`], verified by `TEST_1`.
    fn index() -> ProjectIndex {
        let (attributes, outgoing) = written();
        let mut index = ProjectIndex::default();
        index.entities.insert(
            id("REQ_1"),
            EntityRecord {
                type_name: "req".to_string(),
                doc_path: "specs/boot.rst".to_string(),
                title: Some("Boot quickly".to_string()),
                attributes,
                outgoing,
                uml: BTreeMap::new(),
                relation_spans: BTreeMap::new(),
            },
        );
        index.entity_backlinks.insert(
            id("REQ_1"),
            BTreeMap::from([("verified_by".to_string(), vec![id("TEST_1")])]),
        );
        index
    }

    fn attribute_history(current: Option<AttributeValue>, conflict: bool) -> EntityFieldHistory {
        let mut history = EntityFieldHistory::default();
        history.attributes.insert(
            "status".to_string(),
            AttributeFieldHistory {
                original: Some(text("open")),
                applied: vec![AppliedFieldUpdate {
                    update_index: 1,
                    doc_path: "specs/other.rst".to_string(),
                    span: None,
                    mode: FieldMutationMode::Set("closed".to_string()),
                    resulting_value: current.clone(),
                    conflicts_with: conflict.then_some(0),
                }],
                current,
            },
        );
        history
    }

    #[test]
    fn test_of_lists_declared_attributes_in_declared_order_under_their_labels() {
        // Given
        let (attributes, relations) = written();
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let (index, schema) = (index(), schema());

        // When
        let fields = EntityFields::of(&id("REQ_1"), "req", own, &index, &schema);

        // Then — the title is the header's, not a row
        assert_eq!(fields.type_label, "Requirement");
        let shown: Vec<(&str, &str)> = fields
            .attributes
            .iter()
            .map(|attribute| (attribute.name, attribute.label))
            .collect();
        assert_eq!(
            shown,
            [("status", "Current status"), ("priority", "priority")]
        );
        assert!(
            fields
                .attributes
                .iter()
                .all(|attribute| !attribute.conflict)
        );
    }

    #[test]
    fn test_of_appends_an_attribute_the_schema_does_not_declare() {
        // Given
        let (mut attributes, relations) = written();
        attributes.insert("owner".to_string(), text("kernel team"));
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let (index, schema) = (ProjectIndex::default(), schema());

        // When — an entity the index does not hold, so `own` is read
        let fields = EntityFields::of(&id("REQ_9"), "req", own, &index, &schema);

        // Then
        let last = fields.attributes.last().unwrap();
        assert_eq!((last.name, last.label), ("owner", "owner"));
        assert_eq!(last.value, &text("kernel team"));
    }

    #[test]
    fn test_of_shows_an_updated_value_and_marks_a_conflicting_one() {
        // Given
        let (attributes, relations) = written();
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let mut index = index();
        index
            .entity_update_history
            .insert(id("REQ_1"), attribute_history(Some(text("closed")), true));
        let schema = schema();

        // When
        let fields = EntityFields::of(&id("REQ_1"), "req", own, &index, &schema);

        // Then
        let status = &fields.attributes[0];
        assert_eq!(status.value, &text("closed"));
        assert!(status.conflict);
    }

    #[test]
    fn test_of_leaves_out_a_value_an_update_cleared_though_it_was_written() {
        // Given
        let (attributes, relations) = written();
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let mut index = index();
        index
            .entity_update_history
            .insert(id("REQ_1"), attribute_history(None, false));
        let schema = schema();

        // When
        let fields = EntityFields::of(&id("REQ_1"), "req", own, &index, &schema);

        // Then
        assert!(fields.attributes.iter().all(|shown| shown.name != "status"));
    }

    #[test]
    fn test_of_lists_relations_with_targets_and_marks_a_conflicting_one() {
        // Given
        let (attributes, relations) = written();
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let mut index = index();
        let mut history = EntityFieldHistory::default();
        history.relations.insert(
            "links".to_string(),
            RelationFieldHistory {
                original: vec![id("SPEC_1")],
                applied: vec![AppliedRelationUpdate {
                    update_index: 1,
                    doc_path: "specs/other.rst".to_string(),
                    span: None,
                    mode: FieldMutationMode::Set("SPEC_2".to_string()),
                    resulting_targets: vec![id("SPEC_2")],
                    conflicts_with: Some(0),
                }],
                current: vec![id("SPEC_2")],
            },
        );
        index.entity_update_history.insert(id("REQ_1"), history);
        let schema = schema();

        // When
        let fields = EntityFields::of(&id("REQ_1"), "req", own, &index, &schema);

        // Then
        assert_eq!(
            fields.relations,
            [ShownLinks {
                label: "Links to",
                targets: &[id("SPEC_2")],
                conflict: true,
            }]
        );
    }

    #[test]
    fn test_of_lists_the_back_links_the_index_derives() {
        // Given
        let (attributes, relations) = written();
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let (index, schema) = (index(), schema());

        // When
        let fields = EntityFields::of(&id("REQ_1"), "req", own, &index, &schema);

        // Then
        assert_eq!(
            fields.backlinks,
            [ShownLinks {
                label: "Verified by",
                targets: &[id("TEST_1")],
                conflict: false,
            }]
        );
    }

    #[test]
    fn test_of_falls_back_to_the_written_values_of_an_entity_the_index_lacks() {
        // Given — `REQ_1` declared twice, so the index holds no record of it
        let (attributes, relations) = written();
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let (index, schema) = (ProjectIndex::default(), schema());

        // When
        let fields = EntityFields::of(&id("REQ_1"), "req", own, &index, &schema);

        // Then
        assert_eq!(fields.attributes[0].value, &text("open"));
        assert_eq!(fields.relations[0].targets, [id("SPEC_1")]);
        assert!(fields.backlinks.is_empty());
    }

    #[test]
    fn test_of_names_an_undeclared_type_by_its_name_and_lists_no_relation() {
        // Given
        let (attributes, relations) = written();
        let own = OwnFields {
            attributes: &attributes,
            relations: &relations,
        };
        let (index, schema) = (ProjectIndex::default(), schema());

        // When
        let fields = EntityFields::of(&id("REQ_1"), "story", own, &index, &schema);

        // Then — its attributes are all undeclared, so in the map's order
        assert_eq!(fields.type_label, "story");
        let names: Vec<&str> = fields.attributes.iter().map(|shown| shown.name).collect();
        assert_eq!(names, ["priority", "status"]);
        assert!(fields.relations.is_empty());
    }
}

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entity_type::EntityType;
use crate::error::{DeclarationKind, SchemaError};

/// One relation that feeds a back-link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacklinkSource {
    /// The entity type carrying the outgoing relation.
    pub from_type: String,
    /// The outgoing relation's option spelling.
    pub relation: String,
}

/// A back-link a given entity type can receive.
///
/// Derived, never declared: a relation is written on the type that carries it,
/// so the target type's own block says nothing about what points back at it.
/// Computing this once at schema load is what lets the default rendering and
/// the collision checks read the answer instead of each re-deriving it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacklinkSpec {
    /// The back-link's name, from the relations' `incoming`.
    pub name: String,
    /// The heading to display for it.
    pub label: String,
    /// Every relation that contributes edges to this back-link.
    pub sources: Vec<BacklinkSource>,
}

/// Every entity type's derived back-links, keyed by the *target* type's name.
pub type BacklinkTable = BTreeMap<String, Vec<BacklinkSpec>>;

/// Derives the back-link table and enforces the two rules it makes checkable.
///
/// A shared *outgoing* name constrains nothing: `req.links` and `spec.links`
/// are independent declarations that may differ in `to`, in `label` and even
/// in `incoming`, because they render on different types and feed different
/// back-links. The constraints are entirely on the derived side:
///
/// 1. Two relations whose `incoming` names meet **on a shared target type**
///    must agree on the label, since that target has one back-link fed by
///    both. Relations whose `to` sets are disjoint never meet, and may differ
///    freely — rejecting those would refuse valid schemas.
/// 2. A derived name must not collide with an attribute, section or relation
///    the target type declares itself.
///
/// There is deliberately no reserved-word rule: the template context puts
/// attributes, sections, outgoing links and incoming links in separate
/// namespaces, so a back-link cannot shadow `id` or `title` however it is
/// named. Only the collisions above are real, and both are ambiguities a
/// *reader* would hit, not just the renderer.
///
/// # Errors
///
/// Returns every violation found, rather than stopping at the first.
pub fn derive_backlinks(types: &[EntityType]) -> Result<BacklinkTable, Vec<SchemaError>> {
    let mut table: BacklinkTable = BTreeMap::new();
    let mut errors = Vec::new();

    for source_type in types {
        for relation in &source_type.relations {
            let Some(incoming) = relation.incoming.as_deref() else {
                continue;
            };
            let label = relation
                .incoming_display_label()
                .unwrap_or(incoming)
                .to_string();
            let source = BacklinkSource {
                from_type: source_type.name.clone(),
                relation: relation.name.clone(),
            };

            for target in targets_of(relation.to.as_deref(), types) {
                add_backlink(
                    table.entry(target).or_default(),
                    incoming,
                    &label,
                    &source,
                    &mut errors,
                );
            }
        }
    }

    collect_name_clashes(types, &table, &mut errors);

    if errors.is_empty() {
        Ok(table)
    } else {
        Err(errors)
    }
}

/// The entity type names a relation's `to` resolves to.
///
/// An undeclared `to` means "any type", which is expanded here rather than
/// left implicit, so a back-link genuinely appears on every type the author
/// allowed instead of only on the ones they happened to enumerate.
fn targets_of(declared: Option<&[String]>, types: &[EntityType]) -> Vec<String> {
    match declared {
        Some(names) => names.to_vec(),
        None => types.iter().map(|t| t.name.clone()).collect(),
    }
}

/// Adds one source to a target's back-links, merging into an existing entry.
fn add_backlink(
    backlinks: &mut Vec<BacklinkSpec>,
    incoming: &str,
    label: &str,
    source: &BacklinkSource,
    errors: &mut Vec<SchemaError>,
) {
    if let Some(existing) = backlinks.iter_mut().find(|b| b.name == incoming) {
        if existing.label != label {
            errors.push(SchemaError::BacklinkLabelConflict {
                target_type: source.from_type.clone(),
                incoming: incoming.to_string(),
                first: existing.label.clone(),
                second: label.to_string(),
            });
            return;
        }
        existing.sources.push(source.clone());
        return;
    }
    backlinks.push(BacklinkSpec {
        name: incoming.to_string(),
        label: label.to_string(),
        sources: vec![source.clone()],
    });
}

/// Reports every derived name that collides with the target type's own.
fn collect_name_clashes(
    types: &[EntityType],
    table: &BacklinkTable,
    errors: &mut Vec<SchemaError>,
) {
    for entity_type in types {
        let Some(backlinks) = table.get(&entity_type.name) else {
            continue;
        };
        for backlink in backlinks {
            if let Some(kind) = declared_kind(entity_type, &backlink.name) {
                errors.push(SchemaError::BacklinkNameClash {
                    target_type: entity_type.name.clone(),
                    incoming: backlink.name.clone(),
                    clashes_with: kind,
                });
            }
        }
    }
}

/// Which kind of declaration, if any, already owns `name` on this type.
fn declared_kind(entity_type: &EntityType, name: &str) -> Option<DeclarationKind> {
    if entity_type.attribute(name).is_some() {
        return Some(DeclarationKind::Attribute);
    }
    if entity_type.section(name).is_some() {
        return Some(DeclarationKind::Section);
    }
    if entity_type.relation(name).is_some() {
        return Some(DeclarationKind::Relation);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::argument::ArgumentSpec;
    use crate::attribute::{AttributeSchema, AttributeType};
    use crate::id::IdSpec;
    use crate::relation::RelationSpec;
    use crate::section::SectionSpec;

    fn bare_type(name: &str) -> EntityType {
        EntityType {
            name: name.to_string(),
            label: None,
            argument: ArgumentSpec::default(),
            id: IdSpec::default(),
            template: None,
            attributes: Vec::new(),
            sections: Vec::new(),
            relations: Vec::new(),
        }
    }

    fn relation(
        name: &str,
        to: Option<Vec<&str>>,
        incoming: &str,
        label: Option<&str>,
    ) -> RelationSpec {
        RelationSpec {
            name: name.to_string(),
            label: None,
            to: to.map(|types| types.into_iter().map(ToString::to_string).collect()),
            required: false,
            multiple: true,
            incoming: Some(incoming.to_string()),
            incoming_label: label.map(ToString::to_string),
        }
    }

    #[test]
    fn test_derive_backlinks_puts_a_relation_on_its_declared_targets() {
        // Given
        let mut req = bare_type("req");
        req.relations = vec![relation("links", Some(vec!["spec"]), "linked_by", None)];
        let types = vec![req, bare_type("spec")];

        // When
        let table = derive_backlinks(&types).unwrap();

        // Then — the back-link appears on the target, not on the source
        assert!(!table.contains_key("req"));
        let spec_links = &table["spec"];
        assert_eq!(spec_links.len(), 1);
        assert_eq!(spec_links[0].name, "linked_by");
        assert_eq!(
            spec_links[0].sources,
            vec![BacklinkSource {
                from_type: "req".to_string(),
                relation: "links".to_string(),
            }]
        );
    }

    #[test]
    fn test_derive_backlinks_expands_an_undeclared_target_set_to_every_type() {
        // Given — `to` omitted means any type
        let mut req = bare_type("req");
        req.relations = vec![relation("links", None, "linked_by", None)];
        let types = vec![req, bare_type("spec")];

        // When
        let table = derive_backlinks(&types).unwrap();

        // Then
        assert!(table.contains_key("req"));
        assert!(table.contains_key("spec"));
    }

    #[test]
    fn test_derive_backlinks_merges_two_relations_into_one_backlink() {
        // Given — two source types feeding the same back-link on one target
        let mut req = bare_type("req");
        req.relations = vec![relation(
            "links",
            Some(vec!["spec"]),
            "linked_by",
            Some("Linked by"),
        )];
        let mut test = bare_type("test");
        test.relations = vec![relation(
            "verifies",
            Some(vec!["spec"]),
            "linked_by",
            Some("Linked by"),
        )];
        let types = vec![req, test, bare_type("spec")];

        // When
        let table = derive_backlinks(&types).unwrap();

        // Then — one bucket, two sources
        let spec_links = &table["spec"];
        assert_eq!(spec_links.len(), 1);
        assert_eq!(spec_links[0].sources.len(), 2);
    }

    #[test]
    fn test_derive_backlinks_rejects_two_labels_for_one_backlink() {
        // Given — the same target sees both, so there is no coherent heading
        let mut req = bare_type("req");
        req.relations = vec![relation(
            "links",
            Some(vec!["spec"]),
            "linked_by",
            Some("Linked by"),
        )];
        let mut test = bare_type("test");
        test.relations = vec![relation(
            "verifies",
            Some(vec!["spec"]),
            "linked_by",
            Some("Verified by"),
        )];
        let types = vec![req, test, bare_type("spec")];

        // When
        let errors = derive_backlinks(&types).unwrap_err();

        // Then
        assert!(matches!(
            errors.as_slice(),
            [SchemaError::BacklinkLabelConflict { incoming, .. }] if incoming == "linked_by"
        ));
    }

    #[test]
    fn test_derive_backlinks_allows_differing_labels_on_disjoint_targets() {
        // Given — no entity ever sees both, so rejecting this would refuse a valid schema
        let mut req = bare_type("req");
        req.relations = vec![relation(
            "links",
            Some(vec!["spec"]),
            "linked_by",
            Some("Linked by"),
        )];
        let mut test = bare_type("test");
        test.relations = vec![relation(
            "verifies",
            Some(vec!["impl"]),
            "linked_by",
            Some("Verified by"),
        )];
        let types = vec![req, test, bare_type("spec"), bare_type("impl")];

        // When
        let table = derive_backlinks(&types).unwrap();

        // Then
        assert_eq!(table["spec"][0].label, "Linked by");
        assert_eq!(table["impl"][0].label, "Verified by");
    }

    #[test]
    fn test_derive_backlinks_rejects_a_name_clashing_with_an_attribute() {
        // Given
        let mut req = bare_type("req");
        req.relations = vec![relation("links", Some(vec!["spec"]), "status", None)];
        let mut spec = bare_type("spec");
        spec.attributes = vec![AttributeSchema {
            name: "status".to_string(),
            label: None,
            value_type: AttributeType::String,
            required: false,
            default: None,
        }];
        let types = vec![req, spec];

        // When
        let errors = derive_backlinks(&types).unwrap_err();

        // Then
        assert!(matches!(
            errors.as_slice(),
            [SchemaError::BacklinkNameClash {
                clashes_with: DeclarationKind::Attribute,
                ..
            }]
        ));
    }

    #[test]
    fn test_derive_backlinks_rejects_a_name_clashing_with_an_outgoing_relation() {
        // Given — a `spec` writing `:links:` and also receiving a derived `links`
        let mut req = bare_type("req");
        req.relations = vec![relation("links", Some(vec!["spec"]), "links", None)];
        let mut spec = bare_type("spec");
        spec.relations = vec![relation("links", Some(vec!["impl"]), "linked_by", None)];
        let types = vec![req, spec, bare_type("impl")];

        // When
        let errors = derive_backlinks(&types).unwrap_err();

        // Then
        assert!(matches!(
            errors.as_slice(),
            [SchemaError::BacklinkNameClash {
                clashes_with: DeclarationKind::Relation,
                ..
            }]
        ));
    }

    #[test]
    fn test_derive_backlinks_rejects_a_name_clashing_with_a_section() {
        // Given
        let mut req = bare_type("req");
        req.relations = vec![relation("links", Some(vec!["spec"]), "rationale", None)];
        let mut spec = bare_type("spec");
        spec.sections = vec![SectionSpec {
            name: "rationale".to_string(),
            label: None,
            required: false,
            multiple: false,
        }];
        let types = vec![req, spec];

        // When
        let errors = derive_backlinks(&types).unwrap_err();

        // Then
        assert!(matches!(
            errors.as_slice(),
            [SchemaError::BacklinkNameClash {
                clashes_with: DeclarationKind::Section,
                ..
            }]
        ));
    }

    #[test]
    fn test_derive_backlinks_skips_a_one_directional_relation() {
        // Given — `incoming` omitted derives nothing
        let mut req = bare_type("req");
        req.relations = vec![RelationSpec {
            name: "links".to_string(),
            label: None,
            to: Some(vec!["spec".to_string()]),
            required: false,
            multiple: true,
            incoming: None,
            incoming_label: None,
        }];
        let types = vec![req, bare_type("spec")];

        // When
        let table = derive_backlinks(&types).unwrap();

        // Then
        assert!(table.is_empty());
    }

    #[test]
    fn test_derive_backlinks_falls_back_to_the_incoming_name_as_the_label() {
        // Given
        let mut req = bare_type("req");
        req.relations = vec![relation("links", Some(vec!["spec"]), "linked_by", None)];
        let types = vec![req, bare_type("spec")];

        // When
        let table = derive_backlinks(&types).unwrap();

        // Then
        assert_eq!(table["spec"][0].label, "linked_by");
    }

    #[test]
    fn test_derive_backlinks_returns_an_empty_table_for_a_schema_without_relations() {
        // Given
        let types = vec![bare_type("req")];

        // When
        let table = derive_backlinks(&types).unwrap();

        // Then
        assert!(table.is_empty());
    }

    #[test]
    fn test_targets_of_expands_only_when_no_types_are_declared() {
        // Given
        let types = vec![bare_type("req"), bare_type("spec")];

        // When
        let declared = targets_of(Some(&["spec".to_string()]), &types);
        let any = targets_of(None, &types);

        // Then
        assert_eq!(declared, vec!["spec".to_string()]);
        assert_eq!(any, vec!["req".to_string(), "spec".to_string()]);
    }

    #[test]
    fn test_declared_kind_reports_the_owning_declaration() {
        // Given
        let mut entity_type = bare_type("spec");
        entity_type.attributes = vec![AttributeSchema {
            name: "status".to_string(),
            label: None,
            value_type: AttributeType::String,
            required: false,
            default: None,
        }];
        entity_type.sections = vec![SectionSpec {
            name: "rationale".to_string(),
            label: None,
            required: false,
            multiple: false,
        }];
        entity_type.relations = vec![relation("links", None, "linked_by", None)];

        // When / Then
        assert_eq!(
            declared_kind(&entity_type, "status"),
            Some(DeclarationKind::Attribute)
        );
        assert_eq!(
            declared_kind(&entity_type, "rationale"),
            Some(DeclarationKind::Section)
        );
        assert_eq!(
            declared_kind(&entity_type, "links"),
            Some(DeclarationKind::Relation)
        );
        assert_eq!(declared_kind(&entity_type, "unused"), None);
    }
}

//! Applying an entity type's declared attributes and relations to the values
//! collected for one instance.
//!
//! Split out of [`super::entity`] because these are not about the `.. req::`
//! *directive* at all — they are about the schema. Two callers reach them, and
//! the whole point of the split is that they cannot diverge: a written
//! `.. req::` and one imported from a `needs.json` by [`super::needimport`]
//! must accept exactly the same values, apply the same defaults and report the
//! same cardinality faults. A second implementation would drift precisely
//! where an author would notice least — an enum that validated when typed and
//! not when imported.
//!
//! Nothing here reads source text. Every function takes values already
//! extracted from wherever they were written, which is what lets one JSON
//! field and one `:option:` line arrive at the same place.

use std::collections::BTreeMap;

use rusty_sphinx_ast::{AttributeValue, Diagnostic, DiagnosticCode, EntityId, Span};
use rusty_sphinx_entity::{EntityType, parse_attribute_value, split_list};

use crate::diagnostics::Diagnostics;

/// Parses one attribute's text and records it, or diagnoses why it could not.
pub(in crate::directives) fn store_attribute(
    schema: &rusty_sphinx_entity::AttributeSchema,
    name: &str,
    text: &str,
    span: Option<Span>,
    attributes: &mut BTreeMap<String, AttributeValue>,
    diagnostics: &mut Diagnostics,
) {
    match parse_attribute_value(&schema.value_type, text) {
        Ok(value) => {
            attributes.insert(name.to_string(), value);
        }
        Err(error) => diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityInvalidAttributeValue,
            format!("`:{name}:`: {error}"),
            span,
        )),
    }
}

/// Reads a relation option's comma-separated ids, diagnosing illegal ones.
pub(in crate::directives) fn collect_relation_targets(
    text: &str,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> Vec<EntityId> {
    let mut targets = Vec::new();
    for written in split_list(text) {
        match EntityId::new(&written) {
            Ok(id) => targets.push(id),
            Err(error) => diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityInvalidId,
                format!("link target {written:?}: {error}"),
                span,
            )),
        }
    }
    targets
}

/// Supplies the declared default for every attribute left unset.
pub(in crate::directives) fn apply_defaults(
    entity_type: &EntityType,
    attributes: &mut BTreeMap<String, AttributeValue>,
) {
    for schema in &entity_type.attributes {
        let Some(default) = schema.default.as_deref() else {
            continue;
        };
        if attributes.contains_key(&schema.name) {
            continue;
        }
        // A default that does not fit its own type is a schema fault, not an
        // author's, and this is not the phase that reports it — a value that
        // fails to parse here is simply left unset.
        if let Ok(value) = parse_attribute_value(&schema.value_type, default) {
            attributes.insert(schema.name.clone(), value);
        }
    }
}

/// Reports every `required` attribute the entity left unset.
pub(in crate::directives) fn report_missing_attributes(
    entity_type: &EntityType,
    attributes: &BTreeMap<String, AttributeValue>,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    for schema in &entity_type.attributes {
        if schema.required && !attributes.contains_key(&schema.name) {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityMissingRequiredAttribute,
                format!("`.. {}::` requires `:{}:`", entity_type.name, schema.name),
                span,
            ));
        }
    }
}

/// Reports relations written too few or too many times.
pub(in crate::directives) fn report_relation_cardinality(
    entity_type: &EntityType,
    relations: &BTreeMap<String, Vec<EntityId>>,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    for relation in &entity_type.relations {
        let targets = relations.get(&relation.name).map_or(0, Vec::len);
        if relation.required && targets == 0 {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityMissingRequiredRelation,
                format!(
                    "`.. {}::` requires at least one target on `:{}:`",
                    entity_type.name, relation.name
                ),
                span,
            ));
        }
        if !relation.multiple && targets > 1 {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityMultipleRelationTargets,
                format!(
                    "`:{}:` takes one target, but {targets} were given",
                    relation.name
                ),
                span,
            ));
        }
    }
}

/// Reports an id that does not match its type's declared pattern.
///
/// The id is only reported, never replaced: it is still legal, links to it
/// resolve, and the author's fix is to rename it rather than to have the
/// build do so behind their back.
pub(in crate::directives) fn report_id_pattern(
    entity_type: &EntityType,
    id: &EntityId,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    if let Err(mismatch) = entity_type.id.check_pattern(id) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityIdPatternMismatch,
            format!("`.. {}::`: {mismatch}", entity_type.name),
            span,
        ));
    }
}

/// Lists a type's option vocabulary for an unknown-option diagnostic.
pub(in crate::directives) fn describe_options(entity_type: &EntityType) -> String {
    let names = entity_type.option_names();
    names
        .iter()
        .map(|name| format!(":{name}:"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};

    /// A type declaring one attribute of each shape these helpers care about,
    /// plus the two relation cardinalities.
    fn schema() -> EntitySchema {
        load_schema(
            r#"
            [[entity_type]]
            name = "req"
            argument = { fields = ["title"] }
            id = { pattern = "^REQ_" }

              [[entity_type.attribute]]
              name = "title"
              type = "string"

              [[entity_type.attribute]]
              name = "status"
              type = "enum"
              values = ["open", "closed"]
              default = "open"

              [[entity_type.attribute]]
              name = "priority"
              type = "int"

              [[entity_type.attribute]]
              name = "ticket"
              type = "string"
              pattern = "^JIRA-[0-9]+$"

              [[entity_type.attribute]]
              name = "owner"
              type = "string"
              required = true

              [[entity_type.relation]]
              name = "links"
              to = ["req"]
              multiple = true

              [[entity_type.relation]]
              name = "supersedes"
              to = ["req"]
              required = true
            "#,
            &NoReservedNames,
        )
        .expect("the fixture schema is valid")
    }

    fn req(schema: &EntitySchema) -> &EntityType {
        schema.entity_type("req").expect("the fixture declares req")
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
        diagnostics.entries().iter().map(|d| d.code).collect()
    }

    #[test]
    fn store_attribute_records_a_value_that_fits_its_type() {
        // Given an int attribute and a value that parses
        let schema = schema();
        let attribute = req(&schema).attribute("priority").expect("declared");
        let mut attributes = BTreeMap::new();
        let mut diagnostics = Diagnostics::default();

        // When it is stored
        store_attribute(
            attribute,
            "priority",
            "3",
            None,
            &mut attributes,
            &mut diagnostics,
        );

        // Then it is recorded as the declared type, with nothing reported
        assert_eq!(attributes.get("priority"), Some(&AttributeValue::Int(3)));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn store_attribute_reports_a_value_outside_its_type() {
        // Given an enum attribute and a value it does not permit
        let schema = schema();
        let attribute = req(&schema).attribute("status").expect("declared");
        let mut attributes = BTreeMap::new();
        let mut diagnostics = Diagnostics::default();

        // When it is stored
        store_attribute(
            attribute,
            "status",
            "wobbly",
            None,
            &mut attributes,
            &mut diagnostics,
        );

        // Then nothing is recorded and the fault names the option
        assert!(attributes.is_empty());
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::EntityInvalidAttributeValue]
        );
        assert!(diagnostics.entries()[0].message.contains(":status:"));
    }

    #[test]
    fn collect_relation_targets_splits_a_comma_separated_list() {
        // Given several ids written as one option value
        let mut diagnostics = Diagnostics::default();

        // When they are collected
        let targets = collect_relation_targets("REQ_1, REQ_2 , REQ_3", None, &mut diagnostics);

        // Then each becomes an id, with nothing reported
        let written: Vec<&str> = targets.iter().map(EntityId::as_str).collect();
        assert_eq!(written, ["REQ_1", "REQ_2", "REQ_3"]);
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn collect_relation_targets_keeps_the_legal_ids_beside_an_illegal_one() {
        // Given a list holding one id with an illegal character
        let mut diagnostics = Diagnostics::default();

        // When they are collected
        let targets = collect_relation_targets("REQ_1, not a id!, REQ_2", None, &mut diagnostics);

        // Then the legal ones survive and only the bad one is reported
        let written: Vec<&str> = targets.iter().map(EntityId::as_str).collect();
        assert_eq!(written, ["REQ_1", "REQ_2"]);
        assert_eq!(codes(&diagnostics), [DiagnosticCode::EntityInvalidId]);
    }

    #[test]
    fn collect_relation_targets_yields_nothing_for_empty_text() {
        // Given an option with no value
        let mut diagnostics = Diagnostics::default();

        // When targets are collected
        let targets = collect_relation_targets("  ", None, &mut diagnostics);

        // Then there are none, and the emptiness is the caller's to report
        assert!(targets.is_empty());
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn apply_defaults_fills_only_the_attributes_left_unset() {
        // Given a status already set and every other attribute unset
        let schema = schema();
        let mut attributes = BTreeMap::from([(
            "status".to_string(),
            AttributeValue::String("closed".to_string()),
        )]);

        // When defaults are applied
        apply_defaults(req(&schema), &mut attributes);

        // Then the written value stands and no attribute without a default
        // was invented
        assert_eq!(
            attributes.get("status"),
            Some(&AttributeValue::String("closed".to_string()))
        );
        assert!(!attributes.contains_key("priority"));
        assert!(!attributes.contains_key("owner"));
    }

    #[test]
    fn apply_defaults_supplies_a_declared_default() {
        // Given nothing set at all
        let schema = schema();
        let mut attributes = BTreeMap::new();

        // When defaults are applied
        apply_defaults(req(&schema), &mut attributes);

        // Then the one attribute declaring a default gets it
        assert_eq!(
            attributes.get("status"),
            Some(&AttributeValue::String("open".to_string()))
        );
        assert_eq!(attributes.len(), 1);
    }

    #[test]
    fn report_missing_attributes_names_each_required_one_left_unset() {
        // Given an entity that set neither of the interesting attributes
        let schema = schema();
        let attributes = BTreeMap::new();
        let mut diagnostics = Diagnostics::default();

        // When required attributes are checked
        report_missing_attributes(req(&schema), &attributes, None, &mut diagnostics);

        // Then only the `required` one is reported, naming it
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::EntityMissingRequiredAttribute]
        );
        assert!(diagnostics.entries()[0].message.contains(":owner:"));
    }

    #[test]
    fn report_missing_attributes_stays_quiet_when_the_required_one_is_set() {
        // Given the required attribute set
        let schema = schema();
        let attributes = BTreeMap::from([(
            "owner".to_string(),
            AttributeValue::String("alice".to_string()),
        )]);
        let mut diagnostics = Diagnostics::default();

        // When required attributes are checked
        report_missing_attributes(req(&schema), &attributes, None, &mut diagnostics);

        // Then nothing is reported
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn report_relation_cardinality_reports_a_missing_required_relation() {
        // Given an entity naming no target for a required relation
        let schema = schema();
        let relations = BTreeMap::new();
        let mut diagnostics = Diagnostics::default();

        // When cardinality is checked
        report_relation_cardinality(req(&schema), &relations, None, &mut diagnostics);

        // Then the required relation is reported and the optional one is not
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::EntityMissingRequiredRelation]
        );
        assert!(diagnostics.entries()[0].message.contains(":supersedes:"));
    }

    #[test]
    fn report_relation_cardinality_reports_several_targets_on_a_single_relation() {
        // Given two targets on a relation declared without `multiple`
        let schema = schema();
        let relations = BTreeMap::from([(
            "supersedes".to_string(),
            vec![
                EntityId::new("REQ_1").expect("legal"),
                EntityId::new("REQ_2").expect("legal"),
            ],
        )]);
        let mut diagnostics = Diagnostics::default();

        // When cardinality is checked
        report_relation_cardinality(req(&schema), &relations, None, &mut diagnostics);

        // Then the count is reported
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::EntityMultipleRelationTargets]
        );
        assert!(diagnostics.entries()[0].message.contains('2'));
    }

    #[test]
    fn report_relation_cardinality_accepts_several_targets_on_a_multiple_relation() {
        // Given two targets on a relation declared `multiple`, and the
        // required relation satisfied
        let schema = schema();
        let relations = BTreeMap::from([
            (
                "links".to_string(),
                vec![
                    EntityId::new("REQ_1").expect("legal"),
                    EntityId::new("REQ_2").expect("legal"),
                ],
            ),
            (
                "supersedes".to_string(),
                vec![EntityId::new("REQ_3").expect("legal")],
            ),
        ]);
        let mut diagnostics = Diagnostics::default();

        // When cardinality is checked
        report_relation_cardinality(req(&schema), &relations, None, &mut diagnostics);

        // Then nothing is reported
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn store_attribute_reports_a_value_outside_its_pattern_and_drops_it() {
        // Given
        let schema = schema();
        let ticket = req(&schema).attribute("ticket").unwrap();
        let mut attributes = BTreeMap::new();
        let mut diagnostics = Diagnostics::default();

        // When
        store_attribute(
            ticket,
            "ticket",
            "BUG-7",
            None,
            &mut attributes,
            &mut diagnostics,
        );

        // Then — handled exactly like a value outside an enum
        assert!(attributes.is_empty());
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::EntityInvalidAttributeValue]
        );
        assert!(diagnostics.entries()[0].message.contains("^JIRA-[0-9]+$"));
    }

    #[test]
    fn report_id_pattern_reports_an_id_outside_the_pattern() {
        // Given
        let schema = schema();
        let id = EntityId::new("SPEC_1").unwrap();
        let mut diagnostics = Diagnostics::default();

        // When
        report_id_pattern(req(&schema), &id, None, &mut diagnostics);

        // Then
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::EntityIdPatternMismatch]
        );
        assert_eq!(
            diagnostics.entries()[0].message,
            "`.. req::`: id \"SPEC_1\" does not match the pattern `^REQ_`"
        );
    }

    #[test]
    fn report_id_pattern_stays_quiet_for_a_matching_id() {
        // Given
        let schema = schema();
        let id = EntityId::new("REQ_1").unwrap();
        let mut diagnostics = Diagnostics::default();

        // When
        report_id_pattern(req(&schema), &id, None, &mut diagnostics);

        // Then
        assert!(codes(&diagnostics).is_empty());
    }

    #[test]
    fn describe_options_lists_the_whole_vocabulary_as_written() {
        // Given a type declaring attributes and relations
        let schema = schema();

        // When its options are described
        let described = describe_options(req(&schema));

        // Then every option name appears in the `:name:` spelling an author
        // would have typed
        for name in [
            ":id:",
            ":title:",
            ":status:",
            ":priority:",
            ":owner:",
            ":links:",
            ":supersedes:",
        ] {
            assert!(described.contains(name), "{described} should hold {name}");
        }
    }
}

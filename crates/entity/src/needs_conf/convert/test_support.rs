//! What the conversion's tests share.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::load::{RawAttribute, RawEntityType, RawSchema};
use crate::needs_conf::{NeedsConfig, ReportCategory, ReportEntry, SchemasFile, convert_needs};

/// Converts a `[needs]` table written as JSON, with an optional
/// `schemas.json`.
pub(super) fn convert(needs: Value, schemas: Option<Value>) -> (RawSchema, Vec<ReportEntry>) {
    let config: NeedsConfig = serde_json::from_value(needs).expect("a sphinx-needs configuration");
    let schemas: Option<SchemasFile> =
        schemas.map(|value| serde_json::from_value(value).expect("a schemas.json"));
    convert_needs(&config, schemas.as_ref())
}

/// A `schemas.json` holding one rule selecting exactly `type_name`.
pub(super) fn type_rule(rule_id: &str, type_name: &str, local: Value) -> Value {
    let mut rule = json!({"schemas": [{
    "id": rule_id,
    "select": {"$ref": format!("#/$defs/type-{type_name}")},
    "validate": {},
    }]});
    rule["schemas"][0]["validate"]["local"] = local;
    rule
}

/// The entity type named `name`.
pub(super) fn entity<'a>(schema: &'a RawSchema, name: &str) -> &'a RawEntityType {
    schema
        .entity_type
        .iter()
        .find(|entity| entity.name == name)
        .expect("the type is converted")
}

/// The attribute named `name` of `entity`.
pub(super) fn attribute<'a>(entity: &'a RawEntityType, name: &str) -> Option<&'a RawAttribute> {
    entity
        .attribute
        .iter()
        .find(|attribute| attribute.name == name)
}

/// The categories a report holds.
pub(super) fn categories(report: &[ReportEntry]) -> BTreeSet<ReportCategory> {
    report.iter().map(|entry| entry.category).collect()
}

/// The report's value-rule entries alone, without the unrelated ones every
/// conversion carries (such as the generic `.. need::` directive).
pub(super) fn value_rules(report: &[ReportEntry]) -> Vec<&ReportEntry> {
    report
        .iter()
        .filter(|entry| entry.category == ReportCategory::ValueSchemaRule)
        .collect()
}

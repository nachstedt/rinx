//! Reading a sphinx-needs `schemas.json` for the rules the entity model can
//! express.
//!
//! Those are the rules selecting exactly one need type, requiring an option on
//! it or constraining an option's value. Every other rule — a conditional
//! `allOf` select, a `network` (cross-entity) constraint — is reported
//! instead: the entity schema has no vocabulary for them, and silently
//! honouring the `required` half of a conditional rule would be worse than not
//! honouring it at all.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

use super::ordered_table::OrderedTable;
use super::report::{ReportCategory, ReportEntry};

/// A `schemas.json`, as far as the conversion reads it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SchemasFile {
    pub schemas: Vec<SchemaRule>,
}

/// One rule of a `schemas.json`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SchemaRule {
    pub id: Option<String>,
    pub select: Option<Value>,
    pub validate: Option<RuleValidation>,
}

/// What a rule checks.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct RuleValidation {
    /// Checks on the selected need's own values.
    pub local: Option<LocalValidation>,
    /// Checks on the needs it links to.
    pub network: Option<Value>,
}

/// Checks on one need's own values.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct LocalValidation {
    /// Option name → its JSON-Schema fragment.
    pub properties: OrderedTable<Fragment>,
    /// Option names that must be given.
    pub required: Vec<String>,
}

/// One `properties.<name>` value: a JSON-Schema object, or something that
/// is none.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Fragment {
    Object(OrderedTable<Value>),
    Other(Value),
}

impl SchemasFile {
    /// Reads `text`, a `schemas.json`.
    ///
    /// # Errors
    ///
    /// Returns the JSON error when `text` is no JSON of this shape.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// What `schemas.json` narrows onto one need type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeConstraints {
    /// Option names that must be given.
    pub required: BTreeSet<String>,
    /// The pattern every id of the type must match.
    pub id_pattern: Option<String>,
    /// Option name → the pattern its value must match.
    pub patterns: BTreeMap<String, String>,
    /// Option name → the values it is restricted to.
    pub enums: BTreeMap<String, Vec<String>>,
    /// Option name → the rule that constrained its value, for the report when
    /// the type turns out to have nothing to put the constraint on.
    pub sources: BTreeMap<String, String>,
}

/// The `$ref` a rule selecting exactly one need type writes.
static TYPE_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#/\$defs/type-(?P<name>[A-Za-z0-9_]+)$").expect("valid regex"));

/// The constraints each need type's type-scoped rules narrow onto it,
/// reporting every rule, and every part of one, that cannot be carried over.
#[must_use]
pub fn constraints_by_type(
    schemas: &SchemasFile,
    report: &mut Vec<ReportEntry>,
) -> BTreeMap<String, TypeConstraints> {
    let mut by_type: BTreeMap<String, TypeConstraints> = BTreeMap::new();
    for rule in &schemas.schemas {
        let rule_id = rule.id.as_deref().unwrap_or("<unnamed>");
        let reference = rule
            .select
            .as_ref()
            .and_then(|select| select.get("$ref"))
            .and_then(Value::as_str);
        let Some(type_name) = reference
            .and_then(|reference| TYPE_REF.captures(reference))
            .map(|captures| captures["name"].to_string())
        else {
            report.push(ReportEntry::new(
                ReportCategory::ConditionalSchemaRule,
                format!("{rule_id}: selects on a condition, not a type"),
            ));
            continue;
        };
        let validation = rule.validate.clone().unwrap_or_default();
        if validation.network.as_ref().is_some_and(is_set) {
            report.push(ReportEntry::new(
                ReportCategory::NetworkSchemaRule,
                format!("{rule_id}: constrains linked entities"),
            ));
        }
        let local = validation.local.unwrap_or_default();
        let constraints = by_type.entry(type_name).or_default();
        for (name, fragment) in local.properties.iter() {
            translate_property(rule_id, name, fragment, constraints, report);
        }
        constraints.required.extend(local.required);
    }
    by_type
}

/// Whether a configuration value counts as set, as Python's truthiness has
/// it: not `null`, `false`, zero, or an empty string, list or table.
pub(super) fn is_set(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(set) => *set,
        Value::Number(number) => number.as_f64() != Some(0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(entries) => !entries.is_empty(),
    }
}

/// Whether `keyword: value` means something the entity model already
/// enforces, and so needs nothing emitted.
///
/// A field's declared type already fixes `type`, and a value of `""` or `[]`
/// is treated as unset — in sphinx-needs' own validation as much as here,
/// since its `reduce_need` strips both before checking — so a minimum of one
/// is either implied by `required` or has no effect without it.
fn already_enforced(keyword: &str, value: &Value) -> bool {
    match keyword {
        "type" => matches!(value.as_str(), Some("string" | "array")),
        "minLength" | "minItems" => value.as_f64() == Some(1.0),
        _ => false,
    }
}

/// Carries one `properties.<name>` fragment of a type-scoped rule over onto
/// its type, reporting each keyword that has no equivalent.
///
/// `{}` constrains nothing and yields nothing. The report names the keyword
/// rather than the rule alone, so it says what is actually missing.
fn translate_property(
    rule_id: &str,
    name: &str,
    fragment: &Fragment,
    constraints: &mut TypeConstraints,
    report: &mut Vec<ReportEntry>,
) {
    let Fragment::Object(fragment) = fragment else {
        report.push(ReportEntry::new(
            ReportCategory::ValueSchemaRule,
            format!("{rule_id}: `{name}` is not a JSON-Schema object"),
        ));
        return;
    };
    let mut leftover: Vec<(&str, &Value)> = Vec::new();
    for (keyword, value) in fragment.iter() {
        if already_enforced(keyword, value) {
            continue;
        }
        match (keyword, value) {
            ("pattern", Value::String(pattern)) if name == "id" => match &constraints.id_pattern {
                Some(kept) if kept != pattern => report.push(ReportEntry::new(
                    ReportCategory::ValueSchemaRule,
                    format!("{rule_id}: a second id pattern for one type; `{kept}` is kept"),
                )),
                _ => constraints.id_pattern = Some(pattern.clone()),
            },
            ("pattern", Value::String(pattern)) => {
                constraints
                    .patterns
                    .insert(name.to_string(), pattern.clone());
                constraints
                    .sources
                    .insert(name.to_string(), rule_id.to_string());
            }
            ("enum", Value::Array(values)) if name != "id" => {
                constraints
                    .enums
                    .insert(name.to_string(), values.iter().map(python_str).collect());
                constraints
                    .sources
                    .insert(name.to_string(), rule_id.to_string());
            }
            _ => leftover.push((keyword, value)),
        }
    }
    for (keyword, value) in leftover {
        report.push(ReportEntry::new(
            ReportCategory::ValueSchemaRule,
            format!(
                "{rule_id}: `{name}` uses `{keyword}: {value}`, which the entity model cannot express"
            ),
        ));
    }
}

/// A value as Python's `str()` writes it — how sphinx-needs compares an
/// enum's values, and the only spelling an entity schema's enum has.
pub(super) fn python_str(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Null => "None".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schemas(value: Value) -> SchemasFile {
        serde_json::from_value(value).expect("a schemas.json")
    }

    /// A `schemas.json` holding one rule selecting exactly `type_name`.
    pub(crate) fn type_rule(rule_id: &str, type_name: &str, local: Value) -> Value {
        let mut rule = json!({"schemas": [{
        "id": rule_id,
        "select": {"$ref": format!("#/$defs/type-{type_name}")},
        "validate": {},
        }]});
        rule["schemas"][0]["validate"]["local"] = local;
        rule
    }

    fn categories(report: &[ReportEntry]) -> BTreeSet<ReportCategory> {
        report.iter().map(|entry| entry.category).collect()
    }

    #[test]
    fn test_unconditionally_required_option_is_narrowed_onto_one_type() {
        // Given a rule requiring `role` on `person` only
        let file = schemas(type_rule(
            "person-has-role",
            "person",
            json!({"required": ["role"]}),
        ));

        // When
        let constraints = constraints_by_type(&file, &mut Vec::new());

        // Then
        assert_eq!(
            constraints["person"].required,
            BTreeSet::from(["role".to_string()])
        );
    }

    #[test]
    fn test_a_conditional_rule_is_reported_rather_than_honoured() {
        // Given a rule that requires an option only for documents under a path
        let file = schemas(json!({"schemas": [{
            "id": "automotive-req-status-required",
            "select": {"allOf": [{"$ref": "#/$defs/type-req"}]},
            "validate": {"local": {"required": ["status"]}},
        }]}));
        let mut report = Vec::new();

        // When
        let constraints = constraints_by_type(&file, &mut report);

        // Then nothing is narrowed — honouring half of a conditional rule would
        // be worse than not honouring it — and the rule is reported
        assert!(constraints.is_empty());
        assert_eq!(
            categories(&report),
            BTreeSet::from([ReportCategory::ConditionalSchemaRule])
        );
    }

    #[test]
    fn test_a_network_rule_is_reported() {
        // Given a cross-entity constraint
        let file = schemas(json!({"schemas": [{
            "id": "spec-links-to-req",
            "select": {"$ref": "#/$defs/type-spec"},
            "validate": {"network": {"links": {}}},
        }]}));
        let mut report = Vec::new();

        // When
        let _ = constraints_by_type(&file, &mut report);

        // Then
        assert_eq!(
            categories(&report),
            BTreeSet::from([ReportCategory::NetworkSchemaRule])
        );
    }

    #[test]
    fn test_an_inexpressible_keyword_is_reported_by_name() {
        // Given a minimum the entity model has no vocabulary for
        let file = schemas(type_rule(
            "person-long-role",
            "person",
            json!({"properties": {"role": {"minLength": 3, "pattern": "."}}}),
        ));
        let mut report = Vec::new();

        // When
        let constraints = constraints_by_type(&file, &mut report);

        // Then the pattern is still taken, and only the keyword is reported
        assert_eq!(
            constraints["person"].patterns,
            BTreeMap::from([("role".to_string(), ".".to_string())])
        );
        assert_eq!(
            report,
            [ReportEntry::new(
                ReportCategory::ValueSchemaRule,
                "person-long-role: `role` uses `minLength: 3`, which the entity model cannot express"
            )]
        );
    }

    #[test]
    fn test_a_second_id_pattern_for_one_type_is_reported() {
        // Given two rules each constraining `req`'s id differently
        let mut first = type_rule("a", "req", json!({"properties": {"id": {"pattern": "^A"}}}));
        let second = type_rule("b", "req", json!({"properties": {"id": {"pattern": "^B"}}}));
        first["schemas"]
            .as_array_mut()
            .expect("a list")
            .extend(second["schemas"].as_array().expect("a list").clone());
        let mut report = Vec::new();

        // When
        let constraints = constraints_by_type(&schemas(first), &mut report);

        // Then the first is kept, and the conflict is reported
        assert_eq!(constraints["req"].id_pattern.as_deref(), Some("^A"));
        assert_eq!(
            categories(&report),
            BTreeSet::from([ReportCategory::ValueSchemaRule])
        );
    }

    #[test]
    fn test_minimums_of_one_and_type_keywords_need_nothing_reported() {
        // Given the demo corpus' `person-has-role` and `team-has-persons` shapes
        let file = schemas(type_rule(
            "person-rules",
            "person",
            json!({"properties": {
                "role": {"type": "string", "minLength": 1},
                "persons": {"type": "array", "minItems": 1},
                "links": {},
            }}),
        ));
        let mut report = Vec::new();

        // When
        let constraints = constraints_by_type(&file, &mut report);

        // Then
        assert!(report.is_empty());
        assert!(constraints["person"].patterns.is_empty());
    }

    #[test]
    fn test_an_enum_is_read_as_strings() {
        // Given
        let file = schemas(type_rule(
            "req-effort",
            "req",
            json!({"properties": {"effort": {"enum": [1, "two", true]}}}),
        ));

        // When
        let constraints = constraints_by_type(&file, &mut Vec::new());

        // Then
        assert_eq!(constraints["req"].enums["effort"], ["1", "two", "True"]);
        assert_eq!(constraints["req"].sources["effort"], "req-effort");
    }

    #[test]
    fn test_a_fragment_that_is_no_object_is_reported() {
        // Given
        let file = schemas(type_rule("r", "req", json!({"properties": {"x": true}})));
        let mut report = Vec::new();

        // When
        let _ = constraints_by_type(&file, &mut report);

        // Then
        assert_eq!(
            report,
            [ReportEntry::new(
                ReportCategory::ValueSchemaRule,
                "r: `x` is not a JSON-Schema object"
            )]
        );
    }

    #[test]
    fn test_is_set_follows_python_truthiness() {
        assert!(!is_set(&json!(null)));
        assert!(!is_set(&json!(false)));
        assert!(!is_set(&json!(0)));
        assert!(!is_set(&json!("")));
        assert!(!is_set(&json!([])));
        assert!(!is_set(&json!({})));
        assert!(is_set(&json!({"a": 1})));
        assert!(is_set(&json!("x")));
        assert!(is_set(&json!(2)));
    }

    #[test]
    fn test_python_str_writes_values_as_python_does() {
        assert_eq!(python_str(&json!("a")), "a");
        assert_eq!(python_str(&json!(5)), "5");
        assert_eq!(python_str(&json!(false)), "False");
        assert_eq!(python_str(&json!(null)), "None");
    }

    #[test]
    fn test_from_json_reads_a_schemas_file() {
        // Given
        let text = r#"{"$defs": {}, "schemas": [{"id": "a"}]}"#;

        // When
        let file = SchemasFile::from_json(text).expect("valid");

        // Then
        assert_eq!(file.schemas.len(), 1);
        assert_eq!(file.schemas[0].id.as_deref(), Some("a"));
    }
}

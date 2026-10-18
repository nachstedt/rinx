use serde_json::json;

use super::test_support::categories;
use super::*;

fn config(value: serde_json::Value) -> NeedsConfig {
    serde_json::from_value(value).expect("a configuration")
}

#[test]
fn test_constraints_variants_and_global_options_are_reported() {
    // Given a configuration using the three sphinx-needs features the entity
    // model has no vocabulary for
    let needs = config(json!({
        "constraints": {"release_set": {}},
        "variants": {"var": "x"},
        "global_options": {"status": "open"},
        "fields": {"jira": {"parse_variants": true}},
    }));

    // When
    let report = unsupported_constructs(&needs);

    // Then
    let found = categories(&report);
    assert!(found.contains(&ReportCategory::Constraints));
    assert!(found.contains(&ReportCategory::Variants));
    assert!(found.contains(&ReportCategory::GlobalOptions));
    assert!(report.iter().any(|entry| entry.detail
        == "1 [needs.constraints] rule(s): value constraints evaluated at build time"));
    assert!(
        report
            .iter()
            .any(|entry| entry.detail == "[needs.fields.jira] parses variant syntax")
    );
}

#[test]
fn test_empty_settings_are_not_reported() {
    // Given settings written but empty, as Python reads them unset
    let needs = config(
        json!({"constraints": {}, "variants": {}, "global_options": {}, "types": [{"directive": "need"}]}),
    );

    // When
    let report = unsupported_constructs(&needs);

    // Then
    assert!(report.is_empty());
}

#[test]
fn test_the_generic_need_directive_is_reported() {
    // Given a configuration that does not declare `need` as a type of its
    // own — `.. need::` then names its type in an option, which
    // one-directive-per-type cannot express
    let needs = config(json!({"types": [{"directive": "req"}]}));

    // When
    let report = unsupported_constructs(&needs);

    // Then
    assert!(categories(&report).contains(&ReportCategory::GenericNeedDirective));
}

#[test]
fn test_a_project_declaring_need_as_a_type_is_using_the_spelling() {
    // Given a project whose own vocabulary includes a type called `need`
    let needs = config(json!({"types": [{"directive": "need"}]}));

    // When
    let report = unsupported_constructs(&needs);

    // Then nothing is reported — it is an ordinary type, not the built-in
    assert!(!categories(&report).contains(&ReportCategory::GenericNeedDirective));
}

use serde_json::json;

use super::test_support::categories;
use super::*;

fn links(value: serde_json::Value) -> OrderedTable<NeedLink> {
    let mut wrapped = json!({});
    wrapped["links"] = value;
    let config: NeedsConfig = serde_json::from_value(wrapped).expect("a configuration");
    config.all_links()
}

#[test]
fn test_link_becomes_a_relation_with_a_derived_backlink_option() {
    // Given one link type with both labels
    let declared = links(json!({"reqs": {"outgoing": "specifies", "incoming": "specified by"}}));

    // When
    let relations = relations_from_links(&declared, &mut Vec::new());

    // Then the key is the option spelling, the two sphinx-needs strings are
    // labels, and the back-link takes sphinx-needs' own `<name>_back` spelling
    assert_eq!(
        relations,
        [RawRelation {
            name: "reqs".into(),
            label: Some("specifies".into()),
            to: None,
            required: false,
            multiple: true,
            incoming: Some("reqs_back".into()),
            incoming_label: Some("specified by".into()),
        }]
    );
}

#[test]
fn test_a_link_without_labels_is_labelled_by_its_options() {
    // Given
    let declared = links(json!({"links": {}}));

    // When
    let relations = relations_from_links(&declared, &mut Vec::new());

    // Then `to` is left off too, which is how the entity model spells "any type"
    assert_eq!(relations[0].label.as_deref(), Some("links"));
    assert_eq!(relations[0].incoming_label.as_deref(), Some("links_back"));
    assert_eq!(relations[0].to, None);
}

#[test]
fn test_a_link_computed_by_a_dynamic_function_is_reported() {
    // Given sphinx-test-reports' auto-linked `runs`
    let declared = links(json!({"runs": {"predicates": [["type=='test'", "[[tr_link()]]"]]}}));
    let mut report = Vec::new();

    // When
    let relations = relations_from_links(&declared, &mut report);

    // Then the relation is still declared, and the fact that nothing fills it
    // in is reported rather than hidden
    assert_eq!(relations.len(), 1);
    assert!(categories(&report).contains(&ReportCategory::LinkPredicates));
}

#[test]
fn test_a_copied_link_is_reported() {
    // Given
    let declared = links(json!({"links": {"copy": true}}));
    let mut report = Vec::new();

    // When
    let _ = relations_from_links(&declared, &mut report);

    // Then
    assert_eq!(
        report,
        [ReportEntry::new(
            ReportCategory::LinkCopy,
            "[needs.links.links] sets copy = true"
        )]
    );
}

#[test]
fn test_extra_links_are_links_too() {
    // Given the older list spelling
    let config: NeedsConfig = serde_json::from_value(json!({
        "extra_links": [{"option": "blocks", "outgoing": "blocks", "incoming": "is blocked by"}],
    }))
    .expect("a configuration");

    // When
    let relations = relations_from_links(&config.all_links(), &mut Vec::new());

    // Then
    assert_eq!(relations[0].name, "blocks");
    assert_eq!(
        relations[0].incoming_label.as_deref(),
        Some("is blocked by")
    );
}

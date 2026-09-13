use std::collections::BTreeMap;

use rusty_sphinx_ast::{AttributeValue, EntityId, EntityPie, EntityPieSource, PieSlice};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rusty_sphinx_filter::parse_filter;
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use super::count_wedges;

fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
          [[entity_type.attribute]]
          name = "status"
          type = "string"

        [[entity_type]]
        name = "test"
          [[entity_type.relation]]
          name = "verifies"
          to = ["req"]
          incoming = "verified_by"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn id(raw: &str) -> EntityId {
    EntityId::new(raw).unwrap()
}

fn requirement(status: &str, doc: &str) -> EntityRecord {
    EntityRecord {
        type_name: "req".to_string(),
        doc_path: doc.to_string(),
        title: Some("A requirement".to_string()),
        attributes: BTreeMap::from([(
            "status".to_string(),
            AttributeValue::String(status.to_string()),
        )]),
        outgoing: BTreeMap::new(),
        uml: BTreeMap::new(),
    }
}

/// Three requirements — two open, one closed — and one test.
fn index() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_1"), requirement("open", "specs/boot"));
    index
        .entities
        .insert(id("REQ_2"), requirement("open", "specs/boot"));
    index
        .entities
        .insert(id("REQ_3"), requirement("closed", "other/power"));
    index.entities.insert(
        id("TEST_1"),
        EntityRecord {
            type_name: "test".to_string(),
            doc_path: "tests/boot".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::from([("verifies".to_string(), vec![id("REQ_1")])]),
            uml: BTreeMap::new(),
        },
    );
    index
}

/// A chart whose wedges are `filters`, narrowed by an optional `:filter:`.
fn pie(filters: &[&str], prefilter: Option<&str>) -> EntityPie {
    let mut pie = EntityPie::new(EntityPieSource::EntityPie);
    pie.filter = prefilter.map(|text| parse_filter(text).expect("expected the filter to parse"));
    pie.slices = filters
        .iter()
        .map(|text| {
            PieSlice::from_filter(Some(
                parse_filter(text).expect("expected the filter to parse"),
            ))
        })
        .collect();
    pie
}

/// Just the counts, for tests about selection.
fn counts(pie: &EntityPie) -> Vec<u64> {
    count_wedges(pie, &index(), &schema())
        .iter()
        .map(|wedge| wedge.count)
        .collect()
}

#[test]
fn test_each_wedge_counts_the_entities_its_filter_selects() {
    // Given
    let pie = pie(&[r#"type == "req""#, r#"type == "test""#], None);

    // When
    let counted = counts(&pie);

    // Then
    assert_eq!(counted, [3, 1]);
}

#[test]
fn test_wedges_may_overlap_because_each_is_asked_independently() {
    // Given — nothing partitions the graph, so an entity may land in two
    // wedges; that is the author's business, not this module's
    let pie = pie(&[r#"type == "req""#, r#"status == "open""#], None);

    // When
    let counted = counts(&pie);

    // Then
    assert_eq!(counted, [3, 2]);
}

#[test]
fn test_the_chart_filter_narrows_every_wedge() {
    // Given — so a chart can be scoped once rather than in every content line
    let pie = pie(
        &[r#"status == "open""#, r#"status == "closed""#],
        Some(r#""specs" in docname"#),
    );

    // When
    let counted = counts(&pie);

    // Then — REQ_3 is `closed` but lives outside `specs/`
    assert_eq!(counted, [2, 0]);
}

#[test]
fn test_a_wedge_whose_filter_could_not_be_parsed_counts_everything() {
    // Given — the encoding the node uses, and the rule every filtered
    // directive here follows
    let mut pie = pie(&[r#"type == "req""#], None);
    pie.slices.push(PieSlice::from_filter(None));

    // When
    let counted = counts(&pie);

    // Then
    assert_eq!(counted, [3, 4]);
}

#[test]
fn test_a_written_number_is_used_as_the_wedges_size() {
    // Given
    let mut pie = pie(&[r#"type == "req""#], None);
    pie.slices.push(PieSlice::from_count(9));

    // When
    let counted = counts(&pie);

    // Then
    assert_eq!(counted, [3, 9]);
}

#[test]
fn test_a_chart_of_written_numbers_alone_never_reads_the_index() {
    // Given — the one case a chart costs nothing to draw
    let mut pie = EntityPie::new(EntityPieSource::EntityPie);
    pie.slices = vec![PieSlice::from_count(4), PieSlice::from_count(6)];

    // When — against an index with no entities at all
    let wedges = count_wedges(&pie, &ProjectIndex::default(), &schema());

    // Then
    assert_eq!(wedges.iter().map(|w| w.count).collect::<Vec<_>>(), [4, 6]);
}

#[test]
fn test_a_wedge_takes_its_label_from_the_slice() {
    // Given
    let mut pie = pie(&[r#"type == "req""#], None);
    pie.slices[0].label = Some("Requirements".to_string());

    // When
    let wedges = count_wedges(&pie, &index(), &schema());

    // Then
    assert_eq!(wedges[0].label, "Requirements");
}

#[test]
fn test_an_unlabelled_wedge_falls_back_to_its_position() {
    // Given — a wedge with no name still needs one on the chart
    let pie = pie(&[r#"type == "req""#, r#"type == "test""#], None);

    // When
    let wedges = count_wedges(&pie, &index(), &schema());

    // Then
    assert_eq!(wedges[1].label, "#2");
}

#[test]
fn test_a_startswith_filter_counts_by_prefix() {
    // Given — the spelling eight corpus wedges use
    let pie = pie(&[r#"id.startswith("REQ_")"#], None);

    // When
    let counted = counts(&pie);

    // Then
    assert_eq!(counted, [3]);
}

#[test]
fn test_a_chart_matching_nothing_counts_zero_rather_than_failing() {
    // Given — evaluation is total, so an empty answer is still an answer; the
    // renderer is what decides that a chart of nothing cannot be drawn
    let pie = pie(&[r#"type == "nothing""#], None);

    // When
    let counted = counts(&pie);

    // Then
    assert_eq!(counted, [0]);
}

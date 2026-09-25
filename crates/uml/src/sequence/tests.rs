use std::collections::BTreeMap;
use std::num::NonZeroU32;

use rusty_sphinx_ast::{EntityId, EntitySequence, EntitySequenceSource, NonEmptyVector};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use super::*;

fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "component"

          [[entity_type.relation]]
          name = "sends"
          to = ["message"]

          [[entity_type.relation]]
          name = "stops"
          to = ["message"]

        [[entity_type]]
        name = "message"

          [[entity_type.relation]]
          name = "sends"
          to = ["component"]

          [[entity_type.relation]]
          name = "stops"
          to = ["component"]
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn entity(type_name: &str, title: &str, outgoing: &[(&str, &[&str])]) -> EntityRecord {
    EntityRecord {
        type_name: type_name.to_string(),
        doc_path: "arch".to_string(),
        title: Some(title.to_string()),
        attributes: BTreeMap::new(),
        outgoing: outgoing
            .iter()
            .map(|(relation, targets)| {
                (
                    (*relation).to_string(),
                    targets
                        .iter()
                        .map(|id| EntityId::new(id).expect("a valid id"))
                        .collect(),
                )
            })
            .collect(),
        uml: BTreeMap::new(),
    }
}

/// The benchmark corpus's startup sequence, trimmed to four components: the
/// UI starts the HAL and the safety monitor, and the monitor starts the
/// temperature controller. The HAL also has a shutdown message of its own.
fn startup() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for (id, record) in [
        (
            "COMP_UI",
            entity("component", "UI Module", &[("sends", &["S01", "S03"])]),
        ),
        (
            "COMP_HAL",
            entity(
                "component",
                "HAL",
                &[("sends", &["S02", "S05"]), ("stops", &["X01"])],
            ),
        ),
        (
            "COMP_SAFETY",
            entity("component", "Safety Monitor", &[("sends", &["S04", "S06"])]),
        ),
        (
            "COMP_TEMP",
            entity("component", "Temp Controller", &[("sends", &["S07"])]),
        ),
        (
            "S01",
            entity("message", "init()", &[("sends", &["COMP_HAL"])]),
        ),
        (
            "S02",
            entity("message", "init_ok", &[("sends", &["COMP_UI"])]),
        ),
        (
            "S03",
            entity("message", "start safety", &[("sends", &["COMP_SAFETY"])]),
        ),
        (
            "S04",
            entity("message", "read_sensors()", &[("sends", &["COMP_HAL"])]),
        ),
        (
            "S05",
            entity("message", "temp=22", &[("sends", &["COMP_SAFETY"])]),
        ),
        (
            "S06",
            entity("message", "start temp", &[("sends", &["COMP_TEMP"])]),
        ),
        (
            "S07",
            entity("message", "started", &[("sends", &["COMP_SAFETY"])]),
        ),
        ("X01", entity("message", "halt", &[("stops", &["COMP_UI"])])),
    ] {
        index
            .entities
            .insert(EntityId::new(id).expect("a valid id"), record);
    }
    index
}

fn sequence(start: &[&str], relations: &[&str]) -> EntitySequence {
    let ids: Vec<EntityId> = start
        .iter()
        .map(|id| EntityId::new(id).expect("a valid id"))
        .collect();
    let relations: Vec<String> = relations.iter().map(ToString::to_string).collect();
    EntitySequence::new(
        EntitySequenceSource::EntitySequence,
        NonEmptyVector::try_from(ids).expect("a start"),
        NonEmptyVector::try_from(relations).expect("a relation"),
    )
}

fn walk_over(index: &ProjectIndex, sequence: &EntitySequence) -> SequenceDrawing {
    let schema = schema();
    let ctx = UmlContext::new(index, &schema, "arch");
    build_sequence(sequence, &ctx)
}

fn walk(sequence: &EntitySequence) -> SequenceDrawing {
    walk_over(&startup(), sequence)
}

fn text(sequence: &EntitySequence) -> String {
    let drawing = walk(sequence);
    drawing
        .content
        .unwrap_or_else(|| panic!("expected a picture, got {:?}", drawing.problems))
        .body()
        .to_string()
}

fn lines_starting(text: &str, prefix: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.starts_with(prefix))
        .map(ToString::to_string)
        .collect()
}

fn arrows(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.contains(" -> "))
        .map(ToString::to_string)
        .collect()
}

#[test]
fn test_the_walk_draws_messages_depth_first_as_sphinx_needs_does() {
    // Given
    let sequence = sequence(&["COMP_UI"], &["sends"]);

    // When
    let text = text(&sequence);

    // Then — each receiver is walked before the sender's next message
    assert_eq!(
        arrows(&text),
        [
            "COMP_UI -> COMP_HAL : init()",
            "COMP_HAL -> COMP_UI : init_ok",
            "COMP_HAL -> COMP_SAFETY : temp=22",
            "COMP_SAFETY -> COMP_HAL : read_sensors()",
            "COMP_SAFETY -> COMP_TEMP : start temp",
            "COMP_TEMP -> COMP_SAFETY : started",
            "COMP_UI -> COMP_SAFETY : start safety",
        ]
    );
    assert!(walk(&sequence).problems.is_empty());
}

#[test]
fn test_participants_are_declared_in_walk_order_before_any_message() {
    // Given
    let sequence = sequence(&["COMP_UI"], &["sends"]);

    // When
    let text = text(&sequence);

    // Then
    let participants = lines_starting(&text, "participant");
    assert_eq!(participants.len(), 4);
    assert!(participants[0].starts_with("participant \"UI Module\" as COMP_UI"));
    assert!(participants[1].contains("as COMP_HAL"));
    assert!(participants[2].contains("as COMP_SAFETY"));
    assert!(participants[3].contains("as COMP_TEMP"));
    let first_arrow = text.find(" -> ").unwrap();
    assert!(text.rfind("participant").unwrap() < first_arrow, "{text}");
}

#[test]
fn test_a_participant_links_to_its_entitys_anchor() {
    // Given
    let sequence = sequence(&["COMP_UI"], &["sends"]);

    // When
    let text = text(&sequence);

    // Then
    assert!(
        text.contains("as COMP_UI [[arch.html#entity-COMP_UI]]"),
        "{text}"
    );
}

#[test]
fn test_the_markers_plantuml_requires_are_added_around_the_picture() {
    // Given
    let sequence = sequence(&["COMP_UI"], &["sends"]);

    // When
    let text = text(&sequence);

    // Then
    assert!(text.starts_with("@startuml\n"), "{text}");
    assert!(text.ends_with("\n@enduml"), "{text}");
}

#[test]
fn test_only_the_relations_named_are_walked() {
    // Given — the HAL's shutdown message is on a relation not named
    let sequence = sequence(&["COMP_HAL"], &["stops"]);

    // When
    let text = text(&sequence);

    // Then
    assert_eq!(arrows(&text), ["COMP_HAL -> COMP_UI : halt"]);
}

#[test]
fn test_several_relations_are_followed_in_the_order_named() {
    // Given
    let sequence = sequence(&["COMP_HAL"], &["stops", "sends"]);

    // When
    let text = text(&sequence);

    // Then — `stops` first, so the halt precedes everything `sends` reaches
    assert_eq!(arrows(&text)[0], "COMP_HAL -> COMP_UI : halt");
}

#[test]
fn test_a_second_start_continues_rather_than_redrawing_the_first() {
    // Given — the safety monitor is already reached from the UI
    let sequence = sequence(&["COMP_UI", "COMP_SAFETY"], &["sends"]);

    // When
    let text = text(&sequence);

    // Then — one visited set across starts: nothing declared or drawn twice
    assert_eq!(lines_starting(&text, "participant").len(), 4);
    assert_eq!(arrows(&text).len(), 7);
}

#[test]
fn test_the_filter_drops_a_receiver_and_everything_only_it_reaches() {
    // Given — the temperature controller is filtered out
    let mut sequence = sequence(&["COMP_UI"], &["sends"]);
    sequence.filter =
        Some(rusty_sphinx_filter::parse_filter(r#"id != "COMP_TEMP""#).expect("a valid filter"));

    // When
    let text = text(&sequence);

    // Then
    assert!(!text.contains("COMP_TEMP"), "{text}");
    assert_eq!(arrows(&text).len(), 5);
}

#[test]
fn test_the_filter_never_drops_a_start() {
    // Given — a filter the start itself would fail
    let mut sequence = sequence(&["COMP_UI"], &["sends"]);
    sequence.filter =
        Some(rusty_sphinx_filter::parse_filter(r#"id != "COMP_UI""#).expect("a valid filter"));

    // When
    let text = text(&sequence);

    // Then — the UI still sends; only messages *to* it are dropped
    assert!(text.contains("COMP_UI -> COMP_HAL"), "{text}");
    assert!(!text.contains("COMP_HAL -> COMP_UI"), "{text}");
}

#[test]
fn test_max_items_draws_the_first_messages_and_reports_the_total() {
    // Given
    let mut sequence = sequence(&["COMP_UI"], &["sends"]);
    sequence.max_items = NonZeroU32::new(2);

    // When
    let drawing = walk(&sequence);

    // Then
    assert_eq!(drawing.truncation(), Some((2, 7)));
    assert_eq!(
        drawing.problems,
        [SequenceError::Truncated { shown: 2, total: 7 }]
    );
    let text = drawing.content.expect("a picture").body().to_string();
    assert_eq!(
        arrows(&text),
        [
            "COMP_UI -> COMP_HAL : init()",
            "COMP_HAL -> COMP_UI : init_ok",
        ]
    );
}

#[test]
fn test_max_items_still_declares_the_receiver_of_the_last_drawn_message() {
    // Given — the cap is exhausted by the arrow to the HAL, which then reaches
    // its own declaration with no room left
    let mut sequence = sequence(&["COMP_UI"], &["sends"]);
    sequence.max_items = NonZeroU32::new(1);

    // When
    let text = text(&sequence);

    // Then — both ends are declared with their titles, and nothing more
    let participants = lines_starting(&text, "participant");
    assert_eq!(participants.len(), 2, "{text}");
    assert!(participants[1].starts_with("participant \"HAL\" as COMP_HAL"));
}

#[test]
fn test_a_receiver_that_never_sends_is_still_declared_by_title() {
    // Given — the UI's halt message reaches nothing that walks on
    let sequence = sequence(&["COMP_HAL"], &["stops"]);

    // When
    let text = text(&sequence);

    // Then
    assert!(
        text.contains("participant \"UI Module\" as COMP_UI"),
        "{text}"
    );
}

#[test]
fn test_an_unknown_start_is_reported_and_the_rest_still_drawn() {
    // Given
    let sequence = sequence(&["COMP_404", "COMP_HAL"], &["stops"]);

    // When
    let drawing = walk(&sequence);

    // Then
    assert_eq!(
        drawing.problems,
        [SequenceError::UnknownStart("COMP_404".to_string())]
    );
    assert!(drawing.content.is_some());
}

#[test]
fn test_only_unknown_starts_report_nothing_further() {
    // Given
    let sequence = sequence(&["COMP_404"], &["sends"]);

    // When
    let drawing = walk(&sequence);

    // Then — "the walk found nothing" would only restate why
    assert_eq!(drawing.content, None);
    assert_eq!(
        drawing.problems,
        [SequenceError::UnknownStart("COMP_404".to_string())]
    );
}

#[test]
fn test_a_start_that_sends_nothing_is_an_empty_result() {
    // Given — a message entity sends along no relation named
    let sequence = sequence(&["COMP_TEMP"], &["stops"]);

    // When
    let drawing = walk(&sequence);

    // Then
    assert_eq!(drawing.content, None);
    assert_eq!(drawing.problems, [SequenceError::EmptyResult]);
}

#[test]
fn test_a_target_no_document_declares_is_left_out() {
    // Given — the UI sends a message nobody wrote
    let mut index = startup();
    index.entities.insert(
        EntityId::new("COMP_UI").unwrap(),
        entity("component", "UI Module", &[("sends", &["S99", "S01"])]),
    );
    let sequence = sequence(&["COMP_UI"], &["sends"]);

    // When
    let drawing = walk_over(&index, &sequence);

    // Then
    assert!(drawing.problems.is_empty(), "{:?}", drawing.problems);
    let text = drawing.content.expect("a picture").body().to_string();
    assert!(!text.contains("S99"), "{text}");
}

#[test]
fn test_an_id_plantuml_cannot_read_is_aliased_consistently() {
    // Given — a `-` would be read as subtraction
    let mut index = ProjectIndex::default();
    for (id, record) in [
        ("COMP-A", entity("component", "A", &[("sends", &["M-1"])])),
        ("COMP-B", entity("component", "B", &[])),
        ("M-1", entity("message", "ping", &[("sends", &["COMP-B"])])),
    ] {
        index.entities.insert(EntityId::new(id).unwrap(), record);
    }
    let sequence = sequence(&["COMP-A"], &["sends"]);

    // When
    let drawing = walk_over(&index, &sequence);

    // Then — every alias an arrow names is one a declaration introduced
    let text = drawing.content.expect("a picture").body().to_string();
    assert!(!text.contains("COMP-A ->"), "{text}");
    let arrow = &arrows(&text)[0];
    let (from, rest) = arrow.split_once(" -> ").unwrap();
    let (to, _) = rest.split_once(" : ").unwrap();
    for alias in [from, to] {
        assert!(text.contains(&format!("as {alias} ")), "{text}");
    }
}

#[test]
fn test_an_unknown_config_is_reported() {
    // Given
    let mut sequence = sequence(&["COMP_UI"], &["sends"]);
    sequence.config = Some("monochrome".to_string());

    // When
    let drawing = walk(&sequence);

    // Then
    assert_eq!(drawing.content, None);
    assert_eq!(
        drawing.problems,
        [SequenceError::UnknownConfig("monochrome".to_string())]
    );
}

#[test]
fn test_a_builtin_config_is_prepended() {
    // Given
    let mut sequence = sequence(&["COMP_UI"], &["sends"]);
    sequence.config = Some("toptobottom".to_string());

    // When
    let text = text(&sequence);

    // Then
    assert!(text.contains("top to bottom direction"), "{text}");
}

#[test]
fn test_an_unchanged_graph_draws_identical_bytes() {
    // Given
    let sequence = sequence(&["COMP_UI", "COMP_HAL"], &["sends", "stops"]);

    // When
    let first = text(&sequence);
    let second = text(&sequence);

    // Then — the hash names the compiled SVG
    assert_eq!(first, second);
}

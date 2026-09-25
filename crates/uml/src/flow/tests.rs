use std::collections::BTreeMap;

use rinx_ast::{EntityFlow, EntityFlowSource, EntityId, FlowDirection};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rinx_index::{EntityRecord, ProjectIndex};

use super::*;

/// A schema with two types and two relations, one of them labelled.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"

        [[entity_type]]
        name = "test"
        label = "Test Case"

          [[entity_type.relation]]
          name = "verifies"
          label = "Verifies"
          to = ["req"]
          incoming = "verified_by"

          [[entity_type.relation]]
          name = "depends_on"
          to = ["test"]
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

/// One entity of `type_name` on `doc_path`, with the outgoing edges given.
fn entity(type_name: &str, title: &str, outgoing: &[(&str, &[&str])]) -> EntityRecord {
    EntityRecord {
        type_name: type_name.to_string(),
        doc_path: "specs".to_string(),
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

/// A project holding two requirements and the test that verifies one of them.
fn project() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for (id, record) in [
        ("REQ_001", entity("req", "Login", &[])),
        ("REQ_002", entity("req", "Logout", &[])),
        (
            "TEST_001",
            entity("test", "Login works", &[("verifies", &["REQ_001"])]),
        ),
    ] {
        index
            .entities
            .insert(EntityId::new(id).expect("a valid id"), record);
    }
    index
}

/// Draws `flow` over `project()`, returning the finished `PlantUML` text.
fn draw(flow: &EntityFlow) -> String {
    let index = project();
    let schema = schema();
    let ctx = UmlContext::new(&index, &schema, "specs");
    build_flow(flow, &ctx)
        .expect("the flowchart should draw")
        .body()
        .to_string()
}

/// A flowchart with `filter` as its selection.
fn flow_filtered(filter: &str) -> EntityFlow {
    EntityFlow {
        filter: Some(rinx_filter::parse_filter(filter).expect("a valid filter")),
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    }
}

#[test]
fn test_a_flowchart_draws_a_node_per_entity_in_id_order() {
    // Given
    let flow = EntityFlow::new(EntityFlowSource::EntityFlow);

    // When
    let text = draw(&flow);

    // Then
    let nodes: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("rectangle"))
        .collect();
    assert_eq!(nodes.len(), 3);
    assert!(nodes[0].contains("REQ_001"), "{text}");
    assert!(nodes[1].contains("REQ_002"), "{text}");
    assert!(nodes[2].contains("TEST_001"), "{text}");
}

#[test]
fn test_a_node_carries_the_entitys_title_type_and_a_link_to_its_anchor() {
    // Given
    let flow = flow_filtered(r#"id == "REQ_001""#);

    // When
    let text = draw(&flow);

    // Then — a clickable node must land where a `:ref:` to the entity would
    assert!(text.contains("Login"), "{text}");
    assert!(text.contains("req: REQ_001"), "{text}");
    assert!(text.contains("[[specs.html#entity-REQ_001]]"), "{text}");
}

#[test]
fn test_the_markers_plantuml_requires_are_added_around_the_picture() {
    // Given
    let flow = EntityFlow::new(EntityFlowSource::EntityFlow);

    // When
    let text = draw(&flow);

    // Then
    assert!(text.starts_with("@startuml\n"), "{text}");
    assert!(text.ends_with("\n@enduml"), "{text}");
}

#[test]
fn test_a_relation_between_two_drawn_entities_becomes_an_edge() {
    // Given
    let flow = EntityFlow::new(EntityFlowSource::EntityFlow);

    // When
    let text = draw(&flow);

    // Then
    assert!(text.contains("TEST_001 --> REQ_001"), "{text}");
}

#[test]
fn test_an_edge_to_an_entity_the_filter_excluded_is_not_drawn() {
    // Given — a filter selects a subgraph, and PlantUML would otherwise invent
    // an unlabelled box for every entity the filter left out
    let flow = flow_filtered(r#"type == "test""#);

    // When
    let text = draw(&flow);

    // Then
    assert!(text.contains("TEST_001"), "{text}");
    assert!(!text.contains("-->"), "{text}");
    assert!(!text.contains("REQ_001"), "{text}");
}

#[test]
fn test_only_the_relations_named_become_edges() {
    // Given
    let flow = EntityFlow {
        relations: Some(vec!["depends_on".to_string()]),
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    };

    // When
    let text = draw(&flow);

    // Then — `verifies` is declared, and deliberately not drawn
    assert!(!text.contains("-->"), "{text}");
}

#[test]
fn test_show_link_names_labels_an_edge_with_the_schemas_own_label() {
    // Given
    let flow = EntityFlow {
        show_link_names: true,
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    };

    // When
    let text = draw(&flow);

    // Then — the label a rendered entity shows above its outgoing links
    assert!(text.contains("TEST_001 --> REQ_001 : Verifies"), "{text}");
}

#[test]
fn test_a_direction_is_emitted_only_when_it_is_not_plantuml_s_default() {
    // Given
    let default = EntityFlow::new(EntityFlowSource::EntityFlow);
    let sideways = EntityFlow {
        direction: FlowDirection::LeftToRight,
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    };

    // When
    let plain = draw(&default);
    let turned = draw(&sideways);

    // Then
    assert!(!plain.contains("direction"), "{plain}");
    assert!(turned.contains("left to right direction"), "{turned}");
}

#[test]
fn test_a_filter_matching_nothing_is_reported_rather_than_compiled() {
    // Given — PlantUML rejects an empty diagram, so compiling one would fail
    // the build with a syntax error naming a file the author never wrote
    let flow = flow_filtered(r#"type == "spec""#);
    let index = project();
    let schema = schema();
    let ctx = UmlContext::new(&index, &schema, "specs");

    // When
    let error = build_flow(&flow, &ctx).expect_err("nothing was drawn");

    // Then
    assert_eq!(error, FlowError::EmptyResult);
}

#[test]
fn test_a_config_preamble_lands_inside_the_markers() {
    // Given
    let flow = EntityFlow {
        config: Some("mono".to_string()),
        ..flow_filtered(r#"id == "REQ_001""#)
    };
    let configs = BTreeMap::from([("mono".to_string(), "skinparam monochrome true".to_string())]);
    let index = project();
    let schema = schema();
    let ctx = UmlContext::new(&index, &schema, "specs").with_configs(&configs);

    // When
    let text = build_flow(&flow, &ctx).expect("the flowchart should draw");

    // Then
    assert!(
        text.body()
            .starts_with("@startuml\nskinparam monochrome true\n"),
        "{}",
        text.body()
    );
}

#[test]
fn test_a_config_naming_no_preamble_is_refused() {
    // Given
    let flow = EntityFlow {
        config: Some("mono".to_string()),
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    };
    let index = project();
    let schema = schema();
    let ctx = UmlContext::new(&index, &schema, "specs");

    // When
    let error = build_flow(&flow, &ctx).expect_err("refused");

    // Then
    assert_eq!(error, FlowError::UnknownConfig("mono".to_string()));
}

#[test]
fn test_drawing_the_same_project_twice_produces_the_same_bytes() {
    // Given — the hash is the SVG's filename, so bytes that varied between
    // builds would recompile every diagram in the site
    let flow = EntityFlow {
        show_link_names: true,
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    };

    // When
    let first = draw(&flow);
    let second = draw(&flow);

    // Then
    assert_eq!(first, second);
}

#[test]
fn test_an_omitted_relations_option_draws_every_relation_the_schema_declares() {
    // Given — sphinx-needs defaults to `links`, which a schema here need not
    // declare at all
    let flow = EntityFlow::new(EntityFlowSource::EntityFlow);

    // When
    let text = draw(&flow);

    // Then
    assert!(text.contains("TEST_001 --> REQ_001"), "{text}");
}

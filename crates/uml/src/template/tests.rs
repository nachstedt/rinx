use std::collections::BTreeMap;

use rinx_ast::AttributeValue;
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;

use super::*;
use crate::snapshot::tests::{index_with, requirement};

/// Expands `template` against a project holding two requirements.
fn expand_against(index: &ProjectIndex, template: &str) -> Result<String, UmlError> {
    let snapshot = Snapshot::build(index, EntitySchema::empty_ref(), "index.rst");
    render(template, snapshot, &BTreeMap::new(), None)
}

fn two_requirements() -> ProjectIndex {
    index_with(vec![
        ("REQ_001", requirement("reqs.rst", Some("Login"))),
        ("REQ_002", requirement("reqs.rst", Some("Logout"))),
    ])
}

#[test]
fn test_a_template_with_no_markup_passes_through_unchanged() {
    // Given
    let index = ProjectIndex::default();

    // When
    let text = expand_against(&index, "A -> B").expect("expansion succeeds");

    // Then
    assert_eq!(text, "A -> B");
}

#[test]
fn test_need_reads_one_entitys_fields() {
    // Given
    let index = two_requirements();

    // When
    let text = expand_against(&index, "{{ need('REQ_001').title }}").expect("expansion succeeds");

    // Then
    assert_eq!(text, "Login");
}

#[test]
fn test_need_reports_an_id_nothing_declares() {
    // Given
    let index = two_requirements();

    // When
    let error = expand_against(&index, "{{ need('REQ_404').title }}").expect_err("no such entity");

    // Then — the structured failure survives, rather than being flattened into
    // a generic template error the author cannot suppress by name
    assert_eq!(error, UmlError::UnknownEntity("REQ_404".to_string()));
}

#[test]
fn test_needs_holds_every_entity_by_id() {
    // Given
    let index = two_requirements();

    // When
    let text = expand_against(&index, "{% for id in needs %}{{ id }} {% endfor %}")
        .expect("expansion succeeds");

    // Then — in id order, which is what makes the output hashable
    assert_eq!(text, "REQ_001 REQ_002 ");
}

#[test]
fn test_filter_selects_by_the_projects_own_filter_language() {
    // Given
    let mut open = requirement("reqs.rst", Some("Open"));
    open.attributes.insert(
        "status".to_string(),
        AttributeValue::String("open".to_string()),
    );
    let index = index_with(vec![
        ("REQ_001", open),
        ("REQ_002", requirement("reqs.rst", Some("Other"))),
    ]);

    // When
    let text = expand_against(&index, "{{ filter('status == \"open\"') | join(',') }}")
        .expect("expansion succeeds");

    // Then
    assert_eq!(text, "REQ_001");
}

#[test]
fn test_filter_refuses_unsupported_python_by_name() {
    // Given — the filter language's whole point: an unsupported construct is
    // named rather than reported as a generic syntax error
    let index = two_requirements();

    // When
    let error = expand_against(&index, "{{ filter('len(tags) == 0') }}").expect_err("refused");

    // Then
    let UmlError::InvalidFilter { message, .. } = &error else {
        panic!("expected an invalid filter, found {error:?}");
    };
    assert!(message.contains("function call"), "{message}");
}

#[test]
fn test_flow_draws_a_node_aliased_by_the_entity_id() {
    // Given — a template draws edges between nodes by id, so the alias has to
    // be the id
    let index = two_requirements();

    // When
    let text = expand_against(&index, "{{ flow('REQ_001') }}").expect("expansion succeeds");

    // Then
    assert!(text.starts_with("rectangle \"Login"), "{text}");
    assert!(text.contains(" as REQ_001"), "{text}");
}

#[test]
fn test_flow_makes_the_node_clickable() {
    // Given
    let index = two_requirements();

    // When
    let text = expand_against(&index, "{{ flow('REQ_001') }}").expect("expansion succeeds");

    // Then — the link lands on the anchor the renderer writes for the entity
    assert!(text.contains("[[reqs.html#entity-REQ_001]]"), "{text}");
}

#[test]
fn test_flow_reports_an_id_nothing_declares() {
    // Given
    let index = two_requirements();

    // When
    let error = expand_against(&index, "{{ flow('REQ_404') }}").expect_err("no such entity");

    // Then
    assert_eq!(error, UmlError::UnknownEntity("REQ_404".to_string()));
}

#[test]
fn test_ref_links_by_the_entitys_title() {
    // Given
    let index = two_requirements();

    // When
    let text = expand_against(&index, "{{ ref('REQ_001') }}").expect("expansion succeeds");

    // Then
    assert_eq!(text, "[[reqs.html#entity-REQ_001 Login]]");
}

#[test]
fn test_ref_takes_the_text_it_is_given() {
    // Given
    let index = two_requirements();

    // When
    let text =
        expand_against(&index, "{{ ref('REQ_001', 'see it') }}").expect("expansion succeeds");

    // Then
    assert_eq!(text, "[[reqs.html#entity-REQ_001 see it]]");
}

#[test]
fn test_a_quote_in_a_title_cannot_end_a_plantuml_string_early() {
    // Given — a title is prose, so it may hold anything; a stray quote breaks
    // the diagram rather than the page, where the failure is a PlantUML syntax
    // error about a line nobody wrote
    let index = index_with(vec![(
        "REQ_001",
        requirement("reqs.rst", Some("The \"login\" flow")),
    )]);

    // When
    let text = expand_against(&index, "{{ flow('REQ_001') }}").expect("expansion succeeds");

    // Then
    assert!(!text.contains("\"login\""), "{text}");
    assert!(text.contains("'login'"), "{text}");
}

#[test]
fn test_a_syntax_error_is_reported_with_the_line_it_was_found_on() {
    // Given — a template whose second line is unterminated
    let index = ProjectIndex::default();

    // When
    let error = expand_against(&index, "A -> B\n{{ unterminated").expect_err("a syntax error");

    // Then
    let UmlError::Template { line, .. } = &error else {
        panic!("expected a template error, found {error:?}");
    };
    assert_eq!(*line, Some(2));
}

#[test]
fn test_extra_values_are_bound_by_name() {
    // Given
    let index = ProjectIndex::default();
    let extra = BTreeMap::from([("role".to_string(), "owner".to_string())]);
    let snapshot = Snapshot::build(&index, EntitySchema::empty_ref(), "index.rst");

    // When
    let text = render("{{ role }}", snapshot, &extra, None).expect("expansion succeeds");

    // Then
    assert_eq!(text, "owner");
}

#[test]
fn test_the_enclosing_entity_is_bound_as_need() {
    // Given — what an `.. entity-arch::` exists for
    let index = two_requirements();
    let snapshot = Snapshot::build(&index, EntitySchema::empty_ref(), "index.rst");

    // When
    let text = render(
        "{{ need.title }}",
        snapshot,
        &BTreeMap::new(),
        Some("REQ_001"),
    )
    .expect("expansion succeeds");

    // Then
    assert_eq!(text, "Login");
}

#[test]
fn test_an_extra_cannot_shadow_the_enclosing_entity() {
    // Given — an `.. entity-arch::` whose `need` is not the need it sits in
    // would draw a convincing picture of the wrong thing
    let index = two_requirements();
    let extra = BTreeMap::from([("need".to_string(), "not-a-need".to_string())]);
    let snapshot = Snapshot::build(&index, EntitySchema::empty_ref(), "index.rst");

    // When
    let text =
        render("{{ need.title }}", snapshot, &extra, Some("REQ_001")).expect("expansion succeeds");

    // Then
    assert_eq!(text, "Login");
}

#[test]
fn test_expansion_of_the_same_project_is_byte_identical() {
    // Given — the bytes are hashed into the compiled SVG's filename
    let index = two_requirements();
    let template = "{% for id in filter('type == \"req\"') %}{{ flow(id) }}\n{% endfor %}";

    // When
    let first = expand_against(&index, template).expect("expansion succeeds");
    let second = expand_against(&index, template).expect("expansion succeeds");

    // Then
    assert_eq!(first, second);
}

/// A requirement carrying the diagram template `uml`, under `key`.
fn requirement_drawing(
    doc_path: &str,
    title: &str,
    key: &str,
    uml: &str,
) -> rinx_index::EntityRecord {
    let mut record = requirement(doc_path, Some(title));
    record.uml.insert(key.to_string(), uml.to_string());
    record
}

#[test]
fn test_uml_expands_the_diagram_another_entity_wrote() {
    // Given — a component that draws itself, and nothing else
    let index = index_with(vec![(
        "COMP_A",
        requirement_drawing(
            "arch.rst",
            "Component A",
            "",
            "component \"{{ need.title }}\"",
        ),
    )]);

    // When
    let text = expand_against(&index, "{{ uml('COMP_A') }}").expect("expansion succeeds");

    // Then — expanded with `need` rebound to the imported entity, which is
    // what makes one written picture reusable
    assert_eq!(text, "component \"Component A\"");
}

#[test]
fn test_uml_picks_the_diagram_stored_under_a_key() {
    // Given — one entity drawing itself two ways
    let mut record = requirement_drawing("arch.rst", "Component A", "", "the default");
    record
        .uml
        .insert("detail".to_string(), "the detailed one".to_string());
    let index = index_with(vec![("COMP_A", record)]);

    // When
    let text = expand_against(&index, "{{ uml('COMP_A', 'detail') }}").expect("expansion succeeds");

    // Then
    assert_eq!(text, "the detailed one");
}

#[test]
fn test_uml_of_an_entity_that_drew_nothing_contributes_nothing() {
    // Given — what lets `imports()` walk a relation whose targets do not all
    // carry a diagram
    let index = index_with(vec![("COMP_A", requirement("arch.rst", Some("A")))]);

    // When
    let text = expand_against(&index, "[{{ uml('COMP_A') }}]").expect("expansion succeeds");

    // Then
    assert_eq!(text, "[]");
}

#[test]
fn test_uml_reports_an_id_nothing_declares() {
    // Given
    let index = index_with(vec![("COMP_A", requirement("arch.rst", Some("A")))]);

    // When
    let error = expand_against(&index, "{{ uml('COMP_404') }}").expect_err("no such entity");

    // Then
    assert_eq!(error, UmlError::UnknownEntity("COMP_404".to_string()));
}

#[test]
fn test_a_diagram_importing_itself_is_reported_as_a_cycle() {
    // Given — an entity whose own diagram imports itself
    let index = index_with(vec![(
        "COMP_A",
        requirement_drawing("arch.rst", "A", "", "{{ uml('COMP_A') }}"),
    )]);

    // When
    let error = expand_against(&index, "{{ uml('COMP_A') }}").expect_err("a cycle");

    // Then — rather than recursing until the process dies
    let UmlError::RecursiveImport { chain } = &error else {
        panic!("expected a recursive import, found {error:?}");
    };
    assert_eq!(chain, &["COMP_A", "COMP_A"]);
}

#[test]
fn test_a_cycle_through_a_second_entity_is_reported_with_its_route() {
    // Given — A imports B, which imports A again
    let index = index_with(vec![
        (
            "COMP_A",
            requirement_drawing("arch.rst", "A", "", "{{ uml('COMP_B') }}"),
        ),
        (
            "COMP_B",
            requirement_drawing("arch.rst", "B", "", "{{ uml('COMP_A') }}"),
        ),
    ]);

    // When
    let error = expand_against(&index, "{{ uml('COMP_A') }}").expect_err("a cycle");

    // Then — the route, so an author can see where to break it
    let UmlError::RecursiveImport { chain } = &error else {
        panic!("expected a recursive import, found {error:?}");
    };
    assert_eq!(chain, &["COMP_A", "COMP_B", "COMP_A"]);
}

#[test]
fn test_two_imports_of_one_entity_side_by_side_are_not_a_cycle() {
    // Given — a diamond, not a cycle: importing the same picture twice is
    // ordinary composition, and must not be mistaken for recursion
    let index = index_with(vec![(
        "COMP_A",
        requirement_drawing("arch.rst", "A", "", "a"),
    )]);

    // When
    let text = expand_against(&index, "{{ uml('COMP_A') }}{{ uml('COMP_A') }}")
        .expect("expansion succeeds");

    // Then
    assert_eq!(text, "aa");
}

#[test]
fn test_imports_pulls_in_the_diagrams_of_a_relations_targets() {
    // Given — a system pointing at two components, each drawing itself
    let mut system = requirement("arch.rst", Some("System"));
    system.outgoing.insert(
        "links".to_string(),
        vec![
            rinx_ast::EntityId::new("COMP_A").unwrap(),
            rinx_ast::EntityId::new("COMP_B").unwrap(),
        ],
    );
    let index = index_with(vec![
        ("SYS_1", system),
        (
            "COMP_A",
            requirement_drawing("arch.rst", "A", "", "component A"),
        ),
        (
            "COMP_B",
            requirement_drawing("arch.rst", "B", "", "component B"),
        ),
    ]);

    // When
    let text =
        expand_against(&index, "{{ imports('SYS_1', 'links') }}").expect("expansion succeeds");

    // Then — in the order the targets were written, so the bytes are stable
    assert_eq!(text, "component A\ncomponent B");
}

#[test]
fn test_imports_of_a_relation_nothing_points_along_is_empty() {
    // Given
    let index = index_with(vec![("SYS_1", requirement("arch.rst", Some("System")))]);

    // When
    let text =
        expand_against(&index, "[{{ imports('SYS_1', 'links') }}]").expect("expansion succeeds");

    // Then
    assert_eq!(text, "[]");
}

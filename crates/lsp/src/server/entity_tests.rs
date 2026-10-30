//! Entities in the editor: a reference to one, an id written in a
//! relation, and the entity's own directive show its fields as its box on
//! the page lists them, and a relation's ids complete.

use super::test_support::*;
use lsp_types::request::Completion;
use lsp_types::{
    CompletionParams, CompletionResponse, GotoDefinitionResponse, Location, LocationLink, Position,
    Range, TextDocumentPositionParams,
};

#[test]
fn test_hovering_a_need_role_shows_the_need_s_fields_and_back_links() {
    // Given
    let text = "See :need:`R_1`.\n";
    let (workspace, mut state) = project_with("entity_hover_role", text);

    // When
    let shown = hover_markdown(&mut state, workspace.uri("index.rst"), 0, 12);

    // Then
    let reqs = workspace.uri("reqs.rst");
    assert_eq!(
        shown.as_deref(),
        Some(
            format!(
                "**Requirement** R\\_1 — Brakes\n\n[reqs\\.rst]({}#L4)\n\n\
                 - **Priority:** high\n\n\
                 - **implements\\_back:** S\\_1",
                reqs.as_str()
            )
            .as_str()
        )
    );
}

#[test]
fn test_hovering_a_ref_to_an_entity_shows_its_fields_too() {
    // Given
    let text = "See :ref:`R_1`.\n";
    let (workspace, mut state) = project_with("entity_hover_ref", text);

    // When
    let shown = hover_markdown(&mut state, workspace.uri("index.rst"), 0, 11);

    // Then
    let shown = shown.expect("a hover");
    assert!(shown.contains("- **Priority:** high"), "{shown}");
}

#[test]
fn test_hovering_a_label_on_a_section_shows_its_title_alone() {
    // Given
    let text = ".. _intro:\n\nIntro\n=====\n\nSee :ref:`intro`.\n";
    let (workspace, mut state) = project_with("entity_hover_label", text);

    // When
    let shown = hover_markdown(&mut state, workspace.uri("index.rst"), 5, 11);

    // Then
    let shown = shown.expect("a hover");
    assert!(shown.starts_with("**Intro**\n\n"), "{shown}");
}

#[test]
fn test_hovering_an_id_in_a_relation_shows_the_entity_it_names() {
    // Given
    let (workspace, mut state) = project_with("entity_hover_relation", "");
    handle_notification(&mut state, workspace.open("reqs.rst", REQS));

    // When — on `R_1` of `   :implements: R_1`
    let shown = hover_markdown(&mut state, workspace.uri("reqs.rst"), 9, 17);

    // Then — linked at the requirement's directive
    let shown = shown.expect("a hover");
    let linked = format!(
        "**Requirement** R\\_1 — Brakes\n\n[reqs\\.rst]({}#L4)",
        workspace.uri("reqs.rst").as_str()
    );
    assert!(shown.starts_with(&linked), "{shown}");
}

/// The location `definition` leads to, when it is a single plain one.
fn location_of(definition: Option<GotoDefinitionResponse>) -> Option<Location> {
    match definition? {
        GotoDefinitionResponse::Scalar(location) => Some(location),
        other => panic!("not one location: {other:?}"),
    }
}

/// The requirement `R_1`'s directive in `reqs.rst`: `.. req:: Brakes`.
fn requirement_marker(workspace: &Workspace) -> Location {
    Location::new(
        workspace.uri("reqs.rst"),
        Range::new(Position::new(3, 0), Position::new(3, 15)),
    )
}

#[test]
fn test_definition_of_a_need_role_or_a_ref_to_an_entity_is_its_directive() {
    // Given
    let text = "See :need:`R_1` and :ref:`R_1`.\n";
    let (workspace, mut state) = project_with("entity_definition_role", text);
    let uri = workspace.uri("index.rst");

    // When
    let by_role = definition_at(&mut state, uri.clone(), 0, 12);
    let by_ref = definition_at(&mut state, uri, 0, 27);

    // Then
    assert_eq!(location_of(by_role), Some(requirement_marker(&workspace)));
    assert_eq!(location_of(by_ref), Some(requirement_marker(&workspace)));
}

#[test]
fn test_definition_of_an_id_in_a_relation_is_the_entity_s_directive() {
    // Given
    let (workspace, mut state) = project_with("entity_definition_relation", "");
    handle_notification(&mut state, workspace.open("reqs.rst", REQS));

    // When — on `R_1` of `   :implements: R_1`
    let found = definition_at(&mut state, workspace.uri("reqs.rst"), 9, 17);

    // Then
    assert_eq!(location_of(found), Some(requirement_marker(&workspace)));
}

#[test]
fn test_definition_of_an_id_in_a_relation_leads_into_another_document() {
    // Given — a specification in `index.rst`, implementing `reqs.rst`'s
    // requirement among others
    let text = ".. spec:: Discs\n   :id: S_2\n   :implements: S_1, R_1\n";
    let (workspace, mut state) = project_with("entity_definition_elsewhere", text);

    // When — on `R_1`, the second id of the list
    let found = definition_at(&mut state, workspace.uri("index.rst"), 2, 23);

    // Then
    assert_eq!(location_of(found), Some(requirement_marker(&workspace)));
}

#[test]
fn test_definition_of_an_id_an_update_relates_is_the_entity_s_directive() {
    // Given — an update adding a relation to `S_1`
    let text = ".. entity-update:: S_1\n   :implements: R_1\n";
    let (workspace, mut state) = project_with("entity_definition_update", text);

    // When
    let found = definition_at(&mut state, workspace.uri("index.rst"), 1, 18);

    // Then
    assert_eq!(location_of(found), Some(requirement_marker(&workspace)));
}

#[test]
fn test_definition_of_a_relation_links_from_the_id_alone() {
    // Given — a client taking links
    let text = ".. spec:: Discs\n   :id: S_2\n   :implements: R_1\n";
    let (workspace, state) = project_with("entity_definition_link", text);
    let mut state = state.with_definition_links(true);

    // When
    let found = definition_at(&mut state, workspace.uri("index.rst"), 2, 18);

    // Then — the origin is `R_1`, not the option line
    let marker = requirement_marker(&workspace);
    assert_eq!(
        found,
        Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(Range::new(Position::new(2, 16), Position::new(2, 19))),
            target_uri: marker.uri,
            target_range: marker.range,
            target_selection_range: marker.range,
        }]))
    );
}

#[test]
fn test_definition_is_null_for_an_unknown_id_or_an_entity_s_own_lines() {
    // Given — a relation to an id no document declares
    let text = ".. spec:: Discs\n   :id: S_2\n   :implements: R_9\n";
    let (workspace, mut state) = project_with("entity_definition_none", text);
    let uri = workspace.uri("index.rst");

    // When
    let on_unknown = definition_at(&mut state, uri.clone(), 2, 18);
    let on_marker = definition_at(&mut state, uri.clone(), 0, 4);
    let on_id = definition_at(&mut state, uri, 1, 9);

    // Then — the reader is at the entity already
    assert_eq!(on_unknown, None);
    assert_eq!(on_marker, None);
    assert_eq!(on_id, None);
}

#[test]
fn test_hovering_an_entity_s_marker_or_id_line_shows_its_back_links() {
    // Given
    let (workspace, mut state) = project_with("entity_hover_definition", "");
    handle_notification(&mut state, workspace.open("reqs.rst", REQS));
    let uri = workspace.uri("reqs.rst");

    // When
    let on_marker = hover_markdown(&mut state, uri.clone(), 3, 5);
    let on_id = hover_markdown(&mut state, uri.clone(), 4, 10);
    let on_priority = hover_markdown(&mut state, uri, 5, 6);

    // Then — no location: the reader is at the entity already
    let expected = "**Requirement** R\\_1 — Brakes\n\n\
                    - **Priority:** high\n\n\
                    - **implements\\_back:** S\\_1";
    assert_eq!(on_marker.as_deref(), Some(expected));
    assert_eq!(on_id.as_deref(), Some(expected));
    assert_eq!(on_priority, None);
}

#[test]
fn test_hovering_an_entity_only_the_buffer_declares_shows_what_it_wrote() {
    // Given — a requirement typed but not saved
    let text = ".. req:: Pedals\n   :priority: low\n";
    let (workspace, mut state) = project_with("entity_hover_unsaved", text);

    // When
    let shown = hover_markdown(&mut state, workspace.uri("index.rst"), 0, 4);

    // Then — its generated id, and its value
    let shown = shown.expect("a hover");
    assert!(shown.starts_with("**Requirement** "), "{shown}");
    assert!(shown.ends_with("- **Priority:** low"), "{shown}");
}

#[test]
fn test_a_relation_completes_the_ids_of_the_project_s_entities_but_its_own() {
    // Given
    let text = ".. spec:: More\n   :id: S_2\n   :implements: \n";
    let (workspace, mut state) = project_with("entity_complete_relation", text);

    // When
    let response = handle_request(
        &mut state,
        Request::new(
            RequestId::from(9),
            Completion::METHOD.to_string(),
            CompletionParams {
                text_document_position: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(workspace.uri("index.rst")),
                    Position::new(2, 16),
                ),
                work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                partial_result_params: lsp_types::PartialResultParams::default(),
                context: None,
            },
        ),
    );

    // Then
    let result = response.response_result.expect("a result");
    let Some(CompletionResponse::List(list)) = serde_json::from_value(result).expect("a list")
    else {
        panic!("a completion list");
    };
    let shown: Vec<(&str, Option<&str>)> = list
        .items
        .iter()
        .map(|item| (item.label.as_str(), item.detail.as_deref()))
        .collect();
    assert_eq!(shown, [("R_1", Some("Brakes")), ("S_1", Some("Pads"))]);
    assert!(!list.is_incomplete);
}

/// Uses of `R_1` in an open `index.rst`: a need role, a relation listing it
/// second, and an update relating to it.
const USES: &str = "See :need:`R_1`.\n\n\
.. spec:: Discs\n   :id: S_2\n   :implements: S_1, R_1\n\n\
.. entity-update:: S_1\n   :implements: R_1\n";

/// The uses of `R_1` [`REQS`] and [`USES`] write.
fn requirement_uses(workspace: &Workspace) -> Vec<Location> {
    let index = workspace.uri("index.rst");
    vec![
        Location::new(
            index.clone(),
            Range::new(Position::new(0, 4), Position::new(0, 15)),
        ),
        Location::new(
            index.clone(),
            Range::new(Position::new(4, 21), Position::new(4, 24)),
        ),
        Location::new(
            index,
            Range::new(Position::new(7, 16), Position::new(7, 19)),
        ),
        Location::new(
            workspace.uri("reqs.rst"),
            Range::new(Position::new(9, 16), Position::new(9, 19)),
        ),
    ]
}

#[test]
fn test_references_of_a_need_role_are_its_roles_and_the_ids_relating_to_it() {
    // Given
    let (workspace, mut state) = project_with("entity_references_role", USES);

    // When
    let found = references_at(&mut state, workspace.uri("index.rst"), 0, 12, false);

    // Then — each id alone, not its option's line
    assert_eq!(found, Some(requirement_uses(&workspace)));
}

#[test]
fn test_references_of_an_entity_s_marker_are_its_uses() {
    // Given
    let (workspace, mut state) = project_with("entity_references_marker", USES);
    handle_notification(&mut state, workspace.open("reqs.rst", REQS));

    // When — on `.. req:: Brakes`
    let found = references_at(&mut state, workspace.uri("reqs.rst"), 3, 4, false);

    // Then
    assert_eq!(found, Some(requirement_uses(&workspace)));
}

#[test]
fn test_references_of_an_entity_s_id_line_are_its_uses() {
    // Given
    let (workspace, mut state) = project_with("entity_references_id_line", USES);
    handle_notification(&mut state, workspace.open("reqs.rst", REQS));

    // When — on `   :id: R_1`
    let found = references_at(&mut state, workspace.uri("reqs.rst"), 4, 9, false);

    // Then
    assert_eq!(found, Some(requirement_uses(&workspace)));
}

#[test]
fn test_references_of_an_entity_include_the_updates_naming_it() {
    // Given — `S_1`, which [`USES`] relates to and updates
    let (workspace, mut state) = project_with("entity_references_update", USES);
    handle_notification(&mut state, workspace.open("reqs.rst", REQS));

    // When — on `   :id: S_1`
    let found = references_at(&mut state, workspace.uri("reqs.rst"), 8, 9, false);

    // Then — the id in the relation, and the update's argument alone
    let index = workspace.uri("index.rst");
    assert_eq!(
        found,
        Some(vec![
            Location::new(
                index.clone(),
                Range::new(Position::new(4, 16), Position::new(4, 19)),
            ),
            Location::new(
                index,
                Range::new(Position::new(6, 19), Position::new(6, 22)),
            ),
        ])
    );
}

#[test]
fn test_references_of_an_entity_include_a_ref_to_its_label() {
    // Given — `:ref:` names the entity by its label, lowercased
    let text = "See :need:`R_1` and :ref:`r_1`.\n";
    let (workspace, mut state) = project_with("entity_references_label", text);

    // When — on the need role
    let found = references_at(&mut state, workspace.uri("index.rst"), 0, 12, false);

    // Then — both roles, and the relation in reqs.rst
    let index = workspace.uri("index.rst");
    assert_eq!(
        found,
        Some(vec![
            Location::new(
                index.clone(),
                Range::new(Position::new(0, 4), Position::new(0, 15)),
            ),
            Location::new(
                index,
                Range::new(Position::new(0, 20), Position::new(0, 30)),
            ),
            Location::new(
                workspace.uri("reqs.rst"),
                Range::new(Position::new(9, 16), Position::new(9, 19)),
            ),
        ])
    );
}

//! Entities in the editor: a reference to one, an id written in a
//! relation, and the entity's own directive show its fields as its box on
//! the page lists them, and a relation's ids complete.

use super::test_support::*;
use lsp_types::request::Completion;
use lsp_types::{CompletionParams, CompletionResponse, Position, TextDocumentPositionParams};

const CONF: &str = "extensions = ['sphinx_needs']\nneeds_from_toml = 'ubproject.toml'\n";
const UBPROJECT: &str = "\
[[needs.types]]
directive = \"req\"
title = \"Requirement\"

[[needs.types]]
directive = \"spec\"
title = \"Specification\"

[needs.fields.priority]

[needs.links.implements]
";
/// A requirement, and a specification implementing it, written in
/// `reqs.rst`.
const REQS: &str = "\
Requirements
============

.. req:: Brakes
   :id: R_1
   :priority: high

.. spec:: Pads
   :id: S_1
   :implements: R_1
";

/// A scanned sphinx-needs project holding [`REQS`], with `index.rst` opened
/// at `text`.
fn project_with(name: &str, text: &str) -> (Workspace, ServerState) {
    let (workspace, mut state) = scanned(
        name,
        &[
            ("conf.py", CONF),
            ("ubproject.toml", UBPROJECT),
            ("reqs.rst", REQS),
            ("index.rst", text),
        ],
    );
    handle_notification(&mut state, workspace.open("index.rst", text));
    (workspace, state)
}

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
                "**Requirement** R\\_1 — Brakes\n\n[reqs\\.rst]({})\n\n\
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

    // Then
    let shown = shown.expect("a hover");
    assert!(
        shown.starts_with("**Requirement** R\\_1 — Brakes\n\n[reqs\\.rst]("),
        "{shown}"
    );
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

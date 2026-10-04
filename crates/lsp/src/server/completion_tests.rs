//! Completing a `:ref:` or `:doc:` target: the capability the server
//! announces, and the items a request is answered with from the workspace
//! index.

use super::test_support::*;
use lsp_types::request::Completion;
use lsp_types::{
    CompletionList, CompletionOptions, CompletionParams, CompletionResponse, Position,
    TextDocumentPositionParams,
};

/// A section labelled `install` in `setup.rst`, and a titled document in a
/// subdirectory that references nothing yet.
const SETUP: &str = ".. _install:\n\nInstalling\n==========\n\nText.\n";
const NOTES: &str = "Notes\n=====\n\nText.\n";

fn completion(uri: Uri, line: u32, character: u32) -> Request {
    Request::new(
        RequestId::from(7),
        Completion::METHOD.to_string(),
        CompletionParams {
            text_document_position: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(uri),
                Position::new(line, character),
            ),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
            context: None,
        },
    )
}

/// The completion list `response` carries, or `None` for a null result.
fn list(response: &Response) -> Option<CompletionList> {
    let result = response.response_result.clone().expect("a result");
    let result: Option<CompletionResponse> = serde_json::from_value(result).expect("a completion");
    result.map(|result| match result {
        CompletionResponse::List(list) => list,
        CompletionResponse::Array(items) => CompletionList {
            is_incomplete: false,
            items,
        },
    })
}

/// Each item's label and detail.
fn shown(list: &CompletionList) -> Vec<(String, Option<String>)> {
    list.items
        .iter()
        .map(|item| (item.label.clone(), item.detail.clone()))
        .collect()
}

/// A scanned server over `setup.rst` and `guide/notes.rst`, with `notes`
/// opened at `text`.
fn server_with_notes(name: &str, text: &str) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, &[("setup.rst", SETUP), ("guide/notes.rst", NOTES)]);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("guide/notes.rst", text));
    (workspace, state)
}

#[test]
fn test_initialize_result_announces_completion_on_its_trigger_characters() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    let Some(CompletionOptions {
        trigger_characters: Some(triggers),
        ..
    }) = result.capabilities.completion_provider
    else {
        panic!("completion announced with trigger characters");
    };
    assert_eq!(triggers, ["`", "<", "/"]);
}

#[test]
fn test_completion_lists_every_label_with_its_title() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_ref", "See :ref:`\n");

    // When
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 0, 10),
    );

    // Then
    let list = list(&response).expect("items");
    assert!(!list.is_incomplete);
    assert_eq!(
        shown(&list),
        [("install".to_string(), Some("Installing".to_string()))]
    );
}

#[test]
fn test_completion_names_documents_relative_to_the_current_one() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_doc", "See :doc:`se`\n");

    // When — the cursor after `se`
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 0, 12),
    );

    // Then
    let list = list(&response).expect("items");
    assert_eq!(
        shown(&list),
        [
            ("notes".to_string(), Some("<no title>".to_string())),
            ("../setup".to_string(), Some("Installing".to_string())),
        ]
    );
    let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &list.items[1].text_edit else {
        panic!("a text edit");
    };
    assert_eq!(
        edit.range,
        lsp_types::Range::new(Position::new(0, 10), Position::new(0, 12))
    );
}

#[test]
fn test_completion_sees_a_label_defined_in_an_edited_buffer() {
    // Given — the label exists only in the open buffer, never saved
    let (workspace, mut state) = server_with_notes(
        "complete_buffer",
        ".. _notes-start:\n\nNotes\n=====\n\n:ref:`\n",
    );

    // When
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 5, 6),
    );

    // Then
    let labels: Vec<String> = shown(&list(&response).expect("items"))
        .into_iter()
        .map(|(label, _)| label)
        .collect();
    assert_eq!(labels, ["install", "notes-start"]);
}

#[test]
fn test_completion_is_incomplete_while_the_scan_runs() {
    // Given — the scan has not finished
    let workspace = Workspace::new("complete_scanning", &[("setup.rst", SETUP)]);
    let (mut state, _) = workspace.server(false);
    handle_notification(&mut state, workspace.open("notes.rst", ":ref:`\n"));

    // When
    let response = handle_request(&mut state, completion(workspace.uri("notes.rst"), 0, 6));

    // Then
    assert!(list(&response).expect("items").is_incomplete);
}

#[test]
fn test_completion_answers_null_outside_a_role() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_outside", "See :ref:`install`.\n");

    // When
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 0, 19),
    );

    // Then
    assert_eq!(list(&response), None);
}

#[test]
fn test_completion_answers_null_outside_every_workspace_folder() {
    // Given — `uri()` lies in no folder
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open(":ref:`\n"));

    // When
    let response = handle_request(&mut state, completion(uri(), 0, 6));

    // Then
    assert_eq!(list(&response), None);
}

#[test]
fn test_completion_answers_null_for_a_document_not_open() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_closed", ":ref:`\n");

    // When
    let response = handle_request(&mut state, completion(workspace.uri("setup.rst"), 0, 0));

    // Then
    assert_eq!(list(&response), None);
}

#[test]
fn test_completion_refuses_malformed_parameters() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(7),
        Completion::METHOD.to_string(),
        serde_json::json!({ "textDocument": 3 }),
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    assert_eq!(response.id, RequestId::from(7));
    assert_eq!(
        response.response_result.err().map(|error| error.code),
        Some(ErrorCode::InvalidParams as i32)
    );
}

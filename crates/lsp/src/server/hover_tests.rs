//! Hovering a reference: the capability the server announces, and what a
//! hover request is answered with from the workspace index.

use super::test_support::*;
use lsp_types::request::HoverRequest;
use lsp_types::{
    Hover, HoverContents, HoverParams, HoverProviderCapability, Position, Range,
    TextDocumentPositionParams,
};

/// A section labelled `install` in `setup.rst`.
const SETUP: &str = ".. _install:\n\nInstalling\n==========\n\nText.\n";

fn hover(uri: Uri, line: u32, character: u32) -> Request {
    Request::new(
        RequestId::from(8),
        HoverRequest::METHOD.to_string(),
        HoverParams {
            text_document_position_params: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(uri),
                Position::new(line, character),
            ),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        },
    )
}

/// The hover `response` carries, or `None` for a null result.
fn hover_of(response: &Response) -> Option<Hover> {
    let result = response.response_result.clone().expect("a result");
    serde_json::from_value(result).expect("a hover")
}

fn markdown(hover: &Hover) -> &str {
    match &hover.contents {
        HoverContents::Markup(markup) => &markup.value,
        other => panic!("not markup: {other:?}"),
    }
}

/// A scanned server over `setup.rst`, with `notes.rst` opened at `text`.
fn server_with_notes(name: &str, text: &str) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, &[("setup.rst", SETUP), ("notes.rst", "")]);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("notes.rst", text));
    (workspace, state)
}

#[test]
fn test_initialize_result_announces_hover() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    assert_eq!(
        result.capabilities.hover_provider,
        Some(HoverProviderCapability::Simple(true))
    );
}

#[test]
fn test_hover_shows_the_title_and_file_a_reference_leads_to() {
    // Given
    let (workspace, mut state) = server_with_notes("hover_ref", "See :ref:`install` now.\n");

    // When — the cursor inside the role
    let response = handle_request(&mut state, hover(workspace.uri("notes.rst"), 0, 8));

    // Then
    let hover = hover_of(&response).expect("a hover");
    assert_eq!(
        markdown(&hover),
        format!(
            "**Installing**\n\n[setup\\.rst]({})",
            workspace.uri("setup.rst").as_str()
        )
    );
    assert_eq!(
        hover.range,
        Some(Range::new(Position::new(0, 4), Position::new(0, 18)))
    );
}

#[test]
fn test_hover_reads_a_label_typed_a_moment_ago_in_another_document() {
    // Given — the label exists only in an unsaved buffer
    let (workspace, mut state) = server_with_notes("hover_fresh", "See :ref:`usage`.\n");
    handle_notification(
        &mut state,
        workspace.open("setup.rst", ".. _usage:\n\nUsage\n=====\n"),
    );

    // When
    let response = handle_request(&mut state, hover(workspace.uri("notes.rst"), 0, 8));

    // Then
    let hover = hover_of(&response).expect("a hover");
    assert!(markdown(&hover).starts_with("**Usage**"));
}

#[test]
fn test_hover_shows_nothing_for_a_broken_reference_or_plain_text() {
    // Given
    let (workspace, mut state) = server_with_notes("hover_none", "See :ref:`missing` now.\n");

    // When
    let on_broken = handle_request(&mut state, hover(workspace.uri("notes.rst"), 0, 8));
    let on_text = handle_request(&mut state, hover(workspace.uri("notes.rst"), 0, 1));

    // Then
    assert_eq!(hover_of(&on_broken), None);
    assert_eq!(hover_of(&on_text), None);
}

#[test]
fn test_hover_shows_nothing_for_a_document_that_is_not_open() {
    // Given
    let (workspace, mut state) = server_with_notes("hover_closed", "Text.\n");

    // When
    let response = handle_request(&mut state, hover(workspace.uri("setup.rst"), 0, 3));

    // Then
    assert_eq!(hover_of(&response), None);
}

#[test]
fn test_hover_refuses_parameters_that_do_not_parse() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(9),
        HoverRequest::METHOD.to_string(),
        serde_json::json!({ "textDocument": 1 }),
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    assert_eq!(response.id, RequestId::from(9));
    assert_eq!(
        response.response_result.err().map(|error| error.code),
        Some(ErrorCode::InvalidParams as i32)
    );
}

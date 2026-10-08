//! Going to a reference's definition: the capability the server announces,
//! and what a definition request is answered with from the workspace index.

use super::test_support::*;
use lsp_types::request::GotoDefinition;
use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, LocationLink, OneOf, Position, Range,
    TextDocumentPositionParams,
};

/// A section labelled `install` in `setup.rst`.
const SETUP: &str = ".. _install:\n\nInstalling\n==========\n\nText.\n";

fn definition(uri: Uri, line: u32, character: u32) -> Request {
    Request::new(
        RequestId::from(8),
        GotoDefinition::METHOD.to_string(),
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(uri),
                Position::new(line, character),
            ),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        },
    )
}

/// The definition `response` carries, or `None` for a null result.
fn definition_of(response: &Response) -> Option<GotoDefinitionResponse> {
    let result = response.response_result.clone().expect("a result");
    serde_json::from_value(result).expect("a definition")
}

/// The start of the file `uri`, as a bare location.
fn start_of(uri: Uri) -> GotoDefinitionResponse {
    GotoDefinitionResponse::Scalar(Location::new(uri, Range::default()))
}

/// A scanned server over `setup.rst`, with `notes.rst` opened at `text`,
/// answering with links when `links`.
fn server_with_notes(name: &str, text: &str, links: bool) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, &[("setup.rst", SETUP), ("notes.rst", "")]);
    let (state, _) = workspace.server(false);
    let mut state = state.with_definition_links(links);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("notes.rst", text));
    (workspace, state)
}

#[test]
fn test_initialize_result_announces_definition() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    assert_eq!(
        result.capabilities.definition_provider,
        Some(OneOf::Left(true))
    );
}

#[test]
fn test_definition_of_a_ref_is_the_file_its_label_is_in() {
    // Given
    let (workspace, mut state) =
        server_with_notes("definition_ref", "See :ref:`install` now.\n", false);

    // When — the cursor inside the role
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 8));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(start_of(workspace.uri("setup.rst")))
    );
}

#[test]
fn test_definition_of_a_doc_reference_is_the_document_it_names() {
    // Given
    let (workspace, mut state) = server_with_notes("definition_doc", "See :doc:`setup`.\n", false);

    // When
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 8));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(start_of(workspace.uri("setup.rst")))
    );
}

#[test]
fn test_definition_links_from_the_whole_reference_when_the_client_takes_links() {
    // Given
    let (workspace, mut state) =
        server_with_notes("definition_link", "See :ref:`install` now.\n", true);

    // When
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 8));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(Range::new(Position::new(0, 4), Position::new(0, 18))),
            target_uri: workspace.uri("setup.rst"),
            target_range: Range::default(),
            target_selection_range: Range::default(),
        }]))
    );
}

#[test]
fn test_definition_reads_a_label_typed_a_moment_ago_in_another_document() {
    // Given — the label exists only in an unsaved buffer
    let (workspace, mut state) =
        server_with_notes("definition_fresh", "See :ref:`usage`.\n", false);
    handle_notification(
        &mut state,
        workspace.open("setup.rst", ".. _usage:\n\nUsage\n=====\n"),
    );

    // When
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 8));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(start_of(workspace.uri("setup.rst")))
    );
}

#[test]
fn test_definition_is_null_for_a_broken_reference_or_plain_text() {
    // Given
    let (workspace, mut state) =
        server_with_notes("definition_none", "See :ref:`missing` now.\n", false);

    // When
    let on_broken = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 8));
    let on_text = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 1));

    // Then
    assert_eq!(definition_of(&on_broken), None);
    assert_eq!(definition_of(&on_text), None);
}

#[test]
fn test_definition_is_null_for_a_document_that_is_not_open() {
    // Given
    let (workspace, mut state) = server_with_notes("definition_closed", "Text.\n", false);

    // When
    let response = handle_request(&mut state, definition(workspace.uri("setup.rst"), 0, 3));

    // Then
    assert_eq!(definition_of(&response), None);
}

#[test]
fn test_definition_refuses_parameters_that_do_not_parse() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(9),
        GotoDefinition::METHOD.to_string(),
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

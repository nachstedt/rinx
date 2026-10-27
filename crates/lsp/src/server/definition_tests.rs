//! Going to a reference's definition: the capability the server announces,
//! and what a definition request is answered with from the workspace index.

use super::test_support::*;
use lsp_types::request::GotoDefinition;
use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, LocationLink, OneOf, Position, Range,
    TextDocumentPositionParams,
};

/// A section labelled `install` in `setup.rst`, on its fourth line.
const SETUP: &str = "Setup\n=====\n\n.. _install:\n\nInstalling\n==========\n\nText.\n";

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

/// The range from `start` to `end` on the 0-based `line`.
fn on_line(line: u32, start: u32, end: u32) -> Range {
    Range::new(Position::new(line, start), Position::new(line, end))
}

/// A scanned server over `files`, with `notes.rst` opened at `text`.
fn server_over(name: &str, files: &[(&str, &str)], text: &str) -> (Workspace, ServerState) {
    let mut files = files.to_vec();
    files.push(("notes.rst", ""));
    let workspace = Workspace::new(name, &files);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("notes.rst", text));
    (workspace, state)
}

/// The definition of the reference at `character` on the first line of
/// `notes.rst`.
fn definition_in_notes(
    workspace: &Workspace,
    state: &mut ServerState,
    character: u32,
) -> Option<GotoDefinitionResponse> {
    definition_of(&handle_request(
        state,
        definition(workspace.uri("notes.rst"), 0, character),
    ))
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
fn test_definition_of_a_ref_is_the_line_its_label_is_written_on() {
    // Given
    let (workspace, mut state) =
        server_with_notes("definition_ref", "See :ref:`install` now.\n", false);

    // When — the cursor inside the role
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 8));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(GotoDefinitionResponse::Scalar(Location::new(
            workspace.uri("setup.rst"),
            on_line(3, 0, 12)
        )))
    );
}

#[test]
fn test_definition_of_a_term_is_its_line_in_the_glossary() {
    // Given
    let glossary = "Glossary\n========\n\n.. glossary::\n\n   widget\n      A thing.\n";
    let (workspace, mut state) = server_over(
        "definition_term",
        &[("glossary.rst", glossary)],
        "A :term:`widget`.\n",
    );

    // When
    let found = definition_in_notes(&workspace, &mut state, 10);

    // Then
    assert_eq!(
        found,
        Some(GotoDefinitionResponse::Scalar(Location::new(
            workspace.uri("glossary.rst"),
            on_line(5, 3, 9)
        )))
    );
}

#[test]
fn test_definition_of_a_domain_object_is_its_directive() {
    // Given
    let api = "API\n===\n\n.. py:module:: pkg\n\n.. py:function:: run()\n\n   Runs.\n";
    let (workspace, mut state) = server_over(
        "definition_object",
        &[("api.rst", api)],
        "Call :py:func:`pkg.run`.\n",
    );

    // When
    let found = definition_in_notes(&workspace, &mut state, 16);

    // Then — the marker line
    let Some(GotoDefinitionResponse::Scalar(location)) = found else {
        panic!("a location, not {found:?}");
    };
    assert_eq!(location.uri, workspace.uri("api.rst"));
    assert_eq!(location.range.start, Position::new(5, 0));
}

#[test]
fn test_definition_of_an_equation_is_the_equation_it_labels() {
    // Given
    let math = "Math\n====\n\n.. math::\n   :label: euler\n\n   e^{i\\pi} = -1\n";
    let (workspace, mut state) = server_over(
        "definition_equation",
        &[("math.rst", math)],
        "See :eq:`euler`.\n",
    );

    // When
    let found = definition_in_notes(&workspace, &mut state, 8);

    // Then — the equation's own line, where its span starts
    let Some(GotoDefinitionResponse::Scalar(location)) = found else {
        panic!("a location, not {found:?}");
    };
    assert_eq!(location.uri, workspace.uri("math.rst"));
    assert_eq!(location.range.start.line, 6);
}

#[test]
fn test_definition_of_a_label_in_an_included_fragment_is_in_the_fragment() {
    // Given — `.inc`, so the fragment is no document of its own
    let setup = "Setup\n=====\n\n.. include:: shared/label.inc\n";
    let fragment = "Intro.\n\n.. _install:\n\nInstalling\n==========\n";
    let (workspace, mut state) = server_over(
        "definition_fragment",
        &[("setup.rst", setup), ("shared/label.inc", fragment)],
        "See :ref:`install` now.\n",
    );

    // When
    let found = definition_in_notes(&workspace, &mut state, 8);

    // Then
    assert_eq!(
        found,
        Some(GotoDefinitionResponse::Scalar(Location::new(
            workspace.uri("shared/label.inc"),
            on_line(2, 0, 12)
        )))
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
            target_range: on_line(3, 0, 12),
            target_selection_range: on_line(3, 0, 12),
        }]))
    );
}

#[test]
fn test_definition_reads_a_label_typed_a_moment_ago_in_another_document() {
    // Given — the label exists only in an unsaved buffer, on its third line
    let (workspace, mut state) =
        server_with_notes("definition_fresh", "See :ref:`usage`.\n", false);
    handle_notification(
        &mut state,
        workspace.open("setup.rst", "Intro.\n\n.. _usage:\n\nUsage\n=====\n"),
    );

    // When
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 8));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(GotoDefinitionResponse::Scalar(Location::new(
            workspace.uri("setup.rst"),
            on_line(2, 0, 10)
        )))
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

/// A toctree in `notes.rst`: a document on the fourth line, a titled one,
/// `self`, a glob, a missing document and an external link.
const TOCTREE: &str = ".. toctree::\n   :glob:\n\n   setup\n   Getting started <setup>\n   \
                       self\n   api/*\n   gone\n   https://example.org\n";

/// A scanned server over `setup.rst` and two documents under `api/`, with
/// `notes.rst` opened at [`TOCTREE`], answering with links when `links`.
fn server_with_toctree(name: &str, links: bool) -> (Workspace, ServerState) {
    let workspace = Workspace::new(
        name,
        &[
            ("setup.rst", SETUP),
            ("api/a.rst", "Alpha\n=====\n"),
            ("api/b.rst", "Beta\n====\n"),
            ("notes.rst", ""),
        ],
    );
    let (state, _) = workspace.server(false);
    let mut state = state.with_definition_links(links);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("notes.rst", TOCTREE));
    (workspace, state)
}

#[test]
fn test_definition_of_a_toctree_entry_is_the_start_of_its_document() {
    // Given
    let (workspace, mut state) = server_with_toctree("definition_toc_entry", false);

    // When — on `setup`, and on the titled entry's title
    let plain = handle_request(&mut state, definition(workspace.uri("notes.rst"), 3, 5));
    let titled = handle_request(&mut state, definition(workspace.uri("notes.rst"), 4, 6));

    // Then
    assert_eq!(
        definition_of(&plain),
        Some(start_of(workspace.uri("setup.rst")))
    );
    assert_eq!(
        definition_of(&titled),
        Some(start_of(workspace.uri("setup.rst")))
    );
}

#[test]
fn test_definition_of_a_self_toctree_entry_is_its_own_document() {
    // Given
    let (workspace, mut state) = server_with_toctree("definition_toc_self", false);

    // When
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 5, 4));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(start_of(workspace.uri("notes.rst")))
    );
}

#[test]
fn test_definition_of_a_glob_toctree_entry_is_every_document_it_matches() {
    // Given
    let (workspace, mut state) = server_with_toctree("definition_toc_glob", false);

    // When
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 6, 4));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(GotoDefinitionResponse::Array(vec![
            Location::new(workspace.uri("api/a.rst"), Range::default()),
            Location::new(workspace.uri("api/b.rst"), Range::default()),
        ]))
    );
}

#[test]
fn test_definition_of_a_toctree_entry_links_from_the_whole_entry() {
    // Given
    let (workspace, mut state) = server_with_toctree("definition_toc_link", true);

    // When
    let response = handle_request(&mut state, definition(workspace.uri("notes.rst"), 3, 5));

    // Then
    assert_eq!(
        definition_of(&response),
        Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(on_line(3, 3, 8)),
            target_uri: workspace.uri("setup.rst"),
            target_range: Range::default(),
            target_selection_range: Range::default(),
        }]))
    );
}

#[test]
fn test_definition_of_a_missing_or_external_toctree_entry_is_none() {
    // Given
    let (workspace, mut state) = server_with_toctree("definition_toc_none", false);

    // When
    let missing = handle_request(&mut state, definition(workspace.uri("notes.rst"), 7, 4));
    let external = handle_request(&mut state, definition(workspace.uri("notes.rst"), 8, 4));
    let directive = handle_request(&mut state, definition(workspace.uri("notes.rst"), 0, 4));

    // Then
    assert_eq!(definition_of(&missing), None);
    assert_eq!(definition_of(&external), None);
    assert_eq!(definition_of(&directive), None);
}

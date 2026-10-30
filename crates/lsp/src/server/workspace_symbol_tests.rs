//! Searching the workspace's definitions: the capability the server
//! announces, and what a `workspace/symbol` request is answered with from the
//! documents of every project, open or not.

use super::test_support::*;
use lsp_types::request::WorkspaceSymbolRequest;
use lsp_types::{Location, OneOf, Position, Range, SymbolInformation, SymbolKind};

/// A section labelled `install` in a document no editor holds open.
const SETUP: &str = "Setup\n=====\n\n.. _install:\n\nInstalling\n==========\n\nText.\n";

/// A function of `pkg`, and a section titled like one of [`SETUP`]'s.
const API: &str = "API\n===\n\n.. py:function:: pkg.install_all()\n\nSetup\n-----\n";

fn search(query: &str) -> Request {
    Request::new(
        RequestId::from(21),
        WorkspaceSymbolRequest::METHOD.to_string(),
        lsp_types::WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        },
    )
}

/// What `state` answers a search for `query` with.
fn symbols_for(state: &mut ServerState, query: &str) -> Vec<SymbolInformation> {
    let response = handle_request(state, search(query));
    serde_json::from_value(response.response_result.expect("a result")).expect("flat symbols")
}

/// Each symbol's name, kind and the document defining it.
fn shown(symbols: &[SymbolInformation]) -> Vec<(&str, SymbolKind, Option<&str>)> {
    symbols
        .iter()
        .map(|symbol| {
            (
                symbol.name.as_str(),
                symbol.kind,
                symbol.container_name.as_deref(),
            )
        })
        .collect()
}

#[test]
fn test_initialize_result_announces_workspace_symbols() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    assert_eq!(
        result.capabilities.workspace_symbol_provider,
        Some(OneOf::Left(true))
    );
}

#[test]
fn test_workspace_symbols_search_documents_no_editor_holds_open() {
    // Given
    let (workspace, mut state) = scanned(
        "workspace_symbols_closed",
        &[("setup.rst", SETUP), ("api.rst", API)],
    );

    // When
    let symbols = symbols_for(&mut state, "install");

    // Then — the exact name first, each placed where it was written
    assert_eq!(
        shown(&symbols),
        [
            ("install", SymbolKind::KEY, Some("setup")),
            ("Installing", SymbolKind::STRING, Some("setup")),
            ("pkg.install_all", SymbolKind::FUNCTION, Some("api")),
        ]
    );
    assert_eq!(
        symbols[0].location,
        Location::new(
            workspace.uri("setup.rst"),
            Range::new(Position::new(3, 0), Position::new(3, 12))
        )
    );
}

#[test]
fn test_workspace_symbols_tell_apart_sections_titled_alike() {
    // Given
    let (_, mut state) = scanned(
        "workspace_symbols_alike",
        &[("setup.rst", SETUP), ("api.rst", API)],
    );

    // When
    let symbols = symbols_for(&mut state, "setup");

    // Then — by the document defining each
    assert_eq!(
        shown(&symbols),
        [
            ("Setup", SymbolKind::STRING, Some("api")),
            ("Setup", SymbolKind::STRING, Some("setup")),
        ]
    );
}

#[test]
fn test_workspace_symbols_follow_an_unsaved_edit() {
    // Given
    let (workspace, mut state) = scanned("workspace_symbols_edited", &[("setup.rst", SETUP)]);
    handle_notification(&mut state, workspace.open("setup.rst", SETUP));

    // When — a label is added in the editor, above the old one
    let edited = format!(".. _quickstart:\n\n{SETUP}");
    handle_notification(&mut state, workspace.change("setup.rst", 2, &edited));
    let symbols = symbols_for(&mut state, "quickstart");

    // Then — counted in the open buffer
    assert_eq!(
        shown(&symbols),
        [("quickstart", SymbolKind::KEY, Some("setup"))]
    );
    let install = symbols_for(&mut state, "install");
    assert_eq!(install[0].location.range.start, Position::new(5, 0));
}

#[test]
fn test_workspace_symbols_are_empty_when_nothing_matches() {
    // Given
    let (_, mut state) = scanned("workspace_symbols_none", &[("setup.rst", SETUP)]);

    // When / Then
    assert!(symbols_for(&mut state, "zzz").is_empty());
}

#[test]
fn test_workspace_symbols_refuse_parameters_that_do_not_parse() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(22),
        WorkspaceSymbolRequest::METHOD.to_string(),
        serde_json::json!({ "query": 1 }),
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    assert_eq!(
        response.response_result.err().map(|error| error.code),
        Some(ErrorCode::InvalidParams as i32)
    );
}

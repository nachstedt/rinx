//! A document's outline: the capability the server announces, and the
//! symbols an open document's latest parse yields, nested or flat as the
//! client takes them.

use super::test_support::*;
use crate::document_symbol::OutlineShape;
use lsp_types::request::DocumentSymbolRequest;
use lsp_types::{DocumentSymbolParams, OneOf, Position, Range, SymbolInformation};

const GUIDE: &str = "\
Guide
=====

.. py:function:: greet(name)

Usage
-----

.. include:: part.inc
";

/// A section a fragment brings into `guide.rst`.
const PART: &str = "Part\n~~~~\n\nText.\n";

fn symbols(uri: Uri) -> Request {
    Request::new(
        RequestId::from(11),
        DocumentSymbolRequest::METHOD.to_string(),
        DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(uri),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        },
    )
}

/// A scanned server with `guide.rst` open, answering in `shape`.
fn server_with_guide(name: &str, shape: OutlineShape) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, &[("guide.rst", GUIDE), ("part.inc", PART)]);
    let (state, _) = workspace.server(false);
    let mut state = state.with_outline_shape(shape);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("guide.rst", GUIDE));
    (workspace, state)
}

fn result_of(response: &Response) -> serde_json::Value {
    response.response_result.clone().expect("a result")
}

#[test]
fn test_initialize_result_announces_document_symbols() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    assert_eq!(
        result.capabilities.document_symbol_provider,
        Some(OneOf::Left(true))
    );
}

#[test]
fn test_document_symbols_nest_an_open_documents_outline() {
    // Given
    let (workspace, mut state) = server_with_guide("symbols_nested", OutlineShape::Nested);

    // When
    let response = handle_request(&mut state, symbols(workspace.uri("guide.rst")));

    // Then — the function and both sections under the title, the included
    // section on its `.. include::` line
    let outline: Vec<lsp_types::DocumentSymbol> =
        serde_json::from_value(result_of(&response)).expect("nested symbols");
    assert_eq!(outline.len(), 1);
    let below: Vec<(&str, Range)> = outline[0]
        .children
        .iter()
        .flatten()
        .map(|symbol| (symbol.name.as_str(), symbol.selection_range))
        .collect();
    assert_eq!(
        below,
        [
            (
                "greet",
                Range::new(Position::new(3, 0), Position::new(3, 28))
            ),
            (
                "Usage",
                Range::new(Position::new(5, 0), Position::new(5, 5))
            ),
        ]
    );
    let usage = &outline[0].children.as_ref().unwrap()[1];
    let part = &usage.children.as_ref().expect("the included section")[0];
    assert_eq!(part.name, "Part");
    assert_eq!(
        part.selection_range,
        Range::new(Position::new(8, 0), Position::new(8, 21))
    );
}

#[test]
fn test_document_symbols_are_flat_for_a_client_taking_no_nesting() {
    // Given
    let (workspace, mut state) = server_with_guide("symbols_flat", OutlineShape::Flat);

    // When
    let response = handle_request(&mut state, symbols(workspace.uri("guide.rst")));

    // Then
    let flat: Vec<SymbolInformation> =
        serde_json::from_value(result_of(&response)).expect("flat symbols");
    let shown: Vec<(&str, Option<&str>)> = flat
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.container_name.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            ("Guide", None),
            ("greet", Some("Guide")),
            ("Usage", Some("Guide")),
            ("Part", Some("Usage")),
        ]
    );
    assert!(
        flat.iter()
            .all(|symbol| symbol.location.uri == workspace.uri("guide.rst"))
    );
}

#[test]
fn test_document_symbols_follow_the_latest_edit() {
    // Given
    let (workspace, mut state) = server_with_guide("symbols_edited", OutlineShape::Nested);

    // When — the title is renamed in the editor
    handle_notification(
        &mut state,
        workspace.change(
            "guide.rst",
            2,
            &GUIDE.replace("Guide\n=====", "Manual\n======"),
        ),
    );
    let response = handle_request(&mut state, symbols(workspace.uri("guide.rst")));

    // Then
    let outline: Vec<lsp_types::DocumentSymbol> =
        serde_json::from_value(result_of(&response)).expect("nested symbols");
    assert_eq!(outline[0].name, "Manual");
}

#[test]
fn test_document_symbols_are_null_for_a_document_that_is_not_open() {
    // Given
    let (workspace, mut state) = server_with_guide("symbols_closed", OutlineShape::Nested);

    // When
    let response = handle_request(&mut state, symbols(workspace.uri("part.inc")));

    // Then
    assert_eq!(result_of(&response), serde_json::Value::Null);
}

#[test]
fn test_document_symbols_refuse_parameters_that_do_not_parse() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(12),
        DocumentSymbolRequest::METHOD.to_string(),
        serde_json::json!({ "textDocument": 1 }),
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    assert_eq!(
        response.response_result.err().map(|error| error.code),
        Some(ErrorCode::InvalidParams as i32)
    );
}

//! What of a document folds: the capability the server announces, and the
//! ranges an open document's latest parse yields.

use super::test_support::*;
use lsp_types::request::FoldingRangeRequest;
use lsp_types::{
    FoldingRange, FoldingRangeKind, FoldingRangeParams, FoldingRangeProviderCapability,
};

const GUIDE: &str = "\
Guide
=====

.. note::

   Text.

.. A comment
   continued.

.. include:: part.inc
";

/// A fragment whose directive folds in its own file, not in `guide.rst`.
const PART: &str = ".. warning::\n\n   Careful.\n";

fn folding(uri: Uri) -> Request {
    Request::new(
        RequestId::from(21),
        FoldingRangeRequest::METHOD.to_string(),
        FoldingRangeParams {
            text_document: TextDocumentIdentifier::new(uri),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        },
    )
}

/// A scanned server with `guide.rst` open.
fn server_with_guide(name: &str) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, &[("guide.rst", GUIDE), ("part.inc", PART)]);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("guide.rst", GUIDE));
    (workspace, state)
}

/// The (start, end, kind) of each range in `response`.
fn folds(response: &Response) -> Vec<(u32, u32, Option<FoldingRangeKind>)> {
    let ranges: Vec<FoldingRange> =
        serde_json::from_value(response.response_result.clone().expect("a result"))
            .expect("folding ranges");
    ranges
        .into_iter()
        .map(|range| (range.start_line, range.end_line, range.kind))
        .collect()
}

#[test]
fn test_initialize_result_announces_folding_ranges() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    assert_eq!(
        result.capabilities.folding_range_provider,
        Some(FoldingRangeProviderCapability::Simple(true))
    );
}

#[test]
fn test_folding_ranges_fold_an_open_documents_section_directive_and_comment() {
    // Given
    let (workspace, mut state) = server_with_guide("folding_open");

    // When
    let response = handle_request(&mut state, folding(workspace.uri("guide.rst")));

    // Then — the section to its last line, the note, the comment; nothing of
    // the included fragment
    assert_eq!(
        folds(&response),
        vec![
            (0, 10, None),
            (3, 5, None),
            (7, 8, Some(FoldingRangeKind::Comment)),
        ]
    );
}

#[test]
fn test_folding_ranges_follow_the_latest_edit() {
    // Given
    let (workspace, mut state) = server_with_guide("folding_edited");

    // When — the comment is removed in the editor
    handle_notification(
        &mut state,
        workspace.change(
            "guide.rst",
            2,
            &GUIDE.replace(".. A comment\n   continued.\n\n", ""),
        ),
    );
    let response = handle_request(&mut state, folding(workspace.uri("guide.rst")));

    // Then
    assert_eq!(folds(&response), vec![(0, 7, None), (3, 5, None)]);
}

#[test]
fn test_folding_ranges_keep_the_longest_within_the_clients_limit() {
    // Given
    let (workspace, state) = server_with_guide("folding_limited");
    let mut state = state.with_folding_limit(Some(1));

    // When
    let response = handle_request(&mut state, folding(workspace.uri("guide.rst")));

    // Then — the section, which holds the others
    assert_eq!(folds(&response), vec![(0, 10, None)]);
}

#[test]
fn test_folding_ranges_are_null_for_a_document_that_is_not_open() {
    // Given
    let (workspace, mut state) = server_with_guide("folding_closed");

    // When
    let response = handle_request(&mut state, folding(workspace.uri("part.inc")));

    // Then
    assert_eq!(response.response_result.ok(), Some(serde_json::Value::Null));
}

#[test]
fn test_folding_ranges_refuse_parameters_that_do_not_parse() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(22),
        FoldingRangeRequest::METHOD.to_string(),
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

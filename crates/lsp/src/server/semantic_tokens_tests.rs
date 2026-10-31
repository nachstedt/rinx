//! The names a document writes, coloured by kind: the legend the server
//! announces, and the tokens an open document's latest parse yields, whole
//! or for a range.

use super::test_support::*;
use lsp_types::request::{SemanticTokensFullRequest, SemanticTokensRangeRequest};
use lsp_types::{
    Position, Range, SemanticTokens, SemanticTokensFullOptions, SemanticTokensParams,
    SemanticTokensRangeParams, SemanticTokensServerCapabilities,
};

const GUIDE: &str = "\
.. _install:

.. image:: logo.png
   :alt: Logo

See :ref:`install`.
";

fn full(uri: Uri) -> Request {
    Request::new(
        RequestId::from(31),
        SemanticTokensFullRequest::METHOD.to_string(),
        SemanticTokensParams {
            text_document: TextDocumentIdentifier::new(uri),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        },
    )
}

fn ranged(uri: Uri, range: Range) -> Request {
    Request::new(
        RequestId::from(32),
        SemanticTokensRangeRequest::METHOD.to_string(),
        SemanticTokensRangeParams {
            text_document: TextDocumentIdentifier::new(uri),
            range,
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        },
    )
}

/// A scanned server with `guide.rst` open.
fn server_with_guide(name: &str) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, &[("guide.rst", GUIDE)]);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("guide.rst", GUIDE));
    (workspace, state)
}

/// The (line, start, length, type index) of each token in `response`.
fn decoded(response: &Response) -> Vec<(u32, u32, u32, u32)> {
    let tokens: SemanticTokens =
        serde_json::from_value(response.response_result.clone().expect("a result"))
            .expect("semantic tokens");
    let (mut line, mut start) = (0, 0);
    tokens
        .data
        .iter()
        .map(|token| {
            if token.delta_line > 0 {
                start = 0;
            }
            line += token.delta_line;
            start += token.delta_start;
            (line, start, token.length, token.token_type)
        })
        .collect()
}

#[test]
fn test_initialize_result_announces_the_legend_for_whole_documents_and_ranges() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    let Some(SemanticTokensServerCapabilities::SemanticTokensOptions(options)) =
        result.capabilities.semantic_tokens_provider
    else {
        panic!("semantic token options");
    };
    let types: Vec<&str> = options
        .legend
        .token_types
        .iter()
        .map(lsp_types::SemanticTokenType::as_str)
        .collect();
    assert_eq!(
        types,
        ["directive", "option", "role", "reference", "substitution"]
    );
    assert_eq!(options.range, Some(true));
    assert_eq!(options.full, Some(SemanticTokensFullOptions::Bool(true)));
}

#[test]
fn test_semantic_tokens_colour_an_open_documents_names() {
    // Given
    let (workspace, mut state) = server_with_guide("tokens_full");

    // When
    let response = handle_request(&mut state, full(workspace.uri("guide.rst")));

    // Then — the label, the directive, its option, the role and its target
    assert_eq!(
        decoded(&response),
        vec![
            (0, 4, 7, 3),
            (2, 3, 7, 0),
            (3, 3, 5, 1),
            (5, 4, 5, 2),
            (5, 10, 7, 3),
        ]
    );
}

#[test]
fn test_semantic_tokens_for_a_range_keep_only_what_starts_in_it() {
    // Given
    let (workspace, mut state) = server_with_guide("tokens_range");

    // When
    let response = handle_request(
        &mut state,
        ranged(
            workspace.uri("guide.rst"),
            Range::new(Position::new(5, 0), Position::new(6, 0)),
        ),
    );

    // Then
    assert_eq!(decoded(&response), vec![(5, 4, 5, 2), (5, 10, 7, 3)]);
}

#[test]
fn test_semantic_tokens_follow_the_latest_edit() {
    // Given
    let (workspace, mut state) = server_with_guide("tokens_edited");

    // When — the reference is removed in the editor
    handle_notification(
        &mut state,
        workspace.change("guide.rst", 2, &GUIDE.replace("See :ref:`install`.\n", "")),
    );
    let response = handle_request(&mut state, full(workspace.uri("guide.rst")));

    // Then
    assert_eq!(
        decoded(&response),
        vec![(0, 4, 7, 3), (2, 3, 7, 0), (3, 3, 5, 1)]
    );
}

#[test]
fn test_semantic_tokens_are_null_for_a_document_that_is_not_open() {
    // Given
    let (workspace, mut state) = server_with_guide("tokens_closed");

    // When
    let response = handle_request(&mut state, full(workspace.uri("other.rst")));

    // Then
    assert_eq!(response.response_result.ok(), Some(serde_json::Value::Null));
}

#[test]
fn test_semantic_tokens_refuse_parameters_that_do_not_parse() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(33),
        SemanticTokensFullRequest::METHOD.to_string(),
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

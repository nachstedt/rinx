//! Properties over whole editing sessions rather than single texts.

use super::test_support::*;
use proptest::prelude::*;

fn did_change_to(text: &str, version: i32) -> Notification {
    Notification::new(
        DidChangeTextDocument::METHOD.to_string(),
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri(), version),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        },
    )
}

/// A completion request at `line` and `character` of the open document `uri`.
fn completion_at(uri: Uri, line: u32, character: u32) -> Request {
    Request::new(
        RequestId::from(9),
        "textDocument/completion".to_string(),
        serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
        }),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn test_completion_answers_anywhere_in_any_text(
        text in "(:py:|:ref:|:any:|:option:|\\.\\. |default-role:: |[ :`<>~.!a-z_\n-]){0,30}",
        line in 0u32..4,
        character in 0u32..40,
    ) {
        // Given — a scanned folder defining a function and a label
        let (workspace, mut state) = scanned(
            "property_completion",
            &[("api.rst", ".. _intro:\n\n.. py:function:: run()\n")],
        );
        handle_notification(&mut state, workspace.open("index.rst", &text));

        // When
        let response = handle_request(&mut state, completion_at(workspace.uri("index.rst"), line, character));

        // Then — an answer, never an error
        prop_assert!(response.response_result.is_ok(), "{:?}", response.response_result);
    }
}

proptest! {
    #[test]
    fn test_every_edit_publishes_once_for_its_version(
        first in "(\\PC|\n){0,40}",
        edits in prop::collection::vec("(\\PC|\n){0,40}", 0..6),
    ) {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        let opened = handle_notification(&mut state, did_open(&first));
        prop_assert_eq!(opened.len(), 1);

        for (version, text) in (2..).zip(&edits) {
            // When
            let replies = handle_notification(&mut state, did_change_to(text, version));

            // Then
            prop_assert_eq!(replies.len(), 1);
            prop_assert_eq!(published(&replies[0]).version, Some(version));
        }
    }
}

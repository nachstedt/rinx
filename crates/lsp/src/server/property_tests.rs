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

//! The handlers on single notifications: opening, editing and closing documents and the fragments they include.

use super::test_support::*;

#[test]
fn test_initialize_result_announces_full_sync_and_the_encoding() {
    // Given
    let capabilities = ClientCapabilities::default();

    // When
    let (result, encoding) = initialize_result(&capabilities);

    // Then
    assert_eq!(encoding, PositionEncoding::Utf16);
    assert_eq!(
        result.capabilities.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
    );
    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncoding::Utf16.kind())
    );
    assert_eq!(
        result.server_info.map(|info| info.name),
        Some("rinx".to_string())
    );
}

#[test]
fn test_did_open_publishes_the_document_diagnostics() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);

    // When
    let replies = handle_notification(&mut state, did_open(".. foo::\n"));

    // Then
    assert_eq!(replies.len(), 1);
    let params = published(&replies[0]);
    assert_eq!(params.uri, uri());
    assert_eq!(params.version, Some(1));
    assert_eq!(codes(&params), vec!["directive.unknown".to_string()]);
}

#[test]
fn test_did_change_publishes_for_the_new_text() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open(".. foo::\n"));

    // When
    let replies = handle_notification(&mut state, did_change("Clean prose.\n"));

    // Then
    let params = published(&replies[0]);
    assert_eq!(params.version, Some(2));
    assert_eq!(params.diagnostics, Vec::new());
}

#[test]
fn test_did_change_adding_a_noqa_clears_what_it_names() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open(".. foo::\n"));

    // When
    let replies = handle_notification(
        &mut state,
        did_change(".. noqa: directive.unknown\n\n.. foo::\n"),
    );

    // Then
    assert_eq!(published(&replies[0]).diagnostics, Vec::new());
}

#[test]
fn test_did_close_clears_the_diagnostics() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open(".. foo::\n"));

    // When
    let replies = handle_notification(&mut state, did_close());

    // Then
    let params = published(&replies[0]);
    assert_eq!(params.diagnostics, Vec::new());
    assert_eq!(params.version, None);
    assert_eq!(state.documents.get(&uri()), None);
}

/// A project in a scratch directory: `index.rst` and `guide.rst`
/// include `part.rst`, which is saved holding `part`.
struct Project {
    dir: std::path::PathBuf,
}

impl Project {
    fn new(name: &str, part: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("rinx_lsp_server_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("part.rst"), part).expect("write the fragment");
        Self { dir }
    }

    fn uri(&self, name: &str) -> Uri {
        crate::uri::file_uri(&self.dir.join(name)).expect("an absolute path")
    }

    fn open(&self, name: &str, version: i32, text: &str) -> Notification {
        Notification::new(
            DidOpenTextDocument::METHOD.to_string(),
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    self.uri(name),
                    "restructuredtext".to_string(),
                    version,
                    text.to_string(),
                ),
            },
        )
    }

    fn change(&self, name: &str, version: i32, text: &str) -> Notification {
        Notification::new(
            DidChangeTextDocument::METHOD.to_string(),
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(self.uri(name), version),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.to_string(),
                }],
            },
        )
    }

    fn close(&self, name: &str) -> Notification {
        Notification::new(
            DidCloseTextDocument::METHOD.to_string(),
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(self.uri(name)),
            },
        )
    }

    /// What `replies` publish, as `(file name, version, codes)` in order,
    /// with an include's summary — which has no code — as `summary`.
    fn publishes(&self, replies: &[Message]) -> Vec<(String, Option<i32>, Vec<String>)> {
        replies
            .iter()
            .map(|reply| {
                let params = published(reply);
                let path = crate::uri::file_path(&params.uri).expect("a file uri");
                let name = path
                    .strip_prefix(&self.dir)
                    .expect("a file of the project")
                    .to_string_lossy()
                    .into_owned();
                let codes = params
                    .diagnostics
                    .iter()
                    .map(|diagnostic| match &diagnostic.code {
                        Some(NumberOrString::String(code)) => code.clone(),
                        _ => SUMMARY.to_string(),
                    })
                    .collect();
                (name, params.version, codes)
            })
            .collect()
    }
}

const INCLUDES_PART: &str = "Title\n=====\n\n.. include:: part.rst\n";
const BROKEN: &str = "Fine.\n\n.. foo::\n";
const UNKNOWN: &str = "directive.unknown";
const SUMMARY: &str = "summary";

fn entry(name: &str, version: Option<i32>, codes: &[&str]) -> (String, Option<i32>, Vec<String>) {
    (
        name.to_string(),
        version,
        codes.iter().map(ToString::to_string).collect(),
    )
}

#[test]
fn test_opening_an_includer_publishes_the_fragments_mistake_on_the_fragment() {
    // Given
    let project = Project::new("open_includer", BROKEN);
    let mut state = ServerState::new(PositionEncoding::Utf16);

    // When
    let replies = handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

    // Then — the fragment first, the opened document last
    assert_eq!(
        project.publishes(&replies),
        vec![
            entry("part.rst", None, &[UNKNOWN]),
            entry("index.rst", Some(1), &[SUMMARY]),
        ]
    );
}

#[test]
fn test_editing_an_open_fragment_re_diagnoses_its_includer() {
    // Given a fragment saved clean, and its includer open
    let project = Project::new("edit_fragment", "Fine.\n");
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));
    handle_notification(&mut state, project.open("part.rst", 1, "Fine.\n"));

    // When the fragment's buffer breaks, unsaved
    let replies = handle_notification(&mut state, project.change("part.rst", 2, BROKEN));

    // Then — the mistake is found through the includer, on the fragment,
    // and summarized on the include
    assert_eq!(
        project.publishes(&replies),
        vec![
            entry("index.rst", Some(1), &[SUMMARY]),
            entry("part.rst", Some(2), &[UNKNOWN]),
        ]
    );
}

#[test]
fn test_fixing_the_fragment_clears_it() {
    // Given
    let project = Project::new("fix_fragment", BROKEN);
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));
    handle_notification(&mut state, project.open("part.rst", 1, BROKEN));

    // When
    let replies = handle_notification(&mut state, project.change("part.rst", 2, "Fine.\n"));

    // Then — the summary on the include goes with it
    assert_eq!(
        project.publishes(&replies),
        vec![
            entry("index.rst", Some(1), &[]),
            entry("part.rst", Some(2), &[])
        ]
    );
}

#[test]
fn test_removing_the_include_clears_the_fragment() {
    // Given
    let project = Project::new("remove_include", BROKEN);
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

    // When
    let replies = handle_notification(&mut state, project.change("index.rst", 2, "Title\n"));

    // Then
    assert_eq!(
        project.publishes(&replies),
        vec![
            entry("part.rst", None, &[]),
            entry("index.rst", Some(2), &[])
        ]
    );
}

#[test]
fn test_closing_the_includer_clears_a_closed_fragment() {
    // Given
    let project = Project::new("close_includer", BROKEN);
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

    // When
    let replies = handle_notification(&mut state, project.close("index.rst"));

    // Then
    assert_eq!(
        project.publishes(&replies),
        vec![entry("part.rst", None, &[]), entry("index.rst", None, &[])]
    );
}

#[test]
fn test_closing_the_includer_of_an_open_fragment_shows_its_own_parse_again() {
    // Given a fragment using a substitution only its includer defines
    let project = Project::new("standalone_again", "|name|\n");
    let includer = ".. |name| replace:: rinx\n\n.. include:: part.rst\n";
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, project.open("index.rst", 1, includer));
    let opened = handle_notification(&mut state, project.open("part.rst", 1, "|name|\n"));

    // When
    let replies = handle_notification(&mut state, project.close("index.rst"));

    // Then — included, it is clean; alone, the substitution is undefined
    assert_eq!(
        project.publishes(&opened),
        vec![entry("part.rst", Some(1), &[])]
    );
    let [first, last] = project
        .publishes(&replies)
        .try_into()
        .expect("two publishes");
    assert_eq!((first.0.as_str(), first.1), ("part.rst", Some(1)));
    assert_eq!(first.2.len(), 1, "{first:?}");
    assert_eq!(last, entry("index.rst", None, &[]));
}

#[test]
fn test_closing_a_fragment_keeps_what_its_includer_finds_there() {
    // Given a fragment broken only in its unsaved buffer
    let project = Project::new("close_fragment", "Fine.\n");
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));
    handle_notification(&mut state, project.open("part.rst", 1, BROKEN));

    // When its buffer is discarded
    let replies = handle_notification(&mut state, project.close("part.rst"));

    // Then — the includer reads the clean saved text again
    assert_eq!(
        project.publishes(&replies),
        vec![
            entry("index.rst", Some(1), &[]),
            entry("part.rst", None, &[])
        ]
    );
}

#[test]
fn test_two_includers_report_a_shared_fragments_mistake_once() {
    // Given
    let project = Project::new("two_includers", BROKEN);
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

    // When
    let replies = handle_notification(&mut state, project.open("guide.rst", 1, INCLUDES_PART));

    // Then
    assert_eq!(
        project.publishes(&replies),
        vec![
            entry("part.rst", None, &[UNKNOWN]),
            entry("guide.rst", Some(1), &[SUMMARY]),
        ]
    );
}

#[test]
fn test_a_document_including_itself_reports_the_cycle_on_itself() {
    // Given
    let project = Project::new("cycle", "");
    let mut state = ServerState::new(PositionEncoding::Utf16);

    // When
    let replies = handle_notification(
        &mut state,
        project.open("index.rst", 1, ".. include:: index.rst\n"),
    );

    // Then — the saved file does not exist, so it is the open buffer that
    // includes itself, through the overlay
    assert_eq!(
        project.publishes(&replies),
        vec![entry("index.rst", Some(1), &["include.cycle"])]
    );
}

#[test]
fn test_an_unknown_notification_is_ignored() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let notification = Notification::new("$/unknown".to_string(), serde_json::Value::Null);

    // When
    let replies = handle_notification(&mut state, notification);

    // Then
    assert!(replies.is_empty());
}

#[test]
fn test_a_notification_with_malformed_parameters_is_ignored() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let notification = Notification::new(
        DidOpenTextDocument::METHOD.to_string(),
        serde_json::json!({ "nonsense": true }),
    );

    // When
    let replies = handle_notification(&mut state, notification);

    // Then
    assert!(replies.is_empty());
}

#[test]
fn test_a_change_or_close_with_malformed_parameters_is_ignored() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open(".. foo::\n"));

    for method in [DidChangeTextDocument::METHOD, DidCloseTextDocument::METHOD] {
        // When
        let notification =
            Notification::new(method.to_string(), serde_json::json!({ "nonsense": true }));
        let replies = handle_notification(&mut state, notification);

        // Then — and the document stays open as it was
        assert!(replies.is_empty(), "{method}: {replies:?}");
        assert_eq!(
            state.documents.get(&uri()).map(|document| document.version),
            Some(1)
        );
    }
}

#[test]
fn test_a_change_without_content_changes_publishes_nothing() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open(".. foo::\n"));
    let empty_change = Notification::new(
        DidChangeTextDocument::METHOD.to_string(),
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri(), 2),
            content_changes: Vec::new(),
        },
    );

    // When
    let replies = handle_notification(&mut state, empty_change);

    // Then
    assert!(replies.is_empty(), "{replies:?}");
    assert_eq!(
        state.documents.get(&uri()).map(|document| document.version),
        Some(1)
    );
}

#[test]
fn test_handle_request_refuses_an_unsupported_method() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(7),
        "textDocument/hover".to_string(),
        serde_json::Value::Null,
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    assert_eq!(response.id, RequestId::from(7));
    assert_eq!(
        response.response_result.err().map(|error| error.code),
        Some(ErrorCode::MethodNotFound as i32)
    );
}

/// The start of the first diagnostic published for `text`.
fn first_diagnostic_start(text: &str, encoding: PositionEncoding) -> lsp_types::Position {
    let mut state = ServerState::new(encoding);
    let replies = handle_notification(&mut state, did_open(text));
    published(&replies[0]).diagnostics[0].range.start
}

#[test]
fn test_did_open_counts_an_astral_character_twice_in_utf16() {
    // Given — a crab (two UTF-16 units) and a space before the reference
    let text = "🦀 |nosub|\n";

    // When
    let start = first_diagnostic_start(text, PositionEncoding::Utf16);

    // Then
    assert_eq!(start, lsp_types::Position::new(0, 3));
}

#[test]
fn test_did_open_counts_an_astral_character_once_in_utf32() {
    // Given
    let text = "🦀 |nosub|\n";

    // When
    let start = first_diagnostic_start(text, PositionEncoding::Utf32);

    // Then
    assert_eq!(start, lsp_types::Position::new(0, 2));
}

#[test]
fn test_did_change_of_an_unopened_document_publishes_for_it() {
    // Given — a client that skipped didOpen, against the protocol
    let mut state = ServerState::new(PositionEncoding::Utf16);

    // When
    let replies = handle_notification(&mut state, did_change(".. foo::\n"));

    // Then — leniently treated as opening it, rather than dropped
    let params = published(&replies[0]);
    assert_eq!(params.version, Some(2));
    assert_eq!(codes(&params), vec!["directive.unknown".to_string()]);
}

#[test]
fn test_did_close_of_an_unopened_document_clears_its_diagnostics() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);

    // When
    let replies = handle_notification(&mut state, did_close());

    // Then
    assert_eq!(published(&replies[0]).diagnostics, Vec::new());
}

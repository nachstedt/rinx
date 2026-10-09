//! The server with a workspace: the scan's messages, an edit racing the scan,
//! documents that include an open file without being open themselves, and the
//! folded index staying what a fresh scan would build.

use super::test_support::*;
use std::path::Path;

const BROKEN: &str = "Fine.\n\n.. foo::\n";

#[test]
fn test_start_scan_says_indexing_and_asks_for_a_progress_token() {
    // Given
    let workspace = Workspace::new("start", &[("index.rst", "Home\n====\n")]);

    // When
    let (_, started) = workspace.server(true);

    // Then
    assert!(
        matches!(&started[0], Message::Request(request) if request.method == "window/workDoneProgress/create")
    );
    assert_eq!(
        statuses(&started),
        [serde_json::json!({"state": "indexing", "documents": 0, "projects": []})]
    );
}

#[test]
fn test_start_scan_without_a_folder_sends_nothing() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);

    // When / Then
    assert!(state.start_scan().is_empty());
}

#[test]
fn test_a_finished_scan_reports_every_document_ready() {
    // Given
    let workspace = Workspace::new(
        "ready",
        &[
            ("index.rst", "Home\n====\n"),
            ("guide/setup.rst", "Setup\n=====\n"),
        ],
    );
    let (mut state, _) = workspace.server(false);

    // When
    let finished = state.on_scan_event(workspace.scanned());

    // Then
    assert_eq!(
        statuses(&finished),
        [serde_json::json!({
            "state": "ready",
            "documents": 2,
            "elapsedMs": 800,
            "projects": [{"kind": "folder", "root": "rinx_lsp_workspace_ready"}],
        })]
    );
}

#[test]
fn test_an_edit_made_during_the_scan_survives_it() {
    // Given — the scan reads `a.rst` from disk, but the author has already
    // renamed its label in the editor.
    let workspace = Workspace::new("race", &[("a.rst", ".. _saved:\n\nText.\n")]);
    let (mut state, _) = workspace.server(false);
    let scan = workspace.scanned();
    handle_notification(
        &mut state,
        workspace.open("a.rst", ".. _edited:\n\nText.\n"),
    );

    // When
    state.on_scan_event(scan);

    // Then
    assert_eq!(project_labels(&mut state, 0), ["edited"]);
}

#[test]
fn test_closing_a_document_returns_its_index_entry_to_the_saved_text() {
    // Given
    let workspace = Workspace::new("close", &[("a.rst", ".. _saved:\n\nText.\n")]);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("a.rst", ".. _saved:\n\nText.\n"));
    handle_notification(
        &mut state,
        workspace.change("a.rst", 2, ".. _unsaved:\n\nText.\n"),
    );
    assert_eq!(project_labels(&mut state, 0), ["unsaved"]);

    // When — closed without saving.
    handle_notification(&mut state, workspace.close("a.rst"));

    // Then
    assert_eq!(project_labels(&mut state, 0), ["saved"]);
}

#[test]
fn test_a_fragment_opened_alone_shows_what_its_closed_includer_finds() {
    // Given — `index.rst` includes the fragment and is never opened.
    let workspace = Workspace::new(
        "closed_includer",
        &[
            ("index.rst", "Home\n====\n\n.. include:: _part.inc\n"),
            ("_part.inc", BROKEN),
        ],
    );
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());

    // When
    let opened = handle_notification(&mut state, workspace.open("_part.inc", BROKEN));

    // Then — the fragment shows the mistake, and the closed includer nothing.
    assert_eq!(
        workspace.codes_for(&opened, "_part.inc"),
        Some(vec!["directive.unknown".to_string()])
    );
    assert_eq!(workspace.codes_for(&opened, "index.rst"), None);
}

#[test]
fn test_a_scan_finishing_after_a_fragment_opened_brings_in_its_closed_includer() {
    // Given — the fragment opened before the scan knew who includes it.
    let workspace = Workspace::new(
        "late_includer",
        &[
            ("index.rst", "Home\n====\n\n.. include:: _part.inc\n"),
            ("_part.inc", BROKEN),
        ],
    );
    let (mut state, _) = workspace.server(false);
    let scan = workspace.scanned();
    handle_notification(&mut state, workspace.open("_part.inc", BROKEN));

    // When
    let finished = state.on_scan_event(scan);

    // Then
    assert_eq!(
        workspace.codes_for(&finished, "_part.inc"),
        Some(vec!["directive.unknown".to_string()])
    );
}

#[test]
fn test_editing_a_fragment_updates_what_its_closed_includer_finds() {
    // Given
    let workspace = Workspace::new(
        "edit_fragment",
        &[
            ("index.rst", "Home\n====\n\n.. include:: _part.inc\n"),
            ("_part.inc", BROKEN),
        ],
    );
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("_part.inc", BROKEN));

    // When — the mistake is fixed, unsaved.
    let changed = handle_notification(&mut state, workspace.change("_part.inc", 2, "Fine.\n"));

    // Then
    assert_eq!(workspace.codes_for(&changed, "_part.inc"), Some(Vec::new()));
}

#[test]
fn test_an_open_workspace_document_names_its_included_file_relatively() {
    // Given — a workspace document, named by its path within the folder.
    let workspace = Workspace::new(
        "summary_name",
        &[
            ("index.rst", "Home\n====\n\n.. include:: _part.inc\n"),
            ("_part.inc", BROKEN),
        ],
    );
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());

    // When
    let opened = handle_notification(
        &mut state,
        workspace.open("index.rst", "Home\n====\n\n.. include:: _part.inc\n"),
    );

    // Then — the summary on the include quotes the file as written there.
    let summary = opened
        .iter()
        .map(published)
        .find(|params| params.uri == workspace.uri("index.rst"))
        .and_then(|params| params.diagnostics.first().cloned())
        .expect("a summary on the include");
    assert!(
        summary
            .message
            .starts_with("problem in included file '_part.inc'"),
        "{}",
        summary.message
    );
}

#[test]
fn test_a_label_in_a_fragment_belongs_to_its_closed_includer() {
    // Given
    let workspace = Workspace::new(
        "fragment_label",
        &[
            ("index.rst", "Home\n====\n\n.. include:: _part.inc\n"),
            ("_part.inc", ".. _old:\n\nText.\n"),
        ],
    );
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(
        &mut state,
        workspace.open("_part.inc", ".. _old:\n\nText.\n"),
    );

    // When — the fragment's label is renamed, unsaved.
    handle_notification(
        &mut state,
        workspace.change("_part.inc", 2, ".. _new:\n\nText.\n"),
    );

    // Then — the includer's entry changed with it.
    assert_eq!(project_labels(&mut state, 0), ["new"]);
}

#[test]
fn test_closing_the_fragment_lets_go_of_its_closed_includer() {
    // Given
    let workspace = Workspace::new(
        "release_includer",
        &[
            ("index.rst", "Home\n====\n\n.. include:: _part.inc\n"),
            ("_part.inc", BROKEN),
        ],
    );
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("_part.inc", BROKEN));

    // When
    let closed = handle_notification(&mut state, workspace.close("_part.inc"));

    // Then — nobody is looking at the fragment any more.
    assert_eq!(workspace.codes_for(&closed, "_part.inc"), Some(Vec::new()));
    assert!(
        state
            .graph
            .includers_of(&workspace.root.join("_part.inc"))
            .is_empty()
    );
}

#[test]
fn test_the_folded_index_after_edits_equals_a_fresh_scan() {
    // Given — a workspace edited through the server, then saved as edited.
    let workspace = Workspace::new(
        "incremental",
        &[
            ("index.rst", "Home\n====\n\n.. toctree::\n\n   a\n   b\n"),
            ("a.rst", ".. _a-label:\n\nA\n=\n"),
            ("b.rst", ".. _b-label:\n\nB\n=\n\n.. include:: _part.inc\n"),
            ("_part.inc", ".. _part-label:\n\nText.\n"),
        ],
    );
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    let edits = [
        ("a.rst", ".. _renamed:\n\nA\n=\n"),
        ("_part.inc", "No label any more.\n"),
        ("index.rst", "Home\n====\n\n.. toctree::\n\n   b\n"),
    ];
    for (file, text) in edits {
        handle_notification(&mut state, workspace.open(file, text));
        workspace.write(file, text);
        handle_notification(&mut state, workspace.close(file));
    }

    // When
    let (mut fresh, _) = workspace.server(false);
    fresh.on_scan_event(workspace.scanned());

    // Then
    assert_eq!(
        project(&mut state, 0).project_index(),
        project(&mut fresh, 0).project_index()
    );
    assert_eq!(project_labels(&mut state, 0), ["b-label", "renamed"]);
}

#[test]
fn test_the_folded_index_equals_build_project_index_on_the_examples() {
    // Given — the repository's own example site and documentation, as the
    // folder's scan reads them.
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for folder in ["examples", "docs"] {
        let root = repository
            .join(folder)
            .canonicalize()
            .expect("the folder exists");
        let mut state =
            ServerState::new(PositionEncoding::Utf16).with_workspace(vec![root.clone()], false);
        state.start_scan();
        let scanned = scan_folder(&root, &|_, _| {});
        let discovered = scanned[0].discovered.clone();
        state.on_scan_event(ScanEvent::Finished {
            folder: 0,
            projects: scanned,
            elapsed: Duration::ZERO,
        });

        // When
        let documents = crate::project::parse_project_documents(&discovered);
        let built = rinx_analyzer::build_project_index(
            &documents,
            "index",
            &rinx_entity::EntitySchema::empty(),
        );

        // Then
        assert!(!documents.is_empty(), "{folder} holds documents");
        assert_eq!(project(&mut state, 0).project_index(), &built, "{folder}");
    }
}

#[test]
fn test_run_scans_the_workspace_and_reports_it_ready() {
    // Given — a client with a workspace folder, taking progress reports.
    let workspace = Workspace::new(
        "session",
        &[
            ("index.rst", "Home\n====\n"),
            ("guide.rst", "Guide\n=====\n"),
        ],
    );
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || run(&server));
    let params = InitializeParams {
        workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
            uri: crate::uri::file_uri(&workspace.root).expect("an absolute path"),
            name: "docs".to_string(),
        }]),
        capabilities: lsp_types::ClientCapabilities {
            window: Some(lsp_types::WindowClientCapabilities {
                work_done_progress: Some(true),
                ..lsp_types::WindowClientCapabilities::default()
            }),
            ..lsp_types::ClientCapabilities::default()
        },
        ..InitializeParams::default()
    };
    initialize_with(&client, params);

    // When — reading until the index is ready, accepting the progress token.
    let mut seen = Vec::new();
    let ready = loop {
        let message = client
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the server keeps talking");
        if let Message::Request(request) = &message {
            client
                .sender
                .send(Response::new_ok(request.id.clone(), serde_json::Value::Null).into())
                .unwrap();
        }
        let status = statuses(std::slice::from_ref(&message));
        seen.push(message);
        if status
            .first()
            .is_some_and(|status| status["state"] == "ready")
        {
            break status[0].clone();
        }
    };
    client
        .sender
        .send(Request::new(RequestId::from(2), Shutdown::METHOD.to_string(), ()).into())
        .unwrap();
    let _ = client.receiver.recv_timeout(Duration::from_secs(10));
    client.sender.send(exit().into()).unwrap();

    // Then
    assert_eq!(ready["documents"], 2);
    assert!(
        matches!(&seen[0], Message::Request(request) if request.method == "window/workDoneProgress/create"),
        "{seen:?}"
    );
    assert_eq!(statuses(&seen)[0]["state"], "indexing");
    handle.join().expect("server thread").expect("clean exit");
}

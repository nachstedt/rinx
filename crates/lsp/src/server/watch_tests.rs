//! The disk changing outside the editor: the watcher the server asks for, and
//! what a reported creation, change or deletion does to the workspace index
//! and to what the open documents show.

use super::test_support::*;
use lsp_types::FileChangeType;

const CREATED: FileChangeType = FileChangeType::CREATED;
const CHANGED: FileChangeType = FileChangeType::CHANGED;
const DELETED: FileChangeType = FileChangeType::DELETED;

const REFERRING: &str = "Home\n====\n\nSee :doc:`setup`.\n";
const SETUP: &str = ".. _install:\n\nSetup\n=====\n";

/// A scanned server over `files`, with nothing open.
fn scanned(name: &str, files: &[(&str, &str)]) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, files);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    (workspace, state)
}

/// A scanned server with `index.rst`, referring to `setup.rst`, open and
/// rendered.
fn referring(name: &str) -> (Workspace, ServerState) {
    let (workspace, mut state) = scanned(name, &[("index.rst", REFERRING), ("setup.rst", SETUP)]);
    handle_notification(&mut state, workspace.open("index.rst", REFERRING));
    render_all(&mut state);
    (workspace, state)
}

fn broken_doc() -> Vec<String> {
    vec!["link.broken-doc".to_string()]
}

#[test]
fn test_watch_files_registers_one_watcher_for_every_file() {
    // Given
    let workspace = Workspace::new("watch_register", &[("index.rst", "Home\n")]);
    let mut state = ServerState::new(PositionEncoding::Utf16)
        .with_workspace(vec![workspace.root.clone()], true)
        .with_watched_files(true);

    // When
    let registered = state.watch_files();
    let started = state.start_scan();

    // Then
    let [Message::Request(registration)] = registered.as_slice() else {
        panic!("expected one request, got {registered:?}");
    };
    assert_eq!(registration.method, "client/registerCapability");
    assert_eq!(
        registration.params["registrations"][0]["registerOptions"],
        serde_json::json!({"watchers": [{"globPattern": "**/*"}]})
    );
    let Message::Request(progress) = &started[0] else {
        panic!("expected the progress request");
    };
    assert_ne!(progress.id, registration.id);
}

#[test]
fn test_watch_files_asks_nothing_of_a_client_that_cannot_register() {
    // Given
    let workspace = Workspace::new("watch_unsupported", &[]);
    let (mut unsupported, _) = workspace.server(false);
    let mut no_folder = ServerState::new(PositionEncoding::Utf16).with_watched_files(true);

    // When / Then
    assert!(unsupported.watch_files().is_empty());
    assert!(no_folder.watch_files().is_empty());
}

#[test]
fn test_a_deleted_document_leaves_the_index_and_breaks_its_references() {
    // Given
    let (workspace, mut state) = referring("watch_deleted");

    // When
    workspace.remove("setup.rst");
    handle_notification(&mut state, workspace.watched(&[("setup.rst", DELETED)]));
    let rendered = render_all(&mut state);

    // Then
    assert!(project_labels(&mut state, 0).is_empty());
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(broken_doc())
    );
}

#[test]
fn test_a_created_document_joins_the_index_and_mends_its_references() {
    // Given — the referenced document does not exist yet
    let (workspace, mut state) = scanned("watch_created", &[("index.rst", REFERRING)]);
    handle_notification(&mut state, workspace.open("index.rst", REFERRING));
    assert_eq!(
        workspace.codes_for(&render_all(&mut state), "index.rst"),
        Some(broken_doc())
    );

    // When
    workspace.write("setup.rst", SETUP);
    handle_notification(&mut state, workspace.watched(&[("setup.rst", CREATED)]));
    let rendered = render_all(&mut state);

    // Then
    assert_eq!(project_labels(&mut state, 0), ["install"]);
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(Vec::new())
    );
}

#[test]
fn test_a_renamed_document_is_known_by_its_new_name_only() {
    // Given
    let (workspace, mut state) = referring("watch_renamed");

    // When — the Explorer reports a rename as a deletion and a creation
    workspace.rename("setup.rst", "install.rst");
    handle_notification(
        &mut state,
        workspace.watched(&[("setup.rst", DELETED), ("install.rst", CREATED)]),
    );
    let rendered = render_all(&mut state);

    // Then
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(broken_doc())
    );
    let index = project(&mut state, 0).project_index();
    assert!(index.documents.contains("install.rst"));
    assert!(!index.documents.contains("setup.rst"));
}

#[test]
fn test_a_closed_document_changed_on_disk_is_read_again() {
    // Given
    let (workspace, mut state) = scanned("watch_changed", &[("setup.rst", SETUP)]);

    // When
    workspace.write("setup.rst", ".. _configure:\n\nSetup\n=====\n");
    handle_notification(&mut state, workspace.watched(&[("setup.rst", CHANGED)]));

    // Then
    assert_eq!(project_labels(&mut state, 0), ["configure"]);
}

#[test]
fn test_an_open_document_changed_on_disk_keeps_its_buffer() {
    // Given — the buffer defines `edited`, the disk `saved`
    let (workspace, mut state) = scanned("watch_open", &[("setup.rst", SETUP)]);
    handle_notification(
        &mut state,
        workspace.open("setup.rst", ".. _edited:\n\nSetup\n=====\n"),
    );
    let generation = project(&mut state, 0).generation();

    // When
    workspace.write("setup.rst", ".. _saved:\n\nSetup\n=====\n");
    let replies = handle_notification(&mut state, workspace.watched(&[("setup.rst", CHANGED)]));

    // Then
    assert!(replies.is_empty());
    assert_eq!(project(&mut state, 0).generation(), generation);
    assert_eq!(project_labels(&mut state, 0), ["edited"]);
}

#[test]
fn test_a_fragment_changed_on_disk_rediagnoses_its_open_includer() {
    // Given
    let includer = "Home\n====\n\n.. include:: _shared/note.txt\n";
    let (workspace, mut state) = scanned(
        "watch_fragment",
        &[("index.rst", includer), ("_shared/note.txt", "Fine.\n")],
    );
    handle_notification(&mut state, workspace.open("index.rst", includer));

    // When
    workspace.write("_shared/note.txt", "Fine.\n\n.. foo::\n");
    let replies = handle_notification(
        &mut state,
        workspace.watched(&[("_shared/note.txt", CHANGED)]),
    );

    // Then — the fragment shows what its includer found in it
    assert_eq!(
        workspace.codes_for(&replies, "_shared/note.txt"),
        Some(vec!["directive.unknown".to_string()])
    );
}

#[test]
fn test_a_missing_file_created_on_disk_mends_its_reader() {
    // Given — a table reading a file that does not exist yet
    let table = "Home\n====\n\n.. csv-table:: Data\n   :file: data.csv\n";
    let (workspace, mut state) = scanned("watch_missing", &[("index.rst", table)]);
    let opened = handle_notification(&mut state, workspace.open("index.rst", table));
    assert_ne!(workspace.codes_for(&opened, "index.rst"), Some(Vec::new()));

    // When
    workspace.write("data.csv", "a, b\n");
    let replies = handle_notification(&mut state, workspace.watched(&[("data.csv", CREATED)]));

    // Then
    assert_eq!(workspace.codes_for(&replies, "index.rst"), Some(Vec::new()));
}

#[test]
fn test_a_deleted_directory_forgets_every_document_under_it() {
    // Given
    let (workspace, mut state) = scanned(
        "watch_deleted_directory",
        &[
            ("guide/a.rst", ".. _a:\n\nA\n=\n"),
            ("guide/deep/b.rst", ".. _b:\n\nB\n=\n"),
            ("index.rst", ".. _home:\n\nHome\n====\n"),
        ],
    );

    // When — the watcher reports the directory alone
    workspace.remove("guide");
    handle_notification(&mut state, workspace.watched(&[("guide", DELETED)]));

    // Then
    assert_eq!(project_labels(&mut state, 0), ["home"]);
}

#[test]
fn test_a_created_directory_indexes_every_document_in_it() {
    // Given
    let (workspace, mut state) = scanned("watch_created_directory", &[]);

    // When — moved in from elsewhere, reported as the directory alone
    workspace.write("guide/a.rst", ".. _a:\n\nA\n=\n");
    workspace.write("guide/.hidden/b.rst", ".. _b:\n\nB\n=\n");
    handle_notification(&mut state, workspace.watched(&[("guide", CREATED)]));

    // Then — what the scan would have found, and no more
    assert_eq!(project_labels(&mut state, 0), ["a"]);
}

#[test]
fn test_a_file_no_document_reads_changes_nothing() {
    // Given
    let (workspace, mut state) = scanned("watch_unrelated", &[("index.rst", SETUP)]);
    let generation = project(&mut state, 0).generation();

    // When — build output, and a document hidden from the scan
    workspace.write("target/out.o", "binary");
    workspace.write(".venv/readme.rst", ".. _venv:\n\nVenv\n====\n");
    let replies = handle_notification(
        &mut state,
        workspace.watched(&[
            ("target/out.o", CREATED),
            ("target", CREATED),
            (".venv/readme.rst", CREATED),
        ]),
    );

    // Then
    assert!(replies.is_empty());
    assert_eq!(project(&mut state, 0).generation(), generation);
}

#[test]
fn test_a_document_deleted_during_the_scan_is_not_brought_back() {
    // Given — the scan has read `setup.rst`
    let workspace = Workspace::new("watch_scan_race", &[("setup.rst", SETUP)]);
    let (mut state, _) = workspace.server(false);
    let scan = workspace.scanned();

    // When — it is deleted before the scan's result arrives
    workspace.remove("setup.rst");
    handle_notification(&mut state, workspace.watched(&[("setup.rst", DELETED)]));
    state.on_scan_event(scan);

    // Then
    assert!(project_labels(&mut state, 0).is_empty());
}

#[test]
fn test_closing_a_document_deleted_on_disk_drops_it_from_the_index() {
    // Given — open, then deleted while open, as a rename of an open file is
    let (workspace, mut state) = scanned("watch_closed_deleted", &[("setup.rst", SETUP)]);
    handle_notification(&mut state, workspace.open("setup.rst", SETUP));
    workspace.remove("setup.rst");

    // When
    handle_notification(&mut state, workspace.close("setup.rst"));

    // Then
    assert!(project_labels(&mut state, 0).is_empty());
}

#[test]
fn test_a_malformed_watch_notification_is_ignored() {
    // Given
    let (_, mut state) = scanned("watch_malformed", &[]);
    let malformed = Notification::new(
        lsp_types::notification::DidChangeWatchedFiles::METHOD.to_string(),
        serde_json::json!({"changes": "no"}),
    );

    // When / Then
    assert!(handle_notification(&mut state, malformed).is_empty());
}

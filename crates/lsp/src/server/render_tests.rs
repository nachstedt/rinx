//! The second tier of diagnostics: what rendering finds, for the documents the
//! server shows, kept current as the index changes.

use super::test_support::*;

const REFERENCING: &str = "Title\n=====\n\nSee :ref:`install`.\n";
const DEFINING: &str = ".. _install:\n\nInstalling\n==========\n";

/// Every render the server has pending, run to the end, as the messages it
/// sends.
fn render_all(state: &mut ServerState) -> Vec<Message> {
    let mut messages = Vec::new();
    while state.has_pending_renders() {
        messages.extend(state.render_next());
    }
    messages
}

/// A scanned server over `files`, with nothing open.
fn scanned(name: &str, files: &[(&str, &str)]) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, files);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    (workspace, state)
}

#[test]
fn test_a_reference_to_no_label_is_reported_once_rendered() {
    // Given
    let (workspace, mut state) = scanned("render_broken", &[("index.rst", REFERENCING)]);
    let opened = handle_notification(&mut state, workspace.open("index.rst", REFERENCING));

    // When
    let rendered = render_all(&mut state);

    // Then — the parse found nothing; the render found the reference
    assert_eq!(workspace.codes_for(&opened, "index.rst"), Some(Vec::new()));
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(vec!["link.broken-ref".to_string()])
    );
    assert!(!state.has_pending_renders());
}

#[test]
fn test_defining_the_label_in_another_buffer_clears_the_reference() {
    // Given — a rendered broken reference
    let (workspace, mut state) = scanned(
        "render_cleared",
        &[("index.rst", REFERENCING), ("setup.rst", "Setup\n=====\n")],
    );
    handle_notification(&mut state, workspace.open("index.rst", REFERENCING));
    render_all(&mut state);

    // When — another document gains the label, unsaved
    handle_notification(&mut state, workspace.open("setup.rst", DEFINING));
    let rendered = render_all(&mut state);

    // Then
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(Vec::new())
    );
}

#[test]
fn test_an_edit_drops_what_the_last_render_found_until_rendered_again() {
    // Given — a rendered broken reference
    let (workspace, mut state) = scanned("render_edited", &[("index.rst", REFERENCING)]);
    handle_notification(&mut state, workspace.open("index.rst", REFERENCING));
    render_all(&mut state);

    // When — its line moves down: the old position would be wrong
    let edited = format!("\n{REFERENCING}");
    let changed = handle_notification(&mut state, workspace.change("index.rst", 2, &edited));

    // Then
    assert_eq!(workspace.codes_for(&changed, "index.rst"), Some(Vec::new()));
    assert!(state.has_pending_renders());
    let rendered = render_all(&mut state);
    let Some(Message::Notification(notification)) = rendered.last() else {
        panic!("a publish, got {rendered:?}");
    };
    let published: PublishDiagnosticsParams =
        serde_json::from_value(notification.params.clone()).expect("a publish");
    assert_eq!(published.diagnostics[0].range.start.line, 4);
}

#[test]
fn test_a_closed_document_is_not_rendered() {
    // Given / When — a broken reference in a document nobody opened
    let (_workspace, state) = scanned("render_closed", &[("index.rst", REFERENCING)]);

    // Then
    assert!(!state.has_pending_renders());
}

#[test]
fn test_a_document_outside_every_workspace_folder_is_not_rendered() {
    // Given — with no index, every reference to another document would be
    // reported broken
    let mut state = ServerState::new(PositionEncoding::Utf16);

    // When
    handle_notification(&mut state, did_open(REFERENCING));

    // Then
    assert!(!state.has_pending_renders());
}

#[test]
fn test_an_open_fragment_shows_what_rendering_its_closed_includer_found() {
    // Given — the reference is in a fragment, which is no document of its own
    let (workspace, mut state) = scanned(
        "render_fragment",
        &[
            ("index.rst", "Title\n=====\n\n.. include:: part.inc\n"),
            ("part.inc", "See :ref:`install`.\n"),
        ],
    );

    // When
    handle_notification(
        &mut state,
        workspace.open("part.inc", "See :ref:`install`.\n"),
    );
    let rendered = render_all(&mut state);

    // Then — on the fragment's own line; the closed includer shows nothing
    assert_eq!(
        workspace.codes_for(&rendered, "part.inc"),
        Some(vec!["link.broken-ref".to_string()])
    );
    assert_eq!(workspace.codes_for(&rendered, "index.rst"), None);
}

#[test]
fn test_render_next_renders_the_latest_edited_document_first() {
    // Given — two open documents, both pending; `b.rst` edited last
    let (workspace, mut state) = scanned(
        "render_order",
        &[("a.rst", REFERENCING), ("b.rst", REFERENCING)],
    );
    handle_notification(&mut state, workspace.open("a.rst", REFERENCING));
    handle_notification(&mut state, workspace.open("b.rst", REFERENCING));

    // When
    let first = state.render_next();

    // Then
    assert!(workspace.codes_for(&first, "b.rst").is_some(), "{first:?}");
    assert_eq!(workspace.codes_for(&first, "a.rst"), None);
}

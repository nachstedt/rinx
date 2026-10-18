//! The second tier of diagnostics: what rendering finds, for the documents the
//! server shows, kept current as the index changes.

use super::test_support::*;

const REFERENCING: &str = "Title\n=====\n\nSee :ref:`install`.\n";
const DEFINING: &str = ".. _install:\n\nInstalling\n==========\n";

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

/// The code and 0-based line of each diagnostic `replies` last publish for
/// `file`.
fn lines_for(workspace: &Workspace, replies: &[Message], file: &str) -> Vec<(String, u32)> {
    let uri = workspace.uri(file);
    replies
        .iter()
        .filter(|reply| matches!(reply, Message::Notification(n) if n.method == PublishDiagnostics::METHOD))
        .map(published)
        .filter(|params| params.uri == uri)
        .last()
        .map(|params| {
            params
                .diagnostics
                .iter()
                .filter_map(|diagnostic| match &diagnostic.code {
                    Some(NumberOrString::String(code)) => {
                        Some((code.clone(), diagnostic.range.start.line))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

const TOCTREE: &str = "Home\n====\n\n.. toctree::\n\n   setup\n   gone\n";

#[test]
fn test_a_toctree_entry_naming_no_document_is_reported_on_its_line() {
    // Given
    let (workspace, mut state) = scanned(
        "render_index_toctree",
        &[("index.rst", TOCTREE), ("setup.rst", DEFINING)],
    );
    handle_notification(&mut state, workspace.open("index.rst", TOCTREE));

    // When
    let rendered = render_all(&mut state);

    // Then — what only the whole project reveals arrives with the render
    assert_eq!(
        lines_for(&workspace, &rendered, "index.rst"),
        [("toctree.missing-document".to_string(), 6)]
    );
}

#[test]
fn test_a_label_two_documents_define_is_reported_in_each() {
    // Given
    let other = ".. _install:\n\nAgain\n=====\n";
    let (workspace, mut state) = scanned(
        "render_index_duplicate",
        &[("setup.rst", DEFINING), ("other.rst", other)],
    );
    handle_notification(&mut state, workspace.open("setup.rst", DEFINING));
    handle_notification(&mut state, workspace.open("other.rst", other));

    // When
    let rendered = render_all(&mut state);

    // Then — on the first line, since neither definition is the wrong one
    for file in ["setup.rst", "other.rst"] {
        let found = lines_for(&workspace, &rendered, file);
        assert!(
            found.contains(&("target.duplicate-name".to_string(), 0)),
            "{file}: {found:?}"
        );
    }
}

#[test]
fn test_a_noqa_silences_what_the_index_found() {
    // Given — the toctree knows it names a document not written yet
    let text =
        "Home\n====\n\n.. noqa: toctree.missing-document\n\n.. toctree::\n\n   setup\n   gone\n";
    let (workspace, mut state) = scanned(
        "render_index_noqa",
        &[("index.rst", text), ("setup.rst", DEFINING)],
    );
    handle_notification(&mut state, workspace.open("index.rst", text));

    // When
    let rendered = render_all(&mut state);

    // Then — as the build filters it. A finding with no position, such as an
    // orphan, cannot be silenced at all (ADR-003).
    assert_eq!(workspace.codes_for(&rendered, "index.rst"), None);
}

#[test]
fn test_writing_the_missing_document_clears_the_toctree_entry() {
    // Given
    let (workspace, mut state) = scanned(
        "render_index_written",
        &[("index.rst", TOCTREE), ("setup.rst", DEFINING)],
    );
    handle_notification(&mut state, workspace.open("index.rst", TOCTREE));
    render_all(&mut state);

    // When
    workspace.write("gone.rst", "Gone\n====\n");
    handle_notification(
        &mut state,
        workspace.watched(&[("gone.rst", lsp_types::FileChangeType::CREATED)]),
    );
    let rendered = render_all(&mut state);

    // Then
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(Vec::new())
    );
}

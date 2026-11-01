//! The quick fix writing a `.. noqa:`: which findings offer it, what
//! applying it changes, and that the finding is then gone.

use lsp_types::{CodeAction, DocumentChanges, OneOf, TextEdit};

use super::code_action_tests::{actions_at, apply};
use super::renames::EditShape;
use super::test_support::*;

/// An index whose note holds a reference no document defines.
const INDEX: &str = "Home\n====\n\n.. note::\n\n   :ref:`missing` is broken.\n";

/// The edits `action` makes to `file` of `workspace`, sent by URI.
fn edits_of(action: &CodeAction, workspace: &Workspace, file: &str) -> Vec<TextEdit> {
    let changes = action
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.clone())
        .expect("changes by URI");
    changes[&workspace.uri(file)].clone()
}

#[test]
fn test_a_broken_reference_found_rendering_is_suppressed_by_one_click() {
    // Given — the reference reported once the document is rendered
    let (workspace, mut state) = scanned("noqa_action_render", &[("index.rst", INDEX)]);
    handle_notification(&mut state, workspace.open("index.rst", INDEX));
    let rendered = render_all(&mut state);
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(vec!["link.broken-ref".to_string()])
    );

    // When — the cursor on the reference, and the fix applied
    let actions = actions_at(&mut state, &workspace, "index.rst", 5);
    assert_eq!(actions.len(), 1, "{actions:?}");
    let fixed = apply(INDEX, &edits_of(&actions[0], &workspace, "index.rst"));
    let mut replies = handle_notification(&mut state, workspace.change("index.rst", 2, &fixed));
    replies.extend(render_all(&mut state));

    // Then — written inside the note, before the paragraph, and silencing it
    assert_eq!(actions[0].title, "Suppress link.broken-ref with .. noqa:");
    assert_eq!(actions[0].kind, Some(lsp_types::CodeActionKind::QUICKFIX));
    assert_eq!(actions[0].is_preferred, None);
    let diagnostics = actions[0].diagnostics.as_deref().unwrap_or_default();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        fixed,
        "Home\n====\n\n.. note::\n\n   .. noqa: link.broken-ref\n\n   :ref:`missing` is broken.\n"
    );
    assert_eq!(workspace.codes_for(&replies, "index.rst"), Some(vec![]));
}

#[test]
fn test_a_parse_finding_is_suppressed_before_its_block() {
    // Given
    let text = "Home\n====\n\n.. foo::\n";
    let (workspace, mut state) = scanned("noqa_action_parse", &[("index.rst", text)]);
    handle_notification(&mut state, workspace.open("index.rst", text));

    // When
    let actions = actions_at(&mut state, &workspace, "index.rst", 3);
    let fixed = apply(text, &edits_of(&actions[0], &workspace, "index.rst"));
    let replies = handle_notification(&mut state, workspace.change("index.rst", 2, &fixed));

    // Then
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(
        fixed,
        "Home\n====\n\n.. noqa: directive.unknown\n\n.. foo::\n"
    );
    assert_eq!(workspace.codes_for(&replies, "index.rst"), Some(vec![]));
}

#[test]
fn test_no_fix_is_offered_where_nothing_is_found() {
    // Given
    let (workspace, mut state) = scanned("noqa_action_none", &[("index.rst", INDEX)]);
    handle_notification(&mut state, workspace.open("index.rst", INDEX));
    render_all(&mut state);

    // When — on the title
    let actions = actions_at(&mut state, &workspace, "index.rst", 0);

    // Then
    assert_eq!(actions, []);
}

#[test]
fn test_a_fragments_finding_is_suppressed_in_the_fragment() {
    // Given — the fragment closed, shown through the document including it
    let part = "Part\n\n.. foo::\n";
    let index = "Home\n====\n\n.. include:: part.rst\n";
    let (workspace, mut state) = scanned(
        "noqa_action_fragment",
        &[("index.rst", index), ("part.rst", part)],
    );
    handle_notification(&mut state, workspace.open("index.rst", index));

    // When — asked in the fragment, and the fix applied to its buffer
    let actions = actions_at(&mut state, &workspace, "part.rst", 2);
    let fixed = apply(part, &edits_of(&actions[0], &workspace, "part.rst"));
    let replies = handle_notification(&mut state, workspace.open("part.rst", &fixed));

    // Then
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(fixed, "Part\n\n.. noqa: directive.unknown\n\n.. foo::\n");
    assert_eq!(workspace.codes_for(&replies, "part.rst"), Some(vec![]));
}

#[test]
fn test_a_client_taking_versioned_edits_gets_the_open_documents_version() {
    // Given
    let text = ".. foo::\n";
    let (workspace, state) = scanned("noqa_action_versioned", &[("index.rst", text)]);
    let mut state = state.with_edit_shape(EditShape::Versioned);
    handle_notification(&mut state, workspace.open("index.rst", text));

    // When
    let actions = actions_at(&mut state, &workspace, "index.rst", 0);

    // Then
    let changes = actions[0]
        .edit
        .as_ref()
        .and_then(|edit| edit.document_changes.clone());
    let Some(DocumentChanges::Edits(edits)) = changes else {
        panic!("versioned changes, got {changes:?}");
    };
    let [edit] = edits.as_slice() else {
        panic!("one document edit, got {edits:?}");
    };
    assert_eq!(edit.text_document.uri, workspace.uri("index.rst"));
    assert_eq!(edit.text_document.version, Some(1));
    assert!(
        matches!(&edit.edits[..], [OneOf::Left(_)]),
        "{:?}",
        edit.edits
    );
}

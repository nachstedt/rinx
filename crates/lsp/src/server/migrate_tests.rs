//! The "Migrate to rinx" source action (roadmap #37): where it is offered,
//! what its edit writes, and that the migrated project is then asked about
//! as a Bazel workspace.

use lsp_types::request::CodeActionRequest;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, DocumentChangeOperation, DocumentChanges,
    OneOf, ResourceOp,
};

use super::bazel_manifest_tests::{manifest_requests, scanned_with};
use super::bazel_manifests::{BAZEL_MANIFESTS_CHANGED_METHOD, ManifestSource};
use super::renames::EditShape;
use super::test_support::*;
use crate::code_action::MIGRATE_KIND;

const CONF: &str = "project = 'Demo'\n";
const INDEX: &str = "Home\n====\n\n.. include:: snippet.txt\n";

/// A scanned server over a Sphinx project in `docs/`, offered edits that
/// create files.
fn migrating(name: &str) -> (Workspace, ServerState) {
    let workspace = Workspace::new(
        name,
        &[
            ("docs/conf.py", CONF),
            ("docs/index.rst", INDEX),
            ("docs/snippet.txt", "Snippet.\n"),
        ],
    );
    let (state, _) = workspace.server(false);
    let mut state = state.with_edit_shape(EditShape::CreatingFiles);
    state.on_scan_event(workspace.scanned());
    (workspace, state)
}

/// The actions `state` offers in `file`, of the kinds `only` names.
fn actions(
    state: &mut ServerState,
    workspace: &Workspace,
    file: &str,
    only: Option<&[&str]>,
) -> Vec<CodeAction> {
    let at = lsp_types::Position::new(0, 0);
    let mut context = serde_json::json!({"diagnostics": []});
    if let Some(only) = only {
        context["only"] = serde_json::json!(only);
    }
    let response = handle_request(
        state,
        Request::new(
            RequestId::from(3),
            CodeActionRequest::METHOD.to_string(),
            serde_json::json!({
                "textDocument": {"uri": workspace.uri(file)},
                "range": {"start": at, "end": at},
                "context": context,
            }),
        ),
    );
    let offered: Vec<CodeActionOrCommand> =
        serde_json::from_value(response.response_result.expect("a result")).expect("actions");
    offered
        .into_iter()
        .filter_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => Some(action),
            CodeActionOrCommand::Command(_) => None,
        })
        .collect()
}

/// The migration among `actions`, if one is offered.
fn migration(actions: &[CodeAction]) -> Option<&CodeAction> {
    actions
        .iter()
        .find(|action| action.kind.as_ref().map(CodeActionKind::as_str) == Some(MIGRATE_KIND))
}

/// Each file `action` creates, by its name, with the text it writes there.
fn created(action: &CodeAction) -> Vec<(String, String)> {
    let Some(DocumentChanges::Operations(operations)) = action
        .edit
        .as_ref()
        .and_then(|edit| edit.document_changes.clone())
    else {
        panic!("document changes: {action:?}");
    };
    let mut files = Vec::new();
    let mut operations = operations.into_iter();
    while let Some(operation) = operations.next() {
        let DocumentChangeOperation::Op(ResourceOp::Create(create)) = operation else {
            continue;
        };
        let Some(DocumentChangeOperation::Edit(written)) = operations.next() else {
            panic!("a created file is written");
        };
        assert_eq!(written.text_document.uri, create.uri);
        let OneOf::Left(edit) = &written.edits[0] else {
            panic!("a text edit");
        };
        let name = create.uri.as_str().rsplit('/').next().unwrap().to_string();
        files.push((name, edit.new_text.clone()));
    }
    files
}

#[test]
fn test_conf_py_is_offered_the_migration_writing_the_projects_build() {
    // Given
    let (workspace, mut state) = migrating("migrate_offered");

    // When
    let offered = actions(&mut state, &workspace, "docs/conf.py", None);

    // Then
    let action = migration(&offered).expect("the migration is offered");
    let files = created(action);
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["BUILD.bazel", "rinx.toml", "MODULE.bazel"]);
    assert!(
        files[0].1.contains("parse_data = [\"snippet.txt\"]"),
        "{}",
        files[0].1
    );
    assert!(files[1].1.contains("project = \"Demo\""), "{}", files[1].1);
}

#[test]
fn test_the_migration_is_offered_on_a_document_of_the_project_too() {
    // Given — an editor asks about the documents it attaches the server to
    let (workspace, mut state) = migrating("migrate_document");

    // When
    let offered = actions(&mut state, &workspace, "docs/index.rst", None);

    // Then
    assert!(migration(&offered).is_some(), "{offered:?}");
}

#[test]
fn test_a_folder_without_conf_py_is_offered_no_migration() {
    // Given
    let workspace = Workspace::new("migrate_folder", &[("index.rst", "Home\n====\n")]);
    let (state, _) = workspace.server(false);
    let mut state = state.with_edit_shape(EditShape::CreatingFiles);
    state.on_scan_event(workspace.scanned());

    // When
    let offered = actions(&mut state, &workspace, "index.rst", None);

    // Then
    assert!(migration(&offered).is_none(), "{offered:?}");
}

#[test]
fn test_the_migration_is_offered_when_source_actions_are_asked_for_only() {
    // Given
    let (workspace, mut state) = migrating("migrate_only");

    // When
    let quick_fixes = actions(&mut state, &workspace, "docs/conf.py", Some(&["quickfix"]));
    let source = actions(&mut state, &workspace, "docs/conf.py", Some(&["source"]));

    // Then
    assert!(migration(&quick_fixes).is_none());
    assert!(migration(&source).is_some());
}

#[test]
fn test_the_migration_is_not_offered_to_a_client_creating_no_files() {
    // Given — a client taking versioned edits, but creating no file
    let workspace = Workspace::new("migrate_no_create", &[("conf.py", CONF), ("index.rst", "")]);
    let (state, _) = workspace.server(false);
    let mut state = state.with_edit_shape(EditShape::Versioned);
    state.on_scan_event(workspace.scanned());

    // When
    let offered = actions(&mut state, &workspace, "conf.py", None);

    // Then
    assert!(migration(&offered).is_none());
}

#[test]
fn test_the_migration_is_not_offered_before_the_scan_finishes() {
    // Given — the documents are not read yet, so their reads are unknown
    let workspace = Workspace::new("migrate_scanning", &[("conf.py", CONF), ("index.rst", "")]);
    let (state, _) = workspace.server(false);
    let mut state = state.with_edit_shape(EditShape::CreatingFiles);

    // When
    let offered = actions(&mut state, &workspace, "conf.py", None);

    // Then
    assert!(migration(&offered).is_none());
}

#[test]
fn test_the_migration_is_not_offered_over_a_build_file() {
    // Given
    let (workspace, mut state) = migrating("migrate_existing");
    workspace.write("docs/BUILD.bazel", "");

    // When
    let offered = actions(&mut state, &workspace, "docs/conf.py", None);

    // Then
    assert!(migration(&offered).is_none());
}

#[test]
fn test_a_migrated_projects_own_module_is_asked_about_as_a_bazel_workspace() {
    // Given — a folder that is no Bazel workspace, holding a Sphinx project
    let workspace = Workspace::new(
        "migrate_nested",
        &[("docs/conf.py", CONF), ("docs/index.rst", "Home\n====\n")],
    );
    let (mut state, scanned) = scanned_with(&workspace, ManifestSource::AskClient);
    assert!(manifest_requests(&scanned).is_empty());

    // When — the migration writes the project's module, and the editor says
    // the manifests may have changed
    workspace.write("docs/MODULE.bazel", "module(name = \"docs\")\n");
    let messages = handle_notification(
        &mut state,
        Notification::new(
            BAZEL_MANIFESTS_CHANGED_METHOD.to_string(),
            serde_json::Value::Null,
        ),
    );

    // Then — the project's directory is asked about in the folder's place
    let requests = manifest_requests(&messages);
    let [request] = requests.as_slice() else {
        panic!("one request: {messages:?}");
    };
    assert_eq!(
        request.params["folder"],
        serde_json::json!(workspace.uri("docs"))
    );
}

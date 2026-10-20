//! Quick fixes: which a document is offered, what applying one changes, and
//! that a build regenerated from the edited file no longer fails. The
//! manifests are written by hand, as `rules/site.bzl` would write them.

use lsp_types::request::CodeActionRequest;
use lsp_types::{CodeAction, CodeActionOrCommand, TextEdit};

use super::bazel_manifest_tests::{
    answer, bazel_workspace, library, manifest, manifest_requests, scanned_with, shown_for,
};
use super::bazel_manifests::{BAZEL_MANIFESTS_CHANGED_METHOD, ManifestSource};
use super::test_support::*;

/// An index whose toctree names `guide`, of the library `//:index`.
const INDEX: &str = "Home\n====\n\n.. toctree::\n\n   guide\n";

/// `//:index`'s build file, depending on nothing yet.
const BUILD: &str = "rinx_library(\n    name = \"index\",\n    srcs = [\"index.rst\"],\n)\n\nrinx_library(\n    name = \"guide\",\n    srcs = [\"guide.rst\"],\n)\n";

/// The site's manifest, as the build writes it while `//:index` depends on
/// `deps`.
fn site_manifest(deps: &[&str]) -> String {
    let mut allowed = vec!["index"];
    if !deps.is_empty() {
        allowed.push("guide");
    }
    manifest(
        "//:site",
        vec![
            library(
                "//:index",
                &["index.rst"],
                &serde_json::json!({"allowed_docs": allowed, "deps": deps}),
            ),
            library("//:guide", &["guide.rst"], &serde_json::json!({})),
        ],
    )
}

/// The code actions `state` offers for the cursor on `line` of `file`.
fn actions_at(
    state: &mut ServerState,
    workspace: &Workspace,
    file: &str,
    line: u32,
) -> Vec<CodeAction> {
    let at = lsp_types::Position::new(line, 4);
    let request = Request::new(
        RequestId::from(7),
        CodeActionRequest::METHOD.to_string(),
        serde_json::json!({
            "textDocument": {"uri": workspace.uri(file)},
            "range": {"start": at, "end": at},
            "context": {"diagnostics": []},
        }),
    );
    let response = handle_request(state, request);
    let actions: Vec<CodeActionOrCommand> =
        serde_json::from_value(response.response_result.expect("a result")).expect("code actions");
    actions
        .into_iter()
        .filter_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => Some(action),
            CodeActionOrCommand::Command(_) => None,
        })
        .collect()
}

/// `text` with `edits` applied, each a range of ASCII text.
fn apply(text: &str, edits: &[TextEdit]) -> String {
    let offset = |position: lsp_types::Position| {
        let line_start: usize = text
            .split_inclusive('\n')
            .take(position.line as usize)
            .map(str::len)
            .sum();
        line_start + position.character as usize
    };
    let mut ordered: Vec<&TextEdit> = edits.iter().collect();
    ordered.sort_by_key(|edit| std::cmp::Reverse(offset(edit.range.start)));
    let mut result = text.to_string();
    for edit in ordered {
        result.replace_range(
            offset(edit.range.start)..offset(edit.range.end),
            &edit.new_text,
        );
    }
    result
}

/// A server over a workspace whose manifest the client was asked for and
/// answered, with `index.rst` open — and the workspace, and what opening
/// published.
fn opened_with(name: &str) -> (ServerState, Workspace, Vec<Message>) {
    let workspace = bazel_workspace(
        name,
        &[
            ("index.rst", INDEX),
            ("guide.rst", "Guide\n=====\n"),
            ("BUILD.bazel", BUILD),
            ("site.lsp_manifest.json", &site_manifest(&[])),
        ],
    );
    let (mut state, messages) = scanned_with(&workspace, ManifestSource::AskClient);
    let request = &manifest_requests(&messages)[0];
    state.on_response(&answer(&workspace, request, &["site.lsp_manifest.json"]));
    let published = handle_notification(&mut state, workspace.open("index.rst", INDEX));
    (state, workspace, published)
}

/// [`opened_with`], without what opening published.
fn opened(name: &str) -> (ServerState, Workspace) {
    let (state, workspace, _) = opened_with(name);
    (state, workspace)
}

/// Whether `messages` publish diagnostics for `file`.
fn publishes(workspace: &Workspace, messages: &[Message], file: &str) -> bool {
    let uri = workspace.uri(file);
    messages.iter().any(|message| {
        matches!(message, Message::Notification(notification)
            if notification.method == "textDocument/publishDiagnostics")
            && published(message).uri == uri
    })
}

#[test]
fn test_the_initialize_result_announces_quick_fixes() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    assert_eq!(
        serde_json::to_value(result.capabilities.code_action_provider).expect("serializes"),
        serde_json::json!({"codeActionKinds": ["quickfix"]})
    );
}

#[test]
fn test_an_entry_outside_deps_offers_adding_the_library_listing_its_document() {
    // Given
    let (mut state, workspace) = opened("code_action_offered");

    // When — the cursor on the entry
    let actions = actions_at(&mut state, &workspace, "index.rst", 5);

    // Then
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(actions[0].title, "Add //:guide to the deps of //:index");
    let changes = actions[0]
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.clone());
    let edits = &changes.expect("changes")[&workspace.uri("BUILD.bazel")];
    assert_eq!(
        apply(BUILD, edits),
        "rinx_library(\n    name = \"index\",\n    srcs = [\"index.rst\"],\n    deps = [\":guide\"],\n)\n\nrinx_library(\n    name = \"guide\",\n    srcs = [\"guide.rst\"],\n)\n"
    );
}

#[test]
fn test_no_quick_fix_is_offered_away_from_the_entry_or_for_a_closed_folder() {
    // Given
    let (mut state, workspace) = opened("code_action_elsewhere");

    // When — on the title, and in a file the server never saw
    let on_title = actions_at(&mut state, &workspace, "index.rst", 0);
    let unknown = actions_at(&mut state, &workspace, "elsewhere.rst", 5);

    // Then
    assert_eq!(on_title, []);
    assert_eq!(unknown, []);
}

#[test]
fn test_the_build_regenerated_from_the_fixed_file_no_longer_fails() {
    // Given the fix applied, and the manifest Bazel writes from it
    let (mut state, workspace, published) = opened_with("code_action_round_trip");
    assert_eq!(
        shown_for(&workspace, &published, "index.rst"),
        [(
            "toctree.undeclared-dependency".to_string(),
            lsp_types::DiagnosticSeverity::ERROR
        )]
    );
    let actions = actions_at(&mut state, &workspace, "index.rst", 5);
    let changes = actions[0]
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.clone());
    workspace.write(
        "BUILD.bazel",
        &apply(
            BUILD,
            &changes.expect("changes")[&workspace.uri("BUILD.bazel")],
        ),
    );
    workspace.write("site.lsp_manifest.json", &site_manifest(&["//:guide"]));

    // When — the extension says so and answers again
    let asked = handle_notification(
        &mut state,
        Notification::new(
            BAZEL_MANIFESTS_CHANGED_METHOD.to_string(),
            serde_json::json!(null),
        ),
    );
    let answered = state.on_response(&answer(
        &workspace,
        &manifest_requests(&asked)[0],
        &["site.lsp_manifest.json"],
    ));

    // Then
    assert!(
        publishes(&workspace, &answered, "index.rst"),
        "{answered:?}"
    );
    assert_eq!(shown_for(&workspace, &answered, "index.rst"), []);
    assert_eq!(actions_at(&mut state, &workspace, "index.rst", 5), []);
}

#[test]
fn test_a_library_the_site_does_not_collect_is_found_in_its_packages_build_file() {
    // Given — no library of the site lists `team/guide.rst`, but the build
    // file of its package does
    let index = "Home\n====\n\n.. toctree::\n\n   team/guide\n";
    let workspace = bazel_workspace(
        "code_action_outside",
        &[
            ("index.rst", index),
            ("team/guide.rst", "Guide\n=====\n"),
            (
                "BUILD.bazel",
                "rinx_library(\n    name = \"index\",\n    srcs = [\"index.rst\"],\n)\n",
            ),
            (
                "team/BUILD.bazel",
                "rinx_library(\n    name = \"docs\",\n    srcs = glob([\"*.rst\"]),\n)\n",
            ),
            (
                "site.lsp_manifest.json",
                &manifest(
                    "//:site",
                    vec![library(
                        "//:index",
                        &["index.rst"],
                        &serde_json::json!({"allowed_docs": ["index"]}),
                    )],
                ),
            ),
        ],
    );
    let (mut state, messages) = scanned_with(&workspace, ManifestSource::AskClient);
    let request = &manifest_requests(&messages)[0];
    state.on_response(&answer(&workspace, request, &["site.lsp_manifest.json"]));
    handle_notification(&mut state, workspace.open("index.rst", index));

    // When
    let actions = actions_at(&mut state, &workspace, "index.rst", 5);

    // Then
    let titles: Vec<&str> = actions.iter().map(|action| action.title.as_str()).collect();
    assert_eq!(titles, ["Add //team:docs to the deps of //:index"]);
}

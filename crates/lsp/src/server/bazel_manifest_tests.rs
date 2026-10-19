//! Bazel projects: who is asked for manifests, what an answer changes about
//! the documents, which site a shared document belongs to, and when the
//! client is asked again. The manifests are written by hand, as
//! `rules/site.bzl` would write them, naming only source files.

use super::bazel_manifests::{
    BAZEL_MANIFESTS_CHANGED_METHOD, BAZEL_MANIFESTS_METHOD, ManifestSource,
};
use super::test_support::*;

/// A schema declaring the `req` entity type.
const SCHEMA: &str = "[[entity_type]]\nname = \"req\"\n";
/// A document using it, which is an unknown directive without the schema.
const REQ_DOC: &str = "Requirements\n============\n\n.. req::\n   :id: R_1\n";

/// A library of the label `label` listing `srcs`, with `extra` fields.
fn library(label: &str, srcs: &[&str], extra: &serde_json::Value) -> serde_json::Value {
    let mut library = serde_json::json!({
        "label": label,
        "srcs": srcs.iter().map(|src| serde_json::json!({"source": src})).collect::<Vec<_>>(),
        "parse_data": [],
        "default_domain": "py",
        "default_role": null,
        "rst_prolog": null,
        "rst_epilog": null,
        "entity_schema": null,
    });
    if let (Some(fields), Some(extra)) = (library.as_object_mut(), extra.as_object()) {
        fields.extend(extra.clone());
    }
    library
}

/// The manifest of the site `label`, collecting `libraries`.
fn manifest(label: &str, libraries: Vec<serde_json::Value>) -> String {
    serde_json::json!({
        "version": 1,
        "site": {"label": label, "config": {"source": "rinx.toml"}, "entity_schema": null},
        "inventories": [],
        "libraries": serde_json::Value::Array(libraries),
    })
    .to_string()
}

/// A Bazel workspace holding `files` besides its `MODULE.bazel` and an
/// empty `rinx.toml`.
fn bazel_workspace(name: &str, files: &[(&str, &str)]) -> Workspace {
    let workspace = Workspace::new(name, files);
    workspace.write("MODULE.bazel", "module(name = \"docs\")\n");
    workspace.write("rinx.toml", "");
    workspace
}

/// A server over `workspace` coming by manifests through `source`, with the
/// scan's result taken — and the messages that brought about.
fn scanned_with(workspace: &Workspace, source: ManifestSource) -> (ServerState, Vec<Message>) {
    let (state, _) = workspace.server(false);
    let mut state = state.with_bazel_manifests(source);
    let messages = state.on_scan_event(workspace.scanned());
    (state, messages)
}

/// The `rinx/bazelManifests` requests among `messages`.
fn manifest_requests(messages: &[Message]) -> Vec<Request> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Request(request) if request.method == BAZEL_MANIFESTS_METHOD => {
                Some(request.clone())
            }
            _ => None,
        })
        .collect()
}

/// The answer to `request` naming `manifests` in `workspace`.
fn answer(workspace: &Workspace, request: &Request, manifests: &[&str]) -> Response {
    let manifests: Vec<String> = manifests
        .iter()
        .map(|manifest| workspace.root.join(manifest).to_string_lossy().into_owned())
        .collect();
    Response::new_ok(
        request.id.clone(),
        serde_json::json!({ "manifests": manifests }),
    )
}

/// The codes `file`, holding `text`, shows when opened in `state`.
fn opened_codes(
    state: &mut ServerState,
    workspace: &Workspace,
    file: &str,
    text: &str,
) -> Vec<String> {
    let opened = handle_notification(state, workspace.open(file, text));
    workspace.codes_for(&opened, file).unwrap_or_default()
}

/// The `window/showMessage` texts among `messages`.
fn shown(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Notification(notification) if notification.method == "window/showMessage" => {
                notification.params["message"].as_str().map(str::to_string)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn test_a_client_passing_the_option_is_asked_about_a_bazel_folder() {
    // Given
    let workspace = bazel_workspace("bazel_asked", &[("index.rst", "Home\n====\n")]);

    // When
    let (_, messages) = scanned_with(&workspace, ManifestSource::AskClient);

    // Then
    let requests = manifest_requests(&messages);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].params,
        serde_json::json!({"folder": workspace.uri("").as_str().trim_end_matches('/')})
    );
}

#[test]
fn test_no_client_is_asked_about_a_folder_without_bazel_or_without_the_option() {
    // Given
    let plain = Workspace::new("bazel_not_bazel", &[("index.rst", "Home\n====\n")]);
    let bazel = bazel_workspace("bazel_no_option", &[("index.rst", "Home\n====\n")]);

    // When
    let (_, plain_messages) = scanned_with(&plain, ManifestSource::AskClient);
    let (_, bazel_messages) = scanned_with(&bazel, ManifestSource::Off);

    // Then
    assert!(manifest_requests(&plain_messages).is_empty());
    assert!(manifest_requests(&bazel_messages).is_empty());
}

#[test]
fn test_an_answer_parses_each_document_with_its_librarys_settings() {
    // Given one library declaring entities and one that does not
    let workspace = bazel_workspace(
        "bazel_libraries",
        &[
            ("reqs/index.rst", REQ_DOC),
            ("reqs/entities.toml", SCHEMA),
            ("other/index.rst", REQ_DOC),
            (
                "site.lsp_manifest.json",
                &manifest(
                    "//:site",
                    vec![
                        library(
                            "//reqs:docs",
                            &["reqs/index.rst"],
                            &serde_json::json!({"entity_schema": {"source": "reqs/entities.toml"}}),
                        ),
                        library("//other:docs", &["other/index.rst"], &serde_json::json!({})),
                    ],
                ),
            ),
        ],
    );
    let (mut state, messages) = scanned_with(&workspace, ManifestSource::AskClient);
    assert_eq!(
        opened_codes(&mut state, &workspace, "reqs/index.rst", REQ_DOC),
        ["directive.unknown"],
        "before the answer, the folder knows no schema"
    );

    // When
    let request = &manifest_requests(&messages)[0];
    state.on_response(&answer(&workspace, request, &["site.lsp_manifest.json"]));

    // Then
    let changed = handle_notification(&mut state, workspace.change("reqs/index.rst", 2, REQ_DOC));
    assert_eq!(
        workspace.codes_for(&changed, "reqs/index.rst"),
        Some(vec![])
    );
    assert_eq!(
        opened_codes(&mut state, &workspace, "other/index.rst", REQ_DOC),
        ["directive.unknown"]
    );
}

#[test]
fn test_a_file_no_manifest_lists_stays_the_folders() {
    // Given
    let workspace = bazel_workspace(
        "bazel_unlisted",
        &[
            ("listed.rst", "Listed\n======\n"),
            ("README.rst", "Readme\n======\n"),
            (
                "site.lsp_manifest.json",
                &manifest(
                    "//:site",
                    vec![library("//:docs", &["listed.rst"], &serde_json::json!({}))],
                ),
            ),
        ],
    );

    // When
    let (state, _) = scanned_with(
        &workspace,
        ManifestSource::Granted(vec![workspace.root.join("site.lsp_manifest.json")]),
    );

    // Then
    let (listed, _) = state
        .locate(&workspace.root.join("listed.rst"))
        .expect("listed");
    let (readme, _) = state
        .locate(&workspace.root.join("README.rst"))
        .expect("the folder's");
    assert_ne!(listed, readme);
    let documents = |id| {
        state.projects[&id]
            .documents()
            .map(|(doc_path, _)| doc_path.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(documents(listed), ["listed.rst"]);
    assert_eq!(documents(readme), ["README.rst"]);
}

#[test]
fn test_a_document_two_sites_list_belongs_to_the_preferred_site_else_the_first() {
    // Given two sites listing `shared.rst`, `//b` preferred
    let workspace = bazel_workspace(
        "bazel_shared",
        &[
            ("shared.rst", "Shared\n======\n"),
            (
                "a.lsp_manifest.json",
                &manifest(
                    "//a:site",
                    vec![library("//a:docs", &["shared.rst"], &serde_json::json!({}))],
                ),
            ),
            (
                "b.lsp_manifest.json",
                &manifest(
                    "//b:site",
                    vec![library("//b:docs", &["shared.rst"], &serde_json::json!({}))],
                ),
            ),
        ],
    );
    let (mut state, messages) = scanned_with(&workspace, ManifestSource::AskClient);
    let site_of = |state: &ServerState| {
        let (id, _) = state
            .locate(&workspace.root.join("shared.rst"))
            .expect("a project");
        match &state.projects[&id].model().source {
            crate::project::ProjectSource::Bazel { site, .. } => site.clone(),
            other => panic!("{other:?}"),
        }
    };

    // When — preferring //b
    let request = &manifest_requests(&messages)[0];
    let mut preferred = answer(
        &workspace,
        request,
        &["a.lsp_manifest.json", "b.lsp_manifest.json"],
    );
    if let Ok(result) = preferred.response_result.as_mut() {
        result["preferredSite"] = serde_json::json!("//b:site");
    }
    state.on_response(&preferred);

    // Then
    assert_eq!(site_of(&state), "//b:site");

    // When — preferring none
    let asked = state.on_bazel_manifests_changed();
    let request = &manifest_requests(&asked)[0];
    state.on_response(&answer(
        &workspace,
        request,
        &["b.lsp_manifest.json", "a.lsp_manifest.json"],
    ));

    // Then the first by label
    assert_eq!(site_of(&state), "//a:site");
}

#[test]
fn test_an_include_the_library_does_not_declare_is_refused_as_in_the_build() {
    // Given `part.rst` declared by one library and not by the other
    let text = "Title\n=====\n\n.. include:: part.rst\n";
    let workspace = bazel_workspace(
        "bazel_parse_data",
        &[
            ("a/index.rst", text),
            ("a/part.rst", "Part.\n"),
            ("b/index.rst", text),
            ("b/part.rst", "Part.\n"),
            (
                "site.lsp_manifest.json",
                &manifest(
                    "//:site",
                    vec![
                        library(
                            "//a:docs",
                            &["a/index.rst"],
                            &serde_json::json!({"parse_data": ["a/part.rst"]}),
                        ),
                        library("//b:docs", &["b/index.rst"], &serde_json::json!({})),
                    ],
                ),
            ),
        ],
    );
    let (mut state, _) = scanned_with(
        &workspace,
        ManifestSource::Granted(vec![workspace.root.join("site.lsp_manifest.json")]),
    );

    // When / Then
    assert_eq!(
        opened_codes(&mut state, &workspace, "a/index.rst", text),
        Vec::<String>::new()
    );
    assert_eq!(
        opened_codes(&mut state, &workspace, "b/index.rst", text),
        ["include.file-unreadable"]
    );
}

#[test]
fn test_a_prolog_mistake_is_shown_in_the_prolog() {
    // Given
    let workspace = bazel_workspace(
        "bazel_prolog",
        &[
            ("index.rst", "Home\n====\n"),
            ("prolog.rst", "\n.. nonsense::\n"),
            (
                "site.lsp_manifest.json",
                &manifest(
                    "//:site",
                    vec![library(
                        "//:docs",
                        &["index.rst"],
                        &serde_json::json!({"rst_prolog": {"source": "prolog.rst"}}),
                    )],
                ),
            ),
        ],
    );
    let (mut state, _) = scanned_with(
        &workspace,
        ManifestSource::Granted(vec![workspace.root.join("site.lsp_manifest.json")]),
    );

    // When
    let opened = handle_notification(&mut state, workspace.open("index.rst", "Home\n====\n"));

    // Then
    assert_eq!(workspace.codes_for(&opened, "index.rst"), Some(vec![]));
    assert_eq!(
        workspace.codes_for(&opened, "prolog.rst"),
        Some(vec!["directive.unknown".to_string()])
    );
}

#[test]
fn test_the_changed_notification_asks_again_and_keeps_the_projects_meanwhile() {
    // Given an answered folder
    let workspace = bazel_workspace(
        "bazel_changed",
        &[
            ("index.rst", "Home\n====\n"),
            (
                "site.lsp_manifest.json",
                &manifest(
                    "//:site",
                    vec![library("//:docs", &["index.rst"], &serde_json::json!({}))],
                ),
            ),
        ],
    );
    let (mut state, messages) = scanned_with(&workspace, ManifestSource::AskClient);
    let request = &manifest_requests(&messages)[0];
    state.on_response(&answer(&workspace, request, &["site.lsp_manifest.json"]));
    let bazel_projects = |state: &ServerState| {
        state
            .projects
            .values()
            .filter(|project| {
                matches!(
                    project.model().source,
                    crate::project::ProjectSource::Bazel { .. }
                )
            })
            .count()
    };

    // When
    let asked = handle_notification(
        &mut state,
        Notification::new(
            BAZEL_MANIFESTS_CHANGED_METHOD.to_string(),
            serde_json::json!(null),
        ),
    );

    // Then
    assert_eq!(manifest_requests(&asked).len(), 1);
    assert_eq!(bazel_projects(&state), 1);
}

#[test]
fn test_the_status_names_each_site() {
    // Given
    let workspace = bazel_workspace(
        "bazel_status",
        &[
            ("index.rst", "Home\n====\n"),
            (
                "site.lsp_manifest.json",
                &manifest(
                    "//docs:site",
                    vec![library(
                        "//docs:docs",
                        &["index.rst"],
                        &serde_json::json!({}),
                    )],
                ),
            ),
        ],
    );

    // When
    let (_, messages) = scanned_with(
        &workspace,
        ManifestSource::Granted(vec![workspace.root.join("site.lsp_manifest.json")]),
    );

    // Then
    let last = statuses(&messages).pop().expect("a status");
    assert_eq!(
        last["projects"],
        serde_json::json!([{"kind": "bazel", "site": "//docs:site"}])
    );
}

#[test]
fn test_an_unreadable_manifest_is_shown_once_and_the_folder_stays_a_folder() {
    // Given
    let workspace = bazel_workspace("bazel_unreadable", &[("index.rst", "Home\n====\n")]);
    let (mut state, messages) = scanned_with(&workspace, ManifestSource::AskClient);
    let request = &manifest_requests(&messages)[0];

    // When
    let replies = state
        .on_bazel_manifests_response(&answer(&workspace, request, &["missing.lsp_manifest.json"]))
        .expect("an answer to the request");
    let again = state.reload_folder(0);

    // Then
    let shown = shown(&replies);
    assert_eq!(shown.len(), 1);
    assert!(shown[0].contains("missing.lsp_manifest.json"), "{shown:?}");
    assert!(self::shown(&again).is_empty());
    assert!(state.locate(&workspace.root.join("index.rst")).is_some());
}

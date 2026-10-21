//! The preview: what a `rinx/preview` request is answered with, from the
//! open document's latest parse and its project's current index.

use super::bazel_manifest_tests::{
    answer, bazel_workspace, library, manifest_requests, scanned_with,
};
use super::bazel_manifests::ManifestSource;
use super::test_support::*;
use crate::preview::{PREVIEW_METHOD, PreviewParams, PreviewResult};

/// A root document whose toctree names `setup`.
const INDEX: &str = "Home\n====\n\n.. toctree::\n\n   setup\n";
const SETUP: &str = "Installing\n==========\n";

fn preview(uri: Uri) -> Request {
    Request::new(
        RequestId::from(9),
        PREVIEW_METHOD.to_string(),
        PreviewParams {
            text_document: TextDocumentIdentifier::new(uri),
            resource_root: None,
        },
    )
}

/// The page `response` carries.
fn page_of(response: &Response) -> String {
    let result = match &response.response_result {
        Ok(result) => result.clone(),
        Err(error) => panic!("a result, not {error:?}"),
    };
    serde_json::from_value::<PreviewResult>(result)
        .expect("a preview")
        .html
}

/// The error code and message `response` carries.
fn error_of(response: &Response) -> (i32, String) {
    let error = response.response_result.clone().expect_err("an error");
    (error.code, error.message)
}

#[test]
fn test_preview_draws_the_open_documents_page() {
    // Given
    let (workspace, mut state) = scanned(
        "preview_page",
        &[("index.rst", INDEX), ("setup.rst", SETUP)],
    );
    handle_notification(&mut state, workspace.open("index.rst", INDEX));

    // When
    let response = handle_request(&mut state, preview(workspace.uri("index.rst")));

    // Then — the default template, with the sidebar naming `setup`'s title
    let page = page_of(&response);
    assert!(page.starts_with("<!DOCTYPE html>"), "{page}");
    assert!(page.contains("<title>Home"), "{page}");
    assert!(page.contains("Installing"), "{page}");
}

#[test]
fn test_preview_shows_a_title_changed_a_moment_ago_in_another_document() {
    // Given
    let (workspace, mut state) = scanned(
        "preview_fresh",
        &[("index.rst", INDEX), ("setup.rst", SETUP)],
    );
    handle_notification(&mut state, workspace.open("index.rst", INDEX));
    handle_notification(&mut state, workspace.open("setup.rst", SETUP));

    // When — the other document's title changes in its unsaved buffer
    handle_notification(
        &mut state,
        workspace.change("setup.rst", 2, "Getting started\n===============\n"),
    );
    let response = handle_request(&mut state, preview(workspace.uri("index.rst")));

    // Then
    let page = page_of(&response);
    assert!(page.contains("Getting started"), "{page}");
    assert!(!page.contains("Installing"), "{page}");
}

#[test]
fn test_preview_shows_the_buffer_not_the_file() {
    // Given
    let (workspace, mut state) = scanned("preview_buffer", &[("index.rst", INDEX)]);
    handle_notification(&mut state, workspace.open("index.rst", INDEX));

    // When
    handle_notification(
        &mut state,
        workspace.change("index.rst", 2, "Edited\n======\n\nNew words.\n"),
    );
    let response = handle_request(&mut state, preview(workspace.uri("index.rst")));

    // Then
    assert!(page_of(&response).contains("New words."));
}

#[test]
fn test_preview_draws_a_document_in_no_project_on_its_own() {
    // Given — a server with no workspace folder
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open("Alone\n=====\n\nWords.\n"));

    // When
    let response = handle_request(&mut state, preview(uri()));

    // Then
    let page = page_of(&response);
    assert!(page.contains("<title>Alone"), "{page}");
    assert!(page.contains("Words."), "{page}");
}

#[test]
fn test_preview_links_an_image_where_it_lies_under_the_resource_root() {
    // Given — a Sphinx project under `docs/`, whose page one directory down
    // shows an image beside it
    let page = "Intro\n=====\n\n.. image:: logo.png\n";
    let (workspace, mut state) = scanned(
        "preview_image",
        &[
            ("docs/conf.py", "project = 'Images'\n"),
            ("docs/index.rst", INDEX),
            ("docs/guide/intro.rst", page),
        ],
    );
    handle_notification(&mut state, workspace.open("docs/guide/intro.rst", page));
    let mut request = preview(workspace.uri("docs/guide/intro.rst"));
    request.params["resourceRoot"] = serde_json::json!("https://webview/");

    // When
    let response = handle_request(&mut state, request);

    // Then — the file in the source tree, not the site's `_images/` copy
    let image = workspace.uri("docs/guide/logo.png");
    let src = format!("src=\"https://webview{}\"", image.path().as_str());
    let page = page_of(&response);
    assert!(page.contains(&src), "{page}");
}

#[test]
fn test_preview_links_an_image_into_the_site_without_a_resource_root() {
    // Given
    let page = "Intro\n=====\n\n.. image:: logo.png\n";
    let (workspace, mut state) = scanned("preview_image_site", &[("guide/intro.rst", page)]);
    handle_notification(&mut state, workspace.open("guide/intro.rst", page));

    // When
    let response = handle_request(&mut state, preview(workspace.uri("guide/intro.rst")));

    // Then — the page the build writes
    let page = page_of(&response);
    assert!(page.contains("src=\"../_images/guide/logo.png\""), "{page}");
}

#[test]
fn test_preview_refuses_a_document_that_is_not_open() {
    // Given
    let (workspace, mut state) = scanned("preview_closed", &[("index.rst", INDEX)]);

    // When
    let response = handle_request(&mut state, preview(workspace.uri("index.rst")));

    // Then
    assert_eq!(
        error_of(&response),
        (
            lsp_server::ErrorCode::InvalidParams as i32,
            "the document is not open".to_string()
        )
    );
}

#[test]
fn test_preview_refuses_parameters_that_do_not_parse() {
    // Given
    let (_, mut state) = scanned("preview_invalid", &[("index.rst", INDEX)]);
    let request = Request::new(
        RequestId::from(9),
        PREVIEW_METHOD.to_string(),
        serde_json::json!({}),
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    assert_eq!(
        error_of(&response).0,
        lsp_server::ErrorCode::InvalidParams as i32
    );
}

/// A Bazel workspace whose site `//:site` lists `index.rst` and names
/// `page.html` as its template, with the manifests granted.
fn bazel_site(name: &str, template: Option<&str>) -> (Workspace, ServerState) {
    let manifest = serde_json::json!({
        "version": 1,
        "site": {
            "label": "//:site",
            "config": {"source": "rinx.toml"},
            "entity_schema": null,
            "template": {"source": "page.html"},
            "css": {"source": "site.css"},
        },
        "inventories": [],
        "libraries": [library("//:docs", &["index.rst"], &serde_json::json!({}))],
    })
    .to_string();
    let mut files = vec![
        ("index.rst", INDEX),
        ("site.lsp_manifest.json", manifest.as_str()),
    ];
    if let Some(template) = template {
        files.push(("page.html", template));
    }
    let workspace = bazel_workspace(name, &files);
    let (mut state, messages) = scanned_with(&workspace, ManifestSource::AskClient);
    let request = &manifest_requests(&messages)[0];
    state.on_response(&answer(&workspace, request, &["site.lsp_manifest.json"]));
    handle_notification(&mut state, workspace.open("index.rst", INDEX));
    (workspace, state)
}

#[test]
fn test_preview_draws_a_bazel_site_with_its_own_template() {
    // Given
    let (workspace, mut state) = bazel_site(
        "preview_bazel",
        Some("<h1>{{ page_title }}</h1>|{{ css_path }}"),
    );
    let mut request = preview(workspace.uri("index.rst"));
    request.params["resourceRoot"] = serde_json::json!("https://webview/");

    // When
    let response = handle_request(&mut state, request);

    // Then — the stylesheet linked under the root the client serves files at
    let stylesheet = workspace.uri("site.css");
    let stylesheet_path = stylesheet.path().as_str();
    assert_eq!(
        page_of(&response),
        format!("<h1>Home</h1>|https://webview{stylesheet_path}")
    );
}

#[test]
fn test_preview_names_a_template_it_cannot_read() {
    // Given
    let (workspace, mut state) = bazel_site("preview_bazel_gone", None);

    // When
    let response = handle_request(&mut state, preview(workspace.uri("index.rst")));

    // Then
    let (code, message) = error_of(&response);
    assert_eq!(code, lsp_server::ErrorCode::RequestFailed as i32);
    assert!(message.starts_with("cannot read `"), "{message}");
    assert!(message.contains("page.html"), "{message}");
}

//! Following the preview's links: where a `rinx/previewLink` request says
//! an `href` of an open document's page leads.

use super::test_support::*;
use crate::preview::{PREVIEW_METHOD, PreviewParams, PreviewResult};
use crate::preview_link::{
    PREVIEW_LINK_METHOD, PreviewLinkKind, PreviewLinkParams, PreviewLinkResult,
};

/// A root document whose toctree names `guide/setup`.
const INDEX: &str = "Home\n====\n\n.. toctree::\n\n   guide/setup\n";
/// A document defining the label `install`, with a file to download.
const SETUP: &str =
    ".. _install:\n\nInstalling\n==========\n\nGet :download:`the data <data.csv>`.\n";

fn link(uri: Uri, href: &str) -> Request {
    Request::new(
        RequestId::from(7),
        PREVIEW_LINK_METHOD.to_string(),
        PreviewLinkParams {
            text_document: TextDocumentIdentifier::new(uri),
            href: href.to_string(),
        },
    )
}

/// Where `response` says the link leads.
fn target_of(response: &Response) -> Option<PreviewLinkResult> {
    let result = match &response.response_result {
        Ok(result) => result.clone(),
        Err(error) => panic!("a result, not {error:?}"),
    };
    serde_json::from_value(result).expect("a link target")
}

/// The scanned workspace of `INDEX` and `SETUP`, with `file` open.
fn opened(name: &str, file: &str) -> (Workspace, ServerState) {
    let (workspace, mut state) = scanned(
        name,
        &[
            ("index.rst", INDEX),
            ("guide/setup.rst", SETUP),
            ("guide/data.csv", "a,b\n"),
        ],
    );
    let text = if file == "index.rst" { INDEX } else { SETUP };
    handle_notification(&mut state, workspace.open(file, text));
    (workspace, state)
}

#[test]
fn test_preview_link_leads_to_another_documents_source() {
    // Given
    let (workspace, mut state) = opened("link_document", "index.rst");

    // When
    let response = handle_request(
        &mut state,
        link(workspace.uri("index.rst"), "guide/setup.html"),
    );

    // Then
    assert_eq!(
        target_of(&response),
        Some(PreviewLinkResult {
            kind: PreviewLinkKind::Document,
            uri: workspace.uri("guide/setup.rst"),
            fragment: None,
        })
    );
}

#[test]
fn test_preview_link_keeps_the_fragment_a_label_leads_to() {
    // Given
    let (workspace, mut state) = opened("link_fragment", "guide/setup.rst");

    // When
    let response = handle_request(
        &mut state,
        link(workspace.uri("guide/setup.rst"), "../index.html#home"),
    );

    // Then
    assert_eq!(
        target_of(&response),
        Some(PreviewLinkResult {
            kind: PreviewLinkKind::Document,
            uri: workspace.uri("index.rst"),
            fragment: Some("home".to_string()),
        })
    );
}

#[test]
fn test_preview_link_leads_nowhere_from_a_page_the_build_writes_alone() {
    // Given
    let (workspace, mut state) = opened("link_genindex", "guide/setup.rst");

    // When
    let response = handle_request(
        &mut state,
        link(workspace.uri("guide/setup.rst"), "../genindex.html"),
    );

    // Then
    assert_eq!(target_of(&response), None);
}

#[test]
fn test_preview_link_leads_to_a_downloaded_file() {
    // Given
    let (workspace, mut state) = opened("link_download", "guide/setup.rst");

    // When
    let response = handle_request(
        &mut state,
        link(
            workspace.uri("guide/setup.rst"),
            "../_downloads/guide/data.csv",
        ),
    );

    // Then
    assert_eq!(
        target_of(&response),
        Some(PreviewLinkResult {
            kind: PreviewLinkKind::File,
            uri: workspace.uri("guide/data.csv"),
            fragment: None,
        })
    );
}

#[test]
fn test_preview_link_leads_nowhere_to_a_download_that_is_missing() {
    // Given
    let (workspace, mut state) = opened("link_download_gone", "guide/setup.rst");

    // When
    let response = handle_request(
        &mut state,
        link(
            workspace.uri("guide/setup.rst"),
            "../_downloads/guide/gone.csv",
        ),
    );

    // Then
    assert_eq!(target_of(&response), None);
}

#[test]
fn test_preview_link_names_a_documents_file_whatever_its_suffix() {
    // Given — a Sphinx project whose sources end in `.txt`
    let text = "Usage\n=====\n";
    let (workspace, mut state) = scanned(
        "link_suffix",
        &[
            ("conf.py", "source_suffix = '.txt'\n"),
            ("index.txt", INDEX),
            ("usage.txt", text),
        ],
    );
    handle_notification(&mut state, workspace.open("index.txt", INDEX));

    // When
    let response = handle_request(&mut state, link(workspace.uri("index.txt"), "usage.html"));

    // Then
    let target = target_of(&response).expect("a target");
    assert_eq!(target.uri, workspace.uri("usage.txt"));
}

#[test]
fn test_preview_link_in_no_project_leads_only_to_the_document_itself() {
    // Given — a server with no workspace folder
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open("Alone\n=====\n"));

    // When
    let own = handle_request(&mut state, link(uri(), "#alone"));
    let other = handle_request(&mut state, link(uri(), "other.html"));

    // Then
    assert_eq!(
        target_of(&own),
        Some(PreviewLinkResult {
            kind: PreviewLinkKind::Document,
            uri: uri(),
            fragment: Some("alone".to_string()),
        })
    );
    assert_eq!(target_of(&other), None);
}

#[test]
fn test_preview_link_refuses_a_document_that_is_not_open() {
    // Given
    let (workspace, mut state) = scanned("link_closed", &[("index.rst", INDEX)]);

    // When
    let response = handle_request(&mut state, link(workspace.uri("index.rst"), "x.html"));

    // Then
    let error = response.response_result.expect_err("an error");
    assert_eq!(
        (error.code, error.message.as_str()),
        (ErrorCode::InvalidParams as i32, "the document is not open")
    );
}

#[test]
fn test_preview_link_refuses_parameters_that_do_not_parse() {
    // Given
    let (_, mut state) = scanned("link_invalid", &[("index.rst", INDEX)]);
    let request = Request::new(
        RequestId::from(7),
        PREVIEW_LINK_METHOD.to_string(),
        serde_json::json!({}),
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    let error = response.response_result.expect_err("an error");
    assert_eq!(error.code, ErrorCode::InvalidParams as i32);
}

/// Every `href` the HTML `page` writes.
fn hrefs(page: &str) -> Vec<String> {
    page.split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split_once('"'))
        .map(|(href, _)| href.replace("&amp;", "&"))
        .collect()
}

#[test]
fn test_preview_link_follows_every_link_the_page_writes_to_a_source() {
    // Given — the preview of a page linking a document, a label, a
    // download, and the pages its sidebar names
    let page = "Usage\n=====\n\nSee :doc:`setup`, :ref:`install` and :download:`data.csv`.\n";
    let (workspace, mut state) = scanned(
        "link_round_trip",
        &[
            (
                "index.rst",
                "Home\n====\n\n.. toctree::\n\n   guide/usage\n   guide/setup\n",
            ),
            ("guide/usage.rst", page),
            ("guide/setup.rst", SETUP),
            ("guide/data.csv", "a,b\n"),
        ],
    );
    handle_notification(&mut state, workspace.open("guide/usage.rst", page));
    let rendered = handle_request(
        &mut state,
        Request::new(
            RequestId::from(9),
            PREVIEW_METHOD.to_string(),
            PreviewParams {
                text_document: TextDocumentIdentifier::new(workspace.uri("guide/usage.rst")),
                resource_root: None,
                source_lines: false,
            },
        ),
    );
    let page_html =
        serde_json::from_value::<PreviewResult>(rendered.response_result.expect("a page"))
            .expect("a preview")
            .html;

    // When
    let unfollowed: Vec<String> = hrefs(&page_html)
        .into_iter()
        .filter(|href| !href.starts_with("data:"))
        .filter(|href| {
            let response = handle_request(&mut state, link(workspace.uri("guide/usage.rst"), href));
            target_of(&response).is_none()
        })
        .collect();

    // Then
    assert_eq!(unfollowed, Vec::<String>::new(), "{page_html}");
    assert!(page_html.contains("setup.html#install"), "{page_html}");
    assert!(
        page_html.contains("_downloads/guide/data.csv"),
        "{page_html}"
    );
}

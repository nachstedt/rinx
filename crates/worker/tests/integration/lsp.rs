//! The language server as an editor runs it: the `rinx lsp` process, spoken to
//! over its standard input and output with the protocol's own framing.
//!
//! The handlers are tested in `rinx_lsp` without a process; what only these
//! tests see is the wiring around them — the command line, the stdio threads,
//! and the exit code the protocol asks for.

use std::process::{Command, Stdio};

use lsp_server::{ErrorCode, Message};
use serde_json::{Value, json};

use crate::lsp_client::Server;

const URI: &str = "file:///docs/index.rst";

fn did_open(text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": URI,
            "languageId": "restructuredtext",
            "version": 1,
            "text": text,
        }
    })
}

fn did_change(text: &str) -> Value {
    json!({
        "textDocument": { "uri": URI, "version": 2 },
        "contentChanges": [{ "text": text }],
    })
}

fn codes(published: &Value) -> Vec<&str> {
    published["diagnostics"]
        .as_array()
        .expect("a diagnostics array")
        .iter()
        .filter_map(|diagnostic| diagnostic["code"].as_str())
        .collect()
}

#[test]
fn test_lsp_serves_a_session_over_stdio() {
    // Given
    let mut server = Server::spawn();
    let initialized = server.initialize(&json!({}));

    // When — a broken document is opened, then fixed
    server.notify("textDocument/didOpen", did_open(".. foo::\n"));
    let opened = server.published();
    server.notify("textDocument/didChange", did_change("Fixed.\n"));
    let changed = server.published();

    // Then
    let result = initialized.response_result.expect("initialize succeeds");
    assert_eq!(result["serverInfo"]["name"], "rinx");
    assert_eq!(opened["uri"], URI);
    assert_eq!(codes(&opened), vec!["directive.unknown"]);
    assert_eq!(changed["diagnostics"], json!([]));
}

#[test]
fn test_lsp_underlines_a_mistake_in_an_included_file_in_that_file() {
    // Given a document including a fragment whose third line is broken
    let dir = std::env::temp_dir().join(format!("rinx_lsp_stdio_include_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    std::fs::write(dir.join("part.rst"), "Fine.\n\n.. foo::\n").expect("the fragment");
    let index = format!("file://{}", dir.join("index.rst").display());
    let part = format!("file://{}", dir.join("part.rst").display());
    let mut server = Server::spawn();
    server.initialize(&json!({}));

    // When — the includer is opened, then the fragment, fixed in its buffer
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": index, "languageId": "restructuredtext", "version": 1,
            "text": ".. include:: part.rst\n",
        }}),
    );
    let opened = server.published_until(&index);
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": part, "languageId": "restructuredtext", "version": 1,
            "text": "Fine.\n",
        }}),
    );
    let fixed = server.published_until(&part);
    let _ = std::fs::remove_dir_all(&dir);

    // Then — on the fragment's own line, summarized on the include and
    // linked from there; then cleared in both places
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert_eq!(opened[0]["uri"], part.as_str());
    assert_eq!(codes(&opened[0]), vec!["directive.unknown"]);
    assert_eq!(opened[0]["diagnostics"][0]["range"]["start"]["line"], 2);
    assert_eq!(opened[1]["uri"], index.as_str());
    let summary = &opened[1]["diagnostics"][0];
    assert_eq!(summary["range"]["start"]["line"], 0);
    assert_eq!(summary["severity"], 3, "information: {summary}");
    assert_eq!(
        summary["relatedInformation"][0]["location"]["uri"],
        part.as_str()
    );
    assert_eq!(fixed.len(), 2, "{fixed:?}");
    assert_eq!(fixed[0]["uri"], index.as_str());
    assert_eq!(fixed[0]["diagnostics"], json!([]));
    assert_eq!(fixed[1]["diagnostics"], json!([]));
}

#[test]
fn test_lsp_indexes_its_workspace_folder_and_reports_it_ready() {
    // Given — a workspace folder with three documents, one of them hidden
    // away where the scan must not look.
    let root = std::env::temp_dir().join("rinx_lsp_stdio_workspace");
    let _ = std::fs::remove_dir_all(&root);
    for (file, text) in [
        ("index.rst", "Home\n====\n"),
        ("guide/setup.rst", "Setup\n=====\n"),
        ("guide/usage.rst", "Usage\n=====\n"),
        (".venv/lib/readme.rst", "Not a document\n"),
    ] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
    let mut server = Server::spawn();

    // When
    server.initialize_with(json!({
        "capabilities": {},
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));
    let statuses: Vec<Value> = std::iter::from_fn(|| match server.receive() {
        lsp_server::Message::Notification(notification) if notification.method == "rinx/status" => {
            Some(notification.params)
        }
        other => panic!("expected rinx/status, got {other:?}"),
    })
    .take(2)
    .collect();

    // Then
    assert_eq!(statuses[0], json!({"state": "indexing", "documents": 0}));
    assert_eq!(statuses[1]["state"], "ready");
    assert_eq!(statuses[1]["documents"], 3);
    assert!(statuses[1]["elapsedMs"].is_u64(), "{statuses:?}");
}

#[test]
fn test_lsp_completes_a_label_defined_in_another_document() {
    // Given — a scanned folder whose `setup.rst` labels a section
    let root = std::env::temp_dir().join("rinx_lsp_stdio_completion");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("setup.rst"),
        ".. _install:\n\nInstalling\n==========\n",
    )
    .expect("write");
    let notes = format!("file://{}", root.join("notes.rst").display());
    let mut server = Server::spawn();
    let initialized = server.initialize_with(json!({
        "capabilities": {},
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));
    while !matches!(
        server.receive(),
        Message::Notification(notification)
            if notification.method == "rinx/status" && notification.params["state"] == "ready"
    ) {}
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": notes, "languageId": "restructuredtext", "version": 1,
            "text": "See :ref:`\n",
        }}),
    );
    server.published_until(&notes);

    // When
    server.request(
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": notes },
            "position": { "line": 0, "character": 10 },
        }),
    );
    let Message::Response(completed) = server.receive() else {
        panic!("expected the completion");
    };

    // Then
    let capabilities = &initialized.response_result.expect("initialized")["capabilities"];
    assert_eq!(
        capabilities["completionProvider"]["triggerCharacters"],
        json!(["`", "<", "/"])
    );
    let list = completed.response_result.expect("a completion");
    assert_eq!(list["isIncomplete"], false);
    assert_eq!(list["items"][0]["label"], "install");
    assert_eq!(list["items"][0]["detail"], "Installing");
}

#[test]
fn test_lsp_exits_with_success_after_shutdown_and_exit() {
    // Given
    let mut server = Server::spawn();
    server.initialize(&json!({}));

    // When
    server.request(2, "shutdown", Value::Null);
    let shutdown = server.receive();
    server.notify("exit", Value::Null);
    let status = server.wait();

    // Then
    assert!(
        matches!(&shutdown, Message::Response(response) if response.response_result.is_ok()),
        "{shutdown:?}"
    );
    assert!(status.success(), "{status:?}");
}

#[test]
fn test_lsp_exits_with_failure_on_exit_without_shutdown() {
    // Given
    let mut server = Server::spawn();
    server.initialize(&json!({}));

    // When
    server.notify("exit", Value::Null);
    let status = server.wait();

    // Then — the protocol asks for exit code 1
    assert_eq!(status.code(), Some(1));
}

#[test]
fn test_lsp_exits_with_failure_when_its_input_closes_without_shutdown() {
    // Given
    let mut server = Server::spawn();
    server.initialize(&json!({}));

    // When — the editor crashed
    let status = server.wait();

    // Then
    assert_eq!(status.code(), Some(1));
}

#[test]
fn test_lsp_refuses_a_request_before_initialize() {
    // Given
    let mut server = Server::spawn();

    // When
    server.request(7, "textDocument/hover", Value::Null);
    let early = server.receive();
    let initialized = server.initialize(&json!({}));

    // Then
    let Message::Response(early) = early else {
        panic!("expected a response, got {early:?}");
    };
    assert_eq!(
        early.response_result.err().map(|error| error.code),
        Some(ErrorCode::ServerNotInitialized as i32)
    );
    assert!(initialized.response_result.is_ok(), "{initialized:?}");
}

#[test]
fn test_lsp_refuses_an_unknown_argument() {
    // Given / When
    let output = Command::new(env!("CARGO_BIN_EXE_rinx"))
        .args(["lsp", "--socket=9000"])
        .stdin(Stdio::null())
        .output()
        .expect("rinx runs");

    // Then
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown argument '--socket=9000'"),
        "{stderr}"
    );
}

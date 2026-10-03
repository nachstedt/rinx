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

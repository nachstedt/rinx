//! The language server as an editor runs it: the `rinx lsp` process, spoken to
//! over its standard input and output with the protocol's own framing.
//!
//! The handlers are tested in `rinx_lsp` without a process; what only these
//! tests see is the wiring around them — the command line, the stdio threads,
//! and the exit code the protocol asks for.

use std::io::BufReader;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use serde_json::{Value, json};

/// How long any one reply may take before the test fails rather than hangs.
const TIMEOUT: Duration = Duration::from_secs(20);

const URI: &str = "file:///docs/index.rst";

/// A running `rinx lsp`, killed when dropped so a failing test leaves no
/// process behind.
struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Message>,
}

impl Server {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rinx"))
            .args(["lsp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("rinx lsp starts");
        let stdin = child.stdin.take();
        let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        // A reader thread, so that a server which never answers times out
        // instead of blocking the test on a read.
        let (sender, messages) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(Some(message)) = Message::read(&mut stdout) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            messages,
        }
    }

    fn send(&mut self, message: impl Into<Message>) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        message.into().write(stdin).expect("message written");
    }

    fn receive(&self) -> Message {
        self.messages
            .recv_timeout(TIMEOUT)
            .expect("the server answers in time")
    }

    fn request(&mut self, id: i32, method: &str, params: Value) {
        self.send(Request::new(
            RequestId::from(id),
            method.to_string(),
            params,
        ));
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(Notification::new(method.to_string(), params));
    }

    /// Performs the `initialize` handshake, returning the server's answer.
    fn initialize(&mut self) -> Response {
        self.request(1, "initialize", json!({ "capabilities": {} }));
        let Message::Response(response) = self.receive() else {
            panic!("expected the initialize response");
        };
        self.notify("initialized", json!({}));
        response
    }

    /// The parameters of the next message, which must be a
    /// `publishDiagnostics`.
    fn published(&self) -> Value {
        match self.receive() {
            Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics" =>
            {
                notification.params
            }
            other => panic!("expected publishDiagnostics, got {other:?}"),
        }
    }

    /// Closes the server's input and waits for it to exit.
    fn wait(&mut self) -> ExitStatus {
        self.stdin = None;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("process status") {
                return status;
            }
            assert!(Instant::now() < deadline, "rinx lsp did not exit");
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

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
    let initialized = server.initialize();

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
    server.initialize();

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
    server.initialize();

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
    server.initialize();

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
    let initialized = server.initialize();

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

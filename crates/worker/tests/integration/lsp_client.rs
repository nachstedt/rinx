//! A client for the `rinx lsp` process, shared by the tests that speak to it:
//! it spawns the server, frames messages with lsp-server's own `Message`, and
//! bounds every wait so a hung server fails a test rather than stalling it.

use std::io::BufReader;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use lsp_server::{Message, Notification, Request, RequestId, Response};
use serde_json::{Value, json};

/// How long any one reply may take before the test fails rather than hangs.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

/// A running `rinx lsp`, killed when dropped so a failing test leaves no
/// process behind.
pub(crate) struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Message>,
}

impl Server {
    pub(crate) fn spawn() -> Self {
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

    pub(crate) fn send(&mut self, message: impl Into<Message>) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        message.into().write(stdin).expect("message written");
    }

    pub(crate) fn receive(&self) -> Message {
        self.messages
            .recv_timeout(TIMEOUT)
            .expect("the server answers in time")
    }

    pub(crate) fn request(&mut self, id: i32, method: &str, params: Value) {
        self.send(Request::new(
            RequestId::from(id),
            method.to_string(),
            params,
        ));
    }

    pub(crate) fn notify(&mut self, method: &str, params: Value) {
        self.send(Notification::new(method.to_string(), params));
    }

    /// Performs the `initialize` handshake for a client announcing
    /// `capabilities`, returning the server's answer.
    pub(crate) fn initialize(&mut self, capabilities: &Value) -> Response {
        self.request(1, "initialize", json!({ "capabilities": capabilities }));
        let Message::Response(response) = self.receive() else {
            panic!("expected the initialize response");
        };
        self.notify("initialized", json!({}));
        response
    }

    /// The parameters of the next message, which must be a
    /// `publishDiagnostics`.
    pub(crate) fn published(&self) -> Value {
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
    pub(crate) fn wait(&mut self) -> ExitStatus {
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

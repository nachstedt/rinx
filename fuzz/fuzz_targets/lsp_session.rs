//! A whole language-server session through `rinx_lsp::run`: the handshake,
//! any sequence of document edits, unsupported requests, unknown
//! notifications and stray responses, then a clean shutdown. The server must
//! neither panic nor fail, and must still shut down cleanly at the end.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use serde_json::{Value, json};

const URIS: [&str; 2] = ["file:///docs/index.rst", "untitled:Untitled-1"];

#[derive(Arbitrary, Debug)]
enum Step {
    Open {
        document: bool,
        text: String,
    },
    Change {
        document: bool,
        version: i32,
        text: String,
    },
    Close {
        document: bool,
    },
    Request {
        method: String,
        params: String,
    },
    Notify {
        method: String,
        params: String,
    },
    Respond {
        id: i32,
    },
}

#[derive(Arbitrary, Debug)]
struct Session {
    utf32: bool,
    steps: Vec<Step>,
}

/// `text` as JSON when it is some, else as a JSON string.
fn json_or_string(text: String) -> Value {
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

fn uri(document: bool) -> &'static str {
    URIS[usize::from(document)]
}

fn message(step: Step, id: i32) -> Option<Message> {
    let notification =
        |method: &str, params: Value| Some(Notification::new(method.to_string(), params).into());
    match step {
        Step::Open { document, text } => notification(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": uri(document), "languageId": "restructuredtext",
                "version": 1, "text": text,
            }}),
        ),
        Step::Change {
            document,
            version,
            text,
        } => notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri(document), "version": version },
                "contentChanges": [{ "text": text }],
            }),
        ),
        Step::Close { document } => notification(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri(document) } }),
        ),
        // `shutdown` and `exit` end the session, which the end of the input
        // does; `exit` without `shutdown` is a failure by design.
        Step::Request { method, .. } if method == "shutdown" => None,
        Step::Notify { method, .. } if method == "exit" => None,
        Step::Request { method, params } => {
            Some(Request::new(RequestId::from(id), method, json_or_string(params)).into())
        }
        Step::Notify { method, params } => notification(&method, json_or_string(params)),
        Step::Respond { id } => Some(Response::new_ok(RequestId::from(id), Value::Null).into()),
    }
}

fuzz_target!(|session: Session| {
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || rinx_lsp::run(&server));
    let encodings = if session.utf32 {
        json!(["utf-32"])
    } else {
        json!(["utf-16"])
    };
    let initialize = json!({ "capabilities": { "general": { "positionEncodings": encodings } } });
    let send = |message: Message| {
        client
            .sender
            .send(message)
            .expect("the server is listening");
    };

    send(Request::new(RequestId::from(0), "initialize".to_string(), initialize).into());
    send(Notification::new("initialized".to_string(), json!({})).into());
    for (id, step) in (1..).zip(session.steps) {
        if let Some(message) = message(step, id) {
            send(message);
        }
    }
    send(Request::new(RequestId::from(-1), "shutdown".to_string(), Value::Null).into());
    send(Notification::new("exit".to_string(), Value::Null).into());

    // A panic in the server re-raises here, where libFuzzer sees it.
    let outcome = handle.join().expect("the server does not panic");
    assert!(
        outcome.is_ok(),
        "the session did not end cleanly: {outcome:?}"
    );
});

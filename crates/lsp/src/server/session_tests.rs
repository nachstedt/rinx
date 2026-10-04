//! Whole sessions over an in-memory connection, through the protocol loop.

use super::test_support::*;

#[test]
fn test_run_answers_a_request_before_initialize_as_not_initialized() {
    // Given
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || run(&server));

    // When — the client asks something before initializing
    client
        .sender
        .send(
            Request::new(
                RequestId::from(9),
                "textDocument/hover".to_string(),
                serde_json::Value::Null,
            )
            .into(),
        )
        .unwrap();
    let early = client.receiver.recv().unwrap();
    let initialized = initialize(&client);
    drop(client);

    // Then
    assert!(
        matches!(&early, Message::Response(response)
            if response.response_result.as_ref().err().map(|error| error.code)
                == Some(ErrorCode::ServerNotInitialized as i32)),
        "{early:?}"
    );
    assert!(initialized.response_result.is_ok(), "{initialized:?}");
    let _ = handle.join().expect("server thread");
}

#[test]
fn test_run_fails_when_the_client_exits_without_shutdown() {
    // Given
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || run(&server));
    initialize(&client);

    // When
    client.sender.send(exit().into()).unwrap();

    // Then — the protocol asks for exit code 1, which an error becomes
    let outcome = handle.join().expect("server thread");
    assert!(outcome.is_err(), "{outcome:?}");
}

#[test]
fn test_run_refuses_a_request_and_ignores_a_response_mid_session() {
    // Given
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || run(&server));
    initialize(&client);

    // When — a request the server does not support, then a response to
    // nothing the server asked, then a document
    client
        .sender
        .send(
            Request::new(
                RequestId::from(5),
                "textDocument/formatting".to_string(),
                serde_json::Value::Null,
            )
            .into(),
        )
        .unwrap();
    let refused = client.receiver.recv().unwrap();
    client
        .sender
        .send(Response::new_ok(RequestId::from(99), serde_json::Value::Null).into())
        .unwrap();
    client.sender.send(did_open(".. foo::\n").into()).unwrap();
    let next = client.receiver.recv().unwrap();
    drop(client);

    // Then — the response drew no reply: the next message is the publish
    assert!(
        matches!(&refused, Message::Response(response)
            if response.id == RequestId::from(5)
                && response.response_result.as_ref().err().map(|error| error.code)
                    == Some(ErrorCode::MethodNotFound as i32)),
        "{refused:?}"
    );
    assert_eq!(
        codes(&published(&next)),
        vec!["directive.unknown".to_string()]
    );
    let _ = handle.join().expect("server thread");
}

#[test]
fn test_run_fails_when_the_client_disconnects_without_shutdown() {
    // Given
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || run(&server));
    initialize(&client);

    // When
    drop(client);

    // Then
    let outcome = handle.join().expect("server thread");
    assert!(outcome.is_err(), "{outcome:?}");
}

#[test]
fn test_run_serves_a_whole_session() {
    // Given — a server on one end of an in-memory connection
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || run(&server));

    // When — the client initializes, opens a broken document, fixes it
    // and shuts down
    client
        .sender
        .send(
            Request::new(
                RequestId::from(1),
                Initialize::METHOD.to_string(),
                InitializeParams::default(),
            )
            .into(),
        )
        .unwrap();
    let Message::Response(initialized) = client.receiver.recv().unwrap() else {
        panic!("expected the initialize response");
    };
    client
        .sender
        .send(
            Notification::new(
                lsp_types::notification::Initialized::METHOD.to_string(),
                lsp_types::InitializedParams {},
            )
            .into(),
        )
        .unwrap();
    client.sender.send(did_open(".. foo::\n").into()).unwrap();
    let opened = published(&client.receiver.recv().unwrap());
    client.sender.send(did_change("Fixed.\n").into()).unwrap();
    let changed = published(&client.receiver.recv().unwrap());
    client
        .sender
        .send(
            Request::new(
                RequestId::from(2),
                Shutdown::METHOD.to_string(),
                serde_json::Value::Null,
            )
            .into(),
        )
        .unwrap();
    let shutdown = client.receiver.recv().unwrap();
    client
        .sender
        .send(
            Notification::new(
                lsp_types::notification::Exit::METHOD.to_string(),
                serde_json::Value::Null,
            )
            .into(),
        )
        .unwrap();

    // Then
    assert!(initialized.response_result.is_ok(), "{initialized:?}");
    assert_eq!(codes(&opened), vec!["directive.unknown".to_string()]);
    assert_eq!(changed.diagnostics, Vec::new());
    assert!(
        matches!(&shutdown, Message::Response(response) if response.response_result.is_ok()),
        "{shutdown:?}"
    );
    handle.join().expect("server thread").expect("clean exit");
}

#[test]
fn test_run_publishes_what_rendering_found_once_the_client_is_quiet() {
    // Given — a workspace whose one document references a label nobody defines
    let text = "See :ref:`install`.\n";
    let workspace = Workspace::new("session_render", &[("index.rst", text)]);
    let (server, client) = Connection::memory();
    let handle = std::thread::spawn(move || run(&server));
    initialize_with(
        &client,
        InitializeParams {
            workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
                uri: crate::uri::file_uri(&workspace.root).expect("a folder uri"),
                name: "docs".to_string(),
            }]),
            ..InitializeParams::default()
        },
    );

    // When — the document is opened, and nothing else is sent
    client
        .sender
        .send(workspace.open("index.rst", text).into())
        .unwrap();
    let codes = std::iter::from_fn(|| client.receiver.recv().ok())
        .filter_map(|message| match message {
            Message::Notification(notification)
                if notification.method == PublishDiagnostics::METHOD =>
            {
                Some(codes(&published(&Message::Notification(notification))))
            }
            _ => None,
        })
        .find(|codes| !codes.is_empty());
    drop(client);

    // Then
    assert_eq!(codes, Some(vec!["link.broken-ref".to_string()]));
    let _ = handle.join().expect("server thread");
}

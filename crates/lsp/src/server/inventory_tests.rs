//! A Sphinx project's intersphinx inventories: asked of the client once per
//! address, read from the file it names, and the references into them
//! resolved — or lowered to hints while they are not loaded.

use super::inventories::{FETCH_INVENTORY_METHOD, FetchInventoryParams};
use super::test_support::*;
use lsp_types::{DiagnosticSeverity, FileChangeType};

const PYTHON_CONF: &str = "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'python': ('https://docs.python.org/3/', None)}\nintersphinx_cache_limit = 7\n";

/// A document referencing the Python class `dict`, on line 3 from column 15.
const INDEX: &str = "Home\n====\n\nSee :py:class:`dict`.\n";

const PYTHON_INVENTORY: &str = "https://docs.python.org/3/objects.inv";

/// Writes an inventory listing the Python class `dict` to `path`.
fn write_inventory(path: &std::path::Path) {
    write_python_inventory(path, &[("dict", "py:class", "library/stdtypes.html#$")]);
}

/// A server over `files` that downloads inventories when `fetching`, its
/// scan run — and what the scan's result brought about.
fn fetching_server(
    name: &str,
    files: &[(&str, &str)],
    fetching: bool,
) -> (Workspace, ServerState, Vec<Message>) {
    let workspace = Workspace::new(name, files);
    let (state, _) = workspace.server(false);
    let mut state = state.with_inventory_fetching(fetching);
    let messages = state.on_scan_event(workspace.scanned());
    (workspace, state, messages)
}

/// Every `rinx/fetchInventory` request among `messages`.
fn fetch_requests(messages: &[Message]) -> Vec<(RequestId, FetchInventoryParams)> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Request(request) if request.method == FETCH_INVENTORY_METHOD => Some((
                request.id.clone(),
                serde_json::from_value(request.params.clone()).unwrap(),
            )),
            _ => None,
        })
        .collect()
}

/// Each diagnostic `messages` last publish for `file`, as its code,
/// severity and message.
fn shown(
    workspace: &Workspace,
    messages: &[Message],
    file: &str,
) -> Vec<(String, Option<DiagnosticSeverity>, String)> {
    let uri = workspace.uri(file);
    messages
        .iter()
        .filter(|reply| {
            matches!(reply, Message::Notification(n) if n.method == "textDocument/publishDiagnostics")
        })
        .map(published)
        .filter(|params| params.uri == uri)
        .last()
        .map(|params| {
            params
                .diagnostics
                .into_iter()
                .map(|diagnostic| {
                    let code = match diagnostic.code {
                        Some(NumberOrString::String(code)) => code,
                        _ => String::new(),
                    };
                    (code, diagnostic.severity, diagnostic.message)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn test_a_declared_inventory_is_asked_for_once_with_its_cache_limit() {
    // When — two projects declaring the same inventory
    let (_, _, messages) = fetching_server(
        "inventory_once",
        &[
            ("a/conf.py", PYTHON_CONF),
            ("a/index.rst", INDEX),
            ("b/conf.py", PYTHON_CONF),
            ("b/index.rst", INDEX),
        ],
        true,
    );

    // Then
    let requests = fetch_requests(&messages);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(
        requests[0].1,
        FetchInventoryParams {
            url: PYTHON_INVENTORY.to_string(),
            cache_limit_days: 7,
        }
    );
}

#[test]
fn test_a_downloaded_inventory_resolves_a_reference_and_hover_shows_its_address() {
    // Given
    let (workspace, mut state, messages) = fetching_server(
        "inventory_downloaded",
        &[("conf.py", PYTHON_CONF), ("index.rst", INDEX)],
        true,
    );
    let (id, _) = fetch_requests(&messages).remove(0);
    handle_notification(&mut state, workspace.open("index.rst", INDEX));
    let before = render_all(&mut state);
    assert_eq!(
        shown(&workspace, &before, "index.rst")
            .into_iter()
            .map(|(code, severity, _)| (code, severity))
            .collect::<Vec<_>>(),
        [(
            "link.broken-object".to_string(),
            Some(DiagnosticSeverity::HINT)
        )]
    );
    let cache = workspace.root.join(".cache/python.inv");
    write_inventory(&cache);

    // When — the client answers with the file it saved
    let answered = state.on_response(&Response::new_ok(id, serde_json::json!({ "path": cache })));
    let mut after = answered.clone();
    after.extend(render_all(&mut state));

    // Then
    assert!(fetch_requests(&answered).is_empty());
    assert_eq!(shown(&workspace, &after, "index.rst"), []);
    let hover = hover_markdown(&mut state, workspace.uri("index.rst"), 3, 16).expect("a hover");
    assert!(
        hover.contains("<https://docs.python.org/3/library/stdtypes.html#dict>"),
        "{hover}"
    );
}

#[test]
fn test_a_declined_download_leaves_a_hint_saying_why_and_nothing_in_conf_py() {
    // Given
    let (workspace, mut state, messages) = fetching_server(
        "inventory_declined",
        &[("conf.py", PYTHON_CONF), ("index.rst", INDEX)],
        true,
    );
    let (id, _) = fetch_requests(&messages).remove(0);
    handle_notification(&mut state, workspace.open("index.rst", INDEX));

    // When
    let mut replies = state.on_response(&Response::new_ok(
        id,
        serde_json::json!({ "unavailable": "the download was declined" }),
    ));
    replies.extend(render_all(&mut state));

    // Then
    let found = shown(&workspace, &replies, "index.rst");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, Some(DiagnosticSeverity::HINT));
    assert!(
        found[0].2.contains("the download was declined"),
        "{}",
        found[0].2
    );
    assert_eq!(shown(&workspace, &replies, "conf.py"), []);
}

#[test]
fn test_an_error_answer_counts_as_unavailable() {
    // Given
    let (workspace, mut state, messages) = fetching_server(
        "inventory_error",
        &[("conf.py", PYTHON_CONF), ("index.rst", INDEX)],
        true,
    );
    let (id, _) = fetch_requests(&messages).remove(0);
    handle_notification(&mut state, workspace.open("index.rst", INDEX));

    // When
    let mut replies = state.on_response(&Response::new_err(
        id,
        ErrorCode::MethodNotFound as i32,
        "unhandled method".to_string(),
    ));
    replies.extend(render_all(&mut state));

    // Then
    let found = shown(&workspace, &replies, "index.rst");
    assert!(found[0].2.contains("unhandled method"), "{}", found[0].2);
}

#[test]
fn test_a_client_that_does_not_download_is_never_asked() {
    // Given
    let (workspace, mut state, messages) = fetching_server(
        "inventory_no_client",
        &[("conf.py", PYTHON_CONF), ("index.rst", INDEX)],
        false,
    );
    handle_notification(&mut state, workspace.open("index.rst", INDEX));

    // When
    let rendered = render_all(&mut state);

    // Then
    assert!(fetch_requests(&messages).is_empty());
    let found = shown(&workspace, &rendered, "index.rst");
    assert_eq!(found[0].1, Some(DiagnosticSeverity::HINT));
    assert!(
        found[0]
            .2
            .contains("this editor does not download inventories"),
        "{}",
        found[0].2
    );
}

#[test]
fn test_a_file_inventory_is_read_by_the_server_and_again_once_it_changes() {
    // Given — the file is missing at first
    let conf = "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'python': ('https://docs.python.org/3/', '_inv/python.inv')}\n";
    let (workspace, mut state, messages) = fetching_server(
        "inventory_file",
        &[("conf.py", conf), ("index.rst", INDEX)],
        true,
    );
    assert!(fetch_requests(&messages).is_empty());
    assert_eq!(
        shown(&workspace, &messages, "conf.py")
            .into_iter()
            .map(|(code, _, _)| code)
            .collect::<Vec<_>>(),
        ["conf.inventory-unreadable"]
    );
    handle_notification(&mut state, workspace.open("index.rst", INDEX));
    render_all(&mut state);

    // When — it appears on the disk
    write_inventory(&workspace.root.join("_inv/python.inv"));
    let mut replies = handle_notification(
        &mut state,
        workspace.watched(&[("_inv/python.inv", FileChangeType::CREATED)]),
    );
    replies.extend(render_all(&mut state));

    // Then
    assert_eq!(shown(&workspace, &replies, "conf.py"), []);
    assert_eq!(shown(&workspace, &replies, "index.rst"), []);
}

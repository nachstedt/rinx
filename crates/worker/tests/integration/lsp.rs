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
    assert_eq!(
        statuses[0],
        json!({"state": "indexing", "documents": 0, "projects": []})
    );
    assert_eq!(statuses[1]["state"], "ready");
    assert_eq!(statuses[1]["documents"], 3);
    assert!(statuses[1]["elapsedMs"].is_u64(), "{statuses:?}");
    assert_eq!(
        statuses[1]["projects"],
        json!([{"kind": "folder", "root": "rinx_lsp_stdio_workspace"}])
    );
}

#[test]
fn test_lsp_reads_a_sphinx_projects_conf_py() {
    // Given — a Sphinx project excluding one document, and computing a
    // setting the server reads
    let root = std::env::temp_dir().join("rinx_lsp_stdio_sphinx");
    let _ = std::fs::remove_dir_all(&root);
    for (file, text) in [
        (
            "docs/conf.py",
            "import os\nexclude_patterns = ['drafts']\nroot_doc = os.getenv('ROOT')\n",
        ),
        ("docs/index.rst", "Home\n====\n"),
        ("docs/drafts/wip.rst", "Draft\n=====\n"),
    ] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
    let mut server = Server::spawn();

    // When — reading up to the ready status
    server.initialize_with(json!({
        "capabilities": {},
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));
    let mut published = Vec::new();
    let ready = loop {
        match server.receive() {
            lsp_server::Message::Notification(notification)
                if notification.method == "rinx/status"
                    && notification.params["state"] == "ready" =>
            {
                break notification.params;
            }
            lsp_server::Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics" =>
            {
                published.push(notification.params);
            }
            _ => {}
        }
    };

    // Then
    assert_eq!(ready["documents"], 1);
    assert_eq!(
        ready["projects"],
        json!([{"kind": "sphinx", "conf": "docs/conf.py"}])
    );
    let conf = published
        .iter()
        .find(|params| {
            params["uri"]
                .as_str()
                .is_some_and(|uri| uri.ends_with("docs/conf.py"))
        })
        .expect("conf.py is published");
    assert_eq!(conf["diagnostics"][0]["code"], "conf.unread-setting");
    assert_eq!(conf["diagnostics"][0]["range"]["start"]["line"], 2);
}

#[test]
fn test_lsp_lowers_what_a_sphinx_projects_extensions_explain() {
    // Given — a project declaring autodoc, and an extension no profile models
    let root = std::env::temp_dir().join("rinx_lsp_stdio_extensions");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("conf.py"),
        "extensions = ['sphinx.ext.autodoc', 'my_ext']\n",
    )
    .expect("write");
    let text = "Home\n====\n\n.. automodule:: spam\n";
    std::fs::write(root.join("index.rst"), text).expect("write");
    let mut server = Server::spawn();
    server.initialize_with(json!({
        "capabilities": {},
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));

    // When — reading up to the ready status, then opening the document
    let mut shown = Vec::new();
    loop {
        match server.receive() {
            Message::Notification(notification)
                if notification.method == "rinx/status"
                    && notification.params["state"] == "ready" =>
            {
                break;
            }
            Message::Notification(notification) if notification.method == "window/showMessage" => {
                shown.push(notification.params);
            }
            _ => {}
        }
    }
    let uri = format!("file://{}", root.join("index.rst").display());
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": uri, "languageId": "restructuredtext", "version": 1, "text": text,
        }}),
    );
    let published = std::iter::from_fn(|| match server.receive() {
        Message::Notification(notification) => Some(notification),
        _ => None,
    })
    .filter(|notification| notification.method == "textDocument/publishDiagnostics")
    .map(|notification| notification.params)
    .find(|published| published["uri"] == uri.as_str())
    .expect("the document is published");

    // Then — the unmodelled extension is named once, as information, and the
    // directive autodoc provides is information too
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0]["type"], 3);
    assert!(
        shown[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("`my_ext`")),
        "{shown:?}"
    );
    assert_eq!(published["diagnostics"][0]["code"], "directive.unknown");
    assert_eq!(published["diagnostics"][0]["severity"], 3);
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
fn test_lsp_shows_where_a_reference_leads_on_hover() {
    // Given — a scanned folder whose `setup.rst` labels a section
    let root = std::env::temp_dir().join("rinx_lsp_stdio_hover");
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
            "text": "See :ref:`install`.\n",
        }}),
    );
    server.published_until(&notes);

    // When
    server.request(
        2,
        "textDocument/hover",
        json!({
            "textDocument": { "uri": notes },
            "position": { "line": 0, "character": 8 },
        }),
    );
    let hovered = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Response(response) => Some(response),
            _ => None,
        })
        .expect("the hover");

    // Then
    let capabilities = &initialized.response_result.expect("initialized")["capabilities"];
    assert_eq!(capabilities["hoverProvider"], true);
    let hover = hovered.response_result.expect("a hover");
    assert_eq!(hover["contents"]["kind"], "markdown");
    let markdown = hover["contents"]["value"].as_str().expect("markdown");
    assert!(
        markdown.starts_with("**Installing**\n\n[setup\\.rst](file://"),
        "{markdown}"
    );
    assert_eq!(
        hover["range"]["start"],
        json!({ "line": 0, "character": 4 })
    );
}

#[test]
fn test_lsp_previews_an_open_documents_page() {
    // Given — a scanned folder whose root document names `setup.rst`
    let root = std::env::temp_dir().join("rinx_lsp_stdio_preview");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(root.join("setup.rst"), "Installing\n==========\n").expect("write");
    let index = format!("file://{}", root.join("index.rst").display());
    let mut server = Server::spawn();
    server.initialize_with(json!({
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
            "uri": index, "languageId": "restructuredtext", "version": 1,
            "text": "Home\n====\n\n.. toctree::\n\n   setup\n",
        }}),
    );

    // When
    server.request(
        2,
        "rinx/preview",
        json!({ "textDocument": { "uri": index } }),
    );
    let previewed = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Response(response) => Some(response),
            _ => None,
        })
        .expect("the preview");

    // Then — the default template, with the sidebar naming the other title
    let page = previewed.response_result.expect("a page");
    let html = page["html"].as_str().expect("html");
    assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
    assert!(html.contains("<title>Home"), "{html}");
    assert!(html.contains("Installing"), "{html}");
}

#[test]
fn test_lsp_follows_a_link_of_the_preview_to_its_source() {
    // Given — a scanned folder whose `setup.rst` labels a section
    let root = std::env::temp_dir().join("rinx_lsp_stdio_preview_link");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("setup.rst"),
        ".. _install:\n\nInstalling\n==========\n",
    )
    .expect("write");
    let index = format!("file://{}", root.join("index.rst").display());
    let mut server = Server::spawn();
    server.initialize_with(json!({
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
            "uri": index, "languageId": "restructuredtext", "version": 1,
            "text": "Home\n====\n\nSee :ref:`install`.\n",
        }}),
    );

    // When
    server.request(
        2,
        "rinx/previewLink",
        json!({ "textDocument": { "uri": index }, "href": "setup.html#install" }),
    );
    let answered = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Response(response) => Some(response),
            _ => None,
        })
        .expect("the link's target");

    // Then
    assert_eq!(
        answered.response_result.expect("a target"),
        json!({
            "kind": "document",
            "uri": format!("file://{}", root.join("setup.rst").display()),
            "fragment": "install",
        })
    );
}

#[test]
fn test_lsp_opens_the_document_a_reference_leads_to() {
    // Given — a scanned folder whose `setup.rst` labels a section
    let root = std::env::temp_dir().join("rinx_lsp_stdio_definition");
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
            "text": "See :ref:`install`.\n",
        }}),
    );
    server.published_until(&notes);

    // When
    server.request(
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": notes },
            "position": { "line": 0, "character": 8 },
        }),
    );
    let answered = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Response(response) => Some(response),
            _ => None,
        })
        .expect("the definition");

    // Then
    let capabilities = &initialized.response_result.expect("initialized")["capabilities"];
    assert_eq!(capabilities["definitionProvider"], true);
    let location = answered.response_result.expect("a definition");
    assert_eq!(
        location["uri"],
        format!("file://{}", root.join("setup.rst").display())
    );
    assert_eq!(
        location["range"]["start"],
        json!({ "line": 0, "character": 0 })
    );
}

#[test]
fn test_lsp_underlines_a_reference_to_a_label_no_document_defines() {
    // Given — a workspace folder whose document references a missing label
    let root = std::env::temp_dir().join("rinx_lsp_stdio_broken_reference");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    let text = "Title\n=====\n\nSee :ref:`install`.\n";
    std::fs::write(root.join("index.rst"), text).expect("write");
    let index = format!("file://{}", root.join("index.rst").display());
    let mut server = Server::spawn();
    server.initialize_with(json!({
        "capabilities": {},
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));

    // When — opened; the render tier follows once the client is quiet
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": index, "languageId": "restructuredtext", "version": 1, "text": text,
        }}),
    );
    let rendered = std::iter::from_fn(|| match server.receive() {
        Message::Notification(notification) => Some(notification),
        _ => None,
    })
    .filter(|notification| notification.method == "textDocument/publishDiagnostics")
    .map(|notification| notification.params)
    .find(|published| !codes(published).is_empty())
    .expect("a publish with diagnostics");

    // Then
    assert_eq!(rendered["uri"], index.as_str());
    assert_eq!(codes(&rendered), vec!["link.broken-ref"]);
    assert_eq!(rendered["diagnostics"][0]["range"]["start"]["line"], 3);
    assert_eq!(
        rendered["diagnostics"][0]["message"],
        "broken ref 'install'"
    );
}

#[test]
fn test_lsp_watches_the_workspace_and_breaks_a_reference_to_a_deleted_file() {
    // Given — a scanned folder whose open `index.rst` refers to `setup.rst`,
    // and a client that registers file watchers on request
    let root = std::env::temp_dir().join("rinx_lsp_stdio_watched_files");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(root.join("setup.rst"), "Setup\n=====\n").expect("write");
    let text = "Home\n====\n\nSee :doc:`setup`.\n";
    std::fs::write(root.join("index.rst"), text).expect("write");
    let index = format!("file://{}", root.join("index.rst").display());
    let mut server = Server::spawn();
    server.initialize_with(json!({
        "capabilities": {
            "workspace": { "didChangeWatchedFiles": { "dynamicRegistration": true } },
        },
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));
    let registration = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Request(request) if request.method == "client/registerCapability" => {
                Some(request)
            }
            _ => None,
        })
        .expect("the watcher's registration");
    while !matches!(
        server.receive(),
        Message::Notification(notification)
            if notification.method == "rinx/status" && notification.params["state"] == "ready"
    ) {}
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": index, "languageId": "restructuredtext", "version": 1, "text": text,
        }}),
    );
    server.published_until(&index);

    // When — the file is deleted, and the client's watcher reports it
    std::fs::remove_file(root.join("setup.rst")).expect("remove");
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({ "changes": [{
            "uri": format!("file://{}", root.join("setup.rst").display()),
            "type": 3,
        }]}),
    );
    let rendered = std::iter::from_fn(|| match server.receive() {
        Message::Notification(notification) => Some(notification),
        _ => None,
    })
    .filter(|notification| notification.method == "textDocument/publishDiagnostics")
    .map(|notification| notification.params)
    .find(|published| !codes(published).is_empty())
    .expect("a publish with diagnostics");

    // Then
    assert_eq!(
        registration.params["registrations"][0]["method"],
        "workspace/didChangeWatchedFiles"
    );
    assert_eq!(rendered["uri"], index.as_str());
    assert_eq!(codes(&rendered), vec!["link.broken-doc"]);
}

#[test]
fn test_lsp_asks_its_client_for_an_inventory_and_resolves_into_it() {
    // Given — a Sphinx project linking into Python's documentation, and a
    // client that downloads inventories
    let root = std::env::temp_dir().join("rinx_lsp_stdio_inventory");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("conf.py"),
        "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'python': ('https://docs.python.org/3/', None)}\n",
    )
    .expect("write");
    let text = "Home\n====\n\nSee :py:class:`dict`.\n";
    std::fs::write(root.join("index.rst"), text).expect("write");
    let saved = root.join(".cache").join("python.inv");
    std::fs::create_dir_all(saved.parent().expect("a parent")).expect("mkdir");
    let inventory = rinx_inventory::Inventory {
        project: "Python".to_string(),
        version: "3.14".to_string(),
        entries: vec![rinx_inventory::InventoryEntry {
            name: "dict".to_string(),
            entry_type: rinx_inventory::EntryType::new("py:class").expect("a type"),
            priority: 1,
            uri: "library/stdtypes.html#$".to_string(),
            display_name: None,
        }],
    };
    std::fs::write(&saved, rinx_inventory::write_inventory(&inventory)).expect("write");
    let index = format!("file://{}", root.join("index.rst").display());
    let mut server = Server::spawn();
    server.initialize_with(json!({
        "capabilities": {},
        "initializationOptions": { "fetchInventories": true },
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));

    // When — the server asks, and the client answers with the file it saved
    let asked = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Request(request) if request.method == "rinx/fetchInventory" => Some(request),
            _ => None,
        })
        .expect("the inventory's request");
    server.send(lsp_server::Response::new_ok(
        asked.id.clone(),
        json!({ "path": saved }),
    ));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": index, "languageId": "restructuredtext", "version": 1, "text": text,
        }}),
    );
    server.published_until(&index);
    server.request(
        2,
        "textDocument/hover",
        json!({
            "textDocument": { "uri": index },
            "position": { "line": 3, "character": 16 },
        }),
    );
    let hovered = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Response(response) => Some(response),
            _ => None,
        })
        .expect("the hover");

    // Then
    assert_eq!(
        asked.params,
        json!({ "url": "https://docs.python.org/3/objects.inv", "cacheLimitDays": 5 })
    );
    let hover = hovered.response_result.expect("a hover");
    let markdown = hover["contents"]["value"].as_str().expect("markdown");
    assert!(
        markdown.contains("<https://docs.python.org/3/library/stdtypes.html#dict>"),
        "{markdown}"
    );
}

#[cfg(unix)]
#[test]
fn test_lsp_runs_conf_py_in_the_interpreter_its_client_names() {
    use std::os::unix::fs::PermissionsExt;
    // Given — a project computing its root document, and an interpreter
    // standing in for Python, printing what running `conf.py` would
    let root = std::env::temp_dir().join("rinx_lsp_stdio_conf_run");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("conf.py"),
        "import os\nroot_doc = os.getenv('ROOT', 'contents')\n",
    )
    .expect("write");
    std::fs::write(root.join("contents.rst"), "Home\n====\n").expect("write");
    let python = root.join(".bin").join("python");
    std::fs::create_dir_all(python.parent().expect("a parent")).expect("mkdir");
    std::fs::write(
        &python,
        "#!/bin/sh\ncat <<'EOF'\n{\"values\": {\"root_doc\": \"'contents'\"}}\nEOF\n",
    )
    .expect("write");
    std::fs::set_permissions(&python, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let conf = format!("file://{}", root.join("conf.py").display());
    let mut server = Server::spawn();
    server.initialize_with(json!({
        "capabilities": {},
        "initializationOptions": { "evaluateConf": true },
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
    }));

    // When — the server asks, and the client names the interpreter
    let asked = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Request(request) if request.method == "rinx/confInterpreter" => Some(request),
            _ => None,
        })
        .expect("the interpreter's request");
    server.send(lsp_server::Response::new_ok(
        asked.id.clone(),
        json!({ "path": python }),
    ));
    let mut conf_published = None;
    let ran = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics"
                    && notification.params["uri"] == conf.as_str() =>
            {
                conf_published = Some(notification.params);
                None
            }
            Message::Notification(notification)
                if notification.method == "rinx/status"
                    && notification.params["projects"][0]["executedWith"].is_string() =>
            {
                Some(notification.params)
            }
            _ => None,
        })
        .expect("the status after the run");

    // Then — the run's value replaced the reading's warning
    assert_eq!(asked.params, json!({ "conf": conf }));
    assert_eq!(
        ran["projects"][0]["executedWith"],
        python.display().to_string()
    );
    assert_eq!(
        conf_published.expect("conf.py is published again")["diagnostics"],
        json!([])
    );
}

#[test]
fn test_lsp_serves_a_client_announcing_only_what_the_protocol_requires() {
    // Given — what an editor other than VS Code may send: a `rootUri` and no
    // folders, no progress, no watcher registration, no initialization
    // options, and a list of encodings led by one the server does not speak;
    // the folder is a Bazel workspace, whose manifests such a client is never
    // asked for
    let root = std::env::temp_dir().join("rinx_lsp_stdio_minimal_client");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(root.join("MODULE.bazel"), "").expect("write");
    std::fs::write(
        root.join("setup.rst"),
        ".. _install:\n\nInstalling\n==========\n",
    )
    .expect("write");
    let notes = format!("file://{}", root.join("notes.rst").display());
    let mut server = Server::spawn();
    let initialized = server.initialize_with(json!({
        "capabilities": { "general": { "positionEncodings": ["utf-8", "utf-32"] } },
        "rootUri": format!("file://{}", root.display()),
    }));
    let mut before_ready = Vec::new();
    loop {
        let message = server.receive();
        let ready = matches!(
            &message,
            Message::Notification(notification)
                if notification.method == "rinx/status" && notification.params["state"] == "ready"
        );
        before_ready.push(message);
        if ready {
            break;
        }
    }

    // When — a document referencing a label of the scanned folder after a
    // character outside the Basic Multilingual Plane, then a hover on it
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": notes, "languageId": "rst", "version": 1,
            "text": "\u{1F600} See :ref:`install`.\n\n.. foo::\n",
        }}),
    );
    let published = server.published_until(&notes);
    server.request(
        2,
        "textDocument/hover",
        json!({
            "textDocument": { "uri": notes },
            "position": { "line": 0, "character": 9 },
        }),
    );
    let mut after_open = Vec::new();
    let hovered = loop {
        match server.receive() {
            Message::Response(response) => break response,
            other => after_open.push(other),
        }
    };

    // Then — UTF-32 is agreed, the root is scanned, and the server asks the
    // client for nothing it did not announce
    let result = initialized.response_result.expect("initialized");
    assert_eq!(result["capabilities"]["positionEncoding"], "utf-32");
    let opened = published.last().expect("the document's diagnostics");
    assert_eq!(codes(opened), vec!["directive.unknown"]);
    let hover = hovered.response_result.expect("a hover");
    let markdown = hover["contents"]["value"].as_str().expect("markdown");
    assert!(markdown.starts_with("**Installing**"), "{markdown}");
    assert_eq!(
        hover["range"]["start"],
        json!({ "line": 0, "character": 6 })
    );
    let requests: Vec<_> = before_ready
        .iter()
        .chain(&after_open)
        .filter_map(|message| match message {
            Message::Request(request) => Some(request.method.as_str()),
            _ => None,
        })
        .collect();
    assert!(requests.is_empty(), "{requests:?}");
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

/// The codes `rinx lsp --check` shows for `file` of the fixture
/// `crates/worker/tests/bazel_project/`, checked with `args` after it.
fn bazel_project_codes(args: &[&str], file: &str) -> Vec<String> {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bazel_project");
    let output = Command::new(env!("CARGO_BIN_EXE_rinx"))
        .args(["lsp", "--check", project.to_str().expect("a UTF-8 path")])
        .args(args)
        .current_dir(&project)
        .output()
        .expect("rinx runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let check: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    let uri = format!(
        "file://{}",
        std::fs::canonicalize(project.join(file))
            .expect("the file")
            .display()
    );
    check["published"][uri.as_str()]
        .as_array()
        .map(|diagnostics| {
            diagnostics
                .iter()
                .filter_map(|diagnostic| diagnostic["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn test_lsp_check_reads_each_library_of_a_bazel_manifest_as_the_build_does() {
    // Given — the fixture's manifest, as `rinx_site`'s `lsp_manifest` output
    // group would write it: an entity library, and a `c`-domain library whose
    // `default_role` is `any`, with an include it does not declare
    let manifest = ["--manifest", "site.lsp_manifest.json"];

    // When / Then — without the manifest the folder knows none of it
    assert_eq!(
        bazel_project_codes(&[], "reqs/index.rst"),
        ["directive.unknown"]
    );
    assert_eq!(
        bazel_project_codes(&manifest, "reqs/index.rst"),
        Vec::<String>::new()
    );
    // `.. macro::` is the `c` domain's, and a bare `nowhere` a `:any:`
    // reference that resolves nowhere, only under the library's settings
    assert_eq!(
        bazel_project_codes(&[], "api/index.rst"),
        ["directive.unknown"]
    );
    assert_eq!(
        bazel_project_codes(&manifest, "api/index.rst"),
        ["link.broken-any"]
    );
    assert_eq!(
        bazel_project_codes(&[], "api/undeclared.rst"),
        Vec::<String>::new()
    );
    assert_eq!(
        bazel_project_codes(&manifest, "api/undeclared.rst"),
        ["include.file-unreadable"]
    );
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

#[test]
fn test_lsp_offers_the_missing_dependency_of_a_toctree_entry() {
    // Given — a Bazel workspace whose `//:index` toctree names a document of
    // `//:guide`, which it does not depend on
    let root = std::env::temp_dir().join("rinx_lsp_stdio_quick_fix");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    let text = "Home\n====\n\n.. toctree::\n\n   guide\n";
    let build = "rinx_library(\n    name = \"index\",\n    srcs = [\"index.rst\"],\n)\n";
    let library = |label: &str, src: &str, allowed: &[&str]| {
        json!({
            "label": label, "srcs": [{"source": src}], "allowed_docs": allowed,
            "default_domain": "py", "default_role": null, "rst_prolog": null,
            "rst_epilog": null, "entity_schema": null,
        })
    };
    let manifest = json!({
        "version": 1,
        "site": {"label": "//:site", "config": {"source": "rinx.toml"}, "entity_schema": null},
        "inventories": [],
        "libraries": [
            library("//:guide", "guide.rst", &["guide"]),
            library("//:index", "index.rst", &["index"]),
        ],
    });
    for (file, content) in [
        ("MODULE.bazel", "module(name = \"docs\")\n".to_string()),
        ("rinx.toml", String::new()),
        ("BUILD.bazel", build.to_string()),
        ("index.rst", text.to_string()),
        ("guide.rst", "Guide\n=====\n".to_string()),
        ("site.lsp_manifest.json", manifest.to_string()),
    ] {
        std::fs::write(root.join(file), content).expect("write");
    }
    let index = format!("file://{}", root.join("index.rst").display());
    let mut server = Server::spawn();
    let initialized = server.initialize_with(json!({
        "capabilities": {},
        "workspaceFolders": [{"uri": format!("file://{}", root.display()), "name": "docs"}],
        "initializationOptions": {"bazelManifests": true},
    }));
    let asked = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Request(request) if request.method == "rinx/bazelManifests" => Some(request),
            _ => None,
        })
        .expect("the manifests asked for");
    server.send(lsp_server::Response::new_ok(
        asked.id,
        json!({"manifests": [root.join("site.lsp_manifest.json")]}),
    ));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": {
            "uri": index, "languageId": "restructuredtext", "version": 1, "text": text,
        }}),
    );
    std::iter::from_fn(|| match server.receive() {
        Message::Notification(notification) => Some(notification),
        _ => None,
    })
    .filter(|notification| notification.method == "textDocument/publishDiagnostics")
    .map(|notification| notification.params)
    .find(|published| codes(published).contains(&"toctree.undeclared-dependency"))
    .expect("the entry underlined");

    // When
    server.request(
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": index },
            "range": { "start": { "line": 5, "character": 3 }, "end": { "line": 5, "character": 3 } },
            "context": { "diagnostics": [] },
        }),
    );
    let answered = std::iter::from_fn(|| Some(server.receive()))
        .find_map(|message| match message {
            Message::Response(response) => Some(response),
            _ => None,
        })
        .expect("the code actions");

    // Then
    let capabilities = &initialized.response_result.expect("initialized")["capabilities"];
    assert_eq!(
        capabilities["codeActionProvider"],
        json!({"codeActionKinds": ["quickfix"]})
    );
    let actions = answered.response_result.expect("code actions");
    assert_eq!(actions[0]["title"], "Add //:guide to the deps of //:index");
    assert_eq!(actions[0]["kind"], "quickfix");
    let build_uri = format!("file://{}", root.join("BUILD.bazel").display());
    assert_eq!(
        actions[0]["edit"]["changes"][&build_uri],
        // After the comma ending `    srcs = ["index.rst"],`.
        json!([{
            "range": { "start": { "line": 2, "character": 25 }, "end": { "line": 2, "character": 25 } },
            "newText": "\n    deps = [\":guide\"],",
        }])
    );
}

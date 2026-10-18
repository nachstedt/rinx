//! The protocol loop: receives, dispatches to the handlers, sends.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use crossbeam_channel::{Receiver, after, never, select, unbounded};
use lsp_server::{Connection, Message};
use lsp_types::notification::{Exit, Notification as _};
use lsp_types::{ClientCapabilities, InitializeParams, Uri, WorkspaceFolder};
use serde::Deserialize;

use super::conf_runs::ConfExecution;
use super::handlers::{handle_notification, handle_request, initialize_result};
use super::scan::{ScanEvent, spawn_scan};
use super::state::ServerState;
use crate::uri::file_path;

/// How long the client must have sent nothing before the server renders: a
/// render costs far more than a parse, and typing would otherwise pay for one
/// per keystroke.
const RENDER_DELAY: Duration = Duration::from_millis(300);

/// Serves `connection` from the `initialize` handshake until `exit`.
///
/// The workspace scan runs on a thread of its own from the handshake on, so
/// a document opened meanwhile is diagnosed at once; what the scan finds
/// reaches the loop as events, beside the client's messages.
///
/// Renders run once the client has been quiet for [`RENDER_DELAY`], one
/// document per turn of the loop, so a message arriving meanwhile is answered
/// before the next render — and pushes the rest back.
///
/// # Errors
///
/// Fails when the handshake or the shutdown sequence breaks protocol, or when
/// the client disconnects while a message is being sent.
pub fn run(connection: &Connection) -> Result<()> {
    let (initialize_id, params) = connection.initialize_start()?;
    let roots: InitializeRoots =
        serde_json::from_value(params.clone()).context("invalid initialize parameters")?;
    let params: InitializeParams =
        serde_json::from_value(params).context("invalid initialize parameters")?;
    let (result, encoding) = initialize_result(&params.capabilities);
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;

    let mut state = ServerState::new(encoding)
        .with_workspace(
            workspace_roots(&roots),
            supports_progress(&params.capabilities),
        )
        .with_definition_links(supports_definition_links(&params.capabilities))
        .with_watched_files(supports_watched_files(&params.capabilities))
        .with_inventory_fetching(fetches_inventories(&params))
        .with_conf_execution(conf_execution(&params));
    let conf_runs = state.conf_run_events();
    let mut scan_events: Receiver<ScanEvent> = never();
    let roots = state.folder_roots();
    if !roots.is_empty() {
        let (sender, receiver) = unbounded();
        spawn_scan(roots, sender);
        scan_events = receiver;
    }
    // `initialize_finish` waited for `initialized`, so the server may now
    // send requests of its own.
    for message in state.watch_files().into_iter().chain(state.start_scan()) {
        connection.sender.send(message)?;
    }

    let mut quiet_since = Instant::now();
    loop {
        let render_due = if state.has_pending_renders() {
            after(RENDER_DELAY.saturating_sub(quiet_since.elapsed()))
        } else {
            never()
        };
        let replies = select! {
            recv(connection.receiver) -> message => {
                quiet_since = Instant::now();
                // The protocol asks a server to exit with code 1 unless it
                // was shut down, which the caller makes of an error; this
                // one is a client that crashed.
                let Ok(message) = message else {
                    bail!("the client disconnected without 'shutdown'");
                };
                match message {
                    Message::Request(request) => {
                        if connection.handle_shutdown(&request)? {
                            return Ok(());
                        }
                        vec![handle_request(&mut state, request).into()]
                    }
                    // An `exit` after `shutdown` never reaches this loop: it
                    // is consumed by `handle_shutdown` above.
                    Message::Notification(notification) if notification.method == Exit::METHOD => {
                        bail!("the client sent 'exit' without 'shutdown'");
                    }
                    Message::Notification(notification) => {
                        handle_notification(&mut state, notification)
                    }
                    Message::Response(response) => state.on_response(&response),
                }
            }
            recv(scan_events) -> event => if let Ok(event) = event {
                state.on_scan_event(event)
            } else {
                // The scan is over and its thread gone; stop listening.
                scan_events = never();
                Vec::new()
            },
            recv(conf_runs) -> event => event.map(|event| state.on_conf_run(event)).unwrap_or_default(),
            recv(render_due) -> _ => state.render_next(),
        };
        for reply in replies {
            connection.sender.send(reply)?;
        }
    }
}

/// Where the client says its workspace is: its folders, or the single root
/// of a client predating them.
///
/// Read apart from [`InitializeParams`], which marks `rootUri` deprecated —
/// yet clients still send it alone, and the protocol has a server fall back
/// to it when `workspaceFolders` is absent.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeRoots {
    workspace_folders: Option<Vec<WorkspaceFolder>>,
    root_uri: Option<Uri>,
}

/// The root of every workspace folder the client opened, as an absolute
/// path, or of its `rootUri` when it sent no folder list at all — an empty
/// list means no folder is open. A folder that is not on this machine is
/// left out.
fn workspace_roots(roots: &InitializeRoots) -> Vec<PathBuf> {
    match &roots.workspace_folders {
        Some(folders) => folders
            .iter()
            .filter_map(|folder| file_path(&folder.uri))
            .collect(),
        None => roots.root_uri.iter().filter_map(file_path).collect(),
    }
}

/// Whether the client downloads inventories for the server, as the VS Code
/// extension says with its `fetchInventories` initialization option.
fn fetches_inventories(params: &InitializeParams) -> bool {
    params
        .initialization_options
        .as_ref()
        .and_then(|options| options.get("fetchInventories"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Where `conf.py`'s interpreter comes from: the client, when it passes the
/// `evaluateConf` initialization option, which the VS Code extension does —
/// answering only in a trusted workspace that allows running `conf.py`.
fn conf_execution(params: &InitializeParams) -> ConfExecution {
    let asks = params
        .initialization_options
        .as_ref()
        .and_then(|options| options.get("evaluateConf"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if asks {
        ConfExecution::AskClient
    } else {
        ConfExecution::Off
    }
}

/// Whether the client takes `$/progress` reports for work the server starts.
fn supports_progress(capabilities: &ClientCapabilities) -> bool {
    capabilities
        .window
        .as_ref()
        .and_then(|window| window.work_done_progress)
        .unwrap_or(false)
}

/// Whether the client takes a definition as a link from the reference,
/// rather than as a bare location.
fn supports_definition_links(capabilities: &ClientCapabilities) -> bool {
    capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.definition.as_ref())
        .and_then(|definition| definition.link_support)
        .unwrap_or(false)
}

/// Whether the client registers a file watcher when the server asks it to.
fn supports_watched_files(capabilities: &ClientCapabilities) -> bool {
    capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watched| watched.dynamic_registration)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::WindowClientCapabilities;

    fn folder(uri: &str) -> WorkspaceFolder {
        WorkspaceFolder {
            uri: uri.parse::<Uri>().expect("valid uri"),
            name: "docs".to_string(),
        }
    }

    /// The roots read from an `initialize` request's `params`.
    fn roots_of(params: serde_json::Value) -> Vec<PathBuf> {
        let roots: InitializeRoots = serde_json::from_value(params).expect("valid roots");
        workspace_roots(&roots)
    }

    #[test]
    fn test_workspace_roots_reads_every_local_folder() {
        // Given
        let params = serde_json::json!({
            "workspaceFolders": [
                folder("file:///work/docs"),
                folder("vscode-vfs://github/remote"),
                folder("file:///work/other"),
            ],
        });

        // When / Then
        assert_eq!(
            roots_of(params),
            [PathBuf::from("/work/docs"), PathBuf::from("/work/other")]
        );
    }

    #[test]
    fn test_workspace_roots_is_empty_without_folders() {
        // When / Then
        assert!(roots_of(serde_json::json!({})).is_empty());
    }

    #[test]
    fn test_workspace_roots_falls_back_to_the_root_uri_without_folders() {
        // Given
        let params = serde_json::json!({ "rootUri": "file:///work/docs" });

        // When / Then
        assert_eq!(roots_of(params), [PathBuf::from("/work/docs")]);
    }

    #[test]
    fn test_workspace_roots_prefers_the_folders_over_the_root_uri() {
        // Given
        let params = serde_json::json!({
            "workspaceFolders": [folder("file:///work/other")],
            "rootUri": "file:///work/docs",
        });

        // When / Then
        assert_eq!(roots_of(params), [PathBuf::from("/work/other")]);
    }

    #[test]
    fn test_workspace_roots_takes_an_empty_folder_list_as_no_folder() {
        // Given
        let params = serde_json::json!({
            "workspaceFolders": [],
            "rootUri": "file:///work/docs",
        });

        // When / Then
        assert!(roots_of(params).is_empty());
    }

    #[test]
    fn test_workspace_roots_reads_a_null_root_uri_as_none() {
        // Given
        let params = serde_json::json!({ "workspaceFolders": null, "rootUri": null });

        // When / Then
        assert!(roots_of(params).is_empty());
    }

    #[test]
    fn test_conf_execution_asks_only_a_client_passing_the_option() {
        // Given
        let asking = InitializeParams {
            initialization_options: Some(serde_json::json!({"evaluateConf": true})),
            ..InitializeParams::default()
        };
        let declining = InitializeParams {
            initialization_options: Some(serde_json::json!({"evaluateConf": false})),
            ..InitializeParams::default()
        };

        // When / Then
        assert_eq!(conf_execution(&asking), ConfExecution::AskClient);
        assert_eq!(conf_execution(&declining), ConfExecution::Off);
        assert_eq!(
            conf_execution(&InitializeParams::default()),
            ConfExecution::Off
        );
    }

    #[test]
    fn test_supports_progress_reads_the_window_capability() {
        // Given
        let capable = ClientCapabilities {
            window: Some(WindowClientCapabilities {
                work_done_progress: Some(true),
                ..WindowClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        };

        // When / Then
        assert!(supports_progress(&capable));
        assert!(!supports_progress(&ClientCapabilities::default()));
    }

    #[test]
    fn test_supports_definition_links_reads_the_definition_capability() {
        // Given
        let capable = ClientCapabilities {
            text_document: Some(lsp_types::TextDocumentClientCapabilities {
                definition: Some(lsp_types::GotoCapability {
                    link_support: Some(true),
                    ..lsp_types::GotoCapability::default()
                }),
                ..lsp_types::TextDocumentClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        };

        // When / Then
        assert!(supports_definition_links(&capable));
        assert!(!supports_definition_links(&ClientCapabilities::default()));
    }

    #[test]
    fn test_supports_watched_files_reads_the_dynamic_registration_capability() {
        // Given
        let capable = ClientCapabilities {
            workspace: Some(lsp_types::WorkspaceClientCapabilities {
                did_change_watched_files: Some(
                    lsp_types::DidChangeWatchedFilesClientCapabilities {
                        dynamic_registration: Some(true),
                        ..lsp_types::DidChangeWatchedFilesClientCapabilities::default()
                    },
                ),
                ..lsp_types::WorkspaceClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        };

        // When / Then
        assert!(supports_watched_files(&capable));
        assert!(!supports_watched_files(&ClientCapabilities::default()));
    }
}

//! The protocol loop: receives, dispatches to the handlers, sends.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use crossbeam_channel::{Receiver, never, select, unbounded};
use lsp_server::{Connection, Message};
use lsp_types::notification::{Exit, Notification as _};
use lsp_types::{ClientCapabilities, InitializeParams};

use super::handlers::{handle_notification, handle_request, initialize_result};
use super::scan::{ScanEvent, spawn_scan};
use super::state::ServerState;
use crate::uri::file_path;

/// Serves `connection` from the `initialize` handshake until `exit`.
///
/// The workspace scan runs on a thread of its own from the handshake on, so
/// a document opened meanwhile is diagnosed at once; what the scan finds
/// reaches the loop as events, beside the client's messages.
///
/// # Errors
///
/// Fails when the handshake or the shutdown sequence breaks protocol, or when
/// the client disconnects while a message is being sent.
pub fn run(connection: &Connection) -> Result<()> {
    let (initialize_id, params) = connection.initialize_start()?;
    let params: InitializeParams =
        serde_json::from_value(params).context("invalid initialize parameters")?;
    let (result, encoding) = initialize_result(&params.capabilities);
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;

    let mut state = ServerState::new(encoding).with_workspace(
        workspace_roots(&params),
        supports_progress(&params.capabilities),
    );
    let mut scan_events: Receiver<ScanEvent> = never();
    let roots = state.folder_roots();
    if !roots.is_empty() {
        let (sender, receiver) = unbounded();
        spawn_scan(roots, sender);
        scan_events = receiver;
    }
    for message in state.start_scan() {
        connection.sender.send(message)?;
    }

    loop {
        let replies = select! {
            recv(connection.receiver) -> message => {
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
            }
        };
        for reply in replies {
            connection.sender.send(reply)?;
        }
    }
}

/// The root of every workspace folder the client opened, as an absolute
/// path. A folder that is not on this machine is left out.
fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    params
        .workspace_folders
        .iter()
        .flatten()
        .filter_map(|folder| file_path(&folder.uri))
        .collect()
}

/// Whether the client takes `$/progress` reports for work the server starts.
fn supports_progress(capabilities: &ClientCapabilities) -> bool {
    capabilities
        .window
        .as_ref()
        .and_then(|window| window.work_done_progress)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Uri, WindowClientCapabilities, WorkspaceFolder};

    fn folder(uri: &str) -> WorkspaceFolder {
        WorkspaceFolder {
            uri: uri.parse::<Uri>().expect("valid uri"),
            name: "docs".to_string(),
        }
    }

    #[test]
    fn test_workspace_roots_reads_every_local_folder() {
        // Given
        let params = InitializeParams {
            workspace_folders: Some(vec![
                folder("file:///work/docs"),
                folder("vscode-vfs://github/remote"),
                folder("file:///work/other"),
            ]),
            ..InitializeParams::default()
        };

        // When / Then
        assert_eq!(
            workspace_roots(&params),
            [PathBuf::from("/work/docs"), PathBuf::from("/work/other")]
        );
    }

    #[test]
    fn test_workspace_roots_is_empty_without_folders() {
        // When / Then
        assert!(workspace_roots(&InitializeParams::default()).is_empty());
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
}

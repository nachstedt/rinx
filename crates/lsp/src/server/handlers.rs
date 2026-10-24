//! The pure handlers: from the server's state and one incoming message to the
//! messages to send back.

use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    Notification as _, PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest,
    Request as _,
};
use lsp_types::{
    ClientCapabilities, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CompletionOptions, CompletionParams, DocumentSymbolParams,
    GotoDefinitionParams, HoverParams, HoverProviderCapability, InitializeResult, MessageType,
    OneOf, PublishDiagnosticsParams, ServerCapabilities, ServerInfo, ShowMessageParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use super::bazel_manifests::BAZEL_MANIFESTS_CHANGED_METHOD;
use super::conf_runs::CONF_INTERPRETER_CHANGED_METHOD;
use super::state::ServerState;
use crate::position::PositionEncoding;
use crate::preview::{PREVIEW_METHOD, PreviewParams, PreviewRefusal, PreviewResult};
use crate::preview_link::{PREVIEW_LINK_METHOD, PreviewLinkParams};

/// The answer to `initialize` for a client announcing `capabilities`, and the
/// position encoding the two have thereby agreed on.
#[must_use]
pub fn initialize_result(
    capabilities: &ClientCapabilities,
) -> (InitializeResult, PositionEncoding) {
    let encoding = PositionEncoding::negotiate(capabilities);
    let result = InitializeResult {
        capabilities: ServerCapabilities {
            position_encoding: Some(encoding.kind()),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            // A role's target opens with a backtick, a titled one's with `<`,
            // and a `:doc:` name turns absolute with `/`; a directive's name
            // follows the `..` of its marker, and an option's name the `:`
            // opening its line.
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(["`", "<", "/", ".", ":"].map(String::from).to_vec()),
                ..CompletionOptions::default()
            }),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                ..CodeActionOptions::default()
            })),
            ..ServerCapabilities::default()
        },
        server_info: Some(ServerInfo {
            name: "rinx".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };
    (result, encoding)
}

/// The messages to send in response to `notification`.
///
/// A notification the server does not handle — or one whose parameters do not
/// parse — is ignored: a notification has no reply in which to report an
/// error, and the protocol requires ignoring what a server does not know.
pub fn handle_notification(state: &mut ServerState, notification: Notification) -> Vec<Message> {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidOpenTextDocumentParams>(DidOpenTextDocument::METHOD)
            else {
                return Vec::new();
            };
            let document = params.text_document;
            state
                .documents
                .open(document.uri.clone(), document.version, document.text);
            state.refresh(&document.uri)
        }
        DidChangeTextDocument::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidChangeTextDocumentParams>(DidChangeTextDocument::METHOD)
            else {
                return Vec::new();
            };
            // Whole-document synchronisation: the last change is the full text.
            let Some(change) = params.content_changes.into_iter().last() else {
                return Vec::new();
            };
            let document = params.text_document;
            state
                .documents
                .replace(document.uri.clone(), document.version, change.text);
            state.refresh(&document.uri)
        }
        DidCloseTextDocument::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidCloseTextDocumentParams>(DidCloseTextDocument::METHOD)
            else {
                return Vec::new();
            };
            let uri = params.text_document.uri;
            state.documents.close(&uri);
            // A closed document's own squiggles would otherwise linger in the
            // Problems view, describing text nobody is looking at; what an
            // open includer finds in it stays, and its includers now read it
            // from the disk.
            state.refresh(&uri)
        }
        DidChangeWatchedFiles::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidChangeWatchedFilesParams>(DidChangeWatchedFiles::METHOD)
            else {
                return Vec::new();
            };
            state.on_watched_files(&params.changes)
        }
        CONF_INTERPRETER_CHANGED_METHOD => state.on_conf_interpreter_changed(),
        BAZEL_MANIFESTS_CHANGED_METHOD => state.on_bazel_manifests_changed(),
        _ => Vec::new(),
    }
}

/// The response to a request other than `shutdown`, which the loop answers
/// itself: a completion, a hover, a definition, a document's outline, the quick fixes or a
/// preview or where one of its links leads, or the refusal of a method rinx does not support or of
/// parameters that do not parse.
pub fn handle_request(state: &mut ServerState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        HoverRequest::METHOD => match serde_json::from_value::<HoverParams>(params) {
            Ok(params) => {
                let position = params.text_document_position_params;
                Response::new_ok(
                    id,
                    state.hover(&position.text_document.uri, position.position),
                )
            }
            Err(error) => invalid_params(id, &method, &error),
        },
        GotoDefinition::METHOD => match serde_json::from_value::<GotoDefinitionParams>(params) {
            Ok(params) => {
                let position = params.text_document_position_params;
                Response::new_ok(
                    id,
                    state.definition(&position.text_document.uri, position.position),
                )
            }
            Err(error) => invalid_params(id, &method, &error),
        },
        Completion::METHOD => match serde_json::from_value::<CompletionParams>(params) {
            Ok(params) => {
                let position = params.text_document_position;
                Response::new_ok(
                    id,
                    state.complete(&position.text_document.uri, position.position),
                )
            }
            Err(error) => invalid_params(id, &method, &error),
        },
        DocumentSymbolRequest::METHOD => {
            match serde_json::from_value::<DocumentSymbolParams>(params) {
                Ok(params) => {
                    Response::new_ok(id, state.document_symbols(&params.text_document.uri))
                }
                Err(error) => invalid_params(id, &method, &error),
            }
        }
        CodeActionRequest::METHOD => match serde_json::from_value::<CodeActionParams>(params) {
            Ok(params) => {
                let actions: Vec<CodeActionOrCommand> = state
                    .code_actions(&params.text_document.uri, params.range)
                    .into_iter()
                    .map(CodeActionOrCommand::CodeAction)
                    .collect();
                Response::new_ok(id, actions)
            }
            Err(error) => invalid_params(id, &method, &error),
        },
        PREVIEW_METHOD => match serde_json::from_value::<PreviewParams>(params) {
            Ok(params) => {
                match state.preview(&params.text_document.uri, params.resource_root.as_deref()) {
                    Ok(html) => Response::new_ok(id, PreviewResult { html }),
                    Err(refusal) => {
                        let code = match refusal {
                            PreviewRefusal::NotOpen => ErrorCode::InvalidParams,
                            PreviewRefusal::Failed(_) => ErrorCode::RequestFailed,
                        };
                        Response::new_err(id, code as i32, refusal.to_string())
                    }
                }
            }
            Err(error) => invalid_params(id, &method, &error),
        },
        PREVIEW_LINK_METHOD => match serde_json::from_value::<PreviewLinkParams>(params) {
            Ok(params) => match state.preview_link(&params.text_document.uri, &params.href) {
                Ok(target) => Response::new_ok(id, target),
                Err(refusal) => {
                    Response::new_err(id, ErrorCode::InvalidParams as i32, refusal.to_string())
                }
            },
            Err(error) => invalid_params(id, &method, &error),
        },
        _ => Response::new_err(
            id,
            ErrorCode::MethodNotFound as i32,
            format!("rinx does not support '{method}'"),
        ),
    }
}

/// The refusal of a `method` request whose parameters did not parse.
fn invalid_params(id: RequestId, method: &str, error: &serde_json::Error) -> Response {
    Response::new_err(
        id,
        ErrorCode::InvalidParams as i32,
        format!("invalid '{method}' parameters: {error}"),
    )
}

/// A `window/showMessage` notification telling the user `message`, as
/// information rather than a warning or an error.
pub(super) fn show_information(message: String) -> Message {
    show_message(MessageType::INFO, message)
}

/// A `window/showMessage` notification warning the user of `message`.
pub(super) fn show_warning(message: String) -> Message {
    show_message(MessageType::WARNING, message)
}

fn show_message(typ: MessageType, message: String) -> Message {
    Notification::new(
        ShowMessage::METHOD.to_string(),
        ShowMessageParams { typ, message },
    )
    .into()
}

/// A `textDocument/publishDiagnostics` notification.
pub(super) fn publish_diagnostics(
    uri: Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
    version: Option<i32>,
) -> Message {
    Notification::new(
        PublishDiagnostics::METHOD.to_string(),
        PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        },
    )
    .into()
}

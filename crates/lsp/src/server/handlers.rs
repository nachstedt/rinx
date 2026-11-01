//! The pure handlers: from the server's state and one incoming message to the
//! messages to send back.

use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    Notification as _, PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentSymbolRequest, FoldingRangeRequest, GotoDefinition,
    HoverRequest, PrepareRenameRequest, References, Rename, Request as _,
    SemanticTokensFullRequest, SemanticTokensRangeRequest, WorkspaceSymbolRequest,
};
use lsp_types::{
    ClientCapabilities, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CompletionOptions, CompletionParams, DocumentSymbolParams,
    FoldingRangeParams, FoldingRangeProviderCapability, GotoDefinitionParams, HoverParams,
    HoverProviderCapability, InitializeResult, MessageType, OneOf, PublishDiagnosticsParams,
    ReferenceParams, RenameOptions, RenameParams, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensRangeParams, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, ShowMessageParams, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
    WorkspaceSymbolParams,
};
use serde::de::DeserializeOwned;

use super::bazel_manifests::BAZEL_MANIFESTS_CHANGED_METHOD;
use super::conf_runs::CONF_INTERPRETER_CHANGED_METHOD;
use super::state::ServerState;
use crate::position::PositionEncoding;
use crate::preview::{PREVIEW_METHOD, PreviewParams, PreviewRefusal, PreviewResult};
use crate::preview_link::{PREVIEW_LINK_METHOD, PreviewLinkParams};
use crate::semantic_tokens::legend;

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
                trigger_characters: Some(["`", "<", "/", ".", ":", " "].map(String::from).to_vec()),
                ..CompletionOptions::default()
            }),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            rename_provider: Some(rename_provider(capabilities)),
            document_symbol_provider: Some(OneOf::Left(true)),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            // The whole document's tokens or a range's, never a delta: a
            // document's tokens are a few kilobytes, sent once per edit.
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    legend: legend(),
                    range: Some(true),
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
            ),
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
/// itself: a completion, a hover, a definition, the uses of a definition, a
/// rename, a document's outline, folding ranges or coloured names, a search
/// of the workspace's definitions, the quick fixes, or a preview or where one
/// of its links leads — or the refusal of a method rinx does not support or
/// of parameters that do not parse.
pub fn handle_request(state: &mut ServerState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        HoverRequest::METHOD => answer(id, &method, params, |id, params: HoverParams| {
            let position = params.text_document_position_params;
            let hover = state.hover(&position.text_document.uri, position.position);
            Response::new_ok(id, hover)
        }),
        GotoDefinition::METHOD => {
            answer(id, &method, params, |id, params: GotoDefinitionParams| {
                let position = params.text_document_position_params;
                let definition = state.definition(&position.text_document.uri, position.position);
                Response::new_ok(id, definition)
            })
        }
        References::METHOD => answer(id, &method, params, |id, params: ReferenceParams| {
            let position = params.text_document_position;
            let uses = state.references(
                &position.text_document.uri,
                position.position,
                params.context.include_declaration,
            );
            Response::new_ok(id, uses)
        }),
        PrepareRenameRequest::METHOD | Rename::METHOD => answer_rename(state, id, &method, params),
        Completion::METHOD => answer(id, &method, params, |id, params: CompletionParams| {
            let position = params.text_document_position;
            let trigger = params
                .context
                .as_ref()
                .and_then(|context| context.trigger_character.as_deref());
            let items = state.complete(&position.text_document.uri, position.position, trigger);
            Response::new_ok(id, items)
        }),
        DocumentSymbolRequest::METHOD
        | FoldingRangeRequest::METHOD
        | SemanticTokensFullRequest::METHOD
        | SemanticTokensRangeRequest::METHOD => answer_document_view(state, id, &method, params),
        WorkspaceSymbolRequest::METHOD => {
            answer(id, &method, params, |id, params: WorkspaceSymbolParams| {
                Response::new_ok(id, state.workspace_symbols(&params.query))
            })
        }
        CodeActionRequest::METHOD => answer(id, &method, params, |id, params: CodeActionParams| {
            let actions: Vec<CodeActionOrCommand> = state
                .code_actions(&params.text_document.uri, params.range)
                .into_iter()
                .map(CodeActionOrCommand::CodeAction)
                .collect();
            Response::new_ok(id, actions)
        }),
        PREVIEW_METHOD => answer(id, &method, params, |id, params: PreviewParams| match state
            .preview(
                &params.text_document.uri,
                params.resource_root.as_deref(),
                params.source_lines,
            ) {
            Ok(html) => Response::new_ok(id, PreviewResult { html }),
            Err(refusal) => {
                let code = match refusal {
                    PreviewRefusal::NotOpen => ErrorCode::InvalidParams,
                    PreviewRefusal::Failed(_) => ErrorCode::RequestFailed,
                };
                Response::new_err(id, code as i32, refusal.to_string())
            }
        }),
        PREVIEW_LINK_METHOD => {
            answer(
                id,
                &method,
                params,
                |id, params: PreviewLinkParams| match state
                    .preview_link(&params.text_document.uri, &params.href)
                {
                    Ok(target) => Response::new_ok(id, target),
                    Err(refusal) => {
                        Response::new_err(id, ErrorCode::InvalidParams as i32, refusal.to_string())
                    }
                },
            )
        }
        _ => Response::new_err(
            id,
            ErrorCode::MethodNotFound as i32,
            format!("rinx does not support '{method}'"),
        ),
    }
}

/// The answer to a `textDocument/prepareRename` or `textDocument/rename`
/// request: its result, or the error saying why the rename is refused.
fn answer_rename(
    state: &mut ServerState,
    id: RequestId,
    method: &str,
    params: serde_json::Value,
) -> Response {
    if method == PrepareRenameRequest::METHOD {
        return answer(
            id,
            method,
            params,
            |id, params: TextDocumentPositionParams| match state
                .prepare_rename(&params.text_document.uri, params.position)
            {
                Ok(prepared) => Response::new_ok(id, prepared),
                Err(refusal) => refused(id, &refusal),
            },
        );
    }
    answer(id, method, params, |id, params: RenameParams| {
        let position = params.text_document_position;
        match state.rename(
            &position.text_document.uri,
            position.position,
            &params.new_name,
        ) {
            Ok(edit) => Response::new_ok(id, edit),
            Err(refusal) => refused(id, &refusal),
        }
    })
}

/// The response to the `method` request `id` asking for a view of a whole
/// open document — its outline, its folding ranges or its coloured names —
/// each read from the document's latest parse.
fn answer_document_view(
    state: &ServerState,
    id: RequestId,
    method: &str,
    params: serde_json::Value,
) -> Response {
    match method {
        DocumentSymbolRequest::METHOD => {
            answer(id, method, params, |id, params: DocumentSymbolParams| {
                Response::new_ok(id, state.document_symbols(&params.text_document.uri))
            })
        }
        FoldingRangeRequest::METHOD => {
            answer(id, method, params, |id, params: FoldingRangeParams| {
                Response::new_ok(id, state.folding_ranges(&params.text_document.uri))
            })
        }
        SemanticTokensFullRequest::METHOD => {
            answer(id, method, params, |id, params: SemanticTokensParams| {
                Response::new_ok(id, state.semantic_tokens(&params.text_document.uri, None))
            })
        }
        _ => answer(
            id,
            method,
            params,
            |id, params: SemanticTokensRangeParams| {
                Response::new_ok(
                    id,
                    state.semantic_tokens(&params.text_document.uri, Some(params.range)),
                )
            },
        ),
    }
}

/// How the server announces renaming to a client with `capabilities`: with
/// a `prepareRename` when the client asks one before a rename, so a refusal
/// is shown before the new name is typed.
fn rename_provider(capabilities: &ClientCapabilities) -> OneOf<bool, RenameOptions> {
    let prepares = capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.rename.as_ref())
        .and_then(|rename| rename.prepare_support)
        .unwrap_or(false);
    if prepares {
        OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })
    } else {
        OneOf::Left(true)
    }
}

/// The error answering a request whose rename is refused, saying why — what
/// an editor shows the author.
fn refused(id: RequestId, refusal: &crate::rename::RenameRefusal) -> Response {
    Response::new_err(id, ErrorCode::RequestFailed as i32, refusal.to_string())
}

/// The response to the `method` request `id`: what `respond` makes of its
/// `params`, read as `P` — or the refusal of parameters that do not read so.
fn answer<P: DeserializeOwned>(
    id: RequestId,
    method: &str,
    params: serde_json::Value,
    respond: impl FnOnce(RequestId, P) -> Response,
) -> Response {
    match serde_json::from_value::<P>(params) {
        Ok(params) => respond(id, params),
        Err(error) => Response::new_err(
            id,
            ErrorCode::InvalidParams as i32,
            format!("invalid '{method}' parameters: {error}"),
        ),
    }
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

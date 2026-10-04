//! Telling the client how far the workspace scan has got.
//!
//! Two channels, for two audiences:
//!
//! - **`rinx/status`**, a notification of our own, carries the state of the
//!   workspace index — indexing, or ready with how many documents in how long
//!   — for the extension's status bar item. Every client gets it; one that
//!   does not know the method ignores it, as the protocol requires.
//! - **`$/progress`** drives the editor's own progress indicator, but only for
//!   a client announcing `window.workDoneProgress`, and only once it has
//!   accepted the token the server asked to create: the protocol forbids
//!   reporting on a token before the client's reply.

use std::collections::BTreeMap;
use std::time::Duration;

use lsp_server::{Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{Notification as _, Progress};
use lsp_types::request::{Request as _, WorkDoneProgressCreate};
use lsp_types::{
    NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress, WorkDoneProgressBegin,
    WorkDoneProgressCreateParams, WorkDoneProgressEnd, WorkDoneProgressReport,
};
use serde::Serialize;

/// The method of the status notification.
pub const STATUS_METHOD: &str = "rinx/status";

/// The token the scan's progress is reported under.
const TOKEN: &str = "rinx/scan";

/// What the workspace index is doing, as `rinx/status` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexState {
    Indexing,
    Ready,
}

/// The parameters of `rinx/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub state: IndexState,
    /// The documents indexed so far — every one, once ready.
    pub documents: usize,
    /// How long the scan took, once ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

impl IndexStatus {
    /// The notification reporting this status.
    #[must_use]
    pub fn notification(&self) -> Message {
        Notification::new(STATUS_METHOD.to_string(), self).into()
    }
}

/// Where the progress token stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// The client did not announce `window.workDoneProgress`.
    Unsupported,
    /// Asked to create; waiting for the client's reply to this request.
    Requested(RequestId),
    /// Created, and the begin report sent.
    Begun,
    /// The client refused it, or the scan ended before it was created.
    Closed,
}

/// The `$/progress` reports of one workspace scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanProgress {
    token: Token,
    /// Documents done and in total, per workspace folder.
    counts: BTreeMap<usize, (usize, usize)>,
}

impl ScanProgress {
    /// The progress of a scan just started, and the messages starting it: a
    /// request to create the token, when the client supports progress, under
    /// `request_id`.
    #[must_use]
    pub fn start(supported: bool, request_id: RequestId) -> (Self, Vec<Message>) {
        if !supported {
            let progress = Self {
                token: Token::Unsupported,
                counts: BTreeMap::new(),
            };
            return (progress, Vec::new());
        }
        let request = Request::new(
            request_id.clone(),
            WorkDoneProgressCreate::METHOD.to_string(),
            WorkDoneProgressCreateParams { token: token() },
        );
        let progress = Self {
            token: Token::Requested(request_id),
            counts: BTreeMap::new(),
        };
        (progress, vec![request.into()])
    }

    /// The messages a reply from the client brings about: the begin report,
    /// when it is the reply creating the token. A reply to anything else is
    /// none of this.
    pub fn on_response(&mut self, response: &Response) -> Vec<Message> {
        if self.token != Token::Requested(response.id.clone()) {
            return Vec::new();
        }
        if response.response_result.is_err() {
            self.token = Token::Closed;
            return Vec::new();
        }
        self.token = Token::Begun;
        let (done, total) = self.totals();
        vec![report(WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: "Indexing".to_string(),
            cancellable: Some(false),
            message: Some(counted(done, total)),
            percentage: percentage(done, total),
        }))]
    }

    /// Records that `folder` has `done` of `total` documents scanned, and the
    /// report to send for it.
    pub fn on_progress(&mut self, folder: usize, done: usize, total: usize) -> Vec<Message> {
        self.counts.insert(folder, (done, total));
        if self.token != Token::Begun {
            return Vec::new();
        }
        let (done, total) = self.totals();
        vec![report(WorkDoneProgress::Report(WorkDoneProgressReport {
            cancellable: Some(false),
            message: Some(counted(done, total)),
            percentage: percentage(done, total),
        }))]
    }

    /// Ends the reports, with `documents` indexed in `elapsed`. A token not
    /// created yet is never begun: the scan is over before it could be.
    pub fn finish(&mut self, documents: usize, elapsed: Duration) -> Vec<Message> {
        let begun = self.token == Token::Begun;
        if self.token != Token::Unsupported {
            self.token = Token::Closed;
        }
        if !begun {
            return Vec::new();
        }
        vec![report(WorkDoneProgress::End(WorkDoneProgressEnd {
            message: Some(indexed(documents, elapsed)),
        }))]
    }

    /// Documents done and in total, over every folder reported so far.
    fn totals(&self) -> (usize, usize) {
        self.counts
            .values()
            .fold((0, 0), |(done, total), (d, t)| (done + d, total + t))
    }
}

/// The token every report names.
fn token() -> NumberOrString {
    NumberOrString::String(TOKEN.to_string())
}

/// A `$/progress` notification carrying `value`.
fn report(value: WorkDoneProgress) -> Message {
    Notification::new(
        Progress::METHOD.to_string(),
        ProgressParams {
            token: token(),
            value: ProgressParamsValue::WorkDone(value),
        },
    )
    .into()
}

/// `done` of `total` as a percentage, or `None` before the total is known.
fn percentage(done: usize, total: usize) -> Option<u32> {
    (total > 0).then(|| u32::try_from(done * 100 / total).unwrap_or(100))
}

/// `"12/512 documents"`.
fn counted(done: usize, total: usize) -> String {
    format!("{done}/{total} documents")
}

/// `"512 documents indexed in 0.8 s"` — what the status bar says too.
#[must_use]
pub fn indexed(documents: usize, elapsed: Duration) -> String {
    format!(
        "{documents} documents indexed in {:.1} s",
        elapsed.as_secs_f64()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `$/progress` value `message` carries.
    fn progress_value(message: &Message) -> WorkDoneProgress {
        let Message::Notification(notification) = message else {
            panic!("expected a notification, got {message:?}");
        };
        let params: ProgressParams = notification
            .clone()
            .extract(Progress::METHOD)
            .expect("progress parameters");
        let ProgressParamsValue::WorkDone(value) = params.value;
        value
    }

    fn ok(id: i32) -> Response {
        Response::new_ok(RequestId::from(id), serde_json::Value::Null)
    }

    #[test]
    fn test_start_asks_to_create_the_token_when_supported() {
        // When
        let (_, messages) = ScanProgress::start(true, RequestId::from(1));

        // Then
        let [Message::Request(request)] = &messages[..] else {
            panic!("expected one request, got {messages:?}");
        };
        assert_eq!(request.method, WorkDoneProgressCreate::METHOD);
        assert_eq!(request.id, RequestId::from(1));
    }

    #[test]
    fn test_an_unsupporting_client_gets_no_progress_at_all() {
        // Given
        let (mut progress, messages) = ScanProgress::start(false, RequestId::from(1));

        // When
        let reported = progress.on_progress(0, 1, 2);
        let finished = progress.finish(2, Duration::from_millis(5));

        // Then
        assert!(messages.is_empty());
        assert!(reported.is_empty());
        assert!(finished.is_empty());
    }

    #[test]
    fn test_nothing_is_reported_before_the_client_creates_the_token() {
        // Given
        let (mut progress, _) = ScanProgress::start(true, RequestId::from(1));

        // When
        let reported = progress.on_progress(0, 3, 10);

        // Then
        assert!(reported.is_empty());
    }

    #[test]
    fn test_creating_the_token_begins_at_the_counts_reached_so_far() {
        // Given
        let (mut progress, _) = ScanProgress::start(true, RequestId::from(1));
        progress.on_progress(0, 3, 10);

        // When
        let begun = progress.on_response(&ok(1));

        // Then
        let [message] = &begun[..] else {
            panic!("expected one report, got {begun:?}");
        };
        let WorkDoneProgress::Begin(opening) = progress_value(message) else {
            panic!("expected a begin report");
        };
        assert_eq!(opening.percentage, Some(30));
        assert_eq!(opening.message.as_deref(), Some("3/10 documents"));
    }

    #[test]
    fn test_a_begun_scan_reports_progress_over_every_folder_and_ends() {
        // Given
        let (mut progress, _) = ScanProgress::start(true, RequestId::from(1));
        progress.on_response(&ok(1));

        // When
        progress.on_progress(0, 10, 10);
        let reported = progress.on_progress(1, 5, 10);
        let finished = progress.finish(20, Duration::from_millis(800));

        // Then
        let WorkDoneProgress::Report(report) = progress_value(&reported[0]) else {
            panic!("expected a report");
        };
        assert_eq!(report.percentage, Some(75));
        let WorkDoneProgress::End(end) = progress_value(&finished[0]) else {
            panic!("expected an end report");
        };
        assert_eq!(
            end.message.as_deref(),
            Some("20 documents indexed in 0.8 s")
        );
    }

    #[test]
    fn test_a_refused_or_late_token_is_never_begun() {
        // Given
        let (mut refused, _) = ScanProgress::start(true, RequestId::from(1));
        let (mut late, _) = ScanProgress::start(true, RequestId::from(2));

        // When
        let refusal = refused.on_response(&Response::new_err(
            RequestId::from(1),
            lsp_server::ErrorCode::InternalError as i32,
            "no".to_string(),
        ));
        late.finish(3, Duration::ZERO);
        let after_end = late.on_response(&ok(2));

        // Then
        assert!(refusal.is_empty());
        assert!(refused.on_progress(0, 1, 2).is_empty());
        assert!(after_end.is_empty());
    }

    #[test]
    fn test_on_response_ignores_a_reply_to_another_request() {
        // Given
        let (mut progress, _) = ScanProgress::start(true, RequestId::from(1));

        // When / Then
        assert!(progress.on_response(&ok(9)).is_empty());
    }

    #[test]
    fn test_status_serializes_in_the_protocols_casing() {
        // Given
        let status = IndexStatus {
            state: IndexState::Ready,
            documents: 512,
            elapsed_ms: Some(800),
        };

        // When
        let json = serde_json::to_value(&status).expect("serializes");

        // Then
        assert_eq!(
            json,
            serde_json::json!({"state": "ready", "documents": 512, "elapsedMs": 800})
        );
    }
}

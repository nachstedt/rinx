//! Running the projects' `conf.py` where that is allowed (ADR-038 §4).
//!
//! The server reads a `conf.py` without running it; a project whose reading
//! fell short — a computed setting, one changed afterwards — is run instead,
//! when the server has an interpreter to run it in. Which one, if any, is not
//! the server's to decide: running a workspace's code needs the user's trust.
//!
//! - **The VS Code extension decides.** A client passing the `evaluateConf`
//!   initialization option is asked with a `rinx/confInterpreter` request,
//!   once per `conf.py`, and answers with an interpreter only in a trusted
//!   workspace whose settings allow running. When the answer may have
//!   changed — trust granted, a setting or the Python environment changed —
//!   it sends `rinx/confInterpreterChanged`, and every `conf.py` is asked
//!   about again.
//! - **`rinx lsp --check --python <exe>`** grants one interpreter for all.
//! - **Any other client** is never asked, and nothing runs.
//!
//! A run is a process on a thread of its own, so the loop answers the client
//! meanwhile; its result reaches the loop as an event and finds the folder's
//! projects again, which takes it over the reading wherever `conf.py` is
//! still the text it ran (see [`crate::project::discover_projects_with`]).
//! A `conf.py` edited since, or a project module it imported, is run again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, unbounded};
use lsp_server::{Message, Request, RequestId, Response};
use lsp_types::Uri;
use serde::{Deserialize, Serialize};

use super::state::ServerState;
use crate::project::{CONF_RUN_TIMEOUT, ConfEvaluation, ConfEvaluations, Project, run_conf};
use crate::uri::file_uri;

/// The request asking the client which interpreter runs a `conf.py`.
pub const CONF_INTERPRETER_METHOD: &str = "rinx/confInterpreter";

/// The notification saying the answers to `rinx/confInterpreter` may have
/// changed.
pub const CONF_INTERPRETER_CHANGED_METHOD: &str = "rinx/confInterpreterChanged";

/// The parameters of `rinx/confInterpreter`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfInterpreterParams {
    /// The `conf.py` to run — which a client may choose the interpreter by.
    pub conf: Uri,
}

/// The answer to `rinx/confInterpreter`: the interpreter to run `conf.py`
/// in, or why it is not run.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfInterpreterResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

impl ConfInterpreterResult {
    fn into_interpreter(self) -> Result<PathBuf, String> {
        match (self.path, self.unavailable) {
            (Some(path), _) => Ok(path),
            (None, Some(reason)) => Err(reason),
            (None, None) => Err("the editor named no interpreter".to_string()),
        }
    }
}

/// Where the server comes by an interpreter to run a `conf.py` in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConfExecution {
    /// Nowhere: no `conf.py` runs.
    #[default]
    Off,
    /// The client answers for each `conf.py`, as one passing the
    /// `evaluateConf` initialization option does.
    AskClient,
    /// Every `conf.py` runs in this one, as `rinx lsp --check --python`
    /// says.
    Granted(PathBuf),
}

/// A run's result, as it reaches the loop.
#[derive(Debug)]
pub struct ConfRunEvent {
    conf: PathBuf,
    /// The answers' generation the run was started in.
    generation: u64,
    evaluation: ConfEvaluation,
}

/// The `conf.py` runs the server has asked for, started, and finished.
#[derive(Debug)]
pub struct ConfRuns {
    execution: ConfExecution,
    /// What the client answered for each `conf.py`.
    interpreters: BTreeMap<PathBuf, Result<PathBuf, String>>,
    /// The `conf.py` each unanswered request asks about.
    in_flight: BTreeMap<RequestId, PathBuf>,
    /// The runs under way, each with the text of `conf.py` it started with.
    running: BTreeMap<PathBuf, String>,
    /// The latest finished run of each `conf.py`.
    evaluations: ConfEvaluations,
    /// Counts the times the answers may have changed, so a run started
    /// before is not taken.
    generation: u64,
    timeout: Duration,
    sender: Sender<ConfRunEvent>,
    receiver: Receiver<ConfRunEvent>,
}

impl Default for ConfRuns {
    fn default() -> Self {
        Self::new(ConfExecution::Off)
    }
}

impl ConfRuns {
    /// No run yet, interpreters coming from `execution`.
    #[must_use]
    pub fn new(execution: ConfExecution) -> Self {
        let (sender, receiver) = unbounded();
        Self {
            execution,
            interpreters: BTreeMap::new(),
            in_flight: BTreeMap::new(),
            running: BTreeMap::new(),
            evaluations: ConfEvaluations::new(),
            generation: 0,
            timeout: CONF_RUN_TIMEOUT,
            sender,
            receiver,
        }
    }

    /// These runs, each given up after `timeout`.
    #[cfg(test)]
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The interpreter `conf` runs in, why it does not run, or `None` while
    /// the client has not said.
    fn interpreter(&self, conf: &Path) -> Option<Result<PathBuf, String>> {
        match &self.execution {
            ConfExecution::Off => Some(Err("running `conf.py` is not enabled".to_string())),
            ConfExecution::Granted(interpreter) => Some(Ok(interpreter.clone())),
            ConfExecution::AskClient => self.interpreters.get(conf).cloned(),
        }
    }

    /// Whether the client is being asked about `conf`.
    fn asking(&self, conf: &Path) -> bool {
        self.in_flight.values().any(|asked| asked == conf)
    }

    /// Whether the latest run of `conf` ran `text`.
    fn is_current(&self, conf: &Path, text: &str) -> bool {
        self.evaluations
            .get(conf)
            .is_some_and(|evaluation| evaluation.conf_text == text)
    }

    /// The latest finished run of each `conf.py`.
    pub(super) fn evaluations(&self) -> &ConfEvaluations {
        &self.evaluations
    }

    /// Where the runs' results arrive.
    pub(super) fn events(&self) -> Receiver<ConfRunEvent> {
        self.receiver.clone()
    }

    /// The interpreter `conf`, holding `text`, was run in, when that run
    /// succeeded and is the one its settings come from.
    pub(super) fn executed_with(&self, conf: &Path, text: &str) -> Option<&Path> {
        self.evaluations
            .get(conf)
            .filter(|evaluation| evaluation.conf_text == text && evaluation.outcome.is_ok())
            .map(|evaluation| evaluation.interpreter.as_path())
    }

    /// Forgets every run that imported one of `paths`, as they changed —
    /// so the next finding of its project runs it again.
    fn forget_importing(&mut self, paths: &[PathBuf]) {
        self.evaluations.retain(|_, evaluation| {
            evaluation.outcome.as_ref().map_or(true, |values| {
                !values.imported.iter().any(|file| paths.contains(file))
            })
        });
    }

    /// Starts running `conf`, holding `text`, in `interpreter`.
    fn start(&mut self, conf: PathBuf, text: String, interpreter: PathBuf) {
        self.running.insert(conf.clone(), text.clone());
        let sender = self.sender.clone();
        let generation = self.generation;
        let timeout = self.timeout;
        thread::spawn(move || {
            let outcome = run_conf(&interpreter, &conf, timeout);
            // A loop that is gone has no use for the result.
            let _ = sender.send(ConfRunEvent {
                conf,
                generation,
                evaluation: ConfEvaluation {
                    conf_text: text,
                    interpreter,
                    outcome,
                },
            });
        });
    }
}

impl ServerState {
    /// Starts a run of every `conf.py` that reading fell short for, unless
    /// its latest run ran the same text or one is under way — and asks the
    /// client for the interpreter of each it has not been asked about,
    /// returning those requests.
    pub(super) fn run_confs(&mut self) -> Vec<Message> {
        let wanted: Vec<(PathBuf, String)> = self
            .projects
            .values()
            .filter(|project| project.wants_conf_run())
            .filter_map(|project| {
                Some((
                    project.conf_path()?.to_path_buf(),
                    project.conf_text().to_string(),
                ))
            })
            .collect();
        let mut messages = Vec::new();
        for (conf, text) in wanted {
            let runs = &self.conf_runs;
            if runs.is_current(&conf, &text) || runs.running.get(&conf) == Some(&text) {
                continue;
            }
            match runs.interpreter(&conf) {
                Some(Ok(interpreter)) => self.conf_runs.start(conf, text, interpreter),
                Some(Err(_)) => {}
                None if runs.asking(&conf) => {}
                None => {
                    let Some(uri) = file_uri(&conf) else {
                        continue;
                    };
                    let id = self.next_request_id();
                    self.conf_runs.in_flight.insert(id.clone(), conf);
                    let params = ConfInterpreterParams { conf: uri };
                    messages
                        .push(Request::new(id, CONF_INTERPRETER_METHOD.to_string(), params).into());
                }
            }
        }
        messages
    }

    /// The messages the answer `response` to a `rinx/confInterpreter`
    /// brings about — or `None` when it answers some other request. An
    /// error, or an answer that does not parse, leaves `conf.py` unrun.
    pub(super) fn on_conf_interpreter_response(
        &mut self,
        response: &Response,
    ) -> Option<Vec<Message>> {
        let conf = self.conf_runs.in_flight.remove(&response.id)?;
        let interpreter = match &response.response_result {
            Err(error) => Err(error.message.clone()),
            Ok(result) => serde_json::from_value::<ConfInterpreterResult>(result.clone())
                .map_err(|error| format!("the editor's answer did not parse: {error}"))
                .and_then(ConfInterpreterResult::into_interpreter),
        };
        self.conf_runs.interpreters.insert(conf, interpreter);
        Some(self.run_confs())
    }

    /// The messages a finished run brings about: its project found again,
    /// taking what it found. A run superseded — by an edit of `conf.py`, or
    /// a change of interpreter — is dropped.
    pub(super) fn on_conf_run(&mut self, event: ConfRunEvent) -> Vec<Message> {
        let runs = &mut self.conf_runs;
        if event.generation != runs.generation
            || runs.running.get(&event.conf) != Some(&event.evaluation.conf_text)
        {
            return Vec::new();
        }
        runs.running.remove(&event.conf);
        runs.evaluations
            .insert(event.conf.clone(), event.evaluation);
        let folder = self
            .projects
            .values()
            .find(|project| project.conf_path() == Some(event.conf.as_path()))
            .map(Project::folder);
        folder
            .map(|folder| self.reload_folder(folder))
            .unwrap_or_default()
    }

    /// The messages `rinx/confInterpreterChanged` brings about: every
    /// answer, and every run, forgotten — the projects that ran found again
    /// as read — and the client asked again.
    pub(super) fn on_conf_interpreter_changed(&mut self) -> Vec<Message> {
        let runs = &mut self.conf_runs;
        runs.generation += 1;
        runs.interpreters.clear();
        runs.in_flight.clear();
        runs.running.clear();
        let ran: BTreeSet<PathBuf> = std::mem::take(&mut runs.evaluations).into_keys().collect();
        let folders: BTreeSet<usize> = self
            .projects
            .values()
            .filter(|project| project.conf_path().is_some_and(|conf| ran.contains(conf)))
            .map(Project::folder)
            .collect();
        let mut messages: Vec<Message> = folders
            .into_iter()
            .flat_map(|folder| self.reload_folder(folder))
            .collect();
        messages.extend(self.run_confs());
        messages
    }

    /// Forgets every run that imported one of `paths`, as they changed on
    /// the disk; the projects are found again by the caller, since each
    /// such file is one of their configuration files.
    pub(super) fn forget_conf_runs_importing(&mut self, paths: &[PathBuf]) {
        self.conf_runs.forget_importing(paths);
    }

    /// Waits for every run under way, returning the messages their results
    /// bring about — for `rinx lsp --check`, which has no client to answer
    /// meanwhile, and for tests.
    pub(super) fn finish_conf_runs(&mut self) -> Vec<Message> {
        let mut messages = Vec::new();
        while !self.conf_runs.running.is_empty() {
            let Ok(event) = self.conf_runs.receiver.recv() else {
                break;
            };
            messages.extend(self.on_conf_run(event));
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_are_spelled_as_the_extension_reads_them() {
        // Given
        let params = ConfInterpreterParams {
            conf: "file:///work/docs/conf.py".parse().expect("a URI"),
        };

        // When / Then
        assert_eq!(
            serde_json::to_value(&params).unwrap(),
            serde_json::json!({"conf": "file:///work/docs/conf.py"})
        );
    }

    #[test]
    fn test_a_result_names_an_interpreter_or_why_there_is_none() {
        // Given
        let parse = |json: serde_json::Value| {
            serde_json::from_value::<ConfInterpreterResult>(json)
                .unwrap()
                .into_interpreter()
        };

        // When / Then
        assert_eq!(
            parse(serde_json::json!({"path": "/venv/bin/python"})),
            Ok(PathBuf::from("/venv/bin/python"))
        );
        assert_eq!(
            parse(serde_json::json!({"unavailable": "untrusted"})),
            Err("untrusted".to_string())
        );
        assert!(parse(serde_json::json!({})).is_err());
    }

    #[test]
    fn test_interpreter_follows_where_the_server_comes_by_one() {
        // Given
        let conf = Path::new("/work/docs/conf.py");
        let granted = ConfRuns::new(ConfExecution::Granted(PathBuf::from("/usr/bin/python3")));
        let mut asking = ConfRuns::new(ConfExecution::AskClient);

        // When / Then
        assert!(matches!(
            ConfRuns::default().interpreter(conf),
            Some(Err(_))
        ));
        assert_eq!(
            granted.interpreter(conf),
            Some(Ok(PathBuf::from("/usr/bin/python3")))
        );
        assert_eq!(asking.interpreter(conf), None);
        asking
            .interpreters
            .insert(conf.to_path_buf(), Err("untrusted".to_string()));
        assert_eq!(asking.interpreter(conf), Some(Err("untrusted".to_string())));
    }

    #[test]
    fn test_forget_importing_forgets_only_the_runs_that_imported_a_path() {
        // Given
        let mut runs = ConfRuns::default();
        let evaluation = |imported: &[&str]| ConfEvaluation {
            conf_text: String::new(),
            interpreter: PathBuf::from("python3"),
            outcome: Ok(crate::project::ConfValues {
                imported: imported.iter().map(PathBuf::from).collect(),
                ..crate::project::ConfValues::default()
            }),
        };
        runs.evaluations
            .insert(PathBuf::from("/a/conf.py"), evaluation(&["/a/version.py"]));
        runs.evaluations
            .insert(PathBuf::from("/b/conf.py"), evaluation(&[]));

        // When
        runs.forget_importing(&[PathBuf::from("/a/version.py")]);

        // Then
        assert_eq!(
            runs.evaluations.keys().collect::<Vec<_>>(),
            [Path::new("/b/conf.py")]
        );
    }
}

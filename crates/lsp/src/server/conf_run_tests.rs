//! Running `conf.py`: who is asked for an interpreter, what a run changes
//! about a project, and when it runs again. The interpreter is a shell
//! script standing in for Python, printing what `conf_dump.py` would.

use super::conf_runs::{
    CONF_INTERPRETER_CHANGED_METHOD, CONF_INTERPRETER_METHOD, ConfExecution, ConfRuns,
};
use super::test_support::*;
use lsp_types::FileChangeType;

/// A `conf.py` computing its `rst_epilog`, which reading it cannot see.
const COMPUTED: &str = "version = '3.14'\nrst_epilog = f'.. |version| replace:: {version}'\n";
/// What running [`COMPUTED`] prints.
const COMPUTED_OUTPUT: &str =
    r#"{"values": {"rst_epilog": "'.. |version| replace:: 3.14'"}, "imported": [], "error": null}"#;
const INDEX: &str = "Home\n====\n\nVersion |version|.\n";

/// An interpreter at a fresh path that counts its runs and prints what
/// [`Self::answer`] last said.
struct FakePython {
    path: PathBuf,
}

impl FakePython {
    fn new(name: &str, output: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!("rinx_lsp_fake_python_{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("mkdir");
        let path = directory.join("python");
        std::fs::write(
            &path,
            "#!/bin/sh\nhere=$(dirname \"$0\")\necho run >> \"$here/runs\"\ncat \"$here/output.json\"\n",
        )
        .expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let python = Self { path };
        python.answer(output);
        python
    }

    /// Makes every later run print `output`.
    fn answer(&self, output: &str) {
        std::fs::write(self.path.with_file_name("output.json"), output).expect("write");
    }

    /// How many times it has run.
    fn runs(&self) -> usize {
        std::fs::read_to_string(self.path.with_file_name("runs"))
            .map_or(0, |runs| runs.lines().count())
    }
}

/// A server over `workspace` coming by interpreters through `execution`,
/// with the scan's result taken — and the messages that brought about.
fn scanned_with(workspace: &Workspace, execution: ConfExecution) -> (ServerState, Vec<Message>) {
    let (state, _) = workspace.server(false);
    let mut state = state.with_conf_execution(execution);
    let messages = state.on_scan_event(workspace.scanned());
    (state, messages)
}

/// The `rinx/confInterpreter` requests among `messages`.
fn interpreter_requests(messages: &[Message]) -> Vec<Request> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Request(request) if request.method == CONF_INTERPRETER_METHOD => {
                Some(request.clone())
            }
            _ => None,
        })
        .collect()
}

/// The codes `conf.py` shows in `workspace`, each with its 0-based line.
fn conf_codes(state: &ServerState, workspace: &Workspace) -> Vec<(String, u32)> {
    state
        .conf_diagnostics_for(&workspace.uri("conf.py"))
        .into_iter()
        .map(|diagnostic| {
            let code = match diagnostic.code {
                Some(lsp_types::NumberOrString::String(code)) => code,
                _ => String::new(),
            };
            (code, diagnostic.range.start.line)
        })
        .collect()
}

/// The codes `index.rst` shows when opened in `state`.
fn index_codes(state: &mut ServerState, workspace: &Workspace) -> Vec<String> {
    let opened = handle_notification(state, workspace.open("index.rst", INDEX));
    workspace
        .codes_for(&opened, "index.rst")
        .unwrap_or_default()
}

#[test]
fn test_a_client_passing_the_option_is_asked_about_each_conf_py_reading_fell_short_for() {
    // Given one project computing a setting and one spelling every setting
    let workspace = Workspace::new(
        "conf_run_asked",
        &[
            ("conf.py", COMPUTED),
            ("literal/conf.py", "project = 'P'\nnumfig = True\n"),
        ],
    );

    // When
    let (_, messages) = scanned_with(&workspace, ConfExecution::AskClient);

    // Then only the first is asked about
    let requests = interpreter_requests(&messages);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].params,
        serde_json::json!({"conf": workspace.uri("conf.py").as_str()})
    );
}

#[test]
fn test_without_the_option_nothing_runs_and_the_reading_stands() {
    // Given
    let workspace = Workspace::new(
        "conf_run_off",
        &[("conf.py", COMPUTED), ("index.rst", INDEX)],
    );

    // When
    let (mut state, messages) = scanned_with(&workspace, ConfExecution::Off);

    // Then
    assert!(interpreter_requests(&messages).is_empty());
    assert_eq!(
        conf_codes(&state, &workspace),
        [("conf.unread-setting".to_string(), 1)]
    );
    assert_eq!(
        index_codes(&mut state, &workspace),
        ["substitution.undefined"]
    );
}

#[test]
fn test_a_run_takes_over_the_reading() {
    // Given
    let python = FakePython::new("takes_over", COMPUTED_OUTPUT);
    let workspace = Workspace::new(
        "conf_run_takes_over",
        &[("conf.py", COMPUTED), ("index.rst", INDEX)],
    );
    let (mut state, _) = scanned_with(&workspace, ConfExecution::Granted(python.path.clone()));

    // When
    let messages = state.finish_conf_runs();

    // Then the computed epilog defines the substitution, and the status
    // names the interpreter
    assert_eq!(python.runs(), 1);
    assert_eq!(conf_codes(&state, &workspace), []);
    assert_eq!(index_codes(&mut state, &workspace), Vec::<String>::new());
    let status = statuses(&messages).pop().expect("a status");
    assert_eq!(
        status["projects"][0],
        serde_json::json!({
            "kind": "sphinx",
            "conf": "conf.py",
            "executedWith": python.path.display().to_string(),
        })
    );
}

#[test]
fn test_the_interpreter_the_client_names_runs_conf_py() {
    // Given
    let python = FakePython::new("client_names", COMPUTED_OUTPUT);
    let workspace = Workspace::new("conf_run_client_names", &[("conf.py", COMPUTED)]);
    let (mut state, messages) = scanned_with(&workspace, ConfExecution::AskClient);
    let request = interpreter_requests(&messages).remove(0);

    // When
    let answered = state.on_response(&Response::new_ok(
        request.id,
        serde_json::json!({"path": python.path}),
    ));
    state.finish_conf_runs();

    // Then the answer starts the run, which nothing asks about again
    assert!(interpreter_requests(&answered).is_empty());
    assert_eq!(python.runs(), 1);
    assert_eq!(conf_codes(&state, &workspace), []);
}

#[test]
fn test_an_interpreter_the_client_withholds_leaves_the_reading() {
    // Given
    let workspace = Workspace::new("conf_run_withheld", &[("conf.py", COMPUTED)]);
    let (mut state, messages) = scanned_with(&workspace, ConfExecution::AskClient);
    let request = interpreter_requests(&messages).remove(0);

    // When
    let answered = state.on_response(&Response::new_ok(
        request.id,
        serde_json::json!({"unavailable": "the workspace is not trusted"}),
    ));

    // Then
    assert!(answered.is_empty());
    assert_eq!(state.finish_conf_runs().len(), 0);
    assert_eq!(
        conf_codes(&state, &workspace),
        [("conf.unread-setting".to_string(), 1)]
    );
}

#[test]
fn test_a_changed_answer_asks_again() {
    // Given an interpreter withheld
    let workspace = Workspace::new("conf_run_changed", &[("conf.py", COMPUTED)]);
    let (mut state, messages) = scanned_with(&workspace, ConfExecution::AskClient);
    let request = interpreter_requests(&messages).remove(0);
    state.on_response(&Response::new_ok(
        request.id,
        serde_json::json!({"unavailable": "untrusted"}),
    ));

    // When the workspace is trusted
    let changed = handle_notification(
        &mut state,
        Notification::new(
            CONF_INTERPRETER_CHANGED_METHOD.to_string(),
            serde_json::Value::Null,
        ),
    );

    // Then
    assert_eq!(interpreter_requests(&changed).len(), 1);
}

#[test]
fn test_a_changed_answer_forgets_what_a_run_found() {
    // Given a project that ran
    let python = FakePython::new("forgets", COMPUTED_OUTPUT);
    let workspace = Workspace::new("conf_run_forgets", &[("conf.py", COMPUTED)]);
    let (mut state, messages) = scanned_with(&workspace, ConfExecution::AskClient);
    let request = interpreter_requests(&messages).remove(0);
    state.on_response(&Response::new_ok(
        request.id,
        serde_json::json!({"path": python.path}),
    ));
    state.finish_conf_runs();

    // When the answer may have changed
    let changed = handle_notification(
        &mut state,
        Notification::new(
            CONF_INTERPRETER_CHANGED_METHOD.to_string(),
            serde_json::Value::Null,
        ),
    );

    // Then it reads as unrun until the client answers again
    assert_eq!(interpreter_requests(&changed).len(), 1);
    assert_eq!(
        conf_codes(&state, &workspace),
        [("conf.unread-setting".to_string(), 1)]
    );
}

#[test]
fn test_a_failed_run_is_reported_at_its_line_and_the_reading_stands() {
    // Given
    let python = FakePython::new(
        "fails",
        r#"{"values": {}, "imported": [], "error": {"message": "NameError: name 'x' is not defined", "line": 2}}"#,
    );
    let workspace = Workspace::new("conf_run_fails", &[("conf.py", COMPUTED)]);
    let (mut state, _) = scanned_with(&workspace, ConfExecution::Granted(python.path.clone()));

    // When
    state.finish_conf_runs();

    // Then
    let codes = conf_codes(&state, &workspace);
    assert_eq!(
        codes,
        [
            ("conf.execution-failed".to_string(), 1),
            ("conf.unread-setting".to_string(), 1),
        ]
    );
    let messages: Vec<String> = state
        .conf_diagnostics_for(&workspace.uri("conf.py"))
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert!(
        messages[0].contains("NameError: name 'x' is not defined"),
        "{messages:?}"
    );
}

#[test]
fn test_a_run_that_does_not_finish_in_time_fails() {
    // Given
    let python = FakePython::new("slow", COMPUTED_OUTPUT);
    std::fs::write(&python.path, "#!/bin/sh\nsleep 5\n").expect("write");
    let workspace = Workspace::new("conf_run_slow", &[("conf.py", COMPUTED)]);
    let (state, _) = workspace.server(false);
    let mut state = state.with_conf_runs(
        ConfRuns::new(ConfExecution::Granted(python.path.clone()))
            .with_timeout(Duration::from_millis(100)),
    );
    state.on_scan_event(workspace.scanned());

    // When
    state.finish_conf_runs();

    // Then
    assert!(
        conf_codes(&state, &workspace)
            .iter()
            .any(|(code, _)| code == "conf.execution-failed")
    );
}

#[test]
fn test_a_value_no_literal_spells_is_reported_on_its_assignment() {
    // Given
    let python = FakePython::new(
        "unrepresentable",
        r#"{"values": {"rst_epilog": "<Epilog object at 0x1>"}}"#,
    );
    let workspace = Workspace::new("conf_run_unrepresentable", &[("conf.py", COMPUTED)]);
    let (mut state, _) = scanned_with(&workspace, ConfExecution::Granted(python.path.clone()));

    // When
    state.finish_conf_runs();

    // Then
    assert_eq!(
        conf_codes(&state, &workspace),
        [("conf.unrepresentable-value".to_string(), 1)]
    );
}

#[test]
fn test_a_computed_exclusion_leaves_its_documents_out() {
    // Given
    let python = FakePython::new(
        "excludes",
        r#"{"values": {"exclude_patterns": "['drafts']"}}"#,
    );
    let workspace = Workspace::new(
        "conf_run_excludes",
        &[
            (
                "conf.py",
                "exclude_patterns = [name for name in ['drafts']]\n",
            ),
            ("index.rst", INDEX),
            ("drafts/wip.rst", "WIP\n===\n"),
        ],
    );
    let (mut state, _) = scanned_with(&workspace, ConfExecution::Granted(python.path.clone()));
    assert_eq!(
        state
            .projects
            .values()
            .map(crate::project::Project::document_count)
            .sum::<usize>(),
        2
    );

    // When
    state.finish_conf_runs();

    // Then
    let documents: Vec<String> = state
        .projects
        .values()
        .flat_map(|project| project.documents().map(|(name, _)| name.to_string()))
        .collect();
    assert_eq!(documents, ["index.rst"]);
}

#[test]
fn test_editing_conf_py_runs_it_again() {
    // Given a project that ran
    let python = FakePython::new("edited", COMPUTED_OUTPUT);
    let workspace = Workspace::new("conf_run_edited", &[("conf.py", COMPUTED)]);
    let (mut state, _) = scanned_with(&workspace, ConfExecution::Granted(python.path.clone()));
    state.finish_conf_runs();

    // When
    workspace.write("conf.py", &format!("{COMPUTED}numfig = True\n"));
    handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CHANGED)]),
    );
    state.finish_conf_runs();

    // Then
    assert_eq!(python.runs(), 2);
}

#[test]
fn test_editing_a_module_conf_py_imported_runs_it_again() {
    // Given a project whose run imported `version.py`
    let workspace = Workspace::new(
        "conf_run_imported",
        &[("conf.py", COMPUTED), ("version.py", "VERSION = '3.14'\n")],
    );
    let imported = workspace.root.join("version.py");
    let python = FakePython::new(
        "imported",
        &format!(
            r#"{{"values": {{"rst_epilog": "''"}}, "imported": [{}]}}"#,
            serde_json::to_string(&imported).expect("a path")
        ),
    );
    let (mut state, _) = scanned_with(&workspace, ConfExecution::Granted(python.path.clone()));
    state.finish_conf_runs();

    // When
    handle_notification(
        &mut state,
        workspace.watched(&[("version.py", FileChangeType::CHANGED)]),
    );
    state.finish_conf_runs();

    // Then
    assert_eq!(python.runs(), 2);
}

#[test]
fn test_an_unchanged_project_is_not_run_again() {
    // Given a project that ran
    let python = FakePython::new("unchanged", COMPUTED_OUTPUT);
    let workspace = Workspace::new(
        "conf_run_unchanged",
        &[("conf.py", COMPUTED), ("index.rst", INDEX)],
    );
    let (mut state, _) = scanned_with(&workspace, ConfExecution::Granted(python.path.clone()));
    state.finish_conf_runs();

    // When a document changes on the disk
    handle_notification(
        &mut state,
        workspace.watched(&[("index.rst", FileChangeType::CHANGED)]),
    );
    state.finish_conf_runs();

    // Then
    assert_eq!(python.runs(), 1);
}

//! Running a Sphinx project's `conf.py`, for the exact values of the settings
//! the static reader could not take as written (ADR-038 §4).
//!
//! Running is opt-in: the server runs a `conf.py` only with an interpreter
//! it was given — by the editor, which asks for Workspace Trust and the
//! user's setting, or by `rinx lsp --check --python` — and only when reading
//! it without running fell short (see [`super::sphinx_conf`]). The script,
//! `conf_dump.py`, is embedded here and run as `python -c`; it prints each
//! setting asked for as its `repr`, which is read back through the same
//! literal reader `conf.py`'s own text goes through. So a computed value is
//! checked exactly as a written one is, and one no literal spells is refused
//! by name rather than guessed at.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rinx_pyconf::{BoundValue, Value, read_module};
use serde::Deserialize;

use super::needs::conf_setting_names;

/// The script a run executes: `python -c <script> <conf.py> <names>`.
pub const CONF_DUMP_SCRIPT: &str = include_str!("conf_dump.py");

/// How long a run may take before it counts as failed: long enough for a
/// `conf.py` importing Sphinx and its extensions, short enough that one
/// waiting on the network is not waited for.
pub const CONF_RUN_TIMEOUT: Duration = Duration::from_secs(20);

/// How often a running `conf.py` is looked in on.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The settings the server reads from `conf.py` besides sphinx-needs' — the
/// ones [`super::sphinx_conf`]'s reader asks for.
const READ_SETTINGS: &[&str] = &[
    "root_doc",
    "master_doc",
    "source_suffix",
    "exclude_patterns",
    "templates_path",
    "html_static_path",
    "html_extra_path",
    "include_patterns",
    "primary_domain",
    "default_role",
    "rst_prolog",
    "rst_epilog",
    "numfig",
    "numfig_secnum_depth",
    "highlight_language",
    "extensions",
    "intersphinx_mapping",
    "intersphinx_cache_limit",
    "needs_from_toml",
    "needs_from_toml_table",
];

/// Every setting a run is asked for: each one the server reads.
#[must_use]
pub fn conf_settings() -> Vec<String> {
    READ_SETTINGS
        .iter()
        .map(|name| (*name).to_string())
        .chain(conf_setting_names().map(|(_, name)| name))
        .collect()
}

/// One setting as running `conf.py` left it.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutedValue {
    /// A value a literal spells, read from its `repr`.
    Literal(Value),
    /// A value no literal spells, with the `repr` it has.
    Unrepresentable(String),
}

/// What running `conf.py` found: the settings it binds, and the project's
/// own modules it imported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfValues {
    /// Each setting asked for that `conf.py` binds, by name.
    pub values: BTreeMap<String, ExecutedValue>,
    /// The files of the modules `conf.py` imported that are no part of the
    /// interpreter's installation — which change what it computes.
    pub imported: Vec<PathBuf>,
}

/// Why running `conf.py` gave no settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfFailure {
    pub message: String,
    /// The line of `conf.py` it failed at, when it failed in `conf.py`.
    pub line: Option<usize>,
}

impl ConfFailure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
        }
    }
}

/// What running `conf.py` came to.
pub type ConfOutcome = Result<ConfValues, ConfFailure>;

/// One run of a `conf.py`: the text it ran, and what that came to.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfEvaluation {
    /// The text of `conf.py` when the run started, which the run is the
    /// answer for — a `conf.py` edited since is run again.
    pub conf_text: String,
    /// The interpreter it ran in.
    pub interpreter: PathBuf,
    pub outcome: ConfOutcome,
}

/// The latest run of each `conf.py`, by its path.
pub type ConfEvaluations = BTreeMap<PathBuf, ConfEvaluation>;

/// Runs the `conf.py` at `conf` in `interpreter`, giving up after `timeout`.
pub fn run_conf(interpreter: &Path, conf: &Path, timeout: Duration) -> ConfOutcome {
    let names = serde_json::to_string(&conf_settings()).unwrap_or_default();
    let directory = conf.parent().unwrap_or_else(|| Path::new("."));
    let mut child = Command::new(interpreter)
        .arg("-c")
        .arg(CONF_DUMP_SCRIPT)
        .arg(conf)
        .arg(names)
        .current_dir(directory)
        // The editor running `conf.py` leaves no `__pycache__` behind.
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            ConfFailure::new(format!(
                "`{}` could not be started: {error}",
                interpreter.display()
            ))
        })?;
    // Read while the process runs, so a full pipe cannot stall it.
    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                // The readers are left to finish on their own: a process
                // `conf.py` started may hold the pipes open.
                return Err(ConfFailure::new(format!(
                    "`conf.py` did not finish within {} s",
                    timeout.as_secs()
                )));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                return Err(ConfFailure::new(format!(
                    "waiting for `conf.py` failed: {error}"
                )));
            }
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    if !status.success() {
        let stderr = stderr.join().unwrap_or_default();
        let last = stderr.lines().rev().find(|line| !line.trim().is_empty());
        return Err(ConfFailure::new(match last {
            Some(line) => format!(
                "`{}` failed ({status}): {}",
                interpreter.display(),
                line.trim()
            ),
            None => format!("`{}` failed ({status})", interpreter.display()),
        }));
    }
    read_conf_output(&stdout)
}

/// Reads `pipe` to its end on a thread of its own.
fn read_in_background(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    })
}

/// What `conf_dump.py` prints.
#[derive(Debug, Deserialize)]
struct Output {
    #[serde(default)]
    values: BTreeMap<String, String>,
    #[serde(default)]
    imported: Vec<PathBuf>,
    #[serde(default)]
    error: Option<OutputError>,
}

#[derive(Debug, Deserialize)]
struct OutputError {
    message: String,
    #[serde(default)]
    line: Option<usize>,
}

/// What the standard output `stdout` of a run says: its last line is the
/// script's, whatever `conf.py` wrote around it.
fn read_conf_output(stdout: &str) -> ConfOutcome {
    let Some(line) = stdout.lines().rev().find(|line| !line.trim().is_empty()) else {
        return Err(ConfFailure::new("running `conf.py` printed nothing"));
    };
    let output: Output = serde_json::from_str(line).map_err(|error| {
        ConfFailure::new(format!(
            "what running `conf.py` printed did not parse: {error}"
        ))
    })?;
    if let Some(error) = output.error {
        return Err(ConfFailure {
            message: error.message,
            line: error.line,
        });
    }
    Ok(ConfValues {
        values: output
            .values
            .into_iter()
            .map(|(name, spelling)| {
                let value = executed_value(&spelling);
                (name, value)
            })
            .collect(),
        imported: output.imported,
    })
}

/// [`read_conf_output`]'s values from `stdout`, for other modules' tests.
#[cfg(test)]
pub(super) fn read_output_for_tests(stdout: &str) -> ConfValues {
    read_conf_output(stdout).expect("a run's output")
}

/// [`executed_value`], for other modules' tests.
#[cfg(test)]
pub(super) fn executed_value_for_tests(spelling: &str) -> ExecutedValue {
    executed_value(spelling)
}

/// The value `spelling`, a `repr`, spells — read as `conf.py`'s own literals
/// are.
fn executed_value(spelling: &str) -> ExecutedValue {
    let module = read_module(&format!("value = {spelling}\n"));
    match module.binding("value").map(|binding| &binding.value) {
        Some(BoundValue::Literal(value))
            if module.error.is_none() && module.bindings.len() == 1 =>
        {
            ExecutedValue::Literal(value.clone())
        }
        _ => ExecutedValue::Unrepresentable(spelling.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_pyconf::ValueKind;
    use std::fs;

    #[test]
    fn test_conf_settings_asks_for_each_setting_once() {
        // When
        let names = conf_settings();

        // Then
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
        assert!(names.iter().any(|name| name == "rst_epilog"));
        assert!(names.iter().any(|name| name == "needs_types"));
    }

    #[test]
    fn test_executed_value_reads_a_repr_as_a_literal() {
        // When
        let value = executed_value("['a', ('b', None)]");

        // Then
        let ExecutedValue::Literal(value) = value else {
            panic!("a literal: {value:?}");
        };
        let ValueKind::List(items) = value.kind else {
            panic!("a list");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_executed_value_refuses_what_no_literal_spells() {
        // When / Then
        for spelling in [
            "<function f at 0x1>",
            "OrderedDict({'a': 1})",
            "1\nother = 2",
            "",
        ] {
            assert_eq!(
                executed_value(spelling),
                ExecutedValue::Unrepresentable(spelling.to_string()),
                "{spelling:?}"
            );
        }
    }

    #[test]
    fn test_read_conf_output_reads_the_last_line() {
        // Given
        let stdout = "noise\n{\"values\": {\"numfig\": \"True\"}, \"imported\": [\"/p/v.py\"], \"error\": null}\n\n";

        // When
        let values = read_conf_output(stdout).expect("read");

        // Then
        assert!(matches!(
            values.values["numfig"],
            ExecutedValue::Literal(Value {
                kind: ValueKind::Bool(true),
                ..
            })
        ));
        assert_eq!(values.imported, [PathBuf::from("/p/v.py")]);
    }

    #[test]
    fn test_read_conf_output_reports_the_error_conf_py_raised() {
        // Given
        let stdout =
            r#"{"values": {}, "imported": [], "error": {"message": "NameError: x", "line": 4}}"#;

        // When / Then
        assert_eq!(
            read_conf_output(stdout),
            Err(ConfFailure {
                message: "NameError: x".to_string(),
                line: Some(4),
            })
        );
    }

    #[test]
    fn test_read_conf_output_fails_on_nothing_or_no_json() {
        // When / Then
        assert!(read_conf_output("").is_err());
        assert!(read_conf_output("Traceback").is_err());
    }

    /// An executable shell script at a fresh path, running `body`.
    #[cfg(unix)]
    fn fake_interpreter(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!("rinx_lsp_conf_eval_{name}"));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("mkdir");
        let path = directory.join("python");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    #[cfg(unix)]
    #[test]
    fn test_run_conf_runs_the_script_in_conf_pys_directory() {
        // Given: an interpreter printing the directory it runs in and its
        // arguments' count, as the script would print settings
        let interpreter = fake_interpreter(
            "directory",
            r#"printf '{"values": {"here": "%s", "count": "%s"}}\n' "'$(pwd -P)'" "$#""#,
        );
        let conf = interpreter.with_file_name("conf.py");

        // When
        let values = run_conf(&interpreter, &conf, CONF_RUN_TIMEOUT).expect("ran");

        // Then
        let here = interpreter.parent().expect("a directory");
        let ExecutedValue::Literal(value) = &values.values["here"] else {
            panic!("a literal");
        };
        assert_eq!(
            value.as_str().map(Path::new),
            Some(fs::canonicalize(here).expect("exists").as_path())
        );
        let ExecutedValue::Literal(count) = &values.values["count"] else {
            panic!("a literal");
        };
        assert_eq!(count.kind, ValueKind::Int(4));
    }

    #[cfg(unix)]
    #[test]
    fn test_run_conf_reports_an_interpreter_that_fails() {
        // Given
        let interpreter = fake_interpreter("fails", "echo 'it broke' >&2\nexit 3");

        // When
        let failure = run_conf(
            &interpreter,
            &interpreter.with_file_name("conf.py"),
            CONF_RUN_TIMEOUT,
        )
        .expect_err("failed");

        // Then
        assert!(failure.message.contains("it broke"), "{}", failure.message);
    }

    #[cfg(unix)]
    #[test]
    fn test_run_conf_gives_up_after_the_timeout() {
        // Given
        let interpreter = fake_interpreter("slow", "sleep 5");

        // When
        let started = Instant::now();
        let failure = run_conf(
            &interpreter,
            &interpreter.with_file_name("conf.py"),
            Duration::from_millis(100),
        )
        .expect_err("timed out");

        // Then
        assert!(started.elapsed() < Duration::from_secs(4));
        assert!(
            failure.message.contains("did not finish"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn test_run_conf_reports_an_interpreter_that_does_not_exist() {
        // When
        let failure = run_conf(
            Path::new("/no/such/python"),
            Path::new("/no/such/conf.py"),
            CONF_RUN_TIMEOUT,
        )
        .expect_err("failed");

        // Then
        assert!(
            failure.message.contains("could not be started"),
            "{}",
            failure.message
        );
    }
}

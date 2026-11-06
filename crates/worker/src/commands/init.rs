//! The `init` subcommand: `rinx init --from-sphinx <dir>` writes the Bazel
//! build of the Sphinx project whose `conf.py` is in `<dir>` — its
//! `BUILD.bazel`, `rinx.toml`, an `entities.toml` for a sphinx-needs project,
//! the files `rst_prolog`/`rst_epilog` become, and the `MODULE.bazel` it
//! needs (roadmap #37).
//!
//! The plan is the language server's ([`rinx_lsp::init_from_sphinx`]), the
//! one the editor's "Migrate to rinx" action applies, so the two write the
//! same files. No file that exists is overwritten: a project with a
//! `BUILD.bazel` already is refused whole. What could not be carried over is
//! printed to stdout as JSON beside the files written, since a script
//! migrating a corpus acts on it — and `--dry-run` prints it without writing
//! anything, so such a script can act first.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rinx_lsp::{
    InitOptions, MigrationFinding, MigrationOptions, MigrationPlan, PlannedContent,
    init_from_sphinx,
};
use serde::Serialize;

use super::cli_args::{flag_groups, flag_value_opt};

/// What `rinx init` prints: the files it writes, those it would have to
/// overwrite, and what it leaves out.
#[derive(Debug, Serialize)]
struct InitReport<'a> {
    files: Vec<&'a Path>,
    conflicts: Vec<&'a Path>,
    findings: Vec<ReportedFinding<'a>>,
}

/// A finding with the sentence an author reads.
#[derive(Debug, Serialize)]
struct ReportedFinding<'a> {
    #[serde(flatten)]
    finding: &'a MigrationFinding,
    message: String,
}

/// What `plan` reports, or — when it is to be `written` — why it must not be.
///
/// # Errors
///
/// When the plan is to be written and would overwrite a file.
fn process_init(plan: &MigrationPlan, written: bool) -> Result<String> {
    let conflicts = plan.conflicts();
    if written && !conflicts.is_empty() {
        let names: Vec<String> = conflicts
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        bail!(
            "init: refusing to overwrite {}; move it aside to migrate",
            names.join(", ")
        );
    }
    let report = InitReport {
        files: plan.files.iter().map(|file| file.path.as_path()).collect(),
        conflicts,
        findings: plan
            .findings
            .iter()
            .map(|finding| ReportedFinding {
                finding,
                message: finding.message(),
            })
            .collect(),
    };
    Ok(serde_json::to_string_pretty(&report)?)
}

/// The options after `init --from-sphinx <dir>`.
fn read_options(args: &[String]) -> Result<InitOptions> {
    let parse_data = flag_groups(args, "--parse-data", 1)?
        .into_iter()
        .flatten()
        .map(|path| fs::canonicalize(&path).with_context(|| format!("init: cannot find '{path}'")))
        .collect::<Result<Vec<PathBuf>>>()?;
    Ok(InitOptions {
        python: flag_value_opt(args, "--python").map(PathBuf::from),
        migration: MigrationOptions {
            jinja: args.iter().any(|arg| arg == "--jinja"),
            parse_data,
        },
    })
}

/// `rinx init --from-sphinx <dir> [--python <exe>] [--jinja]
/// [--parse-data <file>]... [--dry-run]`.
///
/// # Errors
///
/// When `<dir>` is no Sphinx project, a file would be overwritten, or one
/// cannot be written.
pub(crate) fn cmd_init(args: &[String]) -> Result<()> {
    let Some(directory) = flag_value_opt(args, "--from-sphinx") else {
        bail!("init: --from-sphinx <dir> names the Sphinx project to migrate");
    };
    let directory =
        fs::canonicalize(&directory).with_context(|| format!("init: cannot find '{directory}'"))?;
    let plan = init_from_sphinx(&directory, &read_options(args)?)?;
    let write = !args.iter().any(|arg| arg == "--dry-run");
    let report = process_init(&plan, write)?;
    for file in plan.files.iter().filter(|_| write) {
        match &file.content {
            PlannedContent::Create(text) => fs::write(&file.path, text),
            PlannedContent::Append(text) => OpenOptions::new()
                .append(true)
                .open(&file.path)
                .and_then(|mut existing| existing.write_all(text.as_bytes())),
        }
        .with_context(|| format!("init: cannot write '{}'", file.path.display()))?;
    }
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_lsp::PlannedFile;

    fn plan(files: Vec<PlannedFile>, findings: Vec<MigrationFinding>) -> MigrationPlan {
        MigrationPlan { files, findings }
    }

    #[test]
    fn test_process_init_reports_the_files_and_each_finding_with_its_sentence() {
        // Given
        let plan = plan(
            vec![PlannedFile {
                path: std::env::temp_dir().join("rinx_init_report_never_written/BUILD.bazel"),
                content: PlannedContent::Create(String::new()),
            }],
            vec![MigrationFinding::Inventory {
                name: "python".to_string(),
            }],
        );

        // When
        let report: serde_json::Value =
            serde_json::from_str(&process_init(&plan, true).expect("a report")).unwrap();

        // Then
        assert_eq!(report["files"].as_array().unwrap().len(), 1);
        assert_eq!(report["conflicts"], serde_json::json!([]));
        assert_eq!(report["findings"][0]["kind"], "inventory");
        assert_eq!(report["findings"][0]["name"], "python");
        assert!(
            report["findings"][0]["message"]
                .as_str()
                .unwrap()
                .contains("rinx_inventory")
        );
    }

    #[test]
    fn test_process_init_refuses_to_overwrite_a_file() {
        // Given — a file that exists
        let existing = std::env::temp_dir().join("rinx_init_existing_build_file");
        fs::write(&existing, "").unwrap();
        let plan = plan(
            vec![PlannedFile {
                path: existing.clone(),
                content: PlannedContent::Create(String::new()),
            }],
            Vec::new(),
        );

        // When
        let error = process_init(&plan, true).unwrap_err();
        let dry_run: serde_json::Value =
            serde_json::from_str(&process_init(&plan, false).expect("a report")).unwrap();

        // Then — a dry run names the file instead
        assert!(
            error.to_string().contains("refusing to overwrite"),
            "{error}"
        );
        assert_eq!(
            dry_run["conflicts"],
            serde_json::json!([existing.to_string_lossy()])
        );
    }

    #[test]
    fn test_read_options_reads_every_flag() {
        // Given
        let file = std::env::temp_dir().join("rinx_init_parse_data_file.txt");
        fs::write(&file, "").unwrap();
        let args: Vec<String> = [
            "--from-sphinx",
            "docs",
            "--python",
            "/usr/bin/python3",
            "--jinja",
            "--parse-data",
            file.to_str().unwrap(),
        ]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();

        // When
        let options = read_options(&args).expect("options");

        // Then
        assert_eq!(options.python, Some(PathBuf::from("/usr/bin/python3")));
        assert!(options.migration.jinja);
        assert_eq!(
            options.migration.parse_data,
            [fs::canonicalize(&file).unwrap()]
        );
    }
}

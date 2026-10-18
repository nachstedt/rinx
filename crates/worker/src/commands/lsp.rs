//! The `lsp` subcommand: serves the language server over stdio, or — with
//! `--check <folder>` — prints what it would show for every document of a
//! folder, for measuring a corpus; `--python <exe>` after it runs each
//! `conf.py` the server could not read exactly, as an editor allowed to would.
//!
//! The server itself is the `rinx_lsp` crate, whose handlers are already
//! pure; what is left here is the one decision the command line makes —
//! which arguments it accepts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// What `rinx lsp` was asked to do.
#[derive(Debug, PartialEq, Eq)]
enum LspMode {
    /// Serve a client over standard input and output.
    Serve,
    /// Print, as JSON, what the server shows for every document of the
    /// workspace folder `folder` — running `conf.py` in `python`, if given.
    Check {
        folder: PathBuf,
        python: Option<PathBuf>,
    },
}

/// Reads the arguments after `lsp`.
///
/// Accepts `--stdio`, which clients such as `vscode-languageclient` append
/// when told the transport, and which names the only one this server has, or
/// `--check <folder>`, optionally followed by `--python <exe>`. Anything else is refused rather than ignored, so a
/// flag that a later version of the server would understand is not silently
/// dropped by this one.
fn read_lsp_args(args: &[String]) -> Result<LspMode> {
    match args {
        [flag, folder] if flag == "--check" => Ok(LspMode::Check {
            folder: PathBuf::from(folder),
            python: None,
        }),
        [flag, folder, python_flag, python] if flag == "--check" && python_flag == "--python" => {
            Ok(LspMode::Check {
                folder: PathBuf::from(folder),
                python: Some(PathBuf::from(python)),
            })
        }
        [flag, _, python_flag] if flag == "--check" && python_flag == "--python" => {
            bail!("lsp: --python needs the interpreter to run conf.py in")
        }
        [flag] if flag == "--check" => bail!("lsp: --check needs the folder to check"),
        _ => match args.iter().find(|arg| arg.as_str() != "--stdio") {
            Some(unknown) => {
                bail!("lsp: unknown argument '{unknown}'; the server speaks only stdio")
            }
            None => Ok(LspMode::Serve),
        },
    }
}

/// The JSON `rinx lsp --check` prints for the folder at `folder`, which must
/// exist, running `conf.py` in `python` if given.
fn process_lsp_check(folder: &Path, python: Option<&Path>) -> Result<String> {
    let root = std::fs::canonicalize(folder)
        .with_context(|| format!("lsp: cannot check '{}'", folder.display()))?;
    Ok(serde_json::to_string(&rinx_lsp::check_folder(
        &root, python,
    ))?)
}

pub(crate) fn cmd_lsp(args: &[String]) -> Result<()> {
    match read_lsp_args(args)? {
        LspMode::Serve => rinx_lsp::run_stdio(),
        LspMode::Check { folder, python } => {
            println!("{}", process_lsp_check(&folder, python.as_deref())?);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_read_lsp_args_serves_without_arguments() {
        // Given / When / Then
        assert_eq!(read_lsp_args(&[]).ok(), Some(LspMode::Serve));
    }

    #[test]
    fn test_read_lsp_args_serves_over_stdio() {
        // Given / When / Then
        assert_eq!(
            read_lsp_args(&args(&["--stdio"])).ok(),
            Some(LspMode::Serve)
        );
    }

    #[test]
    fn test_read_lsp_args_refuses_another_transport() {
        // Given / When
        let error = read_lsp_args(&args(&["--socket=9000"])).unwrap_err();

        // Then
        assert!(error.to_string().contains("--socket=9000"), "{error}");
    }

    #[test]
    fn test_read_lsp_args_reads_the_folder_to_check() {
        // Given / When / Then
        assert_eq!(
            read_lsp_args(&args(&["--check", "Doc"])).ok(),
            Some(LspMode::Check {
                folder: PathBuf::from("Doc"),
                python: None,
            })
        );
    }

    #[test]
    fn test_read_lsp_args_reads_the_interpreter_to_run_conf_py_in() {
        // Given / When / Then
        assert_eq!(
            read_lsp_args(&args(&["--check", "Doc", "--python", "python3"])).ok(),
            Some(LspMode::Check {
                folder: PathBuf::from("Doc"),
                python: Some(PathBuf::from("python3")),
            })
        );
    }

    #[test]
    fn test_read_lsp_args_refuses_python_without_an_interpreter() {
        // Given / When
        let error = read_lsp_args(&args(&["--check", "Doc", "--python"])).unwrap_err();

        // Then
        assert!(
            error.to_string().contains("needs the interpreter"),
            "{error}"
        );
    }

    #[test]
    fn test_read_lsp_args_refuses_check_without_a_folder() {
        // Given / When
        let error = read_lsp_args(&args(&["--check"])).unwrap_err();

        // Then
        assert!(error.to_string().contains("needs the folder"), "{error}");
    }

    #[test]
    fn test_process_lsp_check_counts_the_folders_documents() {
        // Given
        let root = std::env::temp_dir().join("rinx_lsp_check_command");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(root.join("index.rst"), "Home\n====\n\n.. nope::\n").expect("write");

        // When
        let json = process_lsp_check(&root, None).expect("checked");

        // Then
        let check: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        assert_eq!(check["documents"], 1);
        let published = check["published"].as_object().expect("an object");
        let diagnostics: Vec<&serde_json::Value> = published
            .values()
            .flat_map(|d| d.as_array().expect("a list"))
            .collect();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["code"], "directive.unknown");
    }

    #[test]
    fn test_process_lsp_check_refuses_a_missing_folder() {
        // Given / When
        let error = process_lsp_check(Path::new("/no/such/folder"), None).unwrap_err();

        // Then
        assert!(error.to_string().contains("/no/such/folder"), "{error}");
    }
}

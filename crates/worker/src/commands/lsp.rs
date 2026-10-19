//! The `lsp` subcommand: serves the language server over stdio, or — with
//! `--check <folder>` — prints what it would show for every document of a
//! folder, for measuring a corpus; `--python <exe>` after it runs each
//! `conf.py` the server could not read exactly, as an editor allowed to
//! would, and each `--manifest <file>` reads a Bazel site from the manifest
//! its `lsp_manifest` output group wrote, as the VS Code extension builds.
//!
//! The server itself is the `rinx_lsp` crate, whose handlers are already
//! pure; what is left here is the one decision the command line makes —
//! which arguments it accepts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rinx_lsp::CheckOptions;

/// What `rinx lsp` was asked to do.
#[derive(Debug, PartialEq, Eq)]
enum LspMode {
    /// Serve a client over standard input and output.
    Serve,
    /// Print, as JSON, what the server shows for every document of the
    /// workspace folder `folder`, granting it what `options` grants.
    Check {
        folder: PathBuf,
        options: CheckOptions,
    },
}

/// Reads the arguments after `lsp`.
///
/// Accepts `--stdio`, which clients such as `vscode-languageclient` append
/// when told the transport, and which names the only one this server has, or
/// `--check <folder>`, followed by any of `--python <exe>` (once) and
/// `--manifest <file>` (each a site). Anything else is refused rather than
/// ignored, so a flag that a later version of the server would understand is
/// not silently dropped by this one.
fn read_lsp_args(args: &[String]) -> Result<LspMode> {
    match args {
        [flag, rest @ ..] if flag == "--check" => {
            let Some((folder, rest)) = rest.split_first() else {
                bail!("lsp: --check needs the folder to check");
            };
            Ok(LspMode::Check {
                folder: PathBuf::from(folder),
                options: read_check_options(rest)?,
            })
        }
        _ => match args.iter().find(|arg| arg.as_str() != "--stdio") {
            Some(unknown) => {
                bail!("lsp: unknown argument '{unknown}'; the server speaks only stdio")
            }
            None => Ok(LspMode::Serve),
        },
    }
}

/// Reads the flags after `--check <folder>`.
fn read_check_options(mut args: &[String]) -> Result<CheckOptions> {
    let mut options = CheckOptions::default();
    while let [flag, rest @ ..] = args {
        let Some((value, rest)) = rest.split_first() else {
            match flag.as_str() {
                "--python" => bail!("lsp: --python needs the interpreter to run conf.py in"),
                "--manifest" => bail!("lsp: --manifest needs the manifest file to read"),
                _ => bail!("lsp: unknown argument '{flag}' after --check"),
            }
        };
        match flag.as_str() {
            "--python" if options.python.is_none() => options.python = Some(PathBuf::from(value)),
            "--python" => bail!("lsp: --python is given twice"),
            "--manifest" => options.manifests.push(absolute(Path::new(value))?),
            _ => bail!("lsp: unknown argument '{flag}' after --check"),
        }
        args = rest;
    }
    Ok(options)
}

/// `path`, made absolute against the working directory, as the server
/// names every file by its absolute path.
fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).with_context(|| format!("lsp: cannot resolve '{}'", path.display()))
}

/// The JSON `rinx lsp --check` prints for the folder at `folder`, which must
/// exist, with what `options` grants.
fn process_lsp_check(folder: &Path, options: &CheckOptions) -> Result<String> {
    let root = std::fs::canonicalize(folder)
        .with_context(|| format!("lsp: cannot check '{}'", folder.display()))?;
    Ok(serde_json::to_string(&rinx_lsp::check_folder(
        &root, options,
    ))?)
}

pub(crate) fn cmd_lsp(args: &[String]) -> Result<()> {
    match read_lsp_args(args)? {
        LspMode::Serve => rinx_lsp::run_stdio(),
        LspMode::Check { folder, options } => {
            println!("{}", process_lsp_check(&folder, &options)?);
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
                options: CheckOptions::default(),
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
                options: CheckOptions {
                    python: Some(PathBuf::from("python3")),
                    manifests: Vec::new(),
                },
            })
        );
    }

    #[test]
    fn test_read_lsp_args_reads_every_manifest_beside_the_interpreter() {
        // Given / When
        let mode = read_lsp_args(&args(&[
            "--check",
            "Doc",
            "--manifest",
            "/out/a.json",
            "--python",
            "python3",
            "--manifest",
            "/out/b.json",
        ]))
        .ok();

        // Then
        assert_eq!(
            mode,
            Some(LspMode::Check {
                folder: PathBuf::from("Doc"),
                options: CheckOptions {
                    python: Some(PathBuf::from("python3")),
                    manifests: vec![PathBuf::from("/out/a.json"), PathBuf::from("/out/b.json")],
                },
            })
        );
    }

    #[test]
    fn test_read_lsp_args_makes_a_manifest_absolute() {
        // Given / When
        let Ok(LspMode::Check { options, .. }) =
            read_lsp_args(&args(&["--check", "Doc", "--manifest", "site.json"]))
        else {
            panic!("a check");
        };

        // Then
        assert!(options.manifests[0].is_absolute());
        assert!(options.manifests[0].ends_with("site.json"));
    }

    #[test]
    fn test_read_lsp_args_refuses_manifest_without_a_file() {
        // Given / When
        let error = read_lsp_args(&args(&["--check", "Doc", "--manifest"])).unwrap_err();

        // Then
        assert!(error.to_string().contains("needs the manifest"), "{error}");
    }

    #[test]
    fn test_read_lsp_args_refuses_an_unknown_flag_and_a_second_interpreter() {
        // Given / When
        let unknown = read_lsp_args(&args(&["--check", "Doc", "--fast", "x"])).unwrap_err();
        let twice = read_lsp_args(&args(&["--check", "Doc", "--python", "a", "--python", "b"]))
            .unwrap_err();

        // Then
        assert!(unknown.to_string().contains("--fast"), "{unknown}");
        assert!(twice.to_string().contains("twice"), "{twice}");
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
        let json = process_lsp_check(&root, &CheckOptions::default()).expect("checked");

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
        let error =
            process_lsp_check(Path::new("/no/such/folder"), &CheckOptions::default()).unwrap_err();

        // Then
        assert!(error.to_string().contains("/no/such/folder"), "{error}");
    }
}

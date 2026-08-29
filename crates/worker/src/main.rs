//! Rusty Sphinx Application Entry Point
//!
//! Supports a subcommand-based CLI for Bazel phase integration as well as a
//! legacy single-file mode for quick manual testing.
//!
//! # Subcommands (Bazel phases)
//!
//! ```text
//! rusty-sphinx parse  --input <file.rst>  --output <file.ast>  [--default-domain <py|c>]
//! rusty-sphinx validate_toctree --input <file.ast>  [--allowed <path>...]
//! rusty-sphinx index  --inputs <a.ast> [<b.ast> ...]  --output <project.index>
//! rusty-sphinx render --input <file.ast>  --index <project.index> --doc-path <rel_path> --output <file.html> [--strict-links] [--warnings-output <file.warnings.json>]
//! rusty-sphinx genindex --index <project.index> --output <genindex.html> --config <config.toml> --template <template.html>
//! rusty-sphinx extract_doctests --input <file.ast> --output <file.doctests.json>
//! rusty-sphinx validate_images --inputs <a.ast> [<b.ast> ...] --image-dir <dir>
//! ```
//!
//! # Legacy mode (quick preview)
//!
//! ```text
//! rusty-sphinx <file.rst>    # prints HTML to stdout
//! ```

mod commands;

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_worker::process_rst;
use std::env;
use std::fs;

use commands::{
    cmd_extract_diagrams, cmd_extract_doctests, cmd_genindex, cmd_index, cmd_parse, cmd_preview,
    cmd_render, cmd_validate_images, cmd_validate_toctree,
};

fn cmd_legacy(path: &str) -> Result<()> {
    let rst = fs::read_to_string(path).with_context(|| format!("Error reading '{path}'"))?;
    let html = process_rst(path, &rst);
    print!("{html}");
    Ok(())
}

fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("parse") => cmd_parse(&args[2..]),
        Some("validate_toctree") => cmd_validate_toctree(&args[2..]),
        Some("extract_diagrams") => cmd_extract_diagrams(&args[2..]),
        Some("extract_doctests") => cmd_extract_doctests(&args[2..]),
        Some("validate_images") => cmd_validate_images(&args[2..]),
        Some("index") => cmd_index(&args[2..]),
        Some("render") => cmd_render(&args[2..]),
        Some("genindex") => cmd_genindex(&args[2..]),
        Some("preview") => cmd_preview(&args[2..]),
        Some(path) if !path.starts_with('-') => cmd_legacy(path),
        _ => {
            let program = args.first().map_or("rusty-sphinx", String::as_str);
            let msg = format!(
                "Usage:\n\
                   {program} <file.rst>                                   (legacy preview)\n\
                   {program} parse  --input <file.rst> --output <file.ast> [--default-domain <py|c>]\n\
                   {program} extract_diagrams --input <file.ast> --outdir <puml_dir>\n\
                   {program} extract_doctests --input <file.ast> --output <file.doctests.json>\n\
                   {program} validate_toctree --input <file.ast.raw> --output <file.ast> [--allowed <path>...]\n\
                   {program} index  --inputs <a.ast> [<b.ast> ...] --output <project.index>\n\
                   {program} render --input <file.ast> --index <project.index> --doc-path <rel_path> --output <file.html> --config <config.toml> --template <template.html> [--strict-links] [--warnings-output <file.warnings.json>]\n\
                   {program} genindex --index <project.index> --output <genindex.html> --config <config.toml> --template <template.html>\n\
                   {program} preview --doc-path <rel_path> --config <config.toml> --template <template.html> [--index <project.index>] [--default-domain <py|c>]\n\
                   {program} validate_images --inputs <a.ast> [<b.ast> ...] --image-dir <dir>"
            );
            Err(anyhow!(msg))
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(err) = run(&args) {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}

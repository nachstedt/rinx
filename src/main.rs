//! Rusty Sphinx Application Entry Point
//!
//! Supports a subcommand-based CLI for Bazel phase integration as well as a
//! legacy single-file mode for quick manual testing.
//!
//! # Subcommands (Bazel phases)
//!
//! ```text
//! rusty-sphinx parse  --input <file.rst>  --output <file.ast>
//! rusty-sphinx index  --inputs <a.ast> [<b.ast> ...]  --output <project.index>
//! rusty-sphinx render --input <file.ast>  --index <project.index>  --output <file.html>
//! ```
//!
//! # Legacy mode (quick preview)
//!
//! ```text
//! rusty-sphinx <file.rst>    # prints HTML to stdout
//! ```

use rusty_sphinx::{analyzer, ast, parser, process_rst, renderer};
use std::env;
use std::fs;

use std::process;

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Error reading '{path}': {err}");
        process::exit(1);
    })
}

fn write_file(path: &str, contents: &str) {
    fs::write(path, contents).unwrap_or_else(|err| {
        eprintln!("Error writing '{path}': {err}");
        process::exit(1);
    });
}

// ── Subcommand: parse ────────────────────────────────────────────────────────

/// `rusty-sphinx parse --input <file.rst> --output <file.ast>`
fn cmd_parse(args: &[String]) {
    let input = flag_value(args, "--input");
    let output = flag_value(args, "--output");

    let rst = read_file(&input);
    let doc = parser::parse(&rst);
    let json = serde_json::to_string(&doc).unwrap_or_else(|err| {
        eprintln!("Serialization error: {err}");
        process::exit(1);
    });
    write_file(&output, &json);
}

// ── Subcommand: index ────────────────────────────────────────────────────────

/// `rusty-sphinx index --inputs <a.ast> [<b.ast> ...] --output <project.index>`
fn cmd_index(args: &[String]) {
    let output = flag_value(args, "--output");
    let inputs = flag_values(args, "--inputs");

    let docs: Vec<ast::Document> = inputs
        .iter()
        .map(|p| {
            let json = read_file(p);
            serde_json::from_str(&json).unwrap_or_else(|err| {
                eprintln!("Failed to deserialize '{p}': {err}");
                process::exit(1);
            })
        })
        .collect();

    let index = analyzer::analyze_many(&docs);
    let json = serde_json::to_string(&index).unwrap_or_else(|err| {
        eprintln!("Serialization error: {err}");
        process::exit(1);
    });
    write_file(&output, &json);
}

// ── Subcommand: render ───────────────────────────────────────────────────────

/// `rusty-sphinx render --input <file.ast> --index <project.index> --output <file.html>`
fn cmd_render(args: &[String]) {
    let input = flag_value(args, "--input");
    let index_path = flag_value(args, "--index");
    let output = flag_value(args, "--output");

    let doc: ast::Document = {
        let json = read_file(&input);
        serde_json::from_str(&json).unwrap_or_else(|err| {
            eprintln!("Failed to deserialize '{input}': {err}");
            process::exit(1);
        })
    };

    let index: analyzer::ProjectIndex = {
        let json = read_file(&index_path);
        serde_json::from_str(&json).unwrap_or_else(|err| {
            eprintln!("Failed to deserialize '{index_path}': {err}");
            process::exit(1);
        })
    };

    let html = renderer::render(&doc, &index);
    write_file(&output, &html);
}

// ── Legacy mode ──────────────────────────────────────────────────────────────

/// `rusty-sphinx <file.rst>` – quick preview, prints HTML to stdout.
fn cmd_legacy(path: &str) {
    let rst = read_file(path);
    let html = process_rst(&rst);
    print!("{html}");
}

// ── Argument parsing helpers ─────────────────────────────────────────────────

fn flag_value(args: &[String], flag: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w.get(1).cloned())
        .unwrap_or_else(|| {
            eprintln!("Missing required flag '{flag}'");
            process::exit(1);
        })
}

/// Returns all values that follow `flag` until the next flag (starting with `--`).
fn flag_values(args: &[String], flag: &str) -> Vec<String> {
    let start = args.iter().position(|a| a == flag).unwrap_or_else(|| {
        eprintln!("Missing required flag '{flag}'");
        process::exit(1);
    });
    args[start + 1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .cloned()
        .collect()
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("parse") => cmd_parse(&args[2..]),
        Some("index") => cmd_index(&args[2..]),
        Some("render") => cmd_render(&args[2..]),
        Some(path) if !path.starts_with('-') => cmd_legacy(path),
        _ => {
            let program = &args[0];
            eprintln!(
                "Usage:\n\
                   {program} <file.rst>                                   (legacy preview)\n\
                   {program} parse  --input <file.rst> --output <file.ast>\n\
                   {program} index  --inputs <a.ast> [<b.ast> ...] --output <project.index>\n\
                   {program} render --input <file.ast> --index <project.index> --output <file.html>"
            );
            process::exit(1);
        }
    }
}

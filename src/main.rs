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

use anyhow::{Context, Result, anyhow};
use rusty_sphinx::{analyzer, ast, parser, process_rst, renderer};
use std::env;
use std::fs;

// ── Pure functions for arguments and logic ───────────────────────────────────

fn flag_value(args: &[String], flag: &str) -> Result<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w.get(1).cloned())
        .ok_or_else(|| anyhow!("Missing required flag '{flag}'"))
}

/// Returns all values that follow `flag` until the next flag (starting with `--`).
fn flag_values(args: &[String], flag: &str) -> Result<Vec<String>> {
    let start = args
        .iter()
        .position(|a| a == flag)
        .ok_or_else(|| anyhow!("Missing required flag '{flag}'"))?;

    let values: Vec<String> = args[start + 1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .cloned()
        .collect();

    Ok(values)
}

fn process_parse(path: &str, rst_content: &str) -> Result<String> {
    let doc = parser::parse(path, rst_content);
    serde_json::to_string(&doc).context("Serialization error")
}

fn process_index(ast_jsons: &[String]) -> Result<String> {
    let docs: Vec<ast::Document> = ast_jsons
        .iter()
        .map(|json| serde_json::from_str(json).context("Failed to deserialize AST"))
        .collect::<Result<_>>()?;

    let index = analyzer::analyze_many(&docs);
    serde_json::to_string(&index).context("Serialization error")
}

fn process_render(ast_json: &str, index_json: &str) -> Result<String> {
    let doc: ast::Document =
        serde_json::from_str(ast_json).context("Failed to deserialize AST document")?;
    let index: analyzer::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;

    Ok(renderer::render(&doc, &index))
}

// ── Subcommand Handlers (with IO) ────────────────────────────────────────────

fn cmd_parse(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;

    let rst = fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let json = process_parse(&input, &rst)?;
    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_index(args: &[String]) -> Result<()> {
    let output = flag_value(args, "--output")?;
    let inputs = flag_values(args, "--inputs")?;

    let files: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    let json = process_index(&files)?;
    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_render(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let index_path = flag_value(args, "--index")?;
    let output = flag_value(args, "--output")?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let index_json =
        fs::read_to_string(&index_path).with_context(|| format!("Error reading '{index_path}'"))?;

    let html = process_render(&ast_json, &index_json)?;
    fs::write(&output, html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_legacy(path: &str) -> Result<()> {
    let rst = fs::read_to_string(path).with_context(|| format!("Error reading '{path}'"))?;
    let html = process_rst(path, &rst);
    print!("{html}");
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("parse") => cmd_parse(&args[2..]),
        Some("index") => cmd_index(&args[2..]),
        Some("render") => cmd_render(&args[2..]),
        Some(path) if !path.starts_with('-') => cmd_legacy(path),
        _ => {
            let program = args.first().map_or("rusty-sphinx", String::as_str);
            let msg = format!(
                "Usage:\n\
                   {program} <file.rst>                                   (legacy preview)\n\
                   {program} parse  --input <file.rst> --output <file.ast>\n\
                   {program} index  --inputs <a.ast> [<b.ast> ...] --output <project.index>\n\
                   {program} render --input <file.ast> --index <project.index> --output <file.html>"
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_value_returns_value_when_flag_exists() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_value_returns_error_when_flag_is_missing() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value(&args, "--output");

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_flag_value_returns_error_when_value_is_missing() {
        // Given
        let args = vec!["--input".to_string()];

        // When
        let result = flag_value(&args, "--input");

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_flag_values_returns_list_of_values() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "a.ast".to_string(),
            "b.ast".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        assert_eq!(result.unwrap(), vec!["a.ast", "b.ast"]);
    }

    #[test]
    fn test_flag_values_returns_empty_list_when_no_values_follow_flag() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_flag_values_returns_error_when_flag_is_missing() {
        // Given
        let args = vec!["--output".to_string(), "c.idx".to_string()];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_process_parse_returns_serialized_ast() {
        // Given
        let rst = "Title\n=====";

        // When
        let json = process_parse("team_a/index.rst", rst).unwrap();

        // Then
        assert!(json.contains("Title"));
        assert!(json.contains(r#""path":"team_a/index.rst""#));
    }

    #[test]
    fn test_process_index_returns_serialized_project_index() {
        // Given
        let docs = vec![r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":"Title"}}]}"#.to_string()];

        // When
        let index = process_index(&docs).unwrap();

        // Then
        assert_eq!(index, r#"{"targets":{}}"#);
    }

    #[test]
    fn test_process_render_returns_html_string() {
        // Given
        let doc = r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":"Title"}}]}"#;
        let index = r#"{"targets":{}}"#;

        // When
        let html = process_render(doc, index).unwrap();

        // Then
        assert_eq!(html, "<h1>Title</h1>\n");
    }
}

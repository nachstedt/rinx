//! Rusty Sphinx Application Entry Point
//!
//! Supports a subcommand-based CLI for Bazel phase integration as well as a
//! legacy single-file mode for quick manual testing.
//!
//! # Subcommands (Bazel phases)
//!
//! ```text
//! rusty-sphinx parse  --input <file.rst>  --output <file.ast>
//! rusty-sphinx validate_toctree --input <file.ast>  [--allowed <path>...]
//! rusty-sphinx index  --inputs <a.ast> [<b.ast> ...]  --output <project.index>
//! rusty-sphinx render --input <file.ast>  --index <project.index> --doc-path <rel_path> --output <file.html>
//! rusty-sphinx validate_images --inputs <a.ast> [<b.ast> ...] --image-dir <dir>
//! ```
//!
//! # Legacy mode (quick preview)
//!
//! ```text
//! rusty-sphinx <file.rst>    # prints HTML to stdout
//! ```

use anyhow::{Context, Result, anyhow};
use rusty_sphinx::{analyzer, ast, config, parser, process_rst, renderer, validator};
use std::env;
use std::fs;
use std::io::{self, Read};

// ── Pure functions for arguments and logic ───────────────────────────────────

fn flag_value(args: &[String], flag: &str) -> Result<String> {
    flag_value_opt(args, flag).ok_or_else(|| anyhow!("Missing required flag '{flag}'"))
}

fn flag_value_opt(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w.get(1).cloned())
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

/// Returns all values that follow `flag` until the next flag, returning empty vector if flag is missing.
fn flag_values_opt(args: &[String], flag: &str) -> Vec<String> {
    args.iter()
        .position(|a| a == flag)
        .map(|start| {
            args[start + 1..]
                .iter()
                .take_while(|a| !a.starts_with("--"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
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

    let index = analyzer::build_project_index(&docs);
    serde_json::to_string(&index).context("Serialization error")
}

fn process_preview(
    rst: &str,
    index_json: Option<&str>,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
) -> Result<String> {
    let doc = parser::parse(doc_path, rst);
    let mut index = if let Some(json) = index_json {
        serde_json::from_str(json).context("Failed to deserialize global index")?
    } else {
        analyzer::ProjectIndex::default()
    };

    let local_index = analyzer::analyze(&doc);
    index.merge(local_index);

    let body = renderer::render(&doc, &index, doc_path);

    // Extract page title from the first H1 heading, if any.
    let page_title = doc.title().unwrap_or(doc_path).to_string();

    let depth = doc_path.matches('/').count();
    let css_path = if depth == 0 {
        "default.css".to_string()
    } else {
        format!("{}default.css", "../".repeat(depth))
    };

    renderer::render_page(
        &body,
        template_str,
        config,
        &css_path,
        &page_title,
        doc_path,
        &index.nav_tree,
    )
}

fn process_render(
    ast_json: &str,
    index_json: &str,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
) -> Result<String> {
    let doc: ast::Document =
        serde_json::from_str(ast_json).context("Failed to deserialize AST document")?;
    let index: analyzer::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;

    let body = renderer::render(&doc, &index, doc_path);

    // Extract page title from the first H1 heading, if any.
    let page_title = doc.title().unwrap_or("");

    let css_path = renderer::css_relative_path(doc_path, "default.css");
    renderer::render_page(
        &body,
        template_str,
        config,
        &css_path,
        page_title,
        doc_path,
        &index.nav_tree,
    )
}

fn process_validate_images(ast_jsons: &[String], image_dir: &str) -> Result<()> {
    let mut missing_images = Vec::new();

    for json in ast_jsons {
        let doc: ast::Document = serde_json::from_str(json).context("Failed to deserialize AST")?;
        for node in &doc.nodes {
            if let ast::Node::Directive(ast::Directive::PlantUml(content)) = node {
                let hash = content.hash();
                let svg_name = format!("{hash}.svg");
                let svg_path = std::path::Path::new(image_dir).join(&svg_name);
                if !svg_path.exists() {
                    missing_images.push(format!(
                        "Image {svg_name} missing for document {}",
                        doc.path
                    ));
                }
            }
        }
    }

    if missing_images.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Image validation failed:\n{}",
            missing_images.join("\n")
        ))
    }
}

fn process_extract_diagrams(ast_json: &str, outdir_path: &str) -> Result<()> {
    let doc: ast::Document = serde_json::from_str(ast_json).context("Failed to deserialize AST")?;

    fs::create_dir_all(outdir_path).with_context(|| format!("Error creating '{outdir_path}'"))?;

    for node in &doc.nodes {
        if let ast::Node::Directive(ast::Directive::PlantUml(content)) = node {
            let path = std::path::Path::new(outdir_path).join(format!("{}.puml", content.hash()));
            fs::write(&path, content.body())
                .with_context(|| format!("Error writing {}", path.display()))?;
        }
    }

    Ok(())
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
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;
    let doc_path = flag_value(args, "--doc-path")?;

    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;

    let template_str = fs::read_to_string(&template_path)
        .with_context(|| format!("Error reading template '{template_path}'"))?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let index_json =
        fs::read_to_string(&index_path).with_context(|| format!("Error reading '{index_path}'"))?;

    let html = process_render(
        &ast_json,
        &index_json,
        &site_config,
        &template_str,
        &doc_path,
    )?;
    fs::write(&output, html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_validate_toctree(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;
    let allowed: std::collections::HashSet<String> =
        flag_values_opt(args, "--allowed").into_iter().collect();

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let doc: ast::Document =
        serde_json::from_str(&ast_json).context("Failed to deserialize AST")?;

    validator::validate_toctree(&doc, &allowed)?;
    fs::write(&output, &ast_json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_extract_diagrams(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let outdir = flag_value(args, "--outdir")?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    process_extract_diagrams(&ast_json, &outdir)?;
    Ok(())
}

fn cmd_validate_images(args: &[String]) -> Result<()> {
    let inputs = flag_values(args, "--inputs")?;
    let image_dir = flag_value(args, "--image-dir")?;
    let output = flag_value(args, "--output").ok();

    let ast_jsons: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    process_validate_images(&ast_jsons, &image_dir)?;

    if let Some(out_path) = output {
        fs::write(&out_path, "OK").with_context(|| format!("Error writing '{out_path}'"))?;
    }

    Ok(())
}

fn cmd_preview(args: &[String]) -> Result<()> {
    let index_path = flag_value_opt(args, "--index");
    let doc_path = flag_value(args, "--doc-path")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;

    // Read RST from stdin
    let mut rst = String::new();
    io::stdin()
        .read_to_string(&mut rst)
        .context("Failed to read from stdin")?;

    let index_json = if let Some(p) = index_path {
        Some(fs::read_to_string(&p).with_context(|| format!("Error reading index '{p}'"))?)
    } else {
        None
    };

    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;

    let template_str = fs::read_to_string(&template_path)
        .with_context(|| format!("Error reading template '{template_path}'"))?;

    let html = process_preview(
        &rst,
        index_json.as_deref(),
        &site_config,
        &template_str,
        &doc_path,
    )?;

    println!("{html}");
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
        Some("validate_toctree") => cmd_validate_toctree(&args[2..]),
        Some("extract_diagrams") => cmd_extract_diagrams(&args[2..]),
        Some("validate_images") => cmd_validate_images(&args[2..]),
        Some("index") => cmd_index(&args[2..]),
        Some("render") => cmd_render(&args[2..]),
        Some("preview") => cmd_preview(&args[2..]),
        Some(path) if !path.starts_with('-') => cmd_legacy(path),
        _ => {
            let program = args.first().map_or("rusty-sphinx", String::as_str);
            let msg = format!(
                "Usage:\n\
                   {program} <file.rst>                                   (legacy preview)\n\
                   {program} parse  --input <file.rst> --output <file.ast>\n\
                   {program} extract_diagrams --input <file.ast> --outdir <puml_dir>\n\
                   {program} validate_toctree --input <file.ast.raw> --output <file.ast> [--allowed <path>...]\n\
                   {program} index  --inputs <a.ast> [<b.ast> ...] --output <project.index>\n\
                   {program} render --input <file.ast> --index <project.index> --doc-path <rel_path> --output <file.html> --config <config.toml> --template <template.html>\n\
                   {program} preview --doc-path <rel_path> --config <config.toml> --template <template.html> [--index <project.index>]\n\
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
    fn test_flag_value_opt_returns_some_when_flag_exists() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value_opt(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_value_opt_returns_none_when_flag_is_missing() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value_opt(&args, "--output");

        // Then
        assert!(result.is_none());
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
        let docs = vec![
            r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":"Title"}}]}"#.to_string(),
        ];

        // When
        let index = process_index(&docs).unwrap();

        // Then
        assert_eq!(
            index,
            r#"{"targets":{},"document_titles":{"test.rst":"Title"},"nav_tree":[{"title":"Title","path":"test.rst","children":[]}]}"#
        );
    }

    #[test]
    fn test_process_render_returns_html_string() {
        // Given
        let doc = r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":"Title"}}]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let html = process_render(doc, index, &config, template, "test.rst").unwrap();

        // Then
        assert!(html.contains("<h1>Title</h1>"));
    }

    #[test]
    fn test_process_preview_renders_html_with_merged_index() {
        // Given
        let rst = "Section A\n=========\n\nSee :ref:`section-b`";
        // Global index only knows about section-b in another file
        let global_index = r#"{"targets":{"section-b":{"Internal":"other.rst"}},"document_titles":{"other.rst":"Other"},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let html = process_preview(rst, Some(global_index), &config, template, "test.rst").unwrap();

        // Then
        assert!(html.contains("<h1>Section A</h1>"));
        // Cross-reference to other file should be resolved
        assert!(html.contains("href=\"other.html#section-b\""));
    }

    #[test]
    fn test_process_preview_works_without_global_index() {
        // Given
        let rst = "Section A\n=========";
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let html = process_preview(rst, None, &config, template, "test.rst").unwrap();

        // Then
        assert!(html.contains("<h1>Section A</h1>"));
    }

    #[test]
    fn test_process_extract_diagrams_creates_files() {
        // Given
        let content = ast::HashedContent::new("A -> B".to_string());
        let expected_hash = content.hash().to_string();
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![ast::Node::Directive(ast::Directive::PlantUml(content))],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let outdir = std::env::temp_dir().join(format!(
            "puml_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        process_extract_diagrams(&ast_json, outdir.to_str().unwrap()).unwrap();

        // Then
        let path = outdir.join(format!("{expected_hash}.puml"));
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "A -> B");

        let _ = std::fs::remove_dir_all(outdir);
    }

    #[test]
    fn test_process_extract_diagrams_leaves_empty_dir_when_no_diagrams() {
        // Given
        let ast_json = r#"{"path":"test.rst","nodes":[]}"#;
        let outdir = std::env::temp_dir().join(format!(
            "puml_test_dummy_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        process_extract_diagrams(ast_json, outdir.to_str().unwrap()).unwrap();

        // Then
        assert!(outdir.exists());
        let path = outdir.join(".dummy.puml");
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(outdir);
    }

    // ── process_validate_images ───────────────────────────────────────────────

    fn make_plantuml_ast(doc_path: &str, body: &str) -> String {
        let content = ast::HashedContent::new(body.to_string());
        let doc = ast::Document::new(
            doc_path.to_string(),
            vec![ast::Node::Directive(ast::Directive::PlantUml(content))],
        );
        serde_json::to_string(&doc).unwrap()
    }

    fn make_empty_ast(doc_path: &str) -> String {
        let doc = ast::Document::new(doc_path.to_string(), vec![]);
        serde_json::to_string(&doc).unwrap()
    }

    fn temp_image_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "validate_images_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_process_validate_images_succeeds_when_no_diagrams_present() {
        // Given — a document with no PlantUML directives
        let ast_json = make_empty_ast("test.rst");
        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then — no diagrams means nothing to validate, should pass
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_succeeds_when_all_svgs_present() {
        // Given — a document with a PlantUML diagram whose SVG exists in image_dir
        let body = "A -> B";
        let content = ast::HashedContent::new(body.to_string());
        let expected_hash = content.hash().to_string();
        let ast_json = make_plantuml_ast("team_a/index.rst", body);

        let image_dir = temp_image_dir();
        std::fs::write(image_dir.join(format!("{expected_hash}.svg")), "<svg/>").unwrap();

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_fails_when_svg_is_missing() {
        // Given — a PlantUML diagram but the image_dir is empty
        let body = "A -> B";
        let content = ast::HashedContent::new(body.to_string());
        let expected_hash = content.hash().to_string();
        let ast_json = make_plantuml_ast("team_a/index.rst", body);

        let image_dir = temp_image_dir();
        // Do NOT write the .svg file

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains(&expected_hash),
            "error should mention the missing hash"
        );
        assert!(
            msg.contains("team_a/index.rst"),
            "error should mention the source document"
        );

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_reports_all_missing_svgs() {
        // Given — two documents each with a distinct diagram, neither SVG present
        let body_a = "A -> B";
        let body_b = "C -> D";
        let hash_a = ast::HashedContent::new(body_a.to_string())
            .hash()
            .to_string();
        let hash_b = ast::HashedContent::new(body_b.to_string())
            .hash()
            .to_string();
        let ast_a = make_plantuml_ast("team_a/index.rst", body_a);
        let ast_b = make_plantuml_ast("team_b/index.rst", body_b);

        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[ast_a, ast_b], image_dir.to_str().unwrap());

        // Then — both missing images are reported in a single error
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains(&hash_a), "hash_a should be mentioned");
        assert!(msg.contains(&hash_b), "hash_b should be mentioned");
        assert!(
            msg.contains("team_a/index.rst"),
            "doc a should be mentioned"
        );
        assert!(
            msg.contains("team_b/index.rst"),
            "doc b should be mentioned"
        );

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_succeeds_with_empty_input_list() {
        // Given — no AST files at all
        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[], image_dir.to_str().unwrap());

        // Then — nothing to validate
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_returns_error_for_invalid_ast_json() {
        // Given — invalid JSON
        let bad_json = "not valid json".to_string();

        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[bad_json], image_dir.to_str().unwrap());

        // Then — deserialisation failure is surfaced as an error
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_ignores_non_plantuml_nodes() {
        // Given — a document with only headings and paragraphs, no diagrams
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![
                ast::Node::Heading {
                    level: 1,
                    text: "Title".to_string(),
                },
                ast::Node::Paragraph(vec![ast::InlineNode::Text("text".to_string())]),
            ],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then — non-diagram nodes are ignored, validation passes
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }
}

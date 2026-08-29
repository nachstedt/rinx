//! The `extract_diagrams` and `validate_images` subcommands, kept together
//! since they share [`collect_plantuml_contents`] — the set of diagrams that
//! gets *compiled* and the set that gets *validated* must never drift apart.

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_ast as ast;
use std::fs;

use crate::cli_args::{flag_value, flag_values};

/// Collects every `.. plantuml::` directive in `doc`, however deeply nested.
///
/// Shared by [`process_extract_diagrams`] and [`process_validate_images`] so the
/// set of diagrams that gets *compiled* and the set that gets *validated* can
/// never drift apart — when the two disagreed, a nested diagram silently
/// produced a broken `<img>` that validation did not catch.
pub(super) fn collect_plantuml_contents(doc: &ast::Document) -> Vec<&ast::HashedContent> {
    let mut contents = Vec::new();
    ast::walk_nodes(&doc.nodes, &mut |node| {
        if let ast::Node::Directive(ast::Directive::PlantUml(content)) = node {
            contents.push(content);
        }
    });
    contents
}

pub(super) fn process_extract_diagrams(ast_json: &str, outdir_path: &str) -> Result<()> {
    let doc: ast::Document = serde_json::from_str(ast_json).context("Failed to deserialize AST")?;

    fs::create_dir_all(outdir_path).with_context(|| format!("Error creating '{outdir_path}'"))?;

    for content in collect_plantuml_contents(&doc) {
        let path = std::path::Path::new(outdir_path).join(format!("{}.puml", content.hash()));
        fs::write(&path, content.body())
            .with_context(|| format!("Error writing {}", path.display()))?;
    }

    Ok(())
}

pub(super) fn cmd_extract_diagrams(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let outdir = flag_value(args, "--outdir")?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    process_extract_diagrams(&ast_json, &outdir)?;
    Ok(())
}

pub(super) fn process_validate_images(ast_jsons: &[String], image_dir: &str) -> Result<()> {
    let mut missing_images = Vec::new();

    for json in ast_jsons {
        let doc: ast::Document = serde_json::from_str(json).context("Failed to deserialize AST")?;
        for content in collect_plantuml_contents(&doc) {
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

    if missing_images.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Image validation failed:\n{}",
            missing_images.join("\n")
        ))
    }
}

pub(super) fn cmd_validate_images(args: &[String]) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Wraps `inner` in a `.. note::` so a directive sits one level below the
    /// document root — the shape the old top-level-only scan used to miss.
    fn note_containing(inner: ast::Node) -> ast::Node {
        ast::Node::Directive(ast::Directive::Admonition {
            kind: ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![inner],
        })
    }

    #[test]
    fn test_process_extract_diagrams_extracts_a_diagram_nested_in_an_admonition() {
        // Given — a `.. plantuml::` inside a `.. note::`.
        let content = ast::HashedContent::new("A -> B".to_string());
        let expected_hash = content.hash().to_string();
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![note_containing(ast::Node::Directive(
                ast::Directive::PlantUml(content),
            ))],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let outdir = std::env::temp_dir().join(format!(
            "puml_nested_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        process_extract_diagrams(&ast_json, outdir.to_str().unwrap()).unwrap();

        // Then — the nested diagram is extracted, not silently skipped.
        let path = outdir.join(format!("{expected_hash}.puml"));
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "A -> B");

        let _ = std::fs::remove_dir_all(outdir);
    }

    #[test]
    fn test_process_validate_images_fails_when_a_nested_diagrams_svg_is_missing() {
        // Given — a nested diagram whose SVG was never produced.
        let content = ast::HashedContent::new("A -> B".to_string());
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![note_containing(ast::Node::Directive(
                ast::Directive::PlantUml(content),
            ))],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let empty_image_dir = std::env::temp_dir().join(format!(
            "puml_validate_nested_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&empty_image_dir).unwrap();

        // When
        let result = process_validate_images(&[ast_json], empty_image_dir.to_str().unwrap());

        // Then — validation catches it instead of passing a broken <img> through.
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(empty_image_dir);
    }

    #[test]
    fn test_collect_plantuml_contents_returns_diagrams_in_document_order() {
        // Given — one top-level diagram and one nested inside an admonition.
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![
                ast::Node::Directive(ast::Directive::PlantUml(ast::HashedContent::new(
                    "first".to_string(),
                ))),
                note_containing(ast::Node::Directive(ast::Directive::PlantUml(
                    ast::HashedContent::new("second".to_string()),
                ))),
            ],
        );

        // When
        let contents = collect_plantuml_contents(&doc);

        // Then
        let bodies: Vec<&str> = contents.iter().map(|c| c.body()).collect();
        assert_eq!(bodies, vec!["first", "second"]);
    }

    #[test]
    fn test_collect_plantuml_contents_returns_empty_for_a_document_without_diagrams() {
        // Given
        let doc = ast::Document::new("test.rst".to_string(), vec![ast::Node::Transition]);

        // When
        let contents = collect_plantuml_contents(&doc);

        // Then
        assert!(contents.is_empty());
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

    #[test]
    fn test_process_extract_diagrams_returns_error_for_invalid_ast_json() {
        // Given
        let bad_json = "not valid json";
        let outdir = std::env::temp_dir().join(format!(
            "puml_test_bad_json_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        let result = process_extract_diagrams(bad_json, outdir.to_str().unwrap());

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Failed to deserialize AST"));
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
                    text: vec![ast::InlineNode::Text("Title".to_string())],
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

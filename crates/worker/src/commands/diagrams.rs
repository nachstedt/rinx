//! The `validate_images` subcommand: every picture a page shows must have
//! reached the site's single `_images/` directory.
//!
//! Two populations are checked, because both are served from `_images/` and
//! both fail the same way when absent — a page that looks finished and renders
//! a broken picture:
//!
//! - **Compiled diagrams.** A render of a document whose library set
//!   `diagrams = True` wrote one `<hash>.puml` per diagram, and the `<img>` on
//!   the page names `<hash>.svg` — both from the same process, so they agree by
//!   construction. What can still go wrong is the compile in between, so each
//!   `.puml` must have its `.svg`. Nothing here re-expands a template: the
//!   render already did, and the files are the record of it.
//! - **Authored images** — what a `.. image::` or `.. figure::` names, which
//!   must have been declared in its library's `images` attribute.

use anyhow::{Context, Result, anyhow};
use rinx_ast as ast;
use std::fs;
use std::path::Path;

use super::cli_args::{flag_value, flag_values, flag_values_opt};

/// Collects the project files every `.. image::`/`.. figure::` in `doc`
/// refers to, as paths under the site's `_images/` directory.
///
/// External URLs are skipped: nothing bundles them, so their absence from the
/// site is not a build failure. Resolution goes through
/// [`ast::ImageUri::resolve`], the same function the bundler and the renderer
/// use, so a `..` in a path cannot make the three disagree.
pub(super) fn collect_image_paths(doc: &ast::Document) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    let mut record = |options: &ast::ImageOptions| {
        if let Some(resolved) = options.uri.resolve(&doc.path)
            && !paths.contains(&resolved)
        {
            paths.push(resolved);
        }
    };
    ast::walk_nodes(&doc.nodes, &mut |node| match node {
        ast::Node::Directive(ast::Directive::Image(options)) => record(options),
        ast::Node::Directive(ast::Directive::Figure(figure)) => record(&figure.image),
        // A `.. |name| image::` substitution definition names a real image
        // too, subject to the same `images` attribute requirement.
        ast::Node::Directive(ast::Directive::SubstitutionDefinition(definition)) => {
            if let ast::SubstitutionKind::Image(options) = &definition.kind {
                record(options);
            }
        }
        _ => {}
    });
    paths
}

/// The `.svg` a compile should have produced for each `.puml` under
/// `diagram_dir`, reported when absent from `image_dir`.
///
/// Reads the directory the render wrote rather than re-deriving which diagrams
/// a document holds: the files *are* what the page's `<img>` tags name.
fn missing_diagram_svgs(diagram_dir: &Path, image_dir: &Path) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    let entries = fs::read_dir(diagram_dir)
        .with_context(|| format!("Error reading '{}'", diagram_dir.display()))?;
    let mut sources: Vec<_> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "puml"))
        .collect();
    // Sorted so a report listing several is stable from run to run.
    sources.sort();
    for source in sources {
        let Some(stem) = source.file_stem() else {
            continue;
        };
        let svg = format!("{}.svg", stem.to_string_lossy());
        if !image_dir.join(&svg).exists() {
            missing.push(format!(
                "Image {svg} missing — PlantUML did not compile {}",
                source.display()
            ));
        }
    }
    Ok(missing)
}

pub(super) fn process_validate_images(
    ast_jsons: &[String],
    diagram_dirs: &[String],
    image_dir: &str,
) -> Result<()> {
    let image_dir = Path::new(image_dir);
    let mut missing_images = Vec::new();

    for diagram_dir in diagram_dirs {
        missing_images.extend(missing_diagram_svgs(Path::new(diagram_dir), image_dir)?);
    }

    for json in ast_jsons {
        let doc: ast::Document = serde_json::from_str(json).context("Failed to deserialize AST")?;
        for path in collect_image_paths(&doc) {
            if !image_dir.join(&path).exists() {
                missing_images.push(format!(
                    "Image {} missing for document {} — if this is a Bazel build, add the file \
                     to the library's images attribute",
                    path.display(),
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

pub(crate) fn cmd_validate_images(args: &[String]) -> Result<()> {
    let inputs = flag_values(args, "--inputs")?;
    let image_dir = flag_value(args, "--image-dir")?;
    let output = flag_value(args, "--output").ok();
    // Absent for a site none of whose libraries set `diagrams = True`.
    let diagram_dirs = flag_values_opt(args, "--diagram-dirs");

    let ast_jsons: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    process_validate_images(&ast_jsons, &diagram_dirs, &image_dir)?;

    if let Some(out_path) = output {
        fs::write(&out_path, "OK").with_context(|| format!("Error writing '{out_path}'"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty directory unique to this test run.
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{name}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// [`process_validate_images`] over authored images only.
    fn validate_images(ast_jsons: &[String], image_dir: &str) -> Result<()> {
        process_validate_images(ast_jsons, &[], image_dir)
    }

    /// A document at `doc_path` holding one `.. image::` of `uri`.
    fn document_with_image(doc_path: &str, uri: &str) -> ast::Document {
        ast::Document::new(
            doc_path.to_string(),
            vec![ast::Node::Directive(ast::Directive::Image(Box::new(
                ast::ImageOptions::new(ast::ImageUri::new(uri)),
            )))],
        )
    }

    #[test]
    fn test_collect_image_paths_resolves_against_the_document() {
        // Given
        let doc = document_with_image("guide/intro.rst", "images/logo.png");

        // When
        let paths = collect_image_paths(&doc);

        // Then
        assert_eq!(
            paths,
            vec![std::path::PathBuf::from("guide/images/logo.png")]
        );
    }

    #[test]
    fn test_collect_image_paths_skips_an_external_url() {
        // Given — nothing bundles it, so its absence is not a build failure
        let doc = document_with_image("index.rst", "https://example.com/logo.png");

        // When
        let paths = collect_image_paths(&doc);

        // Then
        assert!(paths.is_empty());
    }

    #[test]
    fn test_collect_image_paths_finds_a_substitution_image() {
        // Given — a `.. |name| image::` definition names a real image too,
        // subject to the same `images` attribute requirement.
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![ast::Node::Directive(
                ast::Directive::SubstitutionDefinition(ast::SubstitutionDefinition {
                    name: "biohazard".to_string(),
                    kind: ast::SubstitutionKind::Image(Box::new(ast::ImageOptions::new(
                        ast::ImageUri::new("biohazard.png"),
                    ))),
                    span: None,
                }),
            )],
        );

        // When
        let paths = collect_image_paths(&doc);

        // Then
        assert_eq!(paths, vec![std::path::PathBuf::from("biohazard.png")]);
    }

    #[test]
    fn test_collect_image_paths_finds_a_figures_image() {
        // Given
        let figure = ast::Figure::new(ast::ImageOptions::new(ast::ImageUri::new("logo.png")));
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![ast::Node::Directive(ast::Directive::Figure(Box::new(
                figure,
            )))],
        );

        // When
        let paths = collect_image_paths(&doc);

        // Then
        assert_eq!(paths, vec![std::path::PathBuf::from("logo.png")]);
    }

    #[test]
    fn test_process_validate_images_reports_a_missing_authored_image() {
        // Given — what an undeclared image looks like: absent from the bundle
        let doc = document_with_image("index.rst", "logo.png");
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let result = validate_images(&[ast_json], "/nonexistent-image-bundle");

        // Then
        let message = result.expect_err("should fail").to_string();
        assert!(message.contains("logo.png"), "got: {message}");
        assert!(message.contains("images attribute"), "got: {message}");
    }

    #[test]
    fn test_process_validate_images_accepts_an_external_url() {
        // Given
        let doc = document_with_image("index.rst", "https://example.com/logo.png");
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let result = validate_images(&[ast_json], "/nonexistent-image-bundle");

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_a_compiled_diagram_passes_validation() {
        // Given — a render wrote a source, and the compile produced its SVG
        let diagrams = temp_dir("validate_diagrams_ok");
        let images = temp_dir("validate_images_ok");
        fs::write(diagrams.join("abc123.puml"), "@startuml\n@enduml").unwrap();
        fs::write(images.join("abc123.svg"), "<svg/>").unwrap();

        // When
        let result = process_validate_images(
            &[],
            &[diagrams.to_string_lossy().to_string()],
            images.to_str().unwrap(),
        );

        // Then
        assert!(result.is_ok(), "{result:?}");
        let _ = fs::remove_dir_all(diagrams);
        let _ = fs::remove_dir_all(images);
    }

    #[test]
    fn test_a_diagram_the_compile_did_not_produce_fails_validation() {
        // Given — a source with no SVG beside it: the page names a picture
        // nothing compiled
        let diagrams = temp_dir("validate_diagrams_missing");
        let images = temp_dir("validate_images_missing");
        fs::write(diagrams.join("abc123.puml"), "@startuml\n@enduml").unwrap();

        // When
        let result = process_validate_images(
            &[],
            &[diagrams.to_string_lossy().to_string()],
            images.to_str().unwrap(),
        );

        // Then
        let message = result.expect_err("should fail").to_string();
        assert!(message.contains("abc123.svg"), "{message}");
        assert!(message.contains("PlantUML"), "{message}");
        let _ = fs::remove_dir_all(diagrams);
        let _ = fs::remove_dir_all(images);
    }

    #[test]
    fn test_an_empty_diagram_directory_passes_validation() {
        // Given — an enabled library's document that happens to draw nothing
        let diagrams = temp_dir("validate_diagrams_empty");
        let images = temp_dir("validate_images_empty");

        // When
        let result = process_validate_images(
            &[],
            &[diagrams.to_string_lossy().to_string()],
            images.to_str().unwrap(),
        );

        // Then
        assert!(result.is_ok());
        let _ = fs::remove_dir_all(diagrams);
        let _ = fs::remove_dir_all(images);
    }

    #[test]
    fn test_missing_diagram_svgs_ignores_files_that_are_not_sources() {
        // Given — only `.puml` files are compiled, so only they need an SVG
        let diagrams = temp_dir("validate_diagrams_other");
        let images = temp_dir("validate_images_other");
        fs::write(diagrams.join("notes.txt"), "not a diagram").unwrap();

        // When
        let missing = missing_diagram_svgs(&diagrams, &images).unwrap();

        // Then
        assert!(missing.is_empty());
        let _ = fs::remove_dir_all(diagrams);
        let _ = fs::remove_dir_all(images);
    }

    #[test]
    fn test_missing_diagram_svgs_lists_every_missing_picture_in_a_stable_order() {
        // Given
        let diagrams = temp_dir("validate_diagrams_several");
        let images = temp_dir("validate_images_several");
        fs::write(diagrams.join("bbb.puml"), "").unwrap();
        fs::write(diagrams.join("aaa.puml"), "").unwrap();

        // When
        let missing = missing_diagram_svgs(&diagrams, &images).unwrap();

        // Then
        assert_eq!(missing.len(), 2);
        assert!(missing[0].contains("aaa.svg"), "{missing:?}");
        assert!(missing[1].contains("bbb.svg"), "{missing:?}");
        let _ = fs::remove_dir_all(diagrams);
        let _ = fs::remove_dir_all(images);
    }

    #[test]
    fn test_process_validate_images_returns_error_for_invalid_ast_json() {
        // Given
        let bad_json = "not json".to_string();

        // When
        let result = validate_images(&[bad_json], "/nonexistent");

        // Then
        assert!(result.is_err());
    }
}

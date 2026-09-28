//! The `validate_assets` subcommand: every file a page shows or links must
//! have reached the site directory it is served from.
//!
//! Three populations are checked, because all three fail the same way when
//! absent — a page that looks finished and links to nothing:
//!
//! - **Compiled diagrams.** A render of a document whose library set
//!   `diagrams = True` wrote one `<hash>.puml` per diagram, and the `<img>` on
//!   the page names `<hash>.svg` — both from the same process, so they agree by
//!   construction. What can still go wrong is the compile in between, so each
//!   `.puml` must have its `.svg`. Nothing here re-expands a template: the
//!   render already did, and the files are the record of it.
//! - **Authored images** — what a `.. image::` or `.. figure::` names, which
//!   must have been declared in its library's `images` attribute.
//! - **Downloads** — what a `:download:` names, which must have been declared
//!   in its library's `downloads` attribute and so reached `_downloads/`.
//!   Reported as a `download.undeclared` error at the role's position.
//!
//! Either bundle directory may be absent — a site declaring no images, or no
//! downloads — and is then treated as empty, so a document naming a file the
//! site never bundled still fails rather than skipping the check.

use anyhow::{Context, Result, anyhow};
use rinx_ast as ast;
use std::fs;
use std::path::{Path, PathBuf};

use super::cli_args::{flag_value_opt, flag_values, flag_values_opt};
use super::diagnostics::{WarningOrigin, format_error_diagnostic};

/// Collects the project files every `.. image::`/`.. figure::` in `doc`
/// refers to, as paths under the site's `_images/` directory.
///
/// External URLs are skipped: nothing bundles them, so their absence from the
/// site is not a build failure. Resolution goes through
/// [`ast::AssetUri::resolve`], the same function the bundler and the renderer
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
fn missing_diagram_svgs(diagram_dir: &Path, image_dir: Option<&Path>) -> Result<Vec<String>> {
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
        if !is_bundled(image_dir, Path::new(&svg)) {
            missing.push(format!(
                "Image {svg} missing — PlantUML did not compile {}",
                source.display()
            ));
        }
    }
    Ok(missing)
}

/// Whether `path` is present under the bundle directory `dir`; nothing is
/// when the site bundled no such directory at all.
fn is_bundled(dir: Option<&Path>, path: &Path) -> bool {
    dir.is_some_and(|dir| dir.join(path).exists())
}

/// Collects the project files every linked `:download:` in the document
/// serialized as `ast_json` refers to, each with where the role was written.
///
/// The roles are found in the AST's JSON rather than by walking the typed
/// tree: inline content sits in paragraphs, titles, table cells, entity
/// sections and more, and [`ast::walk_nodes`] deliberately does not descend
/// into it — the same reason `index.rs`'s `written_reference_targets` reads
/// the JSON. Each hit is still read back as an [`ast::InlineNode`], so its
/// shape is checked rather than assumed. Resolution goes through
/// [`ast::AssetUri::resolve`], the function the renderer builds the `href`
/// with. External URLs and the `!` form name nothing to bundle and are
/// skipped.
fn collect_download_paths(
    ast_json: &serde_json::Value,
    doc_path: &str,
) -> Result<Vec<(PathBuf, Option<ast::Span>)>> {
    fn collect(value: &serde_json::Value, found: &mut Vec<serde_json::Value>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, inner) in fields {
                    if key == "DownloadReference" {
                        found.push(serde_json::json!({ "DownloadReference": inner }));
                    } else {
                        collect(inner, found);
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    collect(item, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    collect(ast_json, &mut found);
    let mut paths = Vec::new();
    for value in found {
        let node: ast::InlineNode =
            serde_json::from_value(value).context("Malformed :download: role in AST")?;
        if let ast::InlineNode::DownloadReference {
            target,
            link: true,
            span,
            ..
        } = node
            && let Some(resolved) = target.resolve(doc_path)
        {
            paths.push((resolved, span));
        }
    }
    Ok(paths)
}

/// The directories a site bundled, each `None` when it bundled none.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Bundles<'a> {
    /// `_images/`: compiled diagrams and authored images.
    pub(super) images: Option<&'a Path>,
    /// `_downloads/`: the files `:download:` links.
    pub(super) downloads: Option<&'a Path>,
}

pub(super) fn process_validate_assets(
    ast_jsons: &[String],
    diagram_dirs: &[String],
    bundles: Bundles<'_>,
) -> Result<()> {
    let mut missing = Vec::new();

    for diagram_dir in diagram_dirs {
        missing.extend(missing_diagram_svgs(
            Path::new(diagram_dir),
            bundles.images,
        )?);
    }

    for json in ast_jsons {
        let value: serde_json::Value =
            serde_json::from_str(json).context("Failed to deserialize AST")?;
        let doc: ast::Document =
            serde_json::from_value(value.clone()).context("Failed to deserialize AST")?;
        for path in collect_image_paths(&doc) {
            if !is_bundled(bundles.images, &path) {
                missing.push(format!(
                    "Image {} missing for document {} — if this is a Bazel build, add the file \
                     to the library's images attribute",
                    path.display(),
                    doc.path
                ));
            }
        }
        let origin = WarningOrigin::new(&doc.path, &doc.source_files);
        for (path, span) in collect_download_paths(&value, &doc.path)? {
            if !is_bundled(bundles.downloads, &path) {
                let diagnostic = ast::Diagnostic::at(
                    ast::DiagnosticCode::DownloadUndeclared,
                    format!(
                        "download file {} was not bundled — if this is a Bazel build, add it \
                         to the library's downloads attribute",
                        path.display()
                    ),
                    span,
                );
                missing.push(format_error_diagnostic(&origin, &diagnostic));
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("Asset validation failed:\n{}", missing.join("\n")))
    }
}

pub(crate) fn cmd_validate_assets(args: &[String]) -> Result<()> {
    let inputs = flag_values(args, "--inputs")?;
    // Each absent for a site that bundled no such directory.
    let image_dir = flag_value_opt(args, "--image-dir");
    let download_dir = flag_value_opt(args, "--download-dir");
    let output = flag_value_opt(args, "--output");
    // Absent for a site none of whose libraries set `diagrams = True`.
    let diagram_dirs = flag_values_opt(args, "--diagram-dirs");

    let ast_jsons: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    process_validate_assets(
        &ast_jsons,
        &diagram_dirs,
        Bundles {
            images: image_dir.as_deref().map(Path::new),
            downloads: download_dir.as_deref().map(Path::new),
        },
    )?;

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

    /// [`process_validate_assets`] over authored images only.
    fn validate_images(ast_jsons: &[String], image_dir: &str) -> Result<()> {
        process_validate_assets(
            ast_jsons,
            &[],
            Bundles {
                images: Some(Path::new(image_dir)),
                downloads: None,
            },
        )
    }

    /// [`process_validate_assets`] over one document's downloads only.
    fn validate_downloads(doc: &ast::Document, download_dir: Option<&Path>) -> Result<()> {
        process_validate_assets(
            &[serde_json::to_string(doc).unwrap()],
            &[],
            Bundles {
                images: None,
                downloads: download_dir,
            },
        )
    }

    /// A document at `doc_path` whose one paragraph holds `inlines`.
    fn document_with_inlines(doc_path: &str, inlines: Vec<ast::InlineNode>) -> ast::Document {
        ast::Document::new(doc_path.to_string(), vec![ast::Node::Paragraph(inlines)])
    }

    /// A linked `:download:` of `target`, written on line 3.
    fn download(target: &str) -> ast::InlineNode {
        ast::InlineNode::DownloadReference {
            display: None,
            target: ast::AssetUri::new(target),
            link: true,
            span: Some(ast::Span::new(
                ast::Position::new(3, 5),
                ast::Position::new(3, 30),
            )),
        }
    }

    /// A document at `doc_path` holding one `.. image::` of `uri`.
    fn document_with_image(doc_path: &str, uri: &str) -> ast::Document {
        ast::Document::new(
            doc_path.to_string(),
            vec![ast::Node::Directive(ast::Directive::Image(Box::new(
                ast::ImageOptions::new(ast::AssetUri::new(uri)),
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
                        ast::AssetUri::new("biohazard.png"),
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
        let figure = ast::Figure::new(ast::ImageOptions::new(ast::AssetUri::new("logo.png")));
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
    fn test_process_validate_assets_reports_a_missing_authored_image() {
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
    fn test_process_validate_assets_accepts_an_external_url() {
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
        let result = process_validate_assets(
            &[],
            &[diagrams.to_string_lossy().to_string()],
            Bundles {
                images: Some(&images),
                downloads: None,
            },
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
        let result = process_validate_assets(
            &[],
            &[diagrams.to_string_lossy().to_string()],
            Bundles {
                images: Some(&images),
                downloads: None,
            },
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
        let result = process_validate_assets(
            &[],
            &[diagrams.to_string_lossy().to_string()],
            Bundles {
                images: Some(&images),
                downloads: None,
            },
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
        let missing = missing_diagram_svgs(&diagrams, Some(&images)).unwrap();

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
        let missing = missing_diagram_svgs(&diagrams, Some(&images)).unwrap();

        // Then
        assert_eq!(missing.len(), 2);
        assert!(missing[0].contains("aaa.svg"), "{missing:?}");
        assert!(missing[1].contains("bbb.svg"), "{missing:?}");
        let _ = fs::remove_dir_all(diagrams);
        let _ = fs::remove_dir_all(images);
    }

    #[test]
    fn test_process_validate_assets_returns_error_for_invalid_ast_json() {
        // Given
        let bad_json = "not json".to_string();

        // When
        let result = validate_images(&[bad_json], "/nonexistent");

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_download_paths_resolves_against_the_document() {
        // Given
        let doc = document_with_inlines("guide/intro.rst", vec![download("../data/a.csv")]);
        let value = serde_json::to_value(&doc).unwrap();

        // When
        let paths = collect_download_paths(&value, &doc.path).unwrap();

        // Then
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, PathBuf::from("data/a.csv"));
        assert_eq!(paths[0].1.map(|span| span.start.line), Some(3));
    }

    #[test]
    fn test_collect_download_paths_finds_a_role_nested_in_a_container() {
        // Given — a role inside a list item, which no block walk reaches
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![ast::Node::BulletList {
                bullet: '-',
                items: vec![ast::ListItem {
                    nodes: vec![ast::Node::Paragraph(vec![download("tool.py")])],
                }],
            }],
        );
        let value = serde_json::to_value(&doc).unwrap();

        // When
        let paths = collect_download_paths(&value, &doc.path).unwrap();

        // Then
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, PathBuf::from("tool.py"));
    }

    #[test]
    fn test_collect_download_paths_skips_external_urls_and_the_bang_form() {
        // Given — neither names a file the site bundles
        let unlinked = ast::InlineNode::DownloadReference {
            display: None,
            target: ast::AssetUri::new("tool.py"),
            link: false,
            span: None,
        };
        let doc = document_with_inlines(
            "index.rst",
            vec![download("https://example.com/tool.zip"), unlinked],
        );
        let value = serde_json::to_value(&doc).unwrap();

        // When
        let paths = collect_download_paths(&value, &doc.path).unwrap();

        // Then
        assert!(paths.is_empty(), "{paths:?}");
    }

    #[test]
    fn test_a_bundled_download_passes_validation() {
        // Given
        let downloads = temp_dir("validate_downloads_ok");
        fs::create_dir_all(downloads.join("data")).unwrap();
        fs::write(downloads.join("data/a.csv"), "x").unwrap();
        let doc = document_with_inlines("guide/intro.rst", vec![download("../data/a.csv")]);

        // When
        let result = validate_downloads(&doc, Some(&downloads));

        // Then
        assert!(result.is_ok(), "{result:?}");
        let _ = fs::remove_dir_all(downloads);
    }

    #[test]
    fn test_an_undeclared_download_fails_validation_at_the_roles_position() {
        // Given — a bundle that lacks the file
        let downloads = temp_dir("validate_downloads_missing");
        let doc = document_with_inlines("guide/intro.rst", vec![download("a.csv")]);

        // When
        let result = validate_downloads(&doc, Some(&downloads));

        // Then
        let message = result.expect_err("should fail").to_string();
        assert!(
            message.contains("error: guide/intro.rst:3:5: download.undeclared:"),
            "{message}"
        );
        assert!(message.contains("guide/a.csv"), "{message}");
        assert!(message.contains("downloads attribute"), "{message}");
        let _ = fs::remove_dir_all(downloads);
    }

    #[test]
    fn test_a_download_fails_validation_when_the_site_bundled_none() {
        // Given — no `_downloads/` at all: the site declared no downloads
        let doc = document_with_inlines("index.rst", vec![download("a.csv")]);

        // When
        let result = validate_downloads(&doc, None);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_an_image_fails_validation_when_the_site_bundled_none() {
        // Given — no `_images/` at all: the site declared no images
        let doc = document_with_image("index.rst", "logo.png");

        // When
        let result = process_validate_assets(
            &[serde_json::to_string(&doc).unwrap()],
            &[],
            Bundles::default(),
        );

        // Then
        let message = result.expect_err("should fail").to_string();
        assert!(message.contains("images attribute"), "{message}");
    }

    #[test]
    fn test_is_bundled_is_false_without_a_directory() {
        // Given / When / Then
        assert!(!is_bundled(None, Path::new("anything")));
    }
}

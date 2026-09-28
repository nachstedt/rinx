//! The `embed_assets` subcommand: the bytes behind every `:loading: embed`
//! image in one document, as a small `uri -> data: URI` sidecar.
//!
//! # Why this is its own build step
//!
//! Three constraints meet here. The renderer performs no I/O, so it cannot
//! open an image file. A `.ast` must stay small and content-addressed, so the
//! bytes must not be baked in at parse time. And a render action must not take
//! the whole site's images as inputs, or every page would re-render whenever
//! any picture changed.
//!
//! A per-document sidecar satisfies all three, and gives the same **cache
//! firewall** the doctest plans have (`docs/decisions/002-doctest-execution.md`):
//! this action's inputs are the document's AST and every declared image, so an
//! image edit re-runs this cheap AST walk for every document — but only a
//! document that actually embeds the changed image produces different bytes,
//! and only that page then re-renders.
//!
//! Keys are the image's *resolved* project path rather than the URI as
//! written, because [`rinx_ast::AssetUri::resolve`] is the one
//! function the renderer resolves with too. Two phases deriving the same key
//! two ways is exactly the drift that function exists to prevent.

use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rinx_ast as ast;
use rinx_renderer::EmbeddedAssets;
use std::fs;
use std::path::{Path, PathBuf};

use super::cli_args::flag_value;

/// The media types an image may be embedded as, by file extension.
///
/// A closed list rather than a guess: a `data:` URI carries its own media type
/// and a browser trusts it, so emitting one for a file whose type we do not
/// actually know would render a broken image with no way to find out why.
/// Anything not listed is reported and linked instead.
const MEDIA_TYPES: &[(&str, &str)] = &[
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("ico", "image/x-icon"),
    ("bmp", "image/bmp"),
];

/// The media type for `path`'s extension, if it names one this build embeds.
fn media_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    MEDIA_TYPES
        .iter()
        .find(|(suffix, _)| *suffix == extension)
        .map(|(_, media_type)| *media_type)
}

/// Reads `path` and returns it as a `data:` URI.
///
/// # Errors
///
/// Returns an error when the extension names no known media type, or when the
/// file cannot be read — in a Bazel build, the second means the file was left
/// out of the library's `images` attribute and so never entered the sandbox.
pub(crate) fn embed_image(path: &Path) -> Result<String> {
    let media_type = media_type(path).with_context(|| {
        format!(
            "cannot embed '{}': its extension names no image type this build knows \
             (expected one of {})",
            path.display(),
            MEDIA_TYPES
                .iter()
                .map(|(suffix, _)| *suffix)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let bytes = fs::read(path).with_context(|| {
        format!(
            "cannot read '{}' to embed it — if this is a Bazel build, add the file to \
             the library's images attribute so it reaches the build action",
            path.display()
        )
    })?;
    Ok(format!("data:{media_type};base64,{}", BASE64.encode(bytes)))
}

/// Every image in `doc` that asked to be embedded, as resolved project paths.
///
/// An external URL is skipped: embedding one would mean fetching it during the
/// build, which the parser already reports.
fn collect_embed_paths(doc: &ast::Document) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut record = |options: &ast::ImageOptions| {
        if options.loading != ast::ImageLoading::Embed {
            return;
        }
        if let Some(resolved) = options.uri.resolve(&doc.path)
            && !paths.contains(&resolved)
        {
            paths.push(resolved);
        }
    };
    ast::walk_nodes(&doc.nodes, &mut |node| match node {
        ast::Node::Directive(ast::Directive::Image(options)) => record(options),
        ast::Node::Directive(ast::Directive::Figure(figure)) => record(&figure.image),
        // A `.. |name| image::` substitution definition is a real image too —
        // every reference to it is spliced with these same `options` by the
        // parser's `resolve_substitutions` pass, so it needs the same asset
        // handling a standalone `.. image::` gets.
        ast::Node::Directive(ast::Directive::SubstitutionDefinition(definition)) => {
            if let ast::SubstitutionKind::Image(options) = &definition.kind {
                record(options);
            }
        }
        _ => {}
    });
    paths
}

/// Builds the embedded-asset table for one document.
///
/// `base_dir` is the directory project paths resolve against — the build
/// action's working directory, which under Bazel is the sandbox root.
///
/// # Errors
///
/// Returns an error when an image asked to be embedded and could not be, so a
/// missing declaration fails the build rather than silently producing a page
/// with a broken picture.
pub(super) fn process_embed_assets(ast_json: &str, base_dir: &Path) -> Result<EmbeddedAssets> {
    let doc: ast::Document = serde_json::from_str(ast_json).context("Failed to deserialize AST")?;

    let mut assets = EmbeddedAssets::new();
    for path in collect_embed_paths(&doc) {
        let data_uri = embed_image(&base_dir.join(&path))?;
        assets.insert(&path, data_uri);
    }
    Ok(assets)
}

/// The embeddable assets for a document, skipping every one that could not be
/// read.
///
/// The resilient counterpart to [`process_embed_assets`], for the live
/// preview: an editor is routinely pointed at a document whose images are
/// half-written or not yet saved, and failing the whole preview over one would
/// leave the author with no page at all. Nothing is lost by degrading — an
/// image missing from this table is reported by the renderer as
/// `image.embed-unavailable` and falls back to a link, which is the same
/// outcome by a gentler route.
pub(super) fn embed_available_assets(doc: &ast::Document, base_dir: &Path) -> EmbeddedAssets {
    let mut assets = EmbeddedAssets::new();
    for path in collect_embed_paths(doc) {
        if let Ok(data_uri) = embed_image(&base_dir.join(&path)) {
            assets.insert(&path, data_uri);
        }
    }
    assets
}

pub(crate) fn cmd_embed_assets(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let assets = process_embed_assets(&ast_json, Path::new("."))?;

    let json = serde_json::to_string(&assets).context("Failed to serialize embedded assets")?;
    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{AssetUri, Directive, Figure, ImageLoading, ImageOptions, Node};

    /// A document at `doc_path` holding one image directive.
    fn document_with(doc_path: &str, options: ImageOptions) -> ast::Document {
        ast::Document::new(
            doc_path.to_string(),
            vec![Node::Directive(Directive::Image(Box::new(options)))],
        )
    }

    fn embedding(uri: &str) -> ImageOptions {
        let mut options = ImageOptions::new(AssetUri::new(uri));
        options.loading = ImageLoading::Embed;
        options
    }

    /// Writes `bytes` to `name` inside a fresh temporary directory.
    fn temp_dir_with(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rinx-embed-{}-{name}", std::process::id()));
        let path = dir.join(name);
        fs::create_dir_all(path.parent().expect("has a parent")).expect("should create dir");
        fs::write(&path, bytes).expect("should write fixture");
        dir
    }

    #[test]
    fn test_media_type_maps_every_known_extension() {
        for (suffix, expected) in MEDIA_TYPES {
            // Given
            let path = PathBuf::from(format!("logo.{suffix}"));

            // When
            let found = media_type(&path);

            // Then
            assert_eq!(found, Some(*expected));
        }
    }

    #[test]
    fn test_media_type_ignores_extension_case() {
        // Given
        let path = PathBuf::from("LOGO.PNG");

        // When / Then
        assert_eq!(media_type(&path), Some("image/png"));
    }

    #[test]
    fn test_media_type_rejects_an_unknown_extension() {
        // Given
        let path = PathBuf::from("logo.tiff");

        // When / Then
        assert_eq!(media_type(&path), None);
    }

    #[test]
    fn test_media_type_rejects_a_file_with_no_extension() {
        // Given
        let path = PathBuf::from("logo");

        // When / Then
        assert_eq!(media_type(&path), None);
    }

    #[test]
    fn test_embed_image_produces_a_data_uri() {
        // Given
        let dir = temp_dir_with("data-uri.svg", b"<svg/>");

        // When
        let data_uri = embed_image(&dir.join("data-uri.svg")).expect("should embed");

        // Then — `<svg/>` is `PHN2Zy8+` in base64
        assert_eq!(data_uri, "data:image/svg+xml;base64,PHN2Zy8+");
    }

    #[test]
    fn test_embed_image_reports_an_unknown_type() {
        // Given
        let dir = temp_dir_with("unknown.tiff", b"xx");

        // When
        let result = embed_image(&dir.join("unknown.tiff"));

        // Then
        let message = result.expect_err("should refuse").to_string();
        assert!(message.contains("names no image type"), "got: {message}");
    }

    #[test]
    fn test_embed_image_reports_a_missing_file_with_the_bazel_hint() {
        // Given
        let path = PathBuf::from("definitely/missing/logo.png");

        // When
        let result = embed_image(&path);

        // Then
        let message = result.expect_err("should fail").to_string();
        assert!(message.contains("images attribute"), "got: {message}");
    }

    #[test]
    fn test_collect_embed_paths_finds_an_embedding_image() {
        // Given
        let doc = document_with("guide/intro.rst", embedding("logo.svg"));

        // When
        let paths = collect_embed_paths(&doc);

        // Then — resolved against the document's own directory
        assert_eq!(paths, vec![PathBuf::from("guide/logo.svg")]);
    }

    #[test]
    fn test_collect_embed_paths_ignores_a_linked_image() {
        // Given
        let doc = document_with("index.rst", ImageOptions::new(AssetUri::new("logo.svg")));

        // When
        let paths = collect_embed_paths(&doc);

        // Then
        assert!(paths.is_empty());
    }

    #[test]
    fn test_collect_embed_paths_ignores_an_external_url() {
        // Given — fetching it would make the build depend on the network
        let doc = document_with("index.rst", embedding("https://example.com/logo.png"));

        // When
        let paths = collect_embed_paths(&doc);

        // Then
        assert!(paths.is_empty());
    }

    #[test]
    fn test_collect_embed_paths_finds_a_figures_image() {
        // Given
        let figure = Figure::new(embedding("logo.svg"));
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Figure(Box::new(figure)))],
        );

        // When
        let paths = collect_embed_paths(&doc);

        // Then
        assert_eq!(paths, vec![PathBuf::from("logo.svg")]);
    }

    #[test]
    fn test_collect_embed_paths_finds_a_substitution_image() {
        // Given — a `.. |name| image::` definition is a real image too,
        // subject to the same asset handling a standalone `.. image::` gets.
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::SubstitutionDefinition(
                rinx_ast::SubstitutionDefinition {
                    name: "logo".to_string(),
                    kind: rinx_ast::SubstitutionKind::Image(Box::new(embedding("logo.svg"))),
                    span: None,
                },
            ))],
        );

        // When
        let paths = collect_embed_paths(&doc);

        // Then
        assert_eq!(paths, vec![PathBuf::from("logo.svg")]);
    }

    #[test]
    fn test_collect_embed_paths_finds_a_nested_image() {
        // Given — inside an admonition body, which only a full walk reaches
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: rinx_ast::AdmonitionKind::Note,
                title: None,
                collapsible: None,
                body: vec![Node::Directive(Directive::Image(Box::new(embedding(
                    "logo.svg",
                ))))],
            })],
        );

        // When
        let paths = collect_embed_paths(&doc);

        // Then
        assert_eq!(paths, vec![PathBuf::from("logo.svg")]);
    }

    #[test]
    fn test_collect_embed_paths_lists_one_image_once() {
        // Given — the same picture embedded twice on a page
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![
                Node::Directive(Directive::Image(Box::new(embedding("logo.svg")))),
                Node::Directive(Directive::Image(Box::new(embedding("./logo.svg")))),
            ],
        );

        // When
        let paths = collect_embed_paths(&doc);

        // Then — one entry, because both resolve to the same project path
        assert_eq!(paths, vec![PathBuf::from("logo.svg")]);
    }

    #[test]
    fn test_process_embed_assets_builds_the_table() {
        // Given
        let dir = temp_dir_with("process.svg", b"<svg/>");
        let doc = document_with("index.rst", embedding("process.svg"));
        let json = serde_json::to_string(&doc).expect("should serialize");

        // When
        let assets = process_embed_assets(&json, &dir).expect("should embed");

        // Then
        assert_eq!(
            assets.get(Path::new("process.svg")),
            Some("data:image/svg+xml;base64,PHN2Zy8+")
        );
    }

    #[test]
    fn test_process_embed_assets_is_empty_when_nothing_embeds() {
        // Given
        let doc = document_with("index.rst", ImageOptions::new(AssetUri::new("logo.svg")));
        let json = serde_json::to_string(&doc).expect("should serialize");

        // When
        let assets = process_embed_assets(&json, Path::new(".")).expect("should succeed");

        // Then
        assert!(assets.is_empty());
    }

    #[test]
    fn test_embed_available_assets_skips_what_it_cannot_read() {
        // Given — one readable image and one that is not there
        let dir = temp_dir_with("available.svg", b"<svg/>");
        let doc = ast::Document::new(
            "index.rst".to_string(),
            vec![
                Node::Directive(Directive::Image(Box::new(embedding("available.svg")))),
                Node::Directive(Directive::Image(Box::new(embedding("gone.svg")))),
            ],
        );

        // When
        let assets = embed_available_assets(&doc, &dir);

        // Then — the preview keeps what it has rather than failing outright
        assert_eq!(assets.len(), 1);
        assert!(assets.get(Path::new("available.svg")).is_some());
        assert!(assets.get(Path::new("gone.svg")).is_none());
    }

    #[test]
    fn test_process_embed_assets_fails_on_a_missing_file() {
        // Given — what an undeclared image looks like inside a Bazel sandbox
        let doc = document_with("index.rst", embedding("missing.svg"));
        let json = serde_json::to_string(&doc).expect("should serialize");

        // When
        let result = process_embed_assets(&json, Path::new("/nonexistent-embed-root"));

        // Then
        assert!(result.is_err());
    }
}

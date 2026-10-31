//! The second tier of diagnostics: what rendering a document finds.
//!
//! A broken reference, an empty listing, an equation that does not convert —
//! these are found only while rendering, against the whole project index,
//! which is why the parse cannot report them. The render is the build's own
//! (`rinx_renderer`), so the codes and the messages are the build's too; only
//! the page around the body is skipped, since nothing reads its HTML.
//!
//! The tier also carries what folding the project found about the document —
//! a toctree entry naming no document, a label two documents define, a
//! relation to an unknown entity — since the render reads that same fold;
//! the server adds those from `Project::index_diagnostics`.
//!
//! It is far more expensive than a parse, syntax highlighting included, so
//! the server runs it only for the documents it shows and only once edits
//! pause (ADR-038 §9). No `PlantUML` is ever compiled: a diagram's template is
//! expanded, which is all its diagnostics need.
//!
//! The site's configuration is the project's, as far as the server reads it:
//! a Sphinx project's `root_doc`, `numfig` and `highlight_language` from its
//! `conf.py`, its entity schema the one its sphinx-needs configuration
//! converts into; a `rinx_site`'s `rinx.toml`, whether it sets
//! `strict_links`, and the images it bundles, which a `:loading: embed`
//! finds.

use rinx_ast::{Diagnostic, Document, Reported};
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;
use rinx_renderer::config::SiteConfig;
use rinx_renderer::{EmbeddedAssets, EntityTemplates};

use crate::project::BundledFiles;

/// What rendering one document found, each in source order and before any
/// `.. noqa:` is applied — the conversion to the protocol filters these
/// together with the parse's own, as the build filters both before it
/// reports or fails.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderFindings {
    /// What the build reports as a warning.
    pub reported: Vec<Diagnostic>,
    /// What fails the build: the broken references of a site setting
    /// `strict_links`.
    pub failing: Vec<Diagnostic>,
}

/// How a project renders its pages, as far as their findings depend on it.
pub struct RenderSettings<'a> {
    pub config: &'a SiteConfig,
    pub schema: &'a EntitySchema,
    /// Whether a broken reference fails the build — `strict_links`.
    pub strict_links: bool,
    /// The images the site bundles, when the build declares them; one of
    /// them embedded with `:loading: embed` is available to the page.
    pub bundled: Option<&'a BundledFiles>,
}

/// What rendering `document` against `index`, as `settings` say, finds.
#[must_use]
pub fn render_diagnostics(
    document: &Document,
    index: &ProjectIndex,
    settings: &RenderSettings<'_>,
) -> RenderFindings {
    // No project template changes what is reported: an entity is checked
    // against its schema, not its presentation.
    let mut output = rinx_renderer::render_with_assets(
        document,
        index,
        &document.path,
        settings.config,
        &embeddable_images(settings.bundled),
        settings.schema,
        &EntityTemplates::new(),
    );
    if !settings.strict_links {
        return RenderFindings {
            reported: output.diagnostics(),
            failing: Vec::new(),
        };
    }
    let failing = std::mem::take(&mut output.broken_links)
        .iter()
        .map(Reported::to_diagnostic)
        .collect();
    RenderFindings {
        reported: output.diagnostics(),
        failing,
    }
}

/// The images a page may embed: every one `bundled`, the build's embed
/// action reading each declared one. Their bytes change nothing the server
/// reports, so each stands as an empty `data:` URI.
pub(crate) fn embeddable_images(bundled: Option<&BundledFiles>) -> EmbeddedAssets {
    let mut assets = EmbeddedAssets::new();
    for image in bundled.into_iter().flat_map(|bundled| &bundled.images) {
        assets.insert(image, EMPTY_DATA_URI.to_string());
    }
    assets
}

/// What an embedded image's bytes are taken to be.
const EMPTY_DATA_URI: &str = "data:,";

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_analyzer::{DocumentAnalysis, IndexSettings, build_project_index_from_analyses};
    use rinx_ast::DiagnosticCode;
    use std::collections::BTreeMap;

    /// The index the documents `(path, text)` fold into.
    fn index_of(documents: &[(&str, &str)]) -> ProjectIndex {
        let analyses: BTreeMap<String, DocumentAnalysis> = documents
            .iter()
            .map(|(path, text)| {
                let document =
                    rinx_parser::parse(path, text, &rinx_parser::ParseCtx::default()).document;
                ((*path).to_string(), DocumentAnalysis::of(&document, None))
            })
            .collect();
        build_project_index_from_analyses(
            &analyses,
            &IndexSettings::new("index"),
            &EntitySchema::empty(),
        )
        .index
    }

    /// The settings of a project declaring nothing.
    fn defaults() -> RenderSettings<'static> {
        static CONFIG: std::sync::LazyLock<SiteConfig> =
            std::sync::LazyLock::new(SiteConfig::default);
        static SCHEMA: std::sync::LazyLock<EntitySchema> =
            std::sync::LazyLock::new(EntitySchema::empty);
        RenderSettings {
            config: &CONFIG,
            schema: &SCHEMA,
            strict_links: false,
            bundled: None,
        }
    }

    const REFERENCING: &str = "Title\n=====\n\nSee :ref:`install`.\n";

    #[test]
    fn test_render_diagnostics_reports_a_label_no_document_defines() {
        // Given
        let document =
            rinx_parser::parse("index.rst", REFERENCING, &rinx_parser::ParseCtx::default())
                .document;
        let index = index_of(&[("index.rst", REFERENCING)]);

        // When
        let found = render_diagnostics(&document, &index, &defaults()).reported;

        // Then
        let codes: Vec<DiagnosticCode> = found.iter().map(|diagnostic| diagnostic.code).collect();
        assert_eq!(codes, [DiagnosticCode::LinkBrokenRef]);
        assert_eq!(found[0].message, "broken ref 'install'");
    }

    #[test]
    fn test_render_diagnostics_finds_nothing_once_another_document_defines_it() {
        // Given
        let document =
            rinx_parser::parse("index.rst", REFERENCING, &rinx_parser::ParseCtx::default())
                .document;
        let index = index_of(&[
            ("index.rst", REFERENCING),
            ("setup.rst", ".. _install:\n\nInstalling\n==========\n"),
        ]);

        // When / Then
        assert_eq!(
            render_diagnostics(&document, &index, &defaults()).reported,
            Vec::new()
        );
    }

    #[test]
    fn test_render_diagnostics_reads_a_doc_name_relative_to_the_document() {
        // Given — `../setup` from `guide/` names `setup.rst`, which exists
        let text = "See :doc:`../setup` and :doc:`gone`.\n";
        let document =
            rinx_parser::parse("guide/intro.rst", text, &rinx_parser::ParseCtx::default()).document;
        let index = index_of(&[("guide/intro.rst", text), ("setup.rst", "Setup\n=====\n")]);

        // When
        let found = render_diagnostics(&document, &index, &defaults()).reported;

        // Then
        let messages: Vec<&str> = found
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(messages, ["broken doc reference 'gone'"]);
    }

    #[test]
    fn test_render_diagnostics_fails_on_a_broken_reference_under_strict_links() {
        // Given
        let document =
            rinx_parser::parse("index.rst", REFERENCING, &rinx_parser::ParseCtx::default())
                .document;
        let index = index_of(&[("index.rst", REFERENCING)]);
        let settings = RenderSettings {
            strict_links: true,
            ..defaults()
        };

        // When
        let found = render_diagnostics(&document, &index, &settings);

        // Then
        let codes: Vec<DiagnosticCode> = found
            .failing
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect();
        assert_eq!(codes, [DiagnosticCode::LinkBrokenRef]);
        assert!(found.reported.is_empty(), "{:?}", found.reported);
    }

    const EMBEDDING: &str = ".. image:: logo.png\n   :loading: embed\n";

    #[test]
    fn test_render_diagnostics_finds_an_undeclared_image_unavailable_to_embed() {
        // Given
        let document =
            rinx_parser::parse("index.rst", EMBEDDING, &rinx_parser::ParseCtx::default()).document;
        let index = index_of(&[("index.rst", EMBEDDING)]);

        // When
        let found = render_diagnostics(&document, &index, &defaults());

        // Then
        let codes: Vec<DiagnosticCode> = found
            .reported
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect();
        assert_eq!(codes, [DiagnosticCode::ImageEmbedUnavailable]);
    }

    #[test]
    fn test_render_diagnostics_embeds_an_image_the_site_bundles() {
        // Given
        let document =
            rinx_parser::parse("index.rst", EMBEDDING, &rinx_parser::ParseCtx::default()).document;
        let index = index_of(&[("index.rst", EMBEDDING)]);
        let bundled = BundledFiles {
            images: std::collections::BTreeSet::from([std::path::PathBuf::from("logo.png")]),
            downloads: std::collections::BTreeSet::new(),
        };
        let settings = RenderSettings {
            bundled: Some(&bundled),
            ..defaults()
        };

        // When
        let found = render_diagnostics(&document, &index, &settings);

        // Then
        assert_eq!(found, RenderFindings::default());
    }

    const CONFIGURED: &str = ".. plantuml::\n   :config: monochrome\n\n   A -> B\n";

    #[test]
    fn test_render_diagnostics_reports_a_preamble_the_site_does_not_declare() {
        // Given
        let document =
            rinx_parser::parse("index.rst", CONFIGURED, &rinx_parser::ParseCtx::default()).document;
        let index = index_of(&[("index.rst", CONFIGURED)]);

        // When
        let found = render_diagnostics(&document, &index, &defaults()).reported;

        // Then
        let codes: Vec<DiagnosticCode> = found.iter().map(|diagnostic| diagnostic.code).collect();
        assert_eq!(codes, [DiagnosticCode::UmlUnknownConfig]);
    }

    #[test]
    fn test_render_diagnostics_finds_nothing_once_the_site_declares_the_preamble() {
        // Given
        let document =
            rinx_parser::parse("index.rst", CONFIGURED, &rinx_parser::ParseCtx::default()).document;
        let index = index_of(&[("index.rst", CONFIGURED)]);
        let config = SiteConfig {
            uml_configs: std::collections::BTreeMap::from([(
                "monochrome".to_string(),
                "skinparam monochrome true".to_string(),
            )]),
            ..SiteConfig::default()
        };
        let settings = RenderSettings {
            config: &config,
            ..defaults()
        };

        // When / Then
        assert_eq!(
            render_diagnostics(&document, &index, &settings),
            RenderFindings::default()
        );
    }
}

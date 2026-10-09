//! The second tier of diagnostics: what rendering a document finds.
//!
//! A broken reference, an empty listing, an equation that does not convert —
//! these are found only while rendering, against the whole project index,
//! which is why the parse cannot report them. The render is the build's own
//! (`rinx_renderer`), so the codes and the messages are the build's too; only
//! the page around the body is skipped, since nothing reads its HTML.
//!
//! It is far more expensive than a parse, syntax highlighting included, so
//! the server runs it only for the documents it shows and only once edits
//! pause (ADR-038 §9). No `PlantUML` is ever compiled: a diagram's template is
//! expanded, which is all its diagnostics need.
//!
//! The site's configuration is the project's, as far as the server reads it:
//! a Sphinx project's `root_doc`, `numfig` and `highlight_language` from its
//! `conf.py`. Inventories come with roadmap #13 and #18, so until then an
//! intersphinx reference is reported as a site with none would.

use rinx_ast::{Diagnostic, Document};
use rinx_index::ProjectIndex;
use rinx_renderer::config::SiteConfig;

/// What rendering `document` against `index` finds, in source order and
/// before any `.. noqa:` is applied — the conversion to the protocol filters
/// these together with the parse's own.
#[must_use]
pub fn render_diagnostics(
    document: &Document,
    index: &ProjectIndex,
    config: &SiteConfig,
) -> Vec<Diagnostic> {
    rinx_renderer::render_with_config(document, index, &document.path, config).diagnostics()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_analyzer::{DocumentAnalysis, IndexSettings, build_project_index_from_analyses};
    use rinx_ast::DiagnosticCode;
    use rinx_entity::EntitySchema;
    use std::collections::BTreeMap;

    /// The index the documents `(path, text)` fold into.
    fn index_of(documents: &[(&str, &str)]) -> ProjectIndex {
        let analyses: BTreeMap<String, DocumentAnalysis> = documents
            .iter()
            .map(|(path, text)| {
                let document = rinx_parser::parse(path, text);
                ((*path).to_string(), DocumentAnalysis::of(&document))
            })
            .collect();
        build_project_index_from_analyses(
            &analyses,
            &IndexSettings::new("index"),
            &EntitySchema::empty(),
        )
        .index
    }

    const REFERENCING: &str = "Title\n=====\n\nSee :ref:`install`.\n";

    #[test]
    fn test_render_diagnostics_reports_a_label_no_document_defines() {
        // Given
        let document = rinx_parser::parse("index.rst", REFERENCING);
        let index = index_of(&[("index.rst", REFERENCING)]);

        // When
        let found = render_diagnostics(&document, &index, &SiteConfig::default());

        // Then
        let codes: Vec<DiagnosticCode> = found.iter().map(|diagnostic| diagnostic.code).collect();
        assert_eq!(codes, [DiagnosticCode::LinkBrokenRef]);
        assert_eq!(found[0].message, "broken ref 'install'");
    }

    #[test]
    fn test_render_diagnostics_finds_nothing_once_another_document_defines_it() {
        // Given
        let document = rinx_parser::parse("index.rst", REFERENCING);
        let index = index_of(&[
            ("index.rst", REFERENCING),
            ("setup.rst", ".. _install:\n\nInstalling\n==========\n"),
        ]);

        // When / Then
        assert_eq!(
            render_diagnostics(&document, &index, &SiteConfig::default()),
            Vec::new()
        );
    }

    #[test]
    fn test_render_diagnostics_reads_a_doc_name_relative_to_the_document() {
        // Given — `../setup` from `guide/` names `setup.rst`, which exists
        let text = "See :doc:`../setup` and :doc:`gone`.\n";
        let document = rinx_parser::parse("guide/intro.rst", text);
        let index = index_of(&[("guide/intro.rst", text), ("setup.rst", "Setup\n=====\n")]);

        // When
        let found = render_diagnostics(&document, &index, &SiteConfig::default());

        // Then
        let messages: Vec<&str> = found
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(messages, ["broken doc reference 'gone'"]);
    }
}

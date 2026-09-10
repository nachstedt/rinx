//! The `preview` subcommand: collapses parse+local-analyze+merge+render into
//! one process reading RST from stdin, for low-latency editor use.

use anyhow::{Context, Result};
use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer::{self as renderer, config};
use std::fs;
use std::io::{self, Read};

use super::cli_args::{flag_value, flag_value_opt};
use super::diagnostics::{
    WarningOrigin, format_broken_link_warning, format_entity_table_error_warning,
    format_highlight_error_warning, format_image_error_warning, format_math_error_warning,
    format_object_type_mismatch_warning, report_diagnostic,
};
use super::embed_assets::embed_available_assets;
use super::entity_schema::{load_entity_schema, load_entity_templates};
use super::parse::parse_default_domain_flag;
use super::parse_files::DocumentRelativeFiles;
use super::parse_inputs::ParseInputs;
use super::suppression::{
    retain_reportable, retain_reportable_entity_table_errors, retain_reportable_highlight_errors,
    retain_reportable_image_errors, retain_reportable_links, retain_reportable_math_errors,
    retain_reportable_mismatches,
};

/// One previewed page, plus every warning it produced.
///
/// A struct rather than a tuple because the warning kinds are no longer two:
/// naming them at the call site is what keeps `math_errors` from being read as
/// `object_type_mismatches`. Mirrors [`super::render::RenderedPage`], minus
/// the `source_path` — a preview is always rendering the document the caller
/// named, so there is no second path to disambiguate.
pub(super) struct PreviewedPage {
    pub html: String,
    pub broken_links: Vec<renderer::BrokenLink>,
    pub object_type_mismatches: Vec<renderer::ObjectTypeMismatch>,
    pub math_errors: Vec<renderer::MathError>,
    /// Listing directives whose filter matched no entity. Always empty when
    /// the caller supplied no global index — see [`process_preview`].
    pub entity_table_errors: Vec<renderer::EntityTableError>,
    pub highlight_errors: Vec<renderer::HighlightError>,
    pub image_errors: Vec<renderer::ImageError>,
    /// The files this document included, so the caller's warnings can resolve
    /// a span that belongs to one of them — see
    /// [`WarningOrigin`](super::diagnostics::WarningOrigin).
    pub source_files: Vec<String>,
}

pub(super) fn process_preview(
    rst: &str,
    index_json: Option<&str>,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
    inputs: &ParseInputs<'_>,
    entity_templates: &renderer::EntityTemplates,
) -> Result<PreviewedPage> {
    let doc = parser::parse_with_ctx(doc_path, rst, &inputs.ctx());
    let origin = WarningOrigin::new(doc_path, &doc.source_files);
    for diagnostic in retain_reportable(&doc.diagnostics, &doc.suppressions) {
        report_diagnostic(&origin, diagnostic);
    }
    let mut index = if let Some(json) = index_json {
        serde_json::from_str(json).context("Failed to deserialize global index")?
    } else {
        rusty_sphinx_index::ProjectIndex::default()
    };

    let local_index = analyzer::analyze(&doc);
    let _ = index.merge(local_index);

    // Read straight from the filesystem rather than from a sidecar: a preview
    // has the real working tree in front of it, and no build step to run.
    let embedded_assets = embed_available_assets(&doc, std::path::Path::new("."));
    let render_output = renderer::render_with_assets(
        &doc,
        &index,
        doc_path,
        config,
        &embedded_assets,
        inputs.schema,
        entity_templates,
    );
    // Filtered here rather than by the caller so that a suppressed link is
    // invisible to *every* consumer of this function, not just the one that
    // remembers to ask.
    let broken_links = retain_reportable_links(&render_output.broken_links, &doc.suppressions);
    let object_type_mismatches =
        retain_reportable_mismatches(&render_output.object_type_mismatches, &doc.suppressions);
    let math_errors = retain_reportable_math_errors(&render_output.math_errors, &doc.suppressions);
    // An empty listing is reported only when there *is* a project to list. With
    // no global index the graph is unknown, so every table would be empty
    // through no fault of the author — the one case this diagnostic must stay
    // quiet in, and the reason it is dropped here rather than never raised.
    let entity_table_errors = if index_json.is_some() {
        retain_reportable_entity_table_errors(&render_output.entity_table_errors, &doc.suppressions)
    } else {
        Vec::new()
    };
    let highlight_errors =
        retain_reportable_highlight_errors(&render_output.highlight_errors, &doc.suppressions);
    let image_errors =
        retain_reportable_image_errors(&render_output.image_errors, &doc.suppressions);

    // Extract page title from the first H1 heading, if any.
    let page_title = doc
        .nodes
        .iter()
        .find_map(|n| {
            if let ast::Node::Heading { level: 1, text } = n {
                Some(ast::inline_plain_text(text))
            } else {
                None
            }
        })
        .unwrap_or_else(|| doc_path.to_string());

    let depth = doc_path.matches('/').count();
    let css_path = if depth == 0 {
        "default.css".to_string()
    } else {
        format!("{}default.css", "../".repeat(depth))
    };

    let html = renderer::render_page(
        &render_output.html,
        template_str,
        config,
        &renderer::PageMeta {
            css_path: &css_path,
            page_title: &page_title,
            doc_path,
            source_path: &doc.path,
            has_genindex: !index.genindex_entries.is_empty(),
            ..renderer::PageMeta::default()
        }
        .with_navigation(&index),
    )?;
    Ok(PreviewedPage {
        html,
        broken_links,
        object_type_mismatches,
        math_errors,
        entity_table_errors,
        highlight_errors,
        image_errors,
        source_files: doc.source_files,
    })
}

pub(crate) fn cmd_preview(args: &[String]) -> Result<()> {
    let index_path = flag_value_opt(args, "--index");
    let doc_path = flag_value(args, "--doc-path")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;
    let default_domain = parse_default_domain_flag(args)?;

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

    let schema = load_entity_schema(args)?;
    let page = process_preview(
        &rst,
        index_json.as_deref(),
        &site_config,
        &template_str,
        &doc_path,
        &ParseInputs {
            default_domain,
            // The editor previews a real file on disk, so `:file:` resolves
            // against its directory exactly as it does in `parse`.
            files: &DocumentRelativeFiles::for_document(&doc_path),
            schema: &schema,
        },
        &load_entity_templates(args)?,
    )?;

    // Preview is deliberately lenient (it renders over incomplete/WIP
    // documents), so broken links are always warnings, never a failure.
    let origin = WarningOrigin::new(&doc_path, &page.source_files);
    for link in &page.broken_links {
        eprintln!("{}", format_broken_link_warning(&origin, link));
    }
    for mismatch in &page.object_type_mismatches {
        eprintln!("{}", format_object_type_mismatch_warning(&origin, mismatch));
    }
    for error in &page.math_errors {
        eprintln!("{}", format_math_error_warning(&origin, error));
    }
    for error in &page.entity_table_errors {
        eprintln!("{}", format_entity_table_error_warning(&origin, error));
    }
    for error in &page.highlight_errors {
        eprintln!("{}", format_highlight_error_warning(&origin, error));
    }
    for error in &page.image_errors {
        eprintln!("{}", format_image_error_warning(&origin, error));
    }

    println!("{}", page.html);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_entity::EntitySchema;

    /// A loader rooted at a directory holding no CSV files, for the tests
    /// whose input has no `:file:` option.
    fn no_parse_files() -> DocumentRelativeFiles {
        DocumentRelativeFiles::for_document("test.rst")
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
        let page = process_preview(
            rst,
            Some(global_index),
            &config,
            template,
            "test.rst",
            &ParseInputs {
                default_domain: ast::Domain::Py,
                files: &no_parse_files(),
                schema: &EntitySchema::empty(),
            },
            &renderer::EntityTemplates::new(),
        )
        .unwrap();

        // Then
        assert!(page.html.contains("<h1 id=\"section-a\">Section A</h1>"));
        // Cross-reference to other file should be resolved
        assert!(page.html.contains("href=\"other.html#section-b\""));
        assert!(page.broken_links.is_empty());
    }

    #[test]
    fn test_process_preview_works_without_global_index() {
        // Given
        let rst = "Section A\n=========";
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let page = process_preview(
            rst,
            None,
            &config,
            template,
            "test.rst",
            &ParseInputs {
                default_domain: ast::Domain::Py,
                files: &no_parse_files(),
                schema: &EntitySchema::empty(),
            },
            &renderer::EntityTemplates::new(),
        )
        .unwrap();

        // Then
        assert!(page.html.contains("<h1 id=\"section-a\">Section A</h1>"));
        assert!(page.broken_links.is_empty());
    }

    #[test]
    fn test_process_preview_reports_broken_reference_without_failing() {
        // Given — preview is lenient: an unresolved :ref: is a diagnostic, not an error
        let rst = "Section A\n=========\n\nSee :ref:`missing`";
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let page = process_preview(
            rst,
            None,
            &config,
            template,
            "test.rst",
            &ParseInputs {
                default_domain: ast::Domain::Py,
                files: &no_parse_files(),
                schema: &EntitySchema::empty(),
            },
            &renderer::EntityTemplates::new(),
        )
        .unwrap();

        // Then
        assert!(page.html.contains("class=\"broken-link\""));
        assert_eq!(page.broken_links.len(), 1);
        assert_eq!(page.broken_links[0].target, "missing");
    }
}

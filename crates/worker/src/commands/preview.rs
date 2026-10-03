//! The `preview` subcommand: collapses parse+local-analyze+merge+render into
//! one process reading RST from stdin, for low-latency editor use.

use anyhow::{Context, Result};
use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_parser as parser;
use rinx_renderer::{self as renderer, config};
use std::fs;
use std::io::{self, Read};

use super::cli_args::{flag_value, flag_value_opt};
use super::diagnostics::{
    WarningOrigin, format_broken_link_warning, format_diagram_error_warning,
    format_empty_listing_error_warning, format_highlight_error_warning, format_image_error_warning,
    format_math_error_warning, format_object_type_mismatch_warning, report_diagnostic,
};
use super::embed_assets::embed_available_assets;
use rinx_ast::retain_reportable;

use super::entity_schema::{import_keys_from_args, load_entity_schema, load_entity_templates};
use super::parse::parse_default_domain_flag;
use super::parse_files::DocumentRelativeFiles;
use super::parse_inputs::{ParseInputs, default_role_from_args, jinja_from_args};

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
    pub empty_listing_errors: Vec<renderer::EmptyListingError>,
    pub diagram_errors: Vec<renderer::DiagramError>,
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
        rinx_index::ProjectIndex::default()
    };

    let local_index = analyzer::analyze(&doc);
    let _ = index.merge(local_index);
    // Figure numbers are project-wide, so the stale index's would miss a
    // figure this edit added and shift every one after it. Renumbering is one
    // cheap walk over data the merge just brought up to date — unlike section
    // numbers, which stay stale until the next build.
    let universe = index.documents.clone();
    index.element_numbers =
        analyzer::assign_element_numbers(&index, &universe, config.numfig_secnum_depth);

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
    // With no global index nothing has a number, so every `:numref:` would be
    // reported as unnumbered through no fault of the author — dropped here for
    // the reason an empty listing is below.
    let broken_links = retain_reportable(&render_output.broken_links, &doc.suppressions)
        .filter(|link| {
            index_json.is_some() || link.kind != renderer::BrokenLinkKind::UnnumberedReference
        })
        .cloned()
        .collect();
    let object_type_mismatches =
        retain_reportable(&render_output.object_type_mismatches, &doc.suppressions)
            .cloned()
            .collect();
    let math_errors = retain_reportable(&render_output.math_errors, &doc.suppressions)
        .cloned()
        .collect();
    // An empty listing is reported only when there *is* a project to list. With
    // no global index the graph is unknown, so every table would be empty
    // through no fault of the author — the one case this diagnostic must stay
    // quiet in, and the reason it is dropped here rather than never raised.
    let empty_listing_errors = if index_json.is_some() {
        retain_reportable(&render_output.empty_listing_errors, &doc.suppressions)
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    // A template asking about an entity is asking about the whole project, so
    // with no global index every `need('REQ_001')` would fail through no fault
    // of the author. Dropped for the same reason an empty listing is, and only
    // for the failures that depend on the graph: a *syntax* error in the
    // template is the author's either way, and is worth seeing in an editor
    // long before a build runs.
    let diagram_errors = retain_reportable(&render_output.diagram_errors, &doc.suppressions)
        .filter(|error| index_json.is_some() || !error.depends_on_the_project())
        .cloned()
        .collect();
    let highlight_errors = retain_reportable(&render_output.highlight_errors, &doc.suppressions)
        .cloned()
        .collect();
    let image_errors = retain_reportable(&render_output.image_errors, &doc.suppressions)
        .cloned()
        .collect();

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
            has_genindex: index.has_genindex_entries(),
            ..renderer::PageMeta::default()
        }
        .with_navigation(&index, config),
    )?;
    Ok(PreviewedPage {
        html,
        broken_links,
        object_type_mismatches,
        math_errors,
        empty_listing_errors,
        diagram_errors,
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
    // The editor's buffer is templated exactly as the file on disk would be,
    // which is the whole point of the preview sharing `ParseInputs`.
    let jinja = jinja_from_args(args)?;
    let import_keys = import_keys_from_args(args, &schema);
    let default_role = default_role_from_args(args, &schema)?;
    let page = process_preview(
        &rst,
        index_json.as_deref(),
        &site_config,
        &template_str,
        &doc_path,
        &ParseInputs {
            default_domain,
            default_role: &default_role,
            // The editor previews a real file on disk, so `:file:` resolves
            // against its directory exactly as it does in `parse`.
            files: &DocumentRelativeFiles::for_document(&doc_path),
            schema: &schema,
            jinja: jinja.as_deref(),
            import_keys: &import_keys,
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
    for error in &page.empty_listing_errors {
        eprintln!("{}", format_empty_listing_error_warning(&origin, error));
    }
    for error in &page.diagram_errors {
        eprintln!("{}", format_diagram_error_warning(&origin, error));
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
    use rinx_entity::EntitySchema;
    use std::collections::BTreeMap;

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
                default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
                files: &no_parse_files(),
                schema: &EntitySchema::empty(),
                jinja: None,
                import_keys: &BTreeMap::new(),
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
                default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
                files: &no_parse_files(),
                schema: &EntitySchema::empty(),
                jinja: None,
                import_keys: &BTreeMap::new(),
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
                default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
                files: &no_parse_files(),
                schema: &EntitySchema::empty(),
                jinja: None,
                import_keys: &BTreeMap::new(),
            },
            &renderer::EntityTemplates::new(),
        )
        .unwrap();

        // Then
        assert!(page.html.contains("class=\"broken-link\""));
        assert_eq!(page.broken_links.len(), 1);
        assert_eq!(page.broken_links[0].target, "missing");
    }

    fn preview(rst: &str, index_json: Option<&str>, config: &config::SiteConfig) -> PreviewedPage {
        process_preview(
            rst,
            index_json,
            config,
            "<html>{{ body }}</html>",
            "test.rst",
            &ParseInputs {
                default_domain: ast::Domain::Py,
                default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
                files: &no_parse_files(),
                schema: &EntitySchema::empty(),
                jinja: None,
                import_keys: &BTreeMap::new(),
            },
            &renderer::EntityTemplates::new(),
        )
        .unwrap()
    }

    #[test]
    fn test_process_preview_renumbers_a_figure_the_stale_index_never_saw() {
        // Given a stale index naming this page its root, and an edit adding a
        // figure to it
        let rst = "Title\n=====\n\n.. figure:: a.png\n   :name: new-fig\n\n   New\n\n\
                   See :numref:`new-fig`.";
        let global_index = r#"{"targets":{},"document_titles":{},"root_documents":["test.rst"]}"#;
        let config = config::SiteConfig {
            numfig: true,
            ..config::SiteConfig::default()
        };

        // When
        let page = preview(rst, Some(global_index), &config);

        // Then
        assert!(
            page.html.contains("caption-number\">Fig. 1 <"),
            "{}",
            page.html
        );
        assert!(page.html.contains(">Fig. 1</span></a>"), "{}", page.html);
        assert!(page.broken_links.is_empty(), "{:?}", page.broken_links);
    }

    #[test]
    fn test_process_preview_stays_quiet_about_unnumbered_references_without_an_index() {
        // Given no global index, so nothing can have a number
        let rst = ".. figure:: a.png\n   :name: fig\n\n   Caption\n\nSee :numref:`fig`.";
        let config = config::SiteConfig {
            numfig: true,
            ..config::SiteConfig::default()
        };

        // When
        let page = preview(rst, None, &config);

        // Then
        assert!(page.broken_links.is_empty(), "{:?}", page.broken_links);
    }
}

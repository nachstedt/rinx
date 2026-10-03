//! The `render` subcommand: one AST + the global index to a final HTML page.

use anyhow::{Context, Result};
use rinx::domain_warnings;
use rinx_ast as ast;
use rinx_renderer::{self as renderer, config};
use std::fs;

use super::cli_args::{flag_value, flag_value_opt};
use super::diagnostics::{
    WarningOrigin, check_broken_links_strict, format_broken_link_warning,
    format_diagram_error_warning, format_empty_listing_error_warning,
    format_highlight_error_warning, format_image_error_warning, format_math_error_warning,
    format_object_type_mismatch_warning,
};
use rinx_ast::retain_reportable;

use super::entity_schema::{load_entity_schema, load_entity_templates};

/// One rendered page, plus everything the caller reports about it.
///
/// `source_path` is the `.rst` the document was parsed from, which is *not*
/// the `doc_path` the render is keyed on: that one is the site-relative
/// logical path (`guide/intro`), used to compute links between pages. A
/// warning has to name a file a reader can actually open, so it uses this.
pub(super) struct RenderedPage {
    pub html: String,
    pub source_path: String,
    /// The files this document included, so a warning about a span inside one
    /// of them names the fragment rather than this document — see
    /// [`WarningOrigin`](super::diagnostics::WarningOrigin).
    pub source_files: Vec<String>,
    pub broken_links: Vec<renderer::BrokenLink>,
    pub object_type_mismatches: Vec<renderer::ObjectTypeMismatch>,
    pub math_errors: Vec<renderer::MathError>,
    /// Listing directives whose filter matched no entity.
    pub empty_listing_errors: Vec<renderer::EmptyListingError>,
    pub diagram_errors: Vec<renderer::DiagramError>,
    /// The `PlantUML` text of each diagram on the page, for the compile action.
    pub diagram_sources: Vec<ast::HashedContent>,
    pub highlight_errors: Vec<renderer::HighlightError>,
    pub image_errors: Vec<renderer::ImageError>,
    /// Entity templates the schema named but the site could not use.
    pub entity_template_errors: Vec<String>,
}

/// The entity-side inputs a render works from.
///
/// One value rather than two parameters, because they are read together and by
/// nothing else: the schema supplies the vocabulary and the labels, and the
/// templates supply the presentation a type may ask for by name.
pub(super) struct EntityInputs<'a> {
    pub schema: &'a rinx_entity::EntitySchema,
    pub templates: &'a renderer::EntityTemplates,
}

pub(super) fn process_render(
    ast_json: &str,
    index_json: &str,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
    embedded_assets: &renderer::EmbeddedAssets,
    entities: &EntityInputs<'_>,
) -> Result<RenderedPage> {
    let doc: ast::Document =
        serde_json::from_str(ast_json).context("Failed to deserialize AST document")?;
    let index: rinx_index::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;

    let render_output = renderer::render_with_assets(
        &doc,
        &index,
        doc_path,
        config,
        embedded_assets,
        entities.schema,
        entities.templates,
    );

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
        .unwrap_or_default();

    let css_path = renderer::css_relative_path(doc_path, "default.css");
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
    Ok(RenderedPage {
        html,
        // Filtered here rather than by the caller so that a suppressed link is
        // invisible to *every* consumer — the warning it would print, the
        // `--strict-links` failure it would cause, and the sidecar it would
        // appear in. Silencing only the message would leave the consequence.
        broken_links: retain_reportable(&render_output.broken_links, &doc.suppressions)
            .cloned()
            .collect(),
        object_type_mismatches: retain_reportable(
            &render_output.object_type_mismatches,
            &doc.suppressions,
        )
        .cloned()
        .collect(),
        math_errors: retain_reportable(&render_output.math_errors, &doc.suppressions)
            .cloned()
            .collect(),
        empty_listing_errors: retain_reportable(
            &render_output.empty_listing_errors,
            &doc.suppressions,
        )
        .cloned()
        .collect(),
        diagram_errors: retain_reportable(&render_output.diagram_errors, &doc.suppressions)
            .cloned()
            .collect(),
        diagram_sources: render_output.diagram_sources,
        highlight_errors: retain_reportable(&render_output.highlight_errors, &doc.suppressions)
            .cloned()
            .collect(),
        image_errors: retain_reportable(&render_output.image_errors, &doc.suppressions)
            .cloned()
            .collect(),
        // Not filtered by `.. noqa:`: a misconfigured template is a fault in
        // the *site*, not in any document, so no document's comment should be
        // able to silence it.
        entity_template_errors: render_output.entity_template_errors,
        source_path: doc.path,
        source_files: doc.source_files,
    })
}

pub(crate) fn cmd_render(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let index_path = flag_value(args, "--index")?;
    let output = flag_value(args, "--output")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;
    let doc_path = flag_value(args, "--doc-path")?;
    let strict_links = args.iter().any(|a| a == "--strict-links");
    let warnings_output = flag_value_opt(args, "--warnings-output");
    let embeds_path = flag_value_opt(args, "--embeds");
    // Given only for a document whose library set `diagrams = True`. The site
    // rule declares the directory as an output of exactly those renders, so a
    // library that draws nothing gets no directory and no compile action.
    let diagram_outdir = flag_value_opt(args, "--diagram-outdir");

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

    // Absent whenever the document embeds nothing, which is the common case:
    // an empty table renders every image as an ordinary link.
    let embedded_assets = match &embeds_path {
        Some(path) => {
            let json = fs::read_to_string(path)
                .with_context(|| format!("Error reading embedded assets '{path}'"))?;
            serde_json::from_str(&json)
                .with_context(|| format!("Error parsing embedded assets '{path}'"))?
        }
        None => renderer::EmbeddedAssets::new(),
    };

    let schema = load_entity_schema(args)?;
    let entity_templates = load_entity_templates(args)?;
    let page = process_render(
        &ast_json,
        &index_json,
        &site_config,
        &template_str,
        &doc_path,
        &embedded_assets,
        &EntityInputs {
            schema: &schema,
            templates: &entity_templates,
        },
    )?;

    // Warnings name the `.rst` the document came from, not the site-relative
    // `doc_path` — a `file:line:column` is only useful if the file opens.
    for message in &page.entity_template_errors {
        eprintln!("error: {message}");
    }
    if !page.entity_template_errors.is_empty() {
        return Err(anyhow::anyhow!(
            "{}: {} entity template(s) could not be used",
            page.source_path,
            page.entity_template_errors.len()
        ));
    }

    let origin = WarningOrigin::new(&page.source_path, &page.source_files);
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

    // Emit the structured domain-object warning sidecar when requested. Written
    // unconditionally (even with zero warnings) and before the strict-links
    // check, so the file always exists as a Bazel-declared output and reflects
    // reality regardless of whether the strict check then fails the render.
    if let Some(warnings_path) = &warnings_output {
        let report = domain_warnings::build_domain_warning_report(
            &doc_path,
            &page.broken_links,
            &page.object_type_mismatches,
        );
        let report_json = serde_json::to_string_pretty(&report)
            .context("Failed to serialize domain warning report")?;
        fs::write(warnings_path, report_json)
            .with_context(|| format!("Error writing '{warnings_path}'"))?;
    }

    check_broken_links_strict(strict_links, &origin, &page.broken_links)?;

    if let Some(outdir) = &diagram_outdir {
        write_diagram_sources(&page.diagram_sources, outdir)?;
    }

    fs::write(&output, page.html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

/// Writes each diagram's `PlantUML` text as `<hash>.puml` under `outdir`.
///
/// The hash is the one the page's `<img>` was built from, in this same render,
/// so the compile action downstream produces exactly the SVG the page names.
/// The directory is created even when there is nothing to write: it is a
/// declared Bazel output, and a document with no diagrams still has to
/// produce it.
fn write_diagram_sources(sources: &[ast::HashedContent], outdir: &str) -> Result<()> {
    fs::create_dir_all(outdir).with_context(|| format!("Error creating '{outdir}'"))?;
    for source in sources {
        let path = std::path::Path::new(outdir).join(format!("{}.puml", source.hash()));
        fs::write(&path, source.body())
            .with_context(|| format!("Error writing {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_entity::EntitySchema;

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
        dir
    }

    #[test]
    fn test_write_diagram_sources_names_each_file_by_its_hash() {
        // Given
        let source = ast::HashedContent::new("@startuml\nA -> B\n@enduml".to_string());
        let dir = temp_dir("diagram_sources");

        // When
        write_diagram_sources(std::slice::from_ref(&source), dir.to_str().unwrap()).unwrap();

        // Then
        let written = fs::read_to_string(dir.join(format!("{}.puml", source.hash()))).unwrap();
        assert_eq!(written, source.body());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_write_diagram_sources_creates_the_directory_even_when_empty() {
        // Given — a declared Bazel output must exist however many diagrams the
        // document turned out to hold
        let dir = temp_dir("diagram_sources_empty");

        // When
        write_diagram_sources(&[], dir.to_str().unwrap()).unwrap();

        // Then
        assert!(dir.is_dir());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_process_render_links_the_module_index_only_when_the_index_enables_it() {
        // Given
        let doc = r#"{"path":"guide/intro.rst","nodes":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{% if modindex_href %}{{ modindex_href }}{% endif %}";
        let enabled = rinx_index::ProjectIndex {
            domain_indices: [rinx_index::DomainIndex::PyModindex].into(),
            ..rinx_index::ProjectIndex::default()
        };
        let render = |index: &rinx_index::ProjectIndex| {
            process_render(
                doc,
                &serde_json::to_string(index).unwrap(),
                &config,
                template,
                "guide/intro.rst",
                &renderer::EmbeddedAssets::new(),
                &EntityInputs {
                    schema: &EntitySchema::empty(),
                    templates: &renderer::EntityTemplates::new(),
                },
            )
            .unwrap()
            .html
        };

        // When
        let with = render(&enabled);
        let without = render(&rinx_index::ProjectIndex::default());

        // Then
        assert_eq!(with, "../py-modindex.html");
        assert_eq!(without, "");
    }

    #[test]
    fn test_process_render_returns_html_string() {
        // Given
        let doc =
            r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":[{"Text":"Title"}]}}]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let page = process_render(
            doc,
            index,
            &config,
            template,
            "test.rst",
            &renderer::EmbeddedAssets::new(),
            &EntityInputs {
                schema: &EntitySchema::empty(),
                templates: &renderer::EntityTemplates::new(),
            },
        )
        .unwrap();

        // Then
        assert!(page.html.contains("<h1 id=\"title\">Title</h1>"));
        assert!(page.broken_links.is_empty());
        assert!(page.object_type_mismatches.is_empty());
        // The warning path comes from the document, not from `doc_path`.
        assert_eq!(page.source_path, "test.rst");
    }

    #[test]
    fn test_process_render_reports_broken_reference() {
        // Given
        let doc = r#"{"path":"test.rst","nodes":[{"Paragraph":[{"Reference":{"display":"missing","target":"missing"}}]}]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let page = process_render(
            doc,
            index,
            &config,
            template,
            "test.rst",
            &renderer::EmbeddedAssets::new(),
            &EntityInputs {
                schema: &EntitySchema::empty(),
                templates: &renderer::EntityTemplates::new(),
            },
        )
        .unwrap();

        // Then
        assert_eq!(page.broken_links.len(), 1);
        assert_eq!(page.broken_links[0].target, "missing");
    }

    #[test]
    fn test_process_render_reports_object_type_mismatch() {
        // Given — mirrors CPython's `xmlrpc.client.rst`: `Fault` is defined
        // via `.. class::` but referenced via `:exc:`.
        let doc = r#"{"path":"test.rst","nodes":[
            {"Directive":{"DomainObject":{"PyClass":{"signatures":["Fault"],"is_final":false,"body":[]}}}},
            {"Paragraph":[{"DomainObjectReference":{"object_type":"py:exception","name":"Fault","display":"Fault","link":true}}]}
        ]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[],"glossary_terms":{},"domain_objects":{"fault":{"py:class":"test.rst"}},"genindex_entries":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let page = process_render(
            doc,
            index,
            &config,
            template,
            "test.rst",
            &renderer::EmbeddedAssets::new(),
            &EntityInputs {
                schema: &EntitySchema::empty(),
                templates: &renderer::EntityTemplates::new(),
            },
        )
        .unwrap();

        // Then — resolved, not broken, but flagged as a mismatch
        assert!(page.broken_links.is_empty());
        assert_eq!(page.object_type_mismatches.len(), 1);
        assert_eq!(page.object_type_mismatches[0].name, "Fault");
        assert_eq!(
            page.object_type_mismatches[0].requested_type,
            ast::ObjectType::Py(ast::PyObjectType::Exception)
        );
        assert_eq!(
            page.object_type_mismatches[0].resolved_type,
            ast::ObjectType::Py(ast::PyObjectType::Class)
        );
    }
}

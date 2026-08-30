//! The `render` subcommand: one AST + the global index to a final HTML page.

use anyhow::{Context, Result};
use rusty_sphinx_ast as ast;
use rusty_sphinx_renderer::{self as renderer, config};
use rusty_sphinx_worker::domain_warnings;
use std::fs;

use super::cli_args::{flag_value, flag_value_opt};
use super::diagnostics::{
    check_broken_links_strict, format_broken_link_warning, format_object_type_mismatch_warning,
};

pub(super) fn process_render(
    ast_json: &str,
    index_json: &str,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
) -> Result<(
    String,
    Vec<renderer::BrokenLink>,
    Vec<renderer::ObjectTypeMismatch>,
)> {
    let doc: ast::Document =
        serde_json::from_str(ast_json).context("Failed to deserialize AST document")?;
    let index: rusty_sphinx_index::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;

    let render_output = renderer::render(&doc, &index, doc_path);

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
            nav_tree: &index.nav_tree,
            has_genindex: !index.genindex_entries.is_empty(),
        },
    )?;
    Ok((
        html,
        render_output.broken_links,
        render_output.object_type_mismatches,
    ))
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

    let (html, broken_links, object_type_mismatches) = process_render(
        &ast_json,
        &index_json,
        &site_config,
        &template_str,
        &doc_path,
    )?;

    for link in &broken_links {
        eprintln!("{}", format_broken_link_warning(&doc_path, link));
    }
    for mismatch in &object_type_mismatches {
        eprintln!(
            "{}",
            format_object_type_mismatch_warning(&doc_path, mismatch)
        );
    }

    // Emit the structured domain-object warning sidecar when requested. Written
    // unconditionally (even with zero warnings) and before the strict-links
    // check, so the file always exists as a Bazel-declared output and reflects
    // reality regardless of whether the strict check then fails the render.
    if let Some(warnings_path) = &warnings_output {
        let report = domain_warnings::build_domain_warning_report(
            &doc_path,
            &broken_links,
            &object_type_mismatches,
        );
        let report_json = serde_json::to_string_pretty(&report)
            .context("Failed to serialize domain warning report")?;
        fs::write(warnings_path, report_json)
            .with_context(|| format!("Error writing '{warnings_path}'"))?;
    }

    check_broken_links_strict(strict_links, &doc_path, &broken_links)?;

    fs::write(&output, html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_render_returns_html_string() {
        // Given
        let doc =
            r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":[{"Text":"Title"}]}}]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let (html, broken_links, object_type_mismatches) =
            process_render(doc, index, &config, template, "test.rst").unwrap();

        // Then
        assert!(html.contains("<h1>Title</h1>"));
        assert!(broken_links.is_empty());
        assert!(object_type_mismatches.is_empty());
    }

    #[test]
    fn test_process_render_reports_broken_reference() {
        // Given
        let doc = r#"{"path":"test.rst","nodes":[{"Paragraph":[{"Reference":{"display":"missing","target":"missing"}}]}]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let (_, broken_links, _) =
            process_render(doc, index, &config, template, "test.rst").unwrap();

        // Then
        assert_eq!(broken_links.len(), 1);
        assert_eq!(broken_links[0].target, "missing");
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
        let (_, broken_links, object_type_mismatches) =
            process_render(doc, index, &config, template, "test.rst").unwrap();

        // Then — resolved, not broken, but flagged as a mismatch
        assert!(broken_links.is_empty());
        assert_eq!(object_type_mismatches.len(), 1);
        assert_eq!(object_type_mismatches[0].name, "Fault");
        assert_eq!(
            object_type_mismatches[0].requested_type,
            ast::ObjectType::Py(ast::PyObjectType::Exception)
        );
        assert_eq!(
            object_type_mismatches[0].resolved_type,
            ast::ObjectType::Py(ast::PyObjectType::Class)
        );
    }
}

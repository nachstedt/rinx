//! The `preview` subcommand: collapses parse+local-analyze+merge+render into
//! one process reading RST from stdin, for low-latency editor use.

use anyhow::{Context, Result};
use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer::{self as renderer, config};
use std::fs;
use std::io::{self, Read};

use crate::cli_args::{flag_value, flag_value_opt};
use crate::cmd_parse::parse_default_domain_flag;
use crate::diagnostics::{format_broken_link_warning, format_object_type_mismatch_warning};

pub(super) fn process_preview(
    rst: &str,
    index_json: Option<&str>,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
    default_domain: ast::Domain,
) -> Result<(
    String,
    Vec<renderer::BrokenLink>,
    Vec<renderer::ObjectTypeMismatch>,
)> {
    let doc = parser::parse_with_domain(doc_path, rst, default_domain);
    let mut index = if let Some(json) = index_json {
        serde_json::from_str(json).context("Failed to deserialize global index")?
    } else {
        rusty_sphinx_index::ProjectIndex::default()
    };

    let local_index = analyzer::analyze(&doc);
    let _ = index.merge(local_index);

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

pub(super) fn cmd_preview(args: &[String]) -> Result<()> {
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

    let (html, broken_links, object_type_mismatches) = process_preview(
        &rst,
        index_json.as_deref(),
        &site_config,
        &template_str,
        &doc_path,
        default_domain,
    )?;

    // Preview is deliberately lenient (it renders over incomplete/WIP
    // documents), so broken links are always warnings, never a failure.
    for link in &broken_links {
        eprintln!("{}", format_broken_link_warning(&doc_path, link));
    }
    for mismatch in &object_type_mismatches {
        eprintln!(
            "{}",
            format_object_type_mismatch_warning(&doc_path, mismatch)
        );
    }

    println!("{html}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_preview_renders_html_with_merged_index() {
        // Given
        let rst = "Section A\n=========\n\nSee :ref:`section-b`";
        // Global index only knows about section-b in another file
        let global_index = r#"{"targets":{"section-b":{"Internal":"other.rst"}},"document_titles":{"other.rst":"Other"},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let (html, broken_links, _) = process_preview(
            rst,
            Some(global_index),
            &config,
            template,
            "test.rst",
            ast::Domain::Py,
        )
        .unwrap();

        // Then
        assert!(html.contains("<h1>Section A</h1>"));
        // Cross-reference to other file should be resolved
        assert!(html.contains("href=\"other.html#section-b\""));
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_process_preview_works_without_global_index() {
        // Given
        let rst = "Section A\n=========";
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let (html, broken_links, _) =
            process_preview(rst, None, &config, template, "test.rst", ast::Domain::Py).unwrap();

        // Then
        assert!(html.contains("<h1>Section A</h1>"));
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_process_preview_reports_broken_reference_without_failing() {
        // Given — preview is lenient: an unresolved :ref: is a diagnostic, not an error
        let rst = "Section A\n=========\n\nSee :ref:`missing`";
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let (html, broken_links, _) =
            process_preview(rst, None, &config, template, "test.rst", ast::Domain::Py).unwrap();

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert_eq!(broken_links.len(), 1);
        assert_eq!(broken_links[0].target, "missing");
    }
}

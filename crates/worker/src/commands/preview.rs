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
use super::csv_files::{DocumentRelativeCsvFiles, parse_ctx};
use super::diagnostics::{
    format_broken_link_warning, format_object_type_mismatch_warning, report_diagnostic,
};
use super::parse::parse_default_domain_flag;
use super::suppression::{
    retain_reportable, retain_reportable_links, retain_reportable_mismatches,
};

pub(super) fn process_preview(
    rst: &str,
    index_json: Option<&str>,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
    default_domain: ast::Domain,
    csv_files: &DocumentRelativeCsvFiles,
) -> Result<(
    String,
    Vec<renderer::BrokenLink>,
    Vec<renderer::ObjectTypeMismatch>,
)> {
    let doc = parser::parse_with_ctx(doc_path, rst, &parse_ctx(default_domain, csv_files));
    for diagnostic in retain_reportable(&doc.diagnostics, &doc.suppressions) {
        report_diagnostic(doc_path, diagnostic);
    }
    let mut index = if let Some(json) = index_json {
        serde_json::from_str(json).context("Failed to deserialize global index")?
    } else {
        rusty_sphinx_index::ProjectIndex::default()
    };

    let local_index = analyzer::analyze(&doc);
    let _ = index.merge(local_index);

    let render_output = renderer::render(&doc, &index, doc_path);
    // Filtered here rather than by the caller so that a suppressed link is
    // invisible to *every* consumer of this function, not just the one that
    // remembers to ask.
    let broken_links = retain_reportable_links(&render_output.broken_links, &doc.suppressions);
    let object_type_mismatches =
        retain_reportable_mismatches(&render_output.object_type_mismatches, &doc.suppressions);

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
    Ok((html, broken_links, object_type_mismatches))
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

    let (html, broken_links, object_type_mismatches) = process_preview(
        &rst,
        index_json.as_deref(),
        &site_config,
        &template_str,
        &doc_path,
        default_domain,
        // The editor previews a real file on disk, so `:file:` resolves
        // against its directory exactly as it does in the `parse` subcommand.
        &DocumentRelativeCsvFiles::for_document(&doc_path),
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

    /// A loader rooted at a directory holding no CSV files, for the tests
    /// whose input has no `:file:` option.
    fn no_csv_files() -> DocumentRelativeCsvFiles {
        DocumentRelativeCsvFiles::for_document("test.rst")
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
        let (html, broken_links, _) = process_preview(
            rst,
            Some(global_index),
            &config,
            template,
            "test.rst",
            ast::Domain::Py,
            &no_csv_files(),
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
        let (html, broken_links, _) = process_preview(
            rst,
            None,
            &config,
            template,
            "test.rst",
            ast::Domain::Py,
            &no_csv_files(),
        )
        .unwrap();

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
        let (html, broken_links, _) = process_preview(
            rst,
            None,
            &config,
            template,
            "test.rst",
            ast::Domain::Py,
            &no_csv_files(),
        )
        .unwrap();

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert_eq!(broken_links.len(), 1);
        assert_eq!(broken_links[0].target, "missing");
    }
}

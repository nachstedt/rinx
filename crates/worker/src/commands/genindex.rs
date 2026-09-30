//! The `genindex` subcommand: renders the project-wide general index page.

use anyhow::{Context, Result};
use rinx_renderer::{self as renderer, config};
use std::fs;

use super::cli_args::flag_value;
use super::cli_args::read_domain_indices;
use super::page_chrome::PageChrome;

pub(super) fn process_genindex(
    index_json: &str,
    config: &config::SiteConfig,
    chrome: &PageChrome<'_>,
) -> Result<String> {
    let index: rinx_index::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;
    renderer::render_genindex(&index, config, chrome.template, chrome.has_modindex())
}

pub(crate) fn cmd_genindex(args: &[String]) -> Result<()> {
    let index_path = flag_value(args, "--index")?;
    let output = flag_value(args, "--output")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;
    let domain_indices = read_domain_indices(args)?;

    let index_json =
        fs::read_to_string(&index_path).with_context(|| format!("Error reading '{index_path}'"))?;
    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;
    let template_str = fs::read_to_string(&template_path)
        .with_context(|| format!("Error reading template '{template_path}'"))?;

    let html = process_genindex(
        &index_json,
        &site_config,
        &PageChrome {
            template: &template_str,
            domain_indices: &domain_indices,
        },
    )?;
    fs::write(&output, html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_genindex_renders_html_with_letter_section() {
        // Given
        let index = r#"{"targets":{},"document_titles":{"guide.rst":"Guide"},"nav_tree":[],"glossary_terms":{},"domain_objects":{},"genindex_entries":[{"primary":"execution","subentry":null,"main":false,"doc_path":"guide.rst","anchor":"index-0"}]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let html = process_genindex(
            index,
            &config,
            &PageChrome {
                template,
                domain_indices: &std::collections::BTreeSet::new(),
            },
        )
        .unwrap();

        // Then
        assert!(html.contains("<h2 id=\"E\">E</h2>"));
        assert!(html.contains("execution"));
        assert!(html.contains("guide.html#index-0"));
    }

    #[test]
    fn test_process_genindex_returns_error_for_invalid_index_json() {
        // Given
        let index = "not json";
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let result = process_genindex(
            index,
            &config,
            &PageChrome {
                template,
                domain_indices: &std::collections::BTreeSet::new(),
            },
        );

        // Then
        assert!(result.is_err());
    }
}

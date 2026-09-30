//! The `modindex` subcommand: renders the Python Module Index page. Run only
//! for a site whose `domain_indices` enables `py-modindex` (ADR-032).

use anyhow::{Context, Result};
use rinx_index::DomainIndex;
use rinx_renderer::{self as renderer, config};
use std::fs;

use super::cli_args::flag_value;

/// The page, plus the warning for a site that enabled the index without
/// documenting a single module.
pub(super) struct RenderedModindex {
    pub html: String,
    pub warning: Option<String>,
}

pub(super) fn process_modindex(
    index_json: &str,
    config: &config::SiteConfig,
    template_str: &str,
) -> Result<RenderedModindex> {
    let index: rinx_index::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;
    let html = renderer::render_modindex(&index, config, template_str)?;
    // Still written: Bazel declared the page before any document was read, so
    // the action must produce it — but an empty index is almost certainly a
    // `domain_indices` entry left over or copied from another site.
    let warning = index.modules.is_empty().then(|| {
        format!(
            "warning: {}: domain_indices enables '{}', but no document defines a \
             `.. py:module::`, so the page is empty",
            renderer::MODINDEX_PATH,
            DomainIndex::PyModindex
        )
    });
    Ok(RenderedModindex { html, warning })
}

pub(crate) fn cmd_modindex(args: &[String]) -> Result<()> {
    let index_path = flag_value(args, "--index")?;
    let output = flag_value(args, "--output")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;

    let index_json =
        fs::read_to_string(&index_path).with_context(|| format!("Error reading '{index_path}'"))?;
    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;
    let template_str = fs::read_to_string(&template_path)
        .with_context(|| format!("Error reading template '{template_path}'"))?;

    let page = process_modindex(&index_json, &site_config, &template_str)?;
    if let Some(warning) = &page.warning {
        eprintln!("{warning}");
    }
    fs::write(&output, page.html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON of an index documenting only `abc`, with a synopsis.
    fn index_with_module() -> String {
        let mut index = rinx_index::ProjectIndex::default();
        index.insert_domain_object(
            rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Module),
            "abc",
            "library/abc.rst",
        );
        index.modules.insert(
            rinx_ast::TargetName::new("abc"),
            rinx_index::ModuleEntry {
                synopsis: Some("Abstract base classes.".to_string()),
                ..rinx_index::ModuleEntry::new("library/abc.rst")
            },
        );
        serde_json::to_string(&index).unwrap()
    }

    /// The JSON of an index documenting nothing.
    fn empty_index() -> String {
        serde_json::to_string(&rinx_index::ProjectIndex::default()).unwrap()
    }

    #[test]
    fn test_process_modindex_renders_every_module_with_its_synopsis() {
        // Given
        let config = config::SiteConfig::default();

        // When
        let page = process_modindex(&index_with_module(), &config, "{{ body }}").unwrap();

        // Then
        assert!(
            page.html
                .contains("href=\"library/abc.html#py:module:abc\"")
        );
        assert!(page.html.contains("<em>Abstract base classes.</em>"));
        assert_eq!(page.warning, None);
    }

    #[test]
    fn test_process_modindex_warns_when_no_module_is_documented() {
        // Given
        let config = config::SiteConfig::default();

        // When
        let page = process_modindex(&empty_index(), &config, "{{ body }}").unwrap();

        // Then — the page is still written, and the warning names the switch
        assert!(page.html.contains("Python Module Index"));
        let warning = page.warning.expect("an empty index is reported");
        assert!(
            warning.starts_with("warning: py-modindex.html: domain_indices enables 'py-modindex'")
        );
    }

    #[test]
    fn test_process_modindex_returns_error_for_invalid_index_json() {
        // Given
        let config = config::SiteConfig::default();

        // When
        let result = process_modindex("not json", &config, "{{ body }}");

        // Then
        assert!(result.is_err());
    }
}

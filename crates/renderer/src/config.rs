//! Site configuration for rusty-sphinx.
//!
//! All site-level settings are specified in a single `rusty_sphinx.toml`
//! file. This file contains only metadata (project name, version) — not
//! file paths. File paths (template, CSS) are passed as separate CLI
//! flags so that build systems like Bazel can resolve them correctly
//! in sandboxed environments.

use serde::Deserialize;

/// Site-level configuration loaded from a TOML file.
///
/// Contains only site metadata — no file paths. File paths are resolved
/// by the build system and passed as separate CLI arguments.
#[derive(Debug, Deserialize)]
pub struct SiteConfig {
    /// The project name displayed in the sidebar header and page title.
    #[serde(default = "default_project")]
    pub project: String,

    /// The project version displayed in the sidebar header.
    #[serde(default)]
    pub version: String,

    /// The document every other one hangs off — Sphinx's `root_doc`, written
    /// without its `.rst` extension (`"index"`, `"docs/index"`).
    ///
    /// Without it, roots are *inferred* as "every document no toctree
    /// references", which quietly makes an orphaned document a second
    /// top-level entry in the sidebar and leaves page order ambiguous. Naming
    /// the root removes both.
    ///
    /// This is a logical document name, not a file path, so it does not
    /// violate the no-paths rule above: nothing here has to survive a sandbox
    /// relocating a file. Falls back to the inferred roots when it names no
    /// document that exists.
    #[serde(default = "default_root_doc")]
    pub root_doc: String,
}

fn default_project() -> String {
    "Documentation".to_string()
}

fn default_root_doc() -> String {
    "index".to_string()
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            project: default_project(),
            version: String::new(),
            root_doc: default_root_doc(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let config = SiteConfig::default();
        assert_eq!(config.project, "Documentation");
        assert_eq!(config.version, "");
        assert_eq!(config.root_doc, "index");
    }

    #[test]
    fn test_deserialize_reads_an_explicit_root_doc() {
        // Given
        let toml_str = r#"
project = "My Project"
root_doc = "contents"
"#;

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.root_doc, "contents");
    }

    #[test]
    fn test_deserialize_defaults_root_doc_when_omitted() {
        // Given — every existing site config predates the option.
        let toml_str = r#"project = "My Project""#;

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.root_doc, "index");
    }

    #[test]
    fn test_deserialize_full_config() {
        // Given
        let toml_str = r#"
project = "My Project"
version = "1.0"
"#;

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.project, "My Project");
        assert_eq!(config.version, "1.0");
    }

    #[test]
    fn test_deserialize_empty_config_uses_defaults() {
        // Given
        let toml_str = "";

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.project, "Documentation");
        assert_eq!(config.version, "");
        assert_eq!(config.root_doc, "index");
    }

    #[test]
    fn test_deserialize_partial_config_fills_defaults() {
        // Given
        let toml_str = r#"project = "Overridden""#;

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.project, "Overridden");
        assert_eq!(config.version, "");
    }

    #[test]
    fn test_deserialize_rejects_project_table_section() {
        // Given
        // Regression: benchmark.py previously generated '[project]\nname = "..."\n'
        // which TOML parses as a table, causing "invalid type: map, expected a string".
        // The correct flat format must be used instead.
        let toml_str = "[project]\nname = \"CPython Benchmark\"\n";

        // When
        let result: Result<SiteConfig, _> = toml::from_str(toml_str);

        // Then
        assert!(
            result.is_err(),
            "A [project] section should be rejected; use flat 'project = \"...\"' format"
        );
    }

    #[test]
    fn test_deserialize_benchmark_generated_format() {
        // Given
        // Regression: this is the exact string benchmark.py writes to rusty_sphinx.toml.
        // If this test breaks, the benchmark config generation must be updated to match.
        let toml_str = "project = \"CPython Benchmark\"\n";

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.project, "CPython Benchmark");
    }
}

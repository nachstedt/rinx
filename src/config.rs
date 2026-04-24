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
}

fn default_project() -> String {
    "Documentation".to_string()
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            project: default_project(),
            version: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

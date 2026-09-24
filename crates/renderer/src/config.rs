//! Site configuration for rusty-sphinx.
//!
//! All site-level settings are specified in a single `rusty_sphinx.toml`
//! file. This file contains only metadata (project name, version) — not
//! file paths. File paths (template, CSS) are passed as separate CLI
//! flags so that build systems like Bazel can resolve them correctly
//! in sandboxed environments.

use rusty_sphinx_ast::ResolvedLanguage;
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

    /// The language a code block with no argument of its own inherits, before
    /// any `.. highlight::` in the document overrides it — Sphinx's
    /// `highlight_language`.
    ///
    /// Deserialized through [`ResolvedLanguage`]'s own parser, so `"none"` and
    /// `"default"` keep their reserved meanings and a value naming nothing at
    /// all is rejected on load rather than silently disabling highlighting
    /// across the whole site.
    ///
    /// A language, not a path, so this respects the no-paths rule above.
    #[serde(default, deserialize_with = "deserialize_highlight_language")]
    pub highlight_language: ResolvedLanguage,

    /// Whether the built-in entity rendering folds an entity's detail behind a
    /// disclosure, leaving the header and the leading prose visible.
    ///
    /// On by default: a page of entities is usually read by scanning, and the
    /// attributes, sections and links of every one of them at once is not what
    /// a reader wants first. Set `false` for the flat rendering.
    ///
    /// It lives here, in the *render*-time config, rather than in the entity
    /// schema, which is a parse-time input — a flag there would re-parse every
    /// document in the library to change how a box looks. A type that wants
    /// something else entirely names a `template` instead.
    ///
    /// A behaviour switch, not a path, so this respects the no-paths rule
    /// above.
    #[serde(default = "default_collapse_entities")]
    pub collapse_entities: bool,

    /// Whether `.. entity-update::`/`.. needextend::` renders its own visible
    /// box — its target, its field mutations and its justification prose.
    ///
    /// On by default, the one deliberate divergence from sphinx-needs' own
    /// `needextend`, which renders nothing at all: the whole point of this
    /// directive's traceable, non-destructive design (see
    /// `docs/decisions/019-entity-update.md`) is the audit trail it leaves,
    /// and an audit trail nobody can see is not much of one. Set `false` for
    /// a site that wants the mutation applied silently.
    #[serde(default = "default_show_entity_updates")]
    pub show_entity_updates: bool,

    /// Named `PlantUML` preambles a diagram may ask for with `:config:`.
    ///
    /// The *text* of a preamble, keyed by the name a directive names it by —
    /// never a path to a file holding one, which is what keeps this on the
    /// right side of the no-paths rule above. A build system that relocated
    /// the file would otherwise break every diagram that referenced it.
    ///
    /// Sphinx-needs keeps the same thing in `needs_flow_configs`. Reading it
    /// from the site config rather than from the schema is deliberate: this is
    /// presentation, and the schema is a parse-time input, so a tweak to how
    /// diagrams look must not re-parse every document.
    #[serde(default)]
    pub uml_configs: std::collections::BTreeMap<String, String>,
}

/// Reads `highlight_language` from a TOML string through the same smart
/// constructor the parser uses, so the config and a `.. highlight::` cannot
/// disagree about what a value means.
fn deserialize_highlight_language<'de, D>(deserializer: D) -> Result<ResolvedLanguage, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    let raw = String::deserialize(deserializer)?;
    ResolvedLanguage::parse(&raw)
        .map_err(|error| D::Error::custom(format!("invalid highlight_language '{raw}': {error}")))
}

fn default_collapse_entities() -> bool {
    true
}

fn default_show_entity_updates() -> bool {
    true
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
            highlight_language: ResolvedLanguage::default(),
            collapse_entities: default_collapse_entities(),
            show_entity_updates: default_show_entity_updates(),
            uml_configs: std::collections::BTreeMap::new(),
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

    #[test]
    fn test_config_defaults_highlight_language_to_sphinxs_own_default() {
        // Given — a config that says nothing about highlighting
        let toml_str = r#"project = "Docs""#;

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.highlight_language, ResolvedLanguage::Default);
    }

    #[test]
    fn test_config_reads_a_named_highlight_language() {
        // Given
        let toml_str = "project = \"Docs\"\nhighlight_language = \"Rust\"\n";

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then — normalized through the same constructor the parser uses
        assert_eq!(
            config.highlight_language,
            ResolvedLanguage::parse("rust").unwrap()
        );
    }

    #[test]
    fn test_config_reads_none_as_a_highlight_language() {
        // Given — turning highlighting off site-wide
        let toml_str = "project = \"Docs\"\nhighlight_language = \"none\"\n";

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(config.highlight_language, ResolvedLanguage::None);
    }

    #[test]
    fn test_config_rejects_an_empty_highlight_language() {
        // Given — a value naming nothing at all, which would silently
        // disable highlighting across the whole site
        let toml_str = "project = \"Docs\"\nhighlight_language = \"  \"\n";

        // When
        let result: Result<SiteConfig, _> = toml::from_str(toml_str);

        // Then — caught on load, where it can be reported
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod collapse_entities_tests {
    use super::*;

    #[test]
    fn test_entities_collapse_unless_the_config_turns_it_off() {
        // Given — a config that says nothing about it
        let config: SiteConfig = toml::from_str("project = \"Docs\"").unwrap();

        // When / Then
        assert!(config.collapse_entities);
    }

    #[test]
    fn test_the_flat_rendering_can_be_asked_for() {
        // Given / When
        let config: SiteConfig = toml::from_str("collapse_entities = false").unwrap();

        // Then
        assert!(!config.collapse_entities);
    }
}

#[cfg(test)]
mod show_entity_updates_tests {
    use super::*;

    #[test]
    fn test_entity_updates_render_unless_the_config_turns_it_off() {
        // Given — a config that says nothing about it
        let config: SiteConfig = toml::from_str("project = \"Docs\"").unwrap();

        // When / Then
        assert!(config.show_entity_updates);
    }

    #[test]
    fn test_silent_application_can_be_asked_for() {
        // Given / When
        let config: SiteConfig = toml::from_str("show_entity_updates = false").unwrap();

        // Then
        assert!(!config.show_entity_updates);
    }
}

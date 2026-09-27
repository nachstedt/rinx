//! Site configuration for rinx.
//!
//! All site-level settings are specified in a single `rinx.toml`
//! file. This file contains only metadata (project name, version) — not
//! file paths. File paths (template, CSS) are passed as separate CLI
//! flags so that build systems like Bazel can resolve them correctly
//! in sandboxed environments.

use rinx_ast::ResolvedLanguage;
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

    /// The version switcher the default template shows when a site is
    /// published in several versions side by side — pydata-sphinx-theme's
    /// `switcher`. Absent, no page mentions it at all.
    ///
    /// Only the *list* of versions is configured, never which one this build
    /// is: the page finds itself in that list by its own address, so the same
    /// commit built for `main/` and for a preview renders identical bytes and
    /// shares one cached render (`docs/decisions/024-versioned-docs.md`).
    #[serde(default)]
    pub version_switcher: Option<VersionSwitcher>,
}

/// The `[version_switcher]` table of a site config.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionSwitcher {
    /// Where the browser fetches the list of published versions from.
    pub json_url: SwitcherUrl,
}

/// The address of a version switcher's `versions.json`, as the browser
/// fetches it: absolute (`https://…`) or relative to the host's root (`/…`).
///
/// A path relative to the page is refused, because it would mean something
/// different on every page of a nested site. A URL is not a file path, so this
/// respects the no-paths rule above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherUrl(String);

impl SwitcherUrl {
    /// Accepts an `http(s)://` URL or a root-relative `/…` path.
    ///
    /// # Errors
    ///
    /// Returns a message naming the accepted forms when `raw` is neither, or
    /// when it holds a character that could end an HTML attribute.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let absolute = raw.starts_with("https://") || raw.starts_with("http://");
        let root_relative = raw.starts_with('/') && !raw.starts_with("//");
        if !(absolute || root_relative) {
            return Err(format!(
                "'{raw}' must be an http(s):// URL or start with '/', since a page-relative \
                 address differs on every page of a nested site"
            ));
        }
        if raw
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>'))
        {
            return Err(format!("'{raw}' contains a character a URL cannot hold"));
        }
        Ok(Self(raw.to_string()))
    }

    /// The URL as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SwitcherUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(|error| D::Error::custom(format!("invalid json_url: {error}")))
    }
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
            version_switcher: None,
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
        // Regression: this is the exact string benchmark.py writes to rinx.toml.
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

#[cfg(test)]
mod version_switcher_tests {
    use super::*;

    #[test]
    fn test_no_switcher_unless_the_config_asks_for_one() {
        // Given — a config that says nothing about it
        let config: SiteConfig = toml::from_str("project = \"Docs\"").unwrap();

        // When / Then
        assert!(config.version_switcher.is_none());
    }

    #[test]
    fn test_the_switcher_table_is_read() {
        // Given
        let toml_str = "[version_switcher]\njson_url = \"https://example.org/versions.json\"\n";

        // When
        let config: SiteConfig = toml::from_str(toml_str).unwrap();

        // Then
        assert_eq!(
            config.version_switcher.unwrap().json_url.as_str(),
            "https://example.org/versions.json"
        );
    }

    #[test]
    fn test_an_unknown_key_in_the_switcher_table_is_rejected() {
        // Given — pydata-sphinx-theme's `version_match`, which this build
        // deliberately never needs
        let toml_str =
            "[version_switcher]\njson_url = \"/versions.json\"\nversion_match = \"1.0\"\n";

        // When
        let result: Result<SiteConfig, _> = toml::from_str(toml_str);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_a_page_relative_json_url_is_rejected_on_load() {
        // Given
        let toml_str = "[version_switcher]\njson_url = \"versions.json\"\n";

        // When
        let result: Result<SiteConfig, _> = toml::from_str(toml_str);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_switcher_url_accepts_absolute_urls() {
        // Given / When / Then
        assert!(SwitcherUrl::parse("https://example.org/v.json").is_ok());
        assert!(SwitcherUrl::parse("http://localhost:8000/v.json").is_ok());
    }

    #[test]
    fn test_switcher_url_accepts_a_root_relative_path() {
        // Given / When
        let url = SwitcherUrl::parse("/rinx/versions.json").unwrap();

        // Then
        assert_eq!(url.as_str(), "/rinx/versions.json");
    }

    #[test]
    fn test_switcher_url_rejects_a_protocol_relative_url() {
        // Given — `//host/…` looks root-relative but names another host
        let result = SwitcherUrl::parse("//example.org/versions.json");

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_switcher_url_rejects_other_schemes() {
        // Given / When / Then
        assert!(SwitcherUrl::parse("javascript:alert(1)").is_err());
        assert!(SwitcherUrl::parse("file:///versions.json").is_err());
    }

    #[test]
    fn test_switcher_url_rejects_characters_that_end_an_attribute() {
        // Given / When / Then
        assert!(SwitcherUrl::parse("https://example.org/\"onload=x").is_err());
        assert!(SwitcherUrl::parse("https://example.org/a b").is_err());
    }
}

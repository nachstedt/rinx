//! The `rinx.toml` a migration writes: the site's metadata, as `conf.py`
//! sets it.
//!
//! Only a setting `conf.py` changes from Sphinx's default is written, and
//! only one rinx's [`SiteConfig`] reads — what the server would render the
//! project with, so the built site looks as the preview did.

use rinx_ast::ResolvedLanguage;
use rinx_renderer::config::{PepBaseUrl, RfcBaseUrl};
use toml::{Table, Value};

/// The site settings a migration writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SiteSettings {
    pub project: Option<String>,
    pub version: Option<String>,
    /// The root document's name as the build names it: from the Bazel
    /// workspace's root, so prefixed with the package's path.
    pub root_doc: String,
    pub numfig: bool,
    pub numfig_secnum_depth: usize,
    pub highlight_language: ResolvedLanguage,
    pub pep_base_url: Option<PepBaseUrl>,
    pub rfc_base_url: Option<RfcBaseUrl>,
    pub modindex_common_prefix: Vec<String>,
}

/// Sphinx's, and rinx's, default root document.
const DEFAULT_ROOT_DOC: &str = "index";

impl SiteSettings {
    /// The file's text.
    #[must_use]
    pub(crate) fn render(&self) -> String {
        let mut table = Table::new();
        let mut set = |key: &str, value: Value| {
            table.insert(key.to_string(), value);
        };
        if let Some(project) = &self.project {
            set("project", Value::from(project.as_str()));
        }
        if let Some(version) = &self.version {
            set("version", Value::from(version.as_str()));
        }
        if self.root_doc != DEFAULT_ROOT_DOC {
            set("root_doc", Value::from(self.root_doc.as_str()));
        }
        if self.highlight_language != ResolvedLanguage::default() {
            set(
                "highlight_language",
                Value::from(self.highlight_language.css_suffix()),
            );
        }
        if self.numfig {
            set("numfig", Value::from(true));
        }
        if self.numfig_secnum_depth != rinx_index::DEFAULT_NUMFIG_SECNUM_DEPTH {
            set(
                "numfig_secnum_depth",
                Value::from(i64::try_from(self.numfig_secnum_depth).unwrap_or(i64::MAX)),
            );
        }
        if let Some(url) = &self.pep_base_url {
            set("pep_base_url", Value::from(url.as_str()));
        }
        if let Some(url) = &self.rfc_base_url {
            set("rfc_base_url", Value::from(url.as_str()));
        }
        if !self.modindex_common_prefix.is_empty() {
            set(
                "modindex_common_prefix",
                Value::from(self.modindex_common_prefix.clone()),
            );
        }
        format!(
            "# Written by `rinx init --from-sphinx` from conf.py: the site's metadata.\n{}",
            toml::to_string(&table).unwrap_or_default()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_renderer::config::SiteConfig;

    fn defaults() -> SiteSettings {
        SiteSettings {
            project: None,
            version: None,
            root_doc: DEFAULT_ROOT_DOC.to_string(),
            numfig: false,
            numfig_secnum_depth: rinx_index::DEFAULT_NUMFIG_SECNUM_DEPTH,
            highlight_language: ResolvedLanguage::default(),
            pep_base_url: None,
            rfc_base_url: None,
            modindex_common_prefix: Vec::new(),
        }
    }

    #[test]
    fn test_render_writes_nothing_sphinx_defaults() {
        // When
        let text = defaults().render();

        // Then
        assert_eq!(
            text,
            "# Written by `rinx init --from-sphinx` from conf.py: the site's metadata.\n"
        );
    }

    #[test]
    fn test_render_writes_a_file_the_site_loads_as_conf_py_set_it() {
        // Given
        let settings = SiteSettings {
            project: Some("Python \"3\"".to_string()),
            version: Some("3.14.2".to_string()),
            root_doc: "Doc/contents".to_string(),
            numfig: true,
            numfig_secnum_depth: 2,
            highlight_language: ResolvedLanguage::parse("python3").unwrap(),
            pep_base_url: Some(PepBaseUrl::parse("https://peps.example.org/").unwrap()),
            rfc_base_url: Some(RfcBaseUrl::parse("https://rfc.example.org/").unwrap()),
            modindex_common_prefix: vec!["email.".to_string()],
        };

        // When
        let config: SiteConfig = toml::from_str(&settings.render()).expect("a site config");

        // Then
        assert_eq!(config.project, "Python \"3\"");
        assert_eq!(config.version, "3.14.2");
        assert_eq!(config.root_doc, "Doc/contents");
        assert!(config.numfig);
        assert_eq!(config.numfig_secnum_depth, 2);
        assert_eq!(config.highlight_language, settings.highlight_language);
        assert_eq!(config.pep_base_url.as_str(), "https://peps.example.org/");
        assert_eq!(config.rfc_base_url.as_str(), "https://rfc.example.org/");
        assert_eq!(config.modindex_common_prefix, ["email."]);
    }

    #[test]
    fn test_render_writes_a_language_that_turns_highlighting_off() {
        // Given
        let settings = SiteSettings {
            highlight_language: ResolvedLanguage::None,
            ..defaults()
        };

        // When
        let config: SiteConfig = toml::from_str(&settings.render()).expect("a site config");

        // Then
        assert_eq!(config.highlight_language, ResolvedLanguage::None);
    }
}

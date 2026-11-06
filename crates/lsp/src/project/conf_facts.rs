//! What a migration carries from a `conf.py` into the files it writes,
//! beyond what the server reads to serve the project (roadmap #37).
//!
//! The site's name and version, its domain indices and registry links change
//! nothing an editor shows, so the server's reader ([`super::sphinx_conf`])
//! does not ask for them — and so does not report one it cannot read. These
//! are read here instead, quietly: a setting this reading cannot take is
//! named in [`ConfFacts::unread`], which the migration reports as a setting
//! it left out of `rinx.toml`, rather than a squiggle in `conf.py`.
//!
//! What running `conf.py` found, when it ran, is authoritative here too: the
//! run asks for [`MIGRATION_SETTINGS`] alongside the server's own.

use std::collections::BTreeMap;

use rinx_entity::needs_conf::ReportEntry;
use rinx_pyconf::{BoundValue, ModuleReading, Value, ValueKind};
use rinx_renderer::config::{PepBaseUrl, RfcBaseUrl};

use super::conf_eval::ExecutedValue;

/// The settings only a migration reads, which a run is asked for too.
pub(crate) const MIGRATION_SETTINGS: &[&str] = &[
    "project",
    "version",
    "release",
    "html_domain_indices",
    "pep_base_url",
    "rfc_base_url",
    "modindex_common_prefix",
];

/// The settings the server reads that a migration carries into the files it
/// writes, beyond [`MIGRATION_SETTINGS`]: one bound to a value reading could
/// not take is left at its default there too, so it is named in
/// [`ConfFacts::unread`].
const CARRIED_SETTINGS: &[&str] = &[
    "root_doc",
    "master_doc",
    "source_suffix",
    "exclude_patterns",
    "include_patterns",
    "primary_domain",
    "default_role",
    "rst_prolog",
    "rst_epilog",
    "numfig",
    "numfig_secnum_depth",
    "highlight_language",
];

/// The Python module index, as `html_domain_indices` lists it.
const PY_MODINDEX: &str = "py-modindex";

/// What a migration takes from a Sphinx project's configuration that the
/// server does not read to serve it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfFacts {
    /// The project's name — `project`.
    pub project: Option<String>,
    /// Its full version — `release`, else `version`.
    pub version: Option<String>,
    /// Whether the HTML builder may write the Python module index —
    /// `html_domain_indices`, true unless it is false or leaves the index
    /// out.
    pub py_modindex: bool,
    /// `pep_base_url`, when set.
    pub pep_base_url: Option<PepBaseUrl>,
    /// `rfc_base_url`, when set.
    pub rfc_base_url: Option<RfcBaseUrl>,
    /// `modindex_common_prefix`.
    pub modindex_common_prefix: Vec<String>,
    /// Each setting `conf.py` binds to a value this reading could not take,
    /// in [`MIGRATION_SETTINGS`]' order.
    pub unread: Vec<String>,
    /// The project's sphinx-needs configuration, converted into an entity
    /// schema — when `extensions` declares sphinx-needs.
    pub needs: Option<ConvertedNeeds>,
}

/// A sphinx-needs configuration as a migration writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertedNeeds {
    /// The schema's `entities.toml` text, and what it could not carry over.
    Schema {
        toml: String,
        unconverted: Vec<ReportEntry>,
    },
    /// The configuration converts into no valid schema, for the reason
    /// `conf.py`'s findings give.
    Invalid,
}

/// What `module`, a `conf.py` read without running it, binds for a
/// migration — or, when it ran, what `executed` holds.
#[must_use]
pub(crate) fn read_conf_facts(
    module: &ModuleReading,
    executed: Option<&BTreeMap<String, ExecutedValue>>,
) -> ConfFacts {
    let mut reader = FactReader {
        module,
        executed,
        unread: Vec::new(),
    };
    let project = reader.string("project");
    let release = reader.string("release");
    let version = reader.string("version");
    let py_modindex = reader.py_modindex();
    let pep_base_url = reader.url("pep_base_url", PepBaseUrl::parse);
    let rfc_base_url = reader.url("rfc_base_url", RfcBaseUrl::parse);
    let modindex_common_prefix = reader.strings("modindex_common_prefix");
    for name in CARRIED_SETTINGS {
        reader.literal(name);
    }
    ConfFacts {
        project,
        version: release.or(version),
        py_modindex,
        pep_base_url,
        rfc_base_url,
        modindex_common_prefix,
        unread: reader.unread,
        needs: None,
    }
}

struct FactReader<'a> {
    module: &'a ModuleReading,
    executed: Option<&'a BTreeMap<String, ExecutedValue>>,
    unread: Vec<String>,
}

impl<'a> FactReader<'a> {
    /// The literal `name` is bound to: as running `conf.py` left it, or as
    /// written when it did not run and nothing changes it afterwards. A
    /// binding that is neither is recorded as unread; no binding is `None`.
    fn literal(&mut self, name: &str) -> Option<&'a Value> {
        let value = if let Some(executed) = self.executed {
            match executed.get(name)? {
                ExecutedValue::Literal(value) => Some(value),
                ExecutedValue::Unrepresentable(_) => None,
            }
        } else {
            let binding = self.module.binding(name)?;
            match &binding.value {
                BoundValue::Literal(value) if binding.modifications.is_empty() => Some(value),
                _ => None,
            }
        };
        if value.is_none() {
            self.unread(name);
        }
        value
    }

    fn unread(&mut self, name: &str) {
        self.unread.push(name.to_string());
    }

    fn string(&mut self, name: &str) -> Option<String> {
        let value = self.literal(name)?;
        let text = value.as_str().map(str::to_string);
        if text.is_none() {
            self.unread(name);
        }
        text
    }

    fn strings(&mut self, name: &str) -> Vec<String> {
        let Some(value) = self.literal(name) else {
            return Vec::new();
        };
        let strings: Option<Vec<String>> = value.as_sequence().and_then(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect()
        });
        strings.unwrap_or_else(|| {
            self.unread(name);
            Vec::new()
        })
    }

    /// `html_domain_indices`: a flag, or the list of the indices written.
    fn py_modindex(&mut self) -> bool {
        let Some(value) = self.literal("html_domain_indices") else {
            return true;
        };
        match &value.kind {
            ValueKind::Bool(flag) => *flag,
            ValueKind::List(items) | ValueKind::Tuple(items) => {
                items.iter().any(|item| item.as_str() == Some(PY_MODINDEX))
            }
            _ => {
                self.unread("html_domain_indices");
                true
            }
        }
    }

    /// The URL `name` is bound to, read by `parse`.
    fn url<T>(&mut self, name: &str, parse: fn(&str) -> Result<T, String>) -> Option<T> {
        let text = self.string(name)?;
        let url = parse(&text).ok();
        if url.is_none() {
            self.unread(name);
        }
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_pyconf::read_module;

    fn facts(text: &str) -> ConfFacts {
        read_conf_facts(&read_module(text), None)
    }

    #[test]
    fn test_read_conf_facts_defaults_what_conf_py_does_not_bind() {
        // When
        let found = facts("extensions = []\n");

        // Then
        assert_eq!(
            found,
            ConfFacts {
                py_modindex: true,
                ..ConfFacts::default()
            }
        );
    }

    #[test]
    fn test_read_conf_facts_reads_the_name_and_prefers_the_release() {
        // When
        let found = facts("project = 'Demo'\nversion = '1.2'\nrelease = '1.2.3'\n");

        // Then
        assert_eq!(found.project.as_deref(), Some("Demo"));
        assert_eq!(found.version.as_deref(), Some("1.2.3"));
        assert!(found.unread.is_empty());
    }

    #[test]
    fn test_read_conf_facts_falls_back_to_the_version() {
        // When
        let found = facts("version = '1.2'\n");

        // Then
        assert_eq!(found.version.as_deref(), Some("1.2"));
    }

    #[test]
    fn test_read_conf_facts_names_a_computed_or_changed_setting_as_unread() {
        // Given — CPython computes its version; a name is changed afterwards
        let text = "version = get_version()\nproject = 'A'\nproject += 'B'\n";

        // When
        let found = facts(text);

        // Then
        assert_eq!(found.project, None);
        assert_eq!(found.version, None);
        assert_eq!(found.unread, ["project", "version"]);
    }

    #[test]
    fn test_read_conf_facts_names_a_value_of_the_wrong_kind_as_unread() {
        // When
        let found = facts("project = 3\nmodindex_common_prefix = [1]\n");

        // Then
        assert_eq!(found.unread, ["project", "modindex_common_prefix"]);
    }

    #[test]
    fn test_read_conf_facts_reads_whether_the_module_index_is_written() {
        // When / Then
        assert!(!facts("html_domain_indices = False\n").py_modindex);
        assert!(!facts("html_domain_indices = ['c-index']\n").py_modindex);
        assert!(facts("html_domain_indices = ['py-modindex']\n").py_modindex);
        assert!(facts("html_domain_indices = True\n").py_modindex);
    }

    #[test]
    fn test_read_conf_facts_reads_the_registry_urls_and_refuses_a_bad_one() {
        // When
        let found =
            facts("pep_base_url = 'https://peps.example.org/'\nrfc_base_url = 'not a url'\n");

        // Then
        assert_eq!(
            found.pep_base_url,
            Some(PepBaseUrl::parse("https://peps.example.org/").unwrap())
        );
        assert_eq!(found.rfc_base_url, None);
        assert_eq!(found.unread, ["rfc_base_url"]);
    }

    #[test]
    fn test_read_conf_facts_reads_the_module_index_prefixes() {
        // When
        let found = facts("modindex_common_prefix = ['email.']\n");

        // Then
        assert_eq!(found.modindex_common_prefix, ["email."]);
    }

    #[test]
    fn test_read_conf_facts_names_a_computed_setting_the_server_reads_too() {
        // Given — an epilog built from the version, as CPython's is
        let text = "rst_epilog = f'.. |v| replace:: {version}'\nnumfig = True\n";

        // When
        let found = facts(text);

        // Then
        assert_eq!(found.unread, ["rst_epilog"]);
    }

    #[test]
    fn test_read_conf_facts_takes_what_a_run_found() {
        // Given — conf.py computes a version that running it reveals
        let module = read_module("version = get_version()\n");
        let executed = BTreeMap::from([
            (
                "version".to_string(),
                super::super::conf_eval::executed_value_for_tests("'3.14.2'"),
            ),
            (
                "project".to_string(),
                ExecutedValue::Unrepresentable("<object>".to_string()),
            ),
        ]);

        // When
        let found = read_conf_facts(&module, Some(&executed));

        // Then
        assert_eq!(found.version.as_deref(), Some("3.14.2"));
        assert_eq!(found.unread, ["project"]);
    }
}

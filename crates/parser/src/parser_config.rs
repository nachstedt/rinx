//! A library's parse configuration — its `rinx_parser.toml`.
//!
//! What a project tells the parser that is neither a document nor a single
//! setting a build rule can spell: today, the roles it ignores. Kept apart
//! from the site's `rinx.toml`, which only the renderer reads, so that each
//! is a cache firewall for the other: editing renderer metadata re-parses
//! nothing, and editing this re-parses only the libraries naming it.
//!
//! The Bazel build hands it to every parse action of a `rinx_library` that
//! names it in `parse_config`; the language server reads the same file beside
//! a Sphinx project's `conf.py`. Both load it through [`load_parser_config`],
//! so the two cannot read it differently.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::inline::is_fixed_role_name;

/// A library's parse configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParserConfig {
    /// The roles the library's documents may write that this build shows as
    /// their text, rather than as the source they were written as.
    pub ignored_roles: IgnoredRoles,
}

impl ParserConfig {
    /// The configuration of a library naming no `rinx_parser.toml`.
    #[must_use]
    pub fn empty_ref() -> &'static Self {
        static EMPTY: LazyLock<ParserConfig> = LazyLock::new(ParserConfig::default);
        &EMPTY
    }
}

/// Roles a project's documents use that this build does not implement, and
/// that the project asks it to show as their text — the roles of a Sphinx
/// extension of its own, say, whose links this build cannot make.
///
/// Every name is one a document can write and no role of this build answers
/// to: a role it implements is matched before an ignored one could be, so
/// listing it would say nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IgnoredRoles(BTreeSet<String>);

impl IgnoredRoles {
    /// The roles `names` lists.
    ///
    /// # Errors
    ///
    /// One [`InvalidIgnoredRole`] per name refused: one no document could
    /// write as a role, one this build implements, or one listed twice.
    pub fn new(names: Vec<String>) -> Result<Self, Vec<InvalidIgnoredRole>> {
        let mut set = BTreeSet::new();
        let mut errors = Vec::new();
        for name in names {
            let reason = if !is_role_spelling(&name) {
                Some(IgnoredRoleFault::NotARoleName)
            } else if is_fixed_role_name(&name) {
                Some(IgnoredRoleFault::Implemented)
            } else if set.contains(&name) {
                Some(IgnoredRoleFault::Repeated)
            } else {
                None
            };
            match reason {
                Some(fault) => errors.push(InvalidIgnoredRole { name, fault }),
                None => {
                    set.insert(name);
                }
            }
        }
        if errors.is_empty() {
            Ok(Self(set))
        } else {
            Err(errors)
        }
    }

    /// Whether `name`, as a document wrote it, is ignored.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }

    /// Every ignored role, in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

/// Whether `name` is spelled as a role a document can write: a name,
/// optionally qualified by domains (`cite:p`, `cpp:class`).
fn is_role_spelling(name: &str) -> bool {
    static SPELLING: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new("^[a-zA-Z][a-zA-Z0-9_-]*(?::[a-zA-Z][a-zA-Z0-9_-]*)*$").unwrap()
    });
    SPELLING.is_match(name)
}

/// A name [`IgnoredRoles::new`] refused, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidIgnoredRole {
    pub name: String,
    pub fault: IgnoredRoleFault,
}

/// Why a name cannot be ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoredRoleFault {
    /// No document could write it as a role.
    NotARoleName,
    /// This build implements it, so it is never unknown.
    Implemented,
    /// It is listed more than once.
    Repeated,
}

impl fmt::Display for InvalidIgnoredRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = &self.name;
        match self.fault {
            IgnoredRoleFault::NotARoleName => {
                write!(
                    f,
                    "ignored role '{name}' is not a name a role is written with"
                )
            }
            IgnoredRoleFault::Implemented => {
                write!(f, "ignored role '{name}' is one rinx implements")
            }
            IgnoredRoleFault::Repeated => write!(f, "ignored role '{name}' is listed twice"),
        }
    }
}

/// Reads a `rinx_parser.toml`, reporting every fault in it rather than the
/// first.
///
/// # Errors
///
/// One message per fault: text that is not a table of the expected shape — an
/// unknown key included, so a misspelt one does not silently mean nothing —
/// or a name [`IgnoredRoles::new`] refuses.
pub fn load_parser_config(text: &str) -> Result<ParserConfig, Vec<String>> {
    let raw: RawParserConfig = toml::from_str(text).map_err(|error| vec![error.to_string()])?;
    let ignored_roles = IgnoredRoles::new(raw.ignored_roles)
        .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    Ok(ParserConfig { ignored_roles })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParserConfig {
    #[serde(default)]
    ignored_roles: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn test_load_parser_config_reads_the_ignored_roles() {
        // Given
        let text = "ignored_roles = [\"issue\", \"cite:p\"]\n";

        // When
        let config = load_parser_config(text).expect("the config should load");

        // Then
        assert_eq!(
            config.ignored_roles.iter().collect::<Vec<_>>(),
            ["cite:p", "issue"]
        );
    }

    #[test]
    fn test_load_parser_config_reads_an_empty_file_as_the_default() {
        // Given / When
        let config = load_parser_config("").expect("the config should load");

        // Then
        assert_eq!(&config, ParserConfig::empty_ref());
    }

    #[test]
    fn test_load_parser_config_refuses_an_unknown_key() {
        // Given — a misspelt key must not silently mean "nothing"
        let text = "ignore_roles = [\"issue\"]\n";

        // When
        let errors = load_parser_config(text).expect_err("the config should be refused");

        // Then
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("ignore_roles"), "{errors:?}");
    }

    #[test]
    fn test_load_parser_config_reports_every_refused_role() {
        // Given
        let text = "ignored_roles = [\"ref\", \"10:30\", \"gh\", \"gh\"]\n";

        // When
        let errors = load_parser_config(text).expect_err("the config should be refused");

        // Then
        assert_eq!(
            errors,
            [
                "ignored role 'ref' is one rinx implements",
                "ignored role '10:30' is not a name a role is written with",
                "ignored role 'gh' is listed twice",
            ]
        );
    }

    #[test]
    fn test_ignored_roles_contains_only_the_names_listed() {
        // Given
        let roles = IgnoredRoles::new(names(&["issue"])).expect("valid");

        // When / Then
        assert!(roles.contains("issue"));
        assert!(!roles.contains("Issue"));
        assert!(!roles.contains("gh"));
    }

    #[test]
    fn test_ignored_roles_refuses_a_domain_role_rinx_implements() {
        // Given / When
        let errors = IgnoredRoles::new(names(&["py:func"])).expect_err("refused");

        // Then
        assert_eq!(errors[0].fault, IgnoredRoleFault::Implemented);
    }

    #[test]
    fn test_is_role_spelling_takes_domain_qualified_names() {
        // Given / When / Then
        assert!(is_role_spelling("cpp:class"));
        assert!(is_role_spelling("monitoring-event"));
        assert!(!is_role_spelling(""));
        assert!(!is_role_spelling(":issue:"));
        assert!(!is_role_spelling("a::b"));
    }
}

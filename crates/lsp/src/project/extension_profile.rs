//! The Sphinx extensions the server knows, and what each contributes to a
//! document (ADR-038 §5).
//!
//! The table is data — `extension_profiles.toml`, compiled in — so that
//! teaching the server an extension needs no Rust. It is read once, into
//! [`ExtensionProfiles::known`], and refused whole if any entry is wrong: a
//! test loads it, so a mistake in it fails CI rather than an editor.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use rinx_ast::{DiagnosticSubject, Domain};
use serde::Deserialize;

/// The extension table this server was built with.
const KNOWN_PROFILES: &str = include_str!("extension_profiles.toml");

/// Which domains' objects an extension defines — where a reference that
/// resolved to nothing may have pointed at something the extension would
/// have generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetScope {
    /// It defines no domain objects.
    None,
    /// It defines objects of these domains, as autodoc defines `py` ones.
    Domains(BTreeSet<Domain>),
    /// It may define anything — what an extension missing from the table is
    /// assumed to do.
    Everything,
}

impl TargetScope {
    /// Whether a reference about `subject`, having resolved to nothing, may
    /// have named something this scope defines. An `:any:` reference could
    /// name any domain's object, so only [`Self::Everything`] covers it; a
    /// subject that is not a reference is covered by nothing.
    #[must_use]
    pub fn covers(&self, subject: &DiagnosticSubject) -> bool {
        match (self, subject) {
            (
                Self::Everything,
                DiagnosticSubject::DomainReference(_) | DiagnosticSubject::AnyReference,
            ) => true,
            (Self::Domains(domains), DiagnosticSubject::DomainReference(domain)) => {
                domains.contains(domain)
            }
            _ => false,
        }
    }
}

/// What one extension contributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionProfile {
    /// The module name a `conf.py` declares it by, e.g. `sphinx.ext.autodoc`.
    pub name: String,
    /// The directives it adds that this build does not implement.
    pub directives: BTreeSet<String>,
    pub produces_targets: TargetScope,
}

/// The table of known extensions, by name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtensionProfiles {
    by_name: BTreeMap<String, ExtensionProfile>,
}

static KNOWN: LazyLock<ExtensionProfiles> = LazyLock::new(|| {
    load_extension_profiles(KNOWN_PROFILES)
        .unwrap_or_else(|errors| panic!("extension_profiles.toml: {}", errors.join("; ")))
});

impl ExtensionProfiles {
    /// The table compiled into the server. A test loads it, so it cannot
    /// panic in a released build.
    #[must_use]
    pub fn known() -> &'static Self {
        &KNOWN
    }

    /// The profile of the extension declared as `name`, when the table has
    /// one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ExtensionProfile> {
        self.by_name.get(name)
    }
}

/// Reads an extension table, reporting every fault in it rather than the
/// first.
///
/// # Errors
///
/// One message per fault: text that is not a table of the expected shape, an
/// extension listed twice, an empty or repeated name, a domain this build
/// does not have, or a directive this build implements — which is never
/// unknown, so listing it would say nothing.
pub fn load_extension_profiles(text: &str) -> Result<ExtensionProfiles, Vec<String>> {
    let raw: RawProfiles = toml::from_str(text).map_err(|error| vec![error.to_string()])?;
    let mut errors = Vec::new();
    let mut by_name = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for profile in raw.extension {
        if !seen.insert(profile.name.clone()) {
            errors.push(format!("extension '{}' is listed twice", profile.name));
        }
        if let Some(profile) = convert_profile(profile, &mut errors) {
            by_name.insert(profile.name.clone(), profile);
        }
    }
    if errors.is_empty() {
        Ok(ExtensionProfiles { by_name })
    } else {
        Err(errors)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfiles {
    #[serde(default)]
    extension: Vec<RawProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    name: String,
    #[serde(default)]
    directives: Vec<String>,
    /// Checked, but not kept: the parser does not report an unknown role, so
    /// there is nothing to filter yet.
    #[serde(default)]
    roles: Vec<String>,
    produces_targets: Option<RawTargetScope>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawTargetScope {
    Named(String),
    Domains { domains: Vec<String> },
}

/// `raw` as a profile, or `None` with its faults pushed onto `errors`.
fn convert_profile(raw: RawProfile, errors: &mut Vec<String>) -> Option<ExtensionProfile> {
    let before = errors.len();
    let name = raw.name;
    if name.is_empty() {
        errors.push("an extension has an empty name".to_string());
    }
    let directives = names(&name, "directive", raw.directives, errors);
    for directive in &directives {
        if rinx_parser::is_builtin_directive_name(directive) {
            errors.push(format!(
                "extension '{name}' lists directive '{directive}', which rinx implements"
            ));
        }
    }
    names(&name, "role", raw.roles, errors);
    let produces_targets = target_scope(&name, raw.produces_targets, errors);
    (errors.len() == before).then_some(ExtensionProfile {
        name,
        directives,
        produces_targets,
    })
}

/// `written` as a set, reporting an empty or repeated `kind` name of
/// `extension`.
fn names(
    extension: &str,
    kind: &str,
    written: Vec<String>,
    errors: &mut Vec<String>,
) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for name in written {
        if name.is_empty() {
            errors.push(format!(
                "extension '{extension}' lists an empty {kind} name"
            ));
        } else if !set.insert(name.clone()) {
            errors.push(format!(
                "extension '{extension}' lists {kind} '{name}' twice"
            ));
        }
    }
    set
}

/// The scope `raw` writes, [`TargetScope::None`] when it is absent.
fn target_scope(
    extension: &str,
    raw: Option<RawTargetScope>,
    errors: &mut Vec<String>,
) -> TargetScope {
    match raw {
        None => TargetScope::None,
        Some(RawTargetScope::Named(named)) => match named.as_str() {
            "none" => TargetScope::None,
            "everything" => TargetScope::Everything,
            _ => {
                errors.push(format!(
                    "extension '{extension}' produces targets '{named}'; expected \
                     \"none\", \"everything\" or {{ domains = [...] }}"
                ));
                TargetScope::None
            }
        },
        Some(RawTargetScope::Domains { domains }) => TargetScope::Domains(
            domains
                .iter()
                .filter_map(|domain| {
                    let parsed = domain.parse::<Domain>().ok();
                    if parsed.is_none() {
                        errors.push(format!(
                            "extension '{extension}' produces targets in unknown domain '{domain}'"
                        ));
                    }
                    parsed
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(text: &str) -> ExtensionProfiles {
        load_extension_profiles(text).expect("the table should load")
    }

    fn refused(text: &str) -> Vec<String> {
        load_extension_profiles(text).expect_err("the table should be refused")
    }

    #[test]
    fn test_the_known_table_loads() {
        // Given / When
        let result = load_extension_profiles(KNOWN_PROFILES);

        // Then
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_known_finds_autodoc_and_what_it_defines() {
        // Given / When
        let autodoc = ExtensionProfiles::known().get("sphinx.ext.autodoc");

        // Then
        let autodoc = autodoc.expect("autodoc is in the table");
        assert!(autodoc.directives.contains("automodule"));
        assert_eq!(
            autodoc.produces_targets,
            TargetScope::Domains(BTreeSet::from([Domain::Py]))
        );
    }

    #[test]
    fn test_get_does_not_find_an_unlisted_extension() {
        // Given / When / Then
        assert_eq!(ExtensionProfiles::known().get("pyspecific"), None);
    }

    #[test]
    fn test_an_extension_with_nothing_listed_defines_nothing() {
        // Given / When
        let profiles = loaded("[[extension]]\nname = \"sphinx_copybutton\"\n");

        // Then
        let profile = profiles.get("sphinx_copybutton").expect("listed");
        assert!(profile.directives.is_empty());
        assert_eq!(profile.produces_targets, TargetScope::None);
    }

    #[test]
    fn test_produces_targets_reads_every_spelling() {
        // Given
        let text = r#"
            [[extension]]
            name = "a"
            produces_targets = "none"

            [[extension]]
            name = "b"
            produces_targets = "everything"

            [[extension]]
            name = "c"
            produces_targets = { domains = ["py", "c"] }
        "#;

        // When
        let profiles = loaded(text);

        // Then
        let scope = |name: &str| profiles.get(name).expect("listed").produces_targets.clone();
        assert_eq!(scope("a"), TargetScope::None);
        assert_eq!(scope("b"), TargetScope::Everything);
        assert_eq!(
            scope("c"),
            TargetScope::Domains(BTreeSet::from([Domain::Py, Domain::C]))
        );
    }

    #[test]
    fn test_a_directive_rinx_implements_is_refused() {
        // Given — `note` is never unknown, so listing it would say nothing
        let text = "[[extension]]\nname = \"x\"\ndirectives = [\"note\"]\n";

        // When
        let errors = refused(text);

        // Then
        assert_eq!(
            errors,
            ["extension 'x' lists directive 'note', which rinx implements"]
        );
    }

    #[test]
    fn test_an_extension_listed_twice_is_refused() {
        // Given
        let text = "[[extension]]\nname = \"x\"\n\n[[extension]]\nname = \"x\"\n";

        // When / Then
        assert_eq!(refused(text), ["extension 'x' is listed twice"]);
    }

    #[test]
    fn test_empty_and_repeated_names_are_refused() {
        // Given
        let text = "[[extension]]\nname = \"\"\ndirectives = [\"a\", \"a\"]\nroles = [\"\"]\n";

        // When
        let errors = refused(text);

        // Then — every fault, not the first
        assert_eq!(
            errors,
            [
                "an extension has an empty name",
                "extension '' lists directive 'a' twice",
                "extension '' lists an empty role name",
            ]
        );
    }

    #[test]
    fn test_an_unknown_domain_or_scope_is_refused() {
        // Given
        let text = r#"
            [[extension]]
            name = "a"
            produces_targets = { domains = ["js"] }

            [[extension]]
            name = "b"
            produces_targets = "some"
        "#;

        // When
        let errors = refused(text);

        // Then
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("unknown domain 'js'"), "{errors:?}");
        assert!(errors[1].contains("produces targets 'some'"), "{errors:?}");
    }

    #[test]
    fn test_an_unknown_key_is_refused() {
        // Given — a misspelt key must not silently mean "nothing"
        let text = "[[extension]]\nname = \"x\"\ndirective = [\"y\"]\n";

        // When / Then
        assert_eq!(refused(text).len(), 1);
    }

    #[test]
    fn test_domains_cover_only_their_own_references() {
        // Given
        let scope = TargetScope::Domains(BTreeSet::from([Domain::Py]));

        // When / Then
        assert!(scope.covers(&DiagnosticSubject::DomainReference(Domain::Py)));
        assert!(!scope.covers(&DiagnosticSubject::DomainReference(Domain::C)));
        assert!(!scope.covers(&DiagnosticSubject::AnyReference));
    }

    #[test]
    fn test_everything_covers_every_reference_but_no_directive() {
        // Given
        let scope = TargetScope::Everything;

        // When / Then
        assert!(scope.covers(&DiagnosticSubject::DomainReference(Domain::Std)));
        assert!(scope.covers(&DiagnosticSubject::AnyReference));
        assert!(!scope.covers(&DiagnosticSubject::Directive("x".to_string())));
    }

    #[test]
    fn test_none_covers_nothing() {
        // Given / When / Then
        assert!(!TargetScope::None.covers(&DiagnosticSubject::DomainReference(Domain::Py)));
    }
}

//! Where `docs/compatibility.rst` speaks of a construct rinx does not support,
//! for the project report (roadmap #36).
//!
//! The table is data — `compatibility_links.toml`, compiled in — beside
//! `extension_profiles.toml`, which says what an extension provides: an
//! extension's directives and roles lead to its row. What no row names leads
//! to the section it would be listed in, and the report says it is not listed.
//! A test holds the table to the page and to what the parser implements.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::extension_profile::ExtensionProfiles;

/// The table this server was built with.
const KNOWN_LINKS: &str = include_str!("compatibility_links.toml");

/// The published compatibility page.
const COMPATIBILITY_PAGE: &str = "https://nachstedt.github.io/rinx/latest/compatibility.html";

/// The section a directive no row names would be listed in.
const DIRECTIVES_SECTION: &str = "compatibility-directives";
/// The section a domain's construct no row names would be listed in.
const DOMAINS_SECTION: &str = "compatibility-domains";
/// The section a role no row names would be listed in.
const ROLES_SECTION: &str = "compatibility-inline-markup";

/// A place on the compatibility page for one construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityLink<'a> {
    /// The label it stands under on the page.
    pub anchor: &'a str,
    /// Whether that is the construct's own row, rather than the section it
    /// would be listed in.
    pub listed: bool,
}

impl CompatibilityLink<'_> {
    /// The link's address on the published page.
    #[must_use]
    pub fn url(&self) -> String {
        format!("{COMPATIBILITY_PAGE}#{}", self.anchor)
    }
}

/// Each construct's row of the compatibility page, by its name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompatibilityLinks {
    directives: BTreeMap<String, String>,
    roles: BTreeMap<String, String>,
    options: BTreeMap<String, String>,
    extensions: BTreeMap<String, String>,
}

static KNOWN: LazyLock<CompatibilityLinks> = LazyLock::new(|| {
    load_compatibility_links(KNOWN_LINKS)
        .unwrap_or_else(|errors| panic!("compatibility_links.toml: {}", errors.join("; ")))
});

impl CompatibilityLinks {
    /// The table compiled into the server. A test loads it, so it cannot
    /// panic in a released build.
    #[must_use]
    pub fn known() -> &'static Self {
        &KNOWN
    }

    /// The row of the extension declared as `name`: one rinx does not support
    /// entirely.
    #[must_use]
    pub fn extension(&self, name: &str) -> Option<CompatibilityLink<'_>> {
        listed(self.extensions.get(name))
    }

    /// Where the page speaks of the directive `name`.
    #[must_use]
    pub fn directive(&self, name: &str, profiles: &ExtensionProfiles) -> CompatibilityLink<'_> {
        listed(self.directives.get(name))
            .or_else(|| self.extension(profiles.providing_directive(name)?))
            .unwrap_or(section(name, DIRECTIVES_SECTION))
    }

    /// Where the page speaks of the role `name`, as written: a domain's may
    /// be written bare too.
    #[must_use]
    pub fn role(&self, name: &str, profiles: &ExtensionProfiles) -> CompatibilityLink<'_> {
        let bare = |domain: &str| self.roles.get(&format!("{domain}:{name}"));
        listed(
            self.roles
                .get(name)
                .or_else(|| ["std", "py", "c"].into_iter().find_map(bare)),
        )
        .or_else(|| self.extension(profiles.providing_role(name)?))
        .unwrap_or(section(name, ROLES_SECTION))
    }

    /// Where the page speaks of the option `option`.
    #[must_use]
    pub fn option(&self, option: &str) -> CompatibilityLink<'_> {
        listed(self.options.get(option)).unwrap_or(CompatibilityLink {
            anchor: DIRECTIVES_SECTION,
            listed: false,
        })
    }
}

/// The link of a row found under `anchor`, if one was.
fn listed(anchor: Option<&String>) -> Option<CompatibilityLink<'_>> {
    anchor.map(|anchor| CompatibilityLink {
        anchor,
        listed: true,
    })
}

/// The section a construct named `name` no row names would be listed in: a
/// domain's, when the name is qualified, or else `otherwise`.
fn section(name: &str, otherwise: &'static str) -> CompatibilityLink<'static> {
    CompatibilityLink {
        anchor: if name.contains(':') {
            DOMAINS_SECTION
        } else {
            otherwise
        },
        listed: false,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLinks {
    #[serde(default)]
    row: Vec<RawRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRow {
    anchor: String,
    #[serde(default)]
    directives: Vec<String>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
}

/// Reads a table of links, reporting every fault in it rather than the first.
///
/// # Errors
///
/// One message per fault: text that is not a table of the expected shape, or
/// a construct listed under two rows.
pub fn load_compatibility_links(text: &str) -> Result<CompatibilityLinks, Vec<String>> {
    let raw: RawLinks = toml::from_str(text).map_err(|error| vec![error.to_string()])?;
    let mut errors = Vec::new();
    let mut links = CompatibilityLinks::default();
    for row in raw.row {
        for (kind, names, list) in [
            ("directive", row.directives, &mut links.directives),
            ("role", row.roles, &mut links.roles),
            ("option", row.options, &mut links.options),
            ("extension", row.extensions, &mut links.extensions),
        ] {
            for name in names {
                if list.insert(name.clone(), row.anchor.clone()).is_some() {
                    errors.push(format!("{kind} '{name}' is listed twice"));
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(links)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// `docs/compatibility.rst` as the repository holds it.
    fn compatibility_page() -> String {
        let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/compatibility.rst");
        std::fs::read_to_string(page).expect("the compatibility page")
    }

    #[test]
    fn test_every_anchor_stands_on_the_compatibility_page() {
        // Given
        let links = CompatibilityLinks::known();
        let page = compatibility_page();

        // When
        let missing: Vec<&str> = [
            &links.directives,
            &links.roles,
            &links.options,
            &links.extensions,
        ]
        .into_iter()
        .flat_map(BTreeMap::values)
        .map(String::as_str)
        .chain([DIRECTIVES_SECTION, DOMAINS_SECTION, ROLES_SECTION])
        .filter(|anchor| !page.contains(&format!(".. _{anchor}:")))
        .collect();

        // Then
        assert_eq!(missing, Vec::<&str>::new());
    }

    #[test]
    fn test_no_linked_directive_or_role_is_one_rinx_implements() {
        // Given
        let links = CompatibilityLinks::known();

        // When
        let directives: Vec<&String> = links
            .directives
            .keys()
            .filter(|name| rinx_parser::is_builtin_directive_name(name))
            .collect();
        let roles: Vec<&String> = links
            .roles
            .keys()
            .filter(|name| rinx_parser::is_fixed_role_name(name))
            .collect();

        // Then
        assert_eq!(directives, Vec::<&String>::new());
        assert_eq!(roles, Vec::<&String>::new());
    }

    #[test]
    fn test_every_extension_adding_what_rinx_lacks_has_a_row() {
        // Given
        let links = CompatibilityLinks::known();

        // When
        let unlinked: Vec<&str> = ExtensionProfiles::known()
            .iter()
            .filter(|profile| !profile.directives.is_empty() || !profile.roles.is_empty())
            .map(|profile| profile.name.as_str())
            .filter(|name| links.extension(name).is_none())
            .collect();

        // Then
        assert_eq!(unlinked, Vec::<&str>::new());
    }

    #[test]
    fn test_every_listed_extension_is_a_known_one() {
        // Given
        let links = CompatibilityLinks::known();

        // When
        let unknown: Vec<&String> = links
            .extensions
            .keys()
            .filter(|name| ExtensionProfiles::known().get(name).is_none())
            .collect();

        // Then
        assert_eq!(unknown, Vec::<&String>::new());
    }

    #[test]
    fn test_every_unsupported_role_has_a_row() {
        // Given
        let links = CompatibilityLinks::known();
        let profiles = ExtensionProfiles::known();

        // When
        let unlisted: Vec<String> = rinx_parser::unsupported_role_names()
            .filter(|name| !links.role(name, profiles).listed)
            .collect();

        // Then
        assert_eq!(unlisted, Vec::<String>::new());
    }

    #[test]
    fn test_a_directive_leads_to_its_row_or_its_extensions_or_its_section() {
        // Given
        let links = CompatibilityLinks::known();
        let profiles = ExtensionProfiles::known();

        // When / Then
        assert_eq!(
            links.directive("rubric", profiles),
            CompatibilityLink {
                anchor: "compatibility-topic",
                listed: true
            }
        );
        assert_eq!(
            links.directive("automodule", profiles),
            CompatibilityLink {
                anchor: "compatibility-autodoc",
                listed: true
            }
        );
        assert_eq!(
            links.directive("availability", profiles),
            CompatibilityLink {
                anchor: DIRECTIVES_SECTION,
                listed: false
            }
        );
        assert_eq!(
            links.directive("cpp:class", profiles).anchor,
            DOMAINS_SECTION
        );
    }

    #[test]
    fn test_a_role_leads_to_its_row_bare_or_qualified() {
        // Given
        let links = CompatibilityLinks::known();
        let profiles = ExtensionProfiles::known();

        // When / Then
        assert_eq!(
            links.role("kbd", profiles).anchor,
            "compatibility-semantic-roles"
        );
        assert_eq!(
            links.role("envvar", profiles).anchor,
            "compatibility-envvar"
        );
        assert_eq!(
            links.role("std:envvar", profiles).anchor,
            "compatibility-envvar"
        );
        assert_eq!(
            links.role("cite:p", profiles).anchor,
            "compatibility-bibtex"
        );
        assert_eq!(
            links.role("gh", profiles),
            CompatibilityLink {
                anchor: ROLES_SECTION,
                listed: false
            }
        );
    }

    #[test]
    fn test_an_option_leads_to_its_row_or_the_directives_section() {
        // Given
        let links = CompatibilityLinks::known();

        // When / Then
        assert_eq!(
            links.option("no-typesetting").anchor,
            "compatibility-no-typesetting"
        );
        assert_eq!(links.option("ref-type").anchor, DIRECTIVES_SECTION);
    }

    #[test]
    fn test_a_link_addresses_the_published_page() {
        // Given
        let link = CompatibilityLink {
            anchor: "compatibility-topic",
            listed: true,
        };

        // When / Then
        assert_eq!(
            link.url(),
            "https://nachstedt.github.io/rinx/latest/compatibility.html#compatibility-topic"
        );
    }

    #[test]
    fn test_a_construct_listed_twice_is_refused() {
        // Given
        let text = "[[row]]\nanchor = \"a\"\nroles = [\"x\"]\n\n[[row]]\nanchor = \"b\"\nroles = [\"x\"]\n";

        // When
        let errors = load_compatibility_links(text).expect_err("refused");

        // Then
        assert_eq!(errors, ["role 'x' is listed twice"]);
    }

    #[test]
    fn test_an_unknown_key_is_refused() {
        // Given / When
        let errors =
            load_compatibility_links("[[row]]\nanchor = \"a\"\nrolez = []\n").expect_err("refused");

        // Then
        assert!(errors[0].contains("rolez"), "{errors:?}");
    }
}

//! [`RegistryTarget`], the target of any of the four registry roles: what
//! the rest of the build asks of a `:pep:`, `:rfc:`, `:cve:` or `:cwe:`
//! without matching on which it is.

use serde::{Deserialize, Serialize};

use super::cve::CveTarget;
use super::cwe::CweTarget;
use super::invalid::InvalidRegistryTarget;
use super::pep::PepTarget;
use super::registry::Registry;
use super::rfc::RfcTarget;

/// A registry role's target, parsed by the rules of its registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryTarget {
    Pep(PepTarget),
    Rfc(RfcTarget),
    Cve(CveTarget),
    Cwe(CweTarget),
}

impl RegistryTarget {
    /// Reads a target written in `registry`'s role.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidRegistryTarget`] when the text is not a target of
    /// that registry.
    pub fn parse(registry: Registry, written: &str) -> Result<Self, InvalidRegistryTarget> {
        Ok(match registry {
            Registry::Pep => Self::Pep(PepTarget::parse(written)?),
            Registry::Rfc => Self::Rfc(RfcTarget::parse(written)?),
            Registry::Cve => Self::Cve(CveTarget::parse(written)?),
            Registry::Cwe => Self::Cwe(CweTarget::parse(written)?),
        })
    }

    /// Which registry the target names a document of.
    #[must_use]
    pub const fn registry(&self) -> Registry {
        match self {
            Self::Pep(_) => Registry::Pep,
            Self::Rfc(_) => Registry::Rfc,
            Self::Cve(_) => Registry::Cve,
            Self::Cwe(_) => Registry::Cwe,
        }
    }

    /// The target as it was written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        match self {
            Self::Pep(target) => target.as_written(),
            Self::Rfc(target) => target.as_written(),
            Self::Cve(target) => target.as_written(),
            Self::Cwe(target) => target.as_written(),
        }
    }

    /// What a link shows without an explicit title, which Sphinx also files
    /// as the mention's general-index subentry: the registry's label and the
    /// target as written — `PEP 8#naming` — except that an RFC spells out a
    /// section, appendix or page anchor.
    #[must_use]
    pub fn display_text(&self) -> String {
        match self {
            Self::Rfc(target) => target.display_text(),
            Self::Pep(_) | Self::Cve(_) | Self::Cwe(_) => {
                format!("{} {}", self.registry().label(), self.as_written())
            }
        }
    }

    /// The document's address relative to its registry's base URL, fragment
    /// included — `pep-0008/`, `rfc2324.html`, `CVE-2024-3094`, `787.html`.
    #[must_use]
    pub fn page_path(&self) -> String {
        match self {
            Self::Pep(target) => target.page_path(),
            Self::Rfc(target) => target.page_path(),
            Self::Cve(target) => target.page_path(),
            Self::Cwe(target) => target.page_path(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(registry: Registry, written: &str) -> RegistryTarget {
        RegistryTarget::parse(registry, written).unwrap()
    }

    #[test]
    fn test_parse_reads_by_the_registry_rules() {
        // Given / When / Then
        for registry in Registry::ALL {
            let written = if registry == Registry::Cve {
                "2024-3094"
            } else {
                "8"
            };
            let target = parse(registry, written);
            assert_eq!(target.registry(), registry);
            assert_eq!(target.as_written(), written);
        }
        assert!(RegistryTarget::parse(Registry::Cve, "8").is_err());
        assert!(RegistryTarget::parse(Registry::Cwe, "2024-3094").is_err());
    }

    #[test]
    fn test_display_text_prefixes_the_label() {
        // Given / When / Then
        assert_eq!(parse(Registry::Pep, "8#x").display_text(), "PEP 8#x");
        assert_eq!(
            parse(Registry::Cve, "2024-3094").display_text(),
            "CVE 2024-3094"
        );
        assert_eq!(parse(Registry::Cwe, "787").display_text(), "CWE 787");
    }

    #[test]
    fn test_display_text_spells_out_an_rfc_section() {
        // Given / When / Then
        assert_eq!(
            parse(Registry::Rfc, "2324#section-2").display_text(),
            "RFC 2324 Section 2"
        );
    }

    #[test]
    fn test_page_path_is_each_registry_page() {
        // Given / When / Then
        assert_eq!(parse(Registry::Pep, "8").page_path(), "pep-0008/");
        assert_eq!(parse(Registry::Rfc, "8").page_path(), "rfc8.html");
        assert_eq!(
            parse(Registry::Cve, "2024-3094").page_path(),
            "CVE-2024-3094"
        );
        assert_eq!(parse(Registry::Cwe, "8").page_path(), "8.html");
    }

    #[test]
    fn test_serde_tags_the_target_with_its_registry() {
        // Given
        let target = parse(Registry::Rfc, "2324");

        // When
        let json = serde_json::to_string(&target).unwrap();

        // Then
        assert_eq!(json, "{\"rfc\":\"2324\"}");
        assert_eq!(
            serde_json::from_str::<RegistryTarget>(&json).unwrap(),
            target
        );
    }
}

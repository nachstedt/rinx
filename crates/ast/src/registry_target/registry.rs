//! Which registry a registry role links into.

use serde::{Deserialize, Serialize};

/// One of the four registries Sphinx links a numbered document of: Python
/// Enhancement Proposals, the IETF's Requests for Comments, MITRE's
/// Common Vulnerabilities and Exposures and its Common Weakness Enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Registry {
    Pep,
    Rfc,
    Cve,
    Cwe,
}

impl Registry {
    /// Every registry, in the order Sphinx's `roles.py` lists their roles
    /// beside `:pep:`.
    pub const ALL: [Self; 4] = [Self::Pep, Self::Rfc, Self::Cve, Self::Cwe];

    /// The name of the role linking into this registry — `pep` — which is
    /// also the class Sphinx gives the link.
    #[must_use]
    pub const fn role_name(self) -> &'static str {
        match self {
            Self::Pep => "pep",
            Self::Rfc => "rfc",
            Self::Cve => "cve",
            Self::Cwe => "cwe",
        }
    }

    /// What a document of this registry is called in text — `PEP` — before
    /// its number.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pep => "PEP",
            Self::Rfc => "RFC",
            Self::Cve => "CVE",
            Self::Cwe => "CWE",
        }
    }

    /// The general-index group a mention is filed under, as Sphinx's role
    /// names it: each mention is one subentry below it.
    #[must_use]
    pub const fn index_group(self) -> &'static str {
        match self {
            Self::Pep => "Python Enhancement Proposals",
            Self::Rfc => "RFC",
            Self::Cve => "Common Vulnerabilities and Exposures",
            Self::Cwe => "Common Weakness Enumeration",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_name_is_the_lowercase_label() {
        // Given / When / Then
        for registry in Registry::ALL {
            assert_eq!(
                registry.role_name(),
                registry.label().to_ascii_lowercase(),
                "{registry:?}"
            );
        }
    }

    #[test]
    fn test_index_group_is_sphinx_wording() {
        // Given / When / Then
        assert_eq!(Registry::Pep.index_group(), "Python Enhancement Proposals");
        assert_eq!(Registry::Rfc.index_group(), "RFC");
        assert_eq!(
            Registry::Cve.index_group(),
            "Common Vulnerabilities and Exposures"
        );
        assert_eq!(Registry::Cwe.index_group(), "Common Weakness Enumeration");
    }

    #[test]
    fn test_serializes_as_the_role_name() {
        // Given / When / Then
        for registry in Registry::ALL {
            assert_eq!(
                serde_json::to_string(&registry).unwrap(),
                format!("\"{}\"", registry.role_name())
            );
        }
    }
}

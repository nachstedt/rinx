use serde::{Deserialize, Serialize};

/// A Sphinx-style documentation domain (e.g. `py`, `c`).
///
/// Domains namespace directives and cross-reference roles so the same
/// object-type name (e.g. `function`) can mean different things in
/// different languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Py,
    C,
    /// The "standard" domain: constructs with no language affiliation (e.g.
    /// `.. option::`/`:option:`). Unlike `Py`/`C`, std-domain directives and
    /// roles are recognized unconditionally by the parser, independent of a
    /// library's `default_domain` attribute — this variant exists only so
    /// `ObjectType`'s `domain:objtype` keys and (de)serialization stay
    /// uniform across all three domains, not to gate dispatch.
    Std,
}

impl Domain {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Py => "py",
            Self::C => "c",
            Self::Std => "std",
        }
    }
}

impl std::str::FromStr for Domain {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "py" => Ok(Self::Py),
            "c" => Ok(Self::C),
            "std" => Ok(Self::Std),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_from_str_accepts_known_domains() {
        // Given / When / Then
        assert_eq!("py".parse::<Domain>(), Ok(Domain::Py));
        assert_eq!("c".parse::<Domain>(), Ok(Domain::C));
        assert_eq!("std".parse::<Domain>(), Ok(Domain::Std));
    }

    #[test]
    fn test_domain_from_str_rejects_unknown_domain() {
        // Given
        let input = "rust";

        // When
        let result = input.parse::<Domain>();

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_domain_as_str_and_display_round_trip() {
        // Given
        let domain = Domain::C;

        // When
        let s = domain.as_str();
        let displayed = domain.to_string();

        // Then
        assert_eq!(s, "c");
        assert_eq!(displayed, "c");
        assert_eq!(s.parse::<Domain>().unwrap(), domain);
    }

    #[test]
    fn test_domain_serialization_roundtrip() {
        // Given
        let domain = Domain::Py;

        // When
        let json = serde_json::to_string(&domain).expect("Failed to serialize");
        let deserialized: Domain = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(json, "\"py\"");
        assert_eq!(domain, deserialized);
    }
}

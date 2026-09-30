//! Which domain index pages a site writes — Sphinx's `html_domain_indices`,
//! set on `rinx_site` as `domain_indices` and passed to every action drawing
//! a page as `--domain-index`.

use std::fmt;
use std::str::FromStr;

/// One domain index page a site may enable.
///
/// Opt-in rather than derived from the documents, because Bazel declares a
/// page before any document is read, and a page no project asked for — a
/// Python Module Index for documentation without Python — should not be
/// written at all (ADR-032).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DomainIndex {
    /// `py-modindex`, the Python Module Index.
    PyModindex,
}

impl DomainIndex {
    /// Every index, so a caller can enumerate them without repeating the list.
    pub const ALL: [Self; 1] = [Self::PyModindex];

    /// The index's name as Sphinx spells it, which is how it is written in
    /// `domain_indices` and on the command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PyModindex => "py-modindex",
        }
    }
}

impl fmt::Display for DomainIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A name that is not a domain index this build writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownDomainIndex(pub String);

impl fmt::Display for UnknownDomainIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let known: Vec<&str> = DomainIndex::ALL.iter().map(|index| index.name()).collect();
        write!(
            f,
            "Unknown domain index '{}'; this build writes: {}",
            self.0,
            known.join(", ")
        )
    }
}

impl std::error::Error for UnknownDomainIndex {}

impl FromStr for DomainIndex {
    type Err = UnknownDomainIndex;

    /// The exact inverse of [`DomainIndex::name`].
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|index| index.name() == name)
            .ok_or_else(|| UnknownDomainIndex(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_is_the_inverse_of_name() {
        // Given / When / Then
        for index in DomainIndex::ALL {
            assert_eq!(index.name().parse::<DomainIndex>(), Ok(index));
        }
    }

    #[test]
    fn test_from_str_refuses_an_unknown_name_listing_the_known_ones() {
        // Given / When
        let error = "modindex".parse::<DomainIndex>().unwrap_err();

        // Then
        assert_eq!(
            error.to_string(),
            "Unknown domain index 'modindex'; this build writes: py-modindex"
        );
    }

    #[test]
    fn test_display_writes_the_name() {
        // Given / When / Then
        assert_eq!(DomainIndex::PyModindex.to_string(), "py-modindex");
    }
}

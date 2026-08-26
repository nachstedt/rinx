use serde::{Deserialize, Serialize};

/// One of the comparison/reporting flags `CPython`'s `doctest` module accepts.
///
/// A closed set, so it is modeled as an enum rather than a string: an
/// unrecognized name in a `:options:` list is an authoring mistake worth a
/// diagnostic, not something to pass through to the runner and discover at test
/// time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DocTestFlagName {
    DontAcceptTrueFor1,
    DontAcceptBlankline,
    NormalizeWhitespace,
    Ellipsis,
    Skip,
    IgnoreExceptionDetail,
    ReportUdiff,
    ReportCdiff,
    ReportNdiff,
    ReportOnlyFirstFailure,
    FailFast,
}

impl DocTestFlagName {
    /// Every flag, in the order [`Self::as_doctest_name`] documents them.
    ///
    /// Kept next to the name mapping so a new variant that is added to one and
    /// forgotten in the other is caught by the round-trip test below.
    pub const ALL: [Self; 11] = [
        Self::DontAcceptTrueFor1,
        Self::DontAcceptBlankline,
        Self::NormalizeWhitespace,
        Self::Ellipsis,
        Self::Skip,
        Self::IgnoreExceptionDetail,
        Self::ReportUdiff,
        Self::ReportCdiff,
        Self::ReportNdiff,
        Self::ReportOnlyFirstFailure,
        Self::FailFast,
    ];

    /// The flag's spelling in a `:options:` list, matching the keys of
    /// `CPython`'s `doctest.OPTIONFLAGS_BY_NAME`.
    #[must_use]
    pub const fn as_doctest_name(self) -> &'static str {
        match self {
            Self::DontAcceptTrueFor1 => "DONT_ACCEPT_TRUE_FOR_1",
            Self::DontAcceptBlankline => "DONT_ACCEPT_BLANKLINE",
            Self::NormalizeWhitespace => "NORMALIZE_WHITESPACE",
            Self::Ellipsis => "ELLIPSIS",
            Self::Skip => "SKIP",
            Self::IgnoreExceptionDetail => "IGNORE_EXCEPTION_DETAIL",
            Self::ReportUdiff => "REPORT_UDIFF",
            Self::ReportCdiff => "REPORT_CDIFF",
            Self::ReportNdiff => "REPORT_NDIFF",
            Self::ReportOnlyFirstFailure => "REPORT_ONLY_FIRST_FAILURE",
            Self::FailFast => "FAIL_FAST",
        }
    }

    /// Resolves a `:options:` spelling to a flag, or `None` if unrecognized.
    #[must_use]
    pub fn from_doctest_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|flag| flag.as_doctest_name() == name)
    }
}

/// A doctest flag together with the `+`/`-` sign it was written with.
///
/// The sign is markup, not part of the name, so it is stripped during parsing
/// and recorded as typed intent here rather than left in the string for the
/// runner to re-interpret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DocTestFlag {
    pub name: DocTestFlagName,
    /// `true` for `+FLAG` (enable), `false` for `-FLAG` (disable).
    pub enabled: bool,
}

impl DocTestFlag {
    /// Creates an enabled (`+`) flag.
    #[must_use]
    pub const fn enable(name: DocTestFlagName) -> Self {
        Self {
            name,
            enabled: true,
        }
    }

    /// Creates a disabled (`-`) flag.
    #[must_use]
    pub const fn disable(name: DocTestFlagName) -> Self {
        Self {
            name,
            enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_doctest_name_matches_cpython_spelling() {
        // Given
        let flag = DocTestFlagName::NormalizeWhitespace;

        // When
        let name = flag.as_doctest_name();

        // Then
        assert_eq!(name, "NORMALIZE_WHITESPACE");
    }

    #[test]
    fn test_from_doctest_name_resolves_a_known_flag() {
        // Given
        let name = "ELLIPSIS";

        // When
        let flag = DocTestFlagName::from_doctest_name(name);

        // Then
        assert_eq!(flag, Some(DocTestFlagName::Ellipsis));
    }

    #[test]
    fn test_from_doctest_name_rejects_an_unknown_flag() {
        // Given
        let name = "NOT_A_FLAG";

        // When
        let flag = DocTestFlagName::from_doctest_name(name);

        // Then
        assert_eq!(flag, None);
    }

    #[test]
    fn test_from_doctest_name_is_case_sensitive() {
        // Given — doctest's own table is upper-case only.
        let name = "ellipsis";

        // When
        let flag = DocTestFlagName::from_doctest_name(name);

        // Then
        assert_eq!(flag, None);
    }

    #[test]
    fn test_every_flag_in_all_roundtrips_through_its_doctest_name() {
        // Given — the hand-maintained ALL table and name mapping.
        for flag in DocTestFlagName::ALL {
            // When
            let resolved = DocTestFlagName::from_doctest_name(flag.as_doctest_name());

            // Then — a half-finished edit to either table fails here.
            assert_eq!(resolved, Some(flag), "flag {flag:?} did not round-trip");
        }
    }

    #[test]
    fn test_all_contains_no_duplicate_names() {
        // Given
        let mut names: Vec<&str> = DocTestFlagName::ALL
            .iter()
            .map(|f| f.as_doctest_name())
            .collect();

        // When
        names.sort_unstable();
        let before = names.len();
        names.dedup();

        // Then
        assert_eq!(names.len(), before, "ALL contains a duplicate doctest name");
    }

    #[test]
    fn test_enable_records_the_plus_sign_as_enabled() {
        // Given / When
        let flag = DocTestFlag::enable(DocTestFlagName::Ellipsis);

        // Then
        assert_eq!(flag.name, DocTestFlagName::Ellipsis);
        assert!(flag.enabled);
    }

    #[test]
    fn test_disable_records_the_minus_sign_as_disabled() {
        // Given / When
        let flag = DocTestFlag::disable(DocTestFlagName::Ellipsis);

        // Then
        assert_eq!(flag.name, DocTestFlagName::Ellipsis);
        assert!(!flag.enabled);
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Given
        let flag = DocTestFlag::disable(DocTestFlagName::ReportOnlyFirstFailure);

        // When
        let json = serde_json::to_string(&flag).expect("Failed to serialize");
        let deserialized: DocTestFlag = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(flag, deserialized);
    }
}

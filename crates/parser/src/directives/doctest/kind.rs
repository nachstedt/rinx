//! Which of the five `sphinx.ext.doctest` directives is being parsed.

/// Which of the five directives is being parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocTestDirectiveKind {
    Doctest,
    TestCode,
    TestOutput,
    TestSetup,
    TestCleanup,
}

impl DocTestDirectiveKind {
    /// Resolves a directive name, or `None` if it is not one of the five.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "doctest" => Some(Self::Doctest),
            "testcode" => Some(Self::TestCode),
            "testoutput" => Some(Self::TestOutput),
            "testsetup" => Some(Self::TestSetup),
            "testcleanup" => Some(Self::TestCleanup),
            _ => None,
        }
    }

    /// The directive's name, for diagnostics.
    pub(crate) const fn as_name(self) -> &'static str {
        match self {
            Self::Doctest => "doctest",
            Self::TestCode => "testcode",
            Self::TestOutput => "testoutput",
            Self::TestSetup => "testsetup",
            Self::TestCleanup => "testcleanup",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_name_resolves_every_directive_in_the_family() {
        // Given
        let cases = [
            ("doctest", DocTestDirectiveKind::Doctest),
            ("testcode", DocTestDirectiveKind::TestCode),
            ("testoutput", DocTestDirectiveKind::TestOutput),
            ("testsetup", DocTestDirectiveKind::TestSetup),
            ("testcleanup", DocTestDirectiveKind::TestCleanup),
        ];

        for (name, expected) in cases {
            // When / Then
            assert_eq!(DocTestDirectiveKind::from_name(name), Some(expected));
        }
    }

    #[test]
    fn test_from_name_rejects_an_unrelated_directive() {
        // Given / When / Then
        assert_eq!(DocTestDirectiveKind::from_name("note"), None);
    }

    #[test]
    fn test_from_name_and_as_name_roundtrip_for_every_kind() {
        // Given — two hand-maintained mappings that must stay in step.
        let kinds = [
            DocTestDirectiveKind::Doctest,
            DocTestDirectiveKind::TestCode,
            DocTestDirectiveKind::TestOutput,
            DocTestDirectiveKind::TestSetup,
            DocTestDirectiveKind::TestCleanup,
        ];

        for kind in kinds {
            // When / Then
            assert_eq!(DocTestDirectiveKind::from_name(kind.as_name()), Some(kind));
        }
    }
}

//! Why a registry role's target was refused — one error for all four
//! registries, so `:pep:`, `:rfc:`, `:cve:` and `:cwe:` word their refusals
//! alike.

use std::fmt;

use super::registry::Registry;

/// A written target a registry role cannot link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidRegistryTarget {
    registry: Registry,
    target: String,
    reason: InvalidRegistryTargetReason,
}

/// What is wrong with a refused registry target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidRegistryTargetReason {
    /// The part before any `#` is not a run of ASCII digits.
    NotANumber,
    /// A run of digits too long to be any document's number.
    TooLarge,
    /// A `:cve:` target that is not a year and a sequence number.
    NotACveId,
    /// A `:cve:` target that repeats the `CVE-` prefix the link adds.
    CvePrefix,
}

impl InvalidRegistryTarget {
    pub(crate) fn new(
        registry: Registry,
        target: &str,
        reason: InvalidRegistryTargetReason,
    ) -> Self {
        Self {
            registry,
            target: target.to_string(),
            reason,
        }
    }

    /// The registry whose role was refused.
    #[must_use]
    pub const fn registry(&self) -> Registry {
        self.registry
    }

    /// The target as written.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Why it was refused.
    #[must_use]
    pub const fn reason(&self) -> InvalidRegistryTargetReason {
        self.reason
    }
}

impl fmt::Display for InvalidRegistryTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Sphinx's own opening, "invalid PEP number", for every registry.
        let Self {
            registry, target, ..
        } = self;
        write!(f, "invalid {} number '{target}': ", registry.label())?;
        match self.reason {
            InvalidRegistryTargetReason::NotANumber => {
                f.write_str("write the number in digits, optionally followed by '#' and an anchor")
            }
            InvalidRegistryTargetReason::TooLarge => f.write_str("too large"),
            InvalidRegistryTargetReason::NotACveId => f.write_str(
                "write the year and the sequence number, such as '2024-3094', optionally \
                 followed by '#' and an anchor",
            ),
            InvalidRegistryTargetReason::CvePrefix => {
                f.write_str("drop the 'CVE-' prefix, which the link and its text already add")
            }
        }
    }
}

impl std::error::Error for InvalidRegistryTarget {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_names_the_registry_and_the_target() {
        // Given
        let error = InvalidRegistryTarget::new(
            Registry::Rfc,
            "abc",
            InvalidRegistryTargetReason::NotANumber,
        );

        // When
        let message = error.to_string();

        // Then
        assert!(
            message.starts_with("invalid RFC number 'abc': "),
            "{message}"
        );
        assert!(message.contains("in digits"), "{message}");
    }

    #[test]
    fn test_display_explains_each_reason() {
        // Given / When / Then
        for (reason, expected) in [
            (InvalidRegistryTargetReason::TooLarge, "too large"),
            (
                InvalidRegistryTargetReason::NotACveId,
                "such as '2024-3094'",
            ),
            (
                InvalidRegistryTargetReason::CvePrefix,
                "drop the 'CVE-' prefix",
            ),
        ] {
            let message = InvalidRegistryTarget::new(Registry::Cve, "x", reason).to_string();
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn test_accessors_return_what_was_refused() {
        // Given
        let error =
            InvalidRegistryTarget::new(Registry::Cwe, "x", InvalidRegistryTargetReason::NotANumber);

        // When / Then
        assert_eq!(error.registry(), Registry::Cwe);
        assert_eq!(error.target(), "x");
        assert_eq!(error.reason(), InvalidRegistryTargetReason::NotANumber);
    }
}

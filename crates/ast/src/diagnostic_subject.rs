use serde::{Deserialize, Serialize};

use crate::domain::Domain;

/// What a [`Diagnostic`](crate::Diagnostic) is about, for a front end that
/// treats findings differently by their subject rather than by their code
/// alone.
///
/// The language server lowers an unknown directive that a declared Sphinx
/// extension provides, and a broken reference into a domain such an extension
/// fills (ADR-038 §5). Both facts were known where the diagnostic was raised
/// and lost in its message; carrying them here keeps the filter from parsing
/// prose. Only the findings such a filter asks about carry one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSubject {
    /// A directive, by its name as written — domain prefix included, as in
    /// `cpp:class`.
    Directive(String),
    /// A reference into a domain's objects that resolved to nothing: a domain
    /// role, or `:option:` (the `std` domain's).
    DomainReference(Domain),
    /// An `:any:` reference that resolved to nothing, which could have named
    /// a target of any domain.
    AnyReference,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_roundtrips_every_subject() {
        // Given
        let subjects = [
            DiagnosticSubject::Directive("cpp:class".to_string()),
            DiagnosticSubject::DomainReference(Domain::C),
            DiagnosticSubject::AnyReference,
        ];

        for subject in subjects {
            // When
            let json = serde_json::to_string(&subject).expect("Failed to serialize");
            let deserialized: DiagnosticSubject =
                serde_json::from_str(&json).expect("Failed to deserialize");

            // Then
            assert_eq!(deserialized, subject, "{json}");
        }
    }

    #[test]
    fn test_serialization_names_a_directive_subject_by_kind() {
        // Given
        let subject = DiagnosticSubject::Directive("automodule".to_string());

        // When
        let json = serde_json::to_string(&subject).expect("Failed to serialize");

        // Then
        assert_eq!(json, r#"{"directive":"automodule"}"#);
    }
}

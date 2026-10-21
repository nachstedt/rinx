//! What a diagram reports while rendering.

use std::fmt;

use rinx_ast::{DiagnosticCode, Span};
use rinx_uml::{FlowError, SequenceError, UmlError};

/// A diagram directive that produced no picture, or — for a sequence
/// diagram — one that drew a picture with something worth reporting about it.
///
/// Only reportable here, like [`crate::EmptyListingError`], and for the same
/// reason: a diagram asks the entity graph questions — whether written as a
/// template or generated from a filter — and whether they can be answered
/// depends on every document in the project. The parser, which sees one, cannot
/// know.
///
/// The failure itself is [`DiagramFailure`], produced by `rinx_uml`.
/// What this type adds
/// is the two things only the renderer holds — where the directive was
/// written, and what the author spelled it — so the warning can point at a
/// line rather than at a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramError {
    /// The directive as the author spelled it — `needuml`, `entity-arch`,
    /// `entity-flow`, …
    pub directive: String,
    /// Why there is no picture.
    pub error: DiagramFailure,
    /// Where the directive was written, when the node carried a position.
    pub span: Option<Span>,
}

/// Why a diagram directive produced no picture.
///
/// One kind per way a diagram comes about: a template that was expanded, a
/// flowchart that was generated, and a sequence diagram that was walked. They are kept apart
/// rather than merged into one error type because a diagnostic code names the
/// construct the author wrote — a flowchart that drew nothing is
/// `entity-flow.empty-result`, not `uml.empty-result` — while everything the
/// worker does with a failure (position it, suppress it, format it) is the
/// same for both, which is why one [`DiagramError`] still carries either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramFailure {
    /// A template that could not be expanded.
    Uml(UmlError),
    /// A flowchart that could not be drawn.
    Flow(FlowError),
    /// A sequence diagram's walk reporting a problem. Unlike the other two,
    /// not every one of these means the picture is missing: an unknown start
    /// or a truncated walk still draws everything else.
    Sequence(SequenceError),
}

impl DiagramFailure {
    /// The diagnostic code this reports under.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Uml(error) => error.code(),
            Self::Flow(error) => error.code(),
            Self::Sequence(error) => error.code(),
        }
    }
}

impl fmt::Display for DiagramFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uml(error) => error.fmt(f),
            Self::Flow(error) => error.fmt(f),
            Self::Sequence(error) => error.fmt(f),
        }
    }
}

impl From<UmlError> for DiagramFailure {
    fn from(error: UmlError) -> Self {
        Self::Uml(error)
    }
}

impl From<SequenceError> for DiagramFailure {
    fn from(error: SequenceError) -> Self {
        Self::Sequence(error)
    }
}

impl From<FlowError> for DiagramFailure {
    fn from(error: FlowError) -> Self {
        Self::Flow(error)
    }
}

impl DiagramError {
    /// The diagnostic code this reports under — what a `.. noqa:` names to
    /// suppress it.
    ///
    /// Delegated to the failure rather than decided here, so the renderer and
    /// any later reporter cannot disagree about what a given failure is
    /// called.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.error.code()
    }

    /// The author-facing message.
    #[must_use]
    pub fn message(&self) -> String {
        format!("{}: {}", self.directive, self.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_for(error: UmlError) -> DiagramError {
        DiagramError {
            directive: "needuml".to_string(),
            error: error.into(),
            span: None,
        }
    }

    #[test]
    fn test_the_code_is_the_failures_own() {
        // Given
        let error = error_for(UmlError::UnknownEntity("REQ_404".to_string()));

        // When / Then
        assert_eq!(error.code(), DiagnosticCode::UmlUnknownEntity);
    }

    #[test]
    fn test_the_message_names_the_directive_the_author_wrote() {
        // Given — an author who wrote `needuml` should not be told about
        // `entity-diagram`
        let error = error_for(UmlError::ArchOutsideEntity);

        // When
        let message = error.message();

        // Then
        assert!(message.starts_with("needuml:"), "{message}");
    }

    #[test]
    fn test_the_message_carries_the_failures_own_words() {
        // Given
        let error = error_for(UmlError::UnknownEntity("REQ_404".to_string()));

        // When
        let message = error.message();

        // Then
        assert!(message.contains("REQ_404"), "{message}");
    }

    /// The same, for a flowchart that could not be drawn.
    fn flow_error_for(error: FlowError) -> DiagramError {
        DiagramError {
            directive: "needflow".to_string(),
            error: error.into(),
            span: None,
        }
    }

    #[test]
    fn test_a_flowchart_reports_under_its_own_family_not_the_diagrams() {
        // Given — a code names the construct the author wrote
        let error = flow_error_for(FlowError::EmptyResult);

        // When / Then
        assert_eq!(error.code(), DiagnosticCode::EntityFlowEmptyResult);
    }

    #[test]
    fn test_a_flowcharts_message_names_the_directive_the_author_wrote() {
        // Given
        let error = flow_error_for(FlowError::EmptyResult);

        // When
        let message = error.message();

        // Then
        assert!(message.starts_with("needflow:"), "{message}");
    }

    /// The same, for a sequence diagram's walk.
    fn sequence_error_for(error: SequenceError) -> DiagramError {
        DiagramError {
            directive: "needsequence".to_string(),
            error: error.into(),
            span: None,
        }
    }

    #[test]
    fn test_a_sequence_diagram_reports_under_its_own_family() {
        // Given — a code names the construct the author wrote
        let error = sequence_error_for(SequenceError::EmptyResult);

        // When / Then
        assert_eq!(error.code(), DiagnosticCode::EntitySequenceEmptyResult);
        assert!(error.message().starts_with("needsequence:"));
    }
}

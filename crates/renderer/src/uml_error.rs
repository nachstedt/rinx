//! What a diagram reports while rendering.

use std::fmt;

use rusty_sphinx_ast::{DiagnosticCode, Span};
use rusty_sphinx_uml::{FlowError, UmlError};

/// A diagram directive that produced no picture.
///
/// Only reportable here, like [`crate::EmptyListingError`], and for the same
/// reason: a diagram asks the entity graph questions — whether written as a
/// template or generated from a filter — and whether they can be answered
/// depends on every document in the project. The parser, which sees one, cannot
/// know.
///
/// The failure itself is [`DiagramFailure`], produced by `rusty_sphinx_uml`.
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
/// Two kinds, because there are two ways a diagram comes about: a template
/// that was expanded, and a flowchart that was generated. They are kept apart
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
}

impl DiagramFailure {
    /// The diagnostic code this reports under.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Uml(error) => error.code(),
            Self::Flow(error) => error.code(),
        }
    }

    /// Whether this failure depends on knowing the whole project.
    #[must_use]
    pub const fn depends_on_the_project(&self) -> bool {
        match self {
            Self::Uml(error) => uml_depends_on_the_project(error),
            // A flowchart's two failures split the same way a diagram's do: an
            // empty result may be nothing but a stale preview index, while a
            // `:config:` naming no preamble is a fault in the site config,
            // which a preview reads for real.
            Self::Flow(FlowError::EmptyResult) => true,
            Self::Flow(FlowError::UnknownConfig(_)) => false,
        }
    }
}

impl fmt::Display for DiagramFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uml(error) => error.fmt(f),
            Self::Flow(error) => error.fmt(f),
        }
    }
}

impl From<UmlError> for DiagramFailure {
    fn from(error: UmlError) -> Self {
        Self::Uml(error)
    }
}

impl From<FlowError> for DiagramFailure {
    fn from(error: FlowError) -> Self {
        Self::Flow(error)
    }
}

/// Whether a template failure depends on knowing the whole project.
///
/// A live preview merges a *stale* global index, or none at all, so a template
/// asking about an entity may fail there for a reason that does not exist in a
/// real build. A syntax error in the template does not: it is the author's
/// either way, and is worth seeing in an editor long before a build runs.
const fn uml_depends_on_the_project(error: &UmlError) -> bool {
    match error {
        // An unknown `:config:` is a fault in the *site* config, which a
        // preview reads for real — so it is worth reporting there too.
        UmlError::Template { .. } | UmlError::ArchOutsideEntity | UmlError::UnknownConfig(_) => {
            false
        }
        UmlError::UnknownEntity(_)
        | UmlError::InvalidFilter { .. }
        | UmlError::RecursiveImport { .. }
        // An empty result usually means a filter matched nothing, which in a
        // preview may only be because the merged index is stale.
        | UmlError::EmptyDiagram => true,
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

    /// Whether this failure depends on knowing the whole project.
    ///
    /// Delegated for the reason the code is: the answer belongs to the failure,
    /// not to the phase reporting it.
    #[must_use]
    pub const fn depends_on_the_project(&self) -> bool {
        self.error.depends_on_the_project()
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
    fn test_a_syntax_error_does_not_depend_on_the_project() {
        // Given — worth reporting in a live preview, which has no real index
        let error = error_for(UmlError::Template {
            message: "unexpected end of input".to_string(),
            line: Some(1),
        });

        // When / Then
        assert!(!error.depends_on_the_project());
    }

    #[test]
    fn test_an_unknown_entity_depends_on_the_project() {
        // Given — in a preview this may fail only because the index is stale
        let error = error_for(UmlError::UnknownEntity("REQ_404".to_string()));

        // When / Then
        assert!(error.depends_on_the_project());
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
    fn test_a_flowchart_that_drew_nothing_depends_on_the_project() {
        // Given — in a preview it may be empty only because the index is stale
        let empty = flow_error_for(FlowError::EmptyResult);
        let config = flow_error_for(FlowError::UnknownConfig("mono".to_string()));

        // When / Then
        assert!(empty.depends_on_the_project());
        assert!(!config.depends_on_the_project());
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
}

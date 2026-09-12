//! What a diagram reports while rendering.

use rusty_sphinx_ast::{DiagnosticCode, Span};
use rusty_sphinx_uml::UmlError;

/// A diagram whose template could not be expanded.
///
/// Only reportable here, like [`crate::EntityTableError`], and for the same
/// reason: a template asks the entity graph questions, and whether it can
/// answer them depends on every document in the project. The parser, which
/// sees one, cannot know.
///
/// The failure itself is [`UmlError`], produced by `rusty_sphinx_uml`'s
/// expander. What this type adds
/// is the two things only the renderer holds — where the directive was
/// written, and what the author spelled it — so the warning can point at a
/// line rather than at a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramError {
    /// The directive as the author spelled it — `needuml`, `entity-arch`, …
    pub directive: String,
    /// Why the expansion failed.
    pub error: UmlError,
    /// Where the directive was written, when the node carried a position.
    pub span: Option<Span>,
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
    /// A live preview merges a *stale* global index, or none at all, so a
    /// template asking about an entity may fail there for a reason that does
    /// not exist in a real build. A syntax error in the template does not: it
    /// is the author's either way, and is worth seeing in an editor long
    /// before a build runs.
    #[must_use]
    pub const fn depends_on_the_project(&self) -> bool {
        match self.error {
            // An unknown `:config:` is a fault in the *site* config, which a
            // preview reads for real — so it is worth reporting there too.
            UmlError::Template { .. }
            | UmlError::ArchOutsideEntity
            | UmlError::UnknownConfig(_) => false,
            UmlError::UnknownEntity(_)
            | UmlError::InvalidFilter { .. }
            | UmlError::RecursiveImport { .. }
            // An empty result usually means a filter matched nothing, which in
            // a preview may only be because the merged index is stale.
            | UmlError::EmptyDiagram => true,
        }
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
            error,
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
}

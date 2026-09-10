//! What a listing directive reports while rendering.

use rusty_sphinx_ast::{DiagnosticCode, Span};

/// A listing directive whose filter matched nothing.
///
/// Only reportable here: whether a filter selects anything depends on every
/// document in the project, so the parser — which sees one — cannot know. Not
/// a [`crate::BrokenLink`], since nothing failed to resolve; the question was
/// well-formed and the answer was empty.
///
/// Reported at all for the reason `.. literalinclude::` reports every way of
/// selecting nothing: an empty table is far more often a filter that no longer
/// matches than a deliberate statement that there is nothing to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTableError {
    /// The directive as the author spelled it — `entity-table` or `needtable`.
    pub directive: String,
    /// Where it was written, when the node carried a position.
    pub span: Option<Span>,
}

impl EntityTableError {
    /// The diagnostic code this reports under — what a `.. noqa:` names to
    /// suppress it.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        DiagnosticCode::EntityTableEmptyResult
    }

    /// The author-facing message.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "{}: the filter matched no entity, so the table is empty",
            self.directive
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_error_reports_under_the_empty_result_code() {
        // Given
        let error = EntityTableError {
            directive: "needtable".to_string(),
            span: None,
        };

        // When
        let code = error.code();

        // Then
        assert_eq!(code, DiagnosticCode::EntityTableEmptyResult);
    }

    #[test]
    fn test_the_message_names_the_spelling_the_author_wrote() {
        // Given — a `.. needtable::` author should not be told about a
        // directive they did not write
        let error = EntityTableError {
            directive: "needtable".to_string(),
            span: None,
        };

        // When
        let message = error.message();

        // Then
        assert!(message.starts_with("needtable:"));
        assert!(message.contains("no entity"));
    }
}

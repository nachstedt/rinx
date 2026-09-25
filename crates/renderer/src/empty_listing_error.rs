//! What a listing directive reports when its question had no answer.

use rusty_sphinx_ast::{DiagnosticCode, Span};

/// A directive whose filter selected nothing at all.
///
/// Only reportable while rendering: whether a filter selects anything depends
/// on every document in the project, so the parser — which sees one — cannot
/// know. Not a [`crate::BrokenLink`], since nothing failed to resolve; the
/// question was well-formed and the answer was empty.
///
/// Reported at all for the reason `.. literalinclude::` reports every way of
/// selecting nothing: an empty listing is far more often a filter that no
/// longer matches than a deliberate statement that there is nothing to show.
///
/// **One type for every such directive, carrying its own code.** A code names
/// the construct, so an `.. entity-table::` reports
/// `entity-table.empty-result` and an `.. entity-pie::` reports
/// `entity-pie.empty-result` — but what happens to the report afterwards is
/// identical, and that is the part worth sharing. In particular the live
/// preview suppresses the whole vector when it has no project index, since
/// there every listing would be empty through no fault of the author; a
/// second, parallel vector would have to remember to do the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyListingError {
    /// The directive as the author spelled it — `entity-table` or `needtable`,
    /// `entity-pie` or `needpie`, `entity-bar` or `needbar`.
    pub directive: String,
    /// The code this reports under, which names the construct.
    pub code: DiagnosticCode,
    /// What was empty, completing "…, so the " in the message.
    pub subject: &'static str,
    /// Where it was written, when the node carried a position.
    pub span: Option<Span>,
}

impl EmptyListingError {
    /// An empty `.. entity-table::` / `.. needtable::`.
    #[must_use]
    pub fn table(directive: &str, span: Option<Span>) -> Self {
        Self {
            directive: directive.to_string(),
            code: DiagnosticCode::EntityTableEmptyResult,
            subject: "table is empty",
            span,
        }
    }

    /// An `.. entity-pie::` / `.. needpie::` whose every wedge counted zero.
    #[must_use]
    pub fn pie(directive: &str, span: Option<Span>) -> Self {
        Self {
            directive: directive.to_string(),
            code: DiagnosticCode::EntityPieEmptyResult,
            subject: "chart has nothing to draw",
            span,
        }
    }

    /// An `.. entity-bar::` / `.. needbar::` whose every cell counted zero.
    #[must_use]
    pub fn bar(directive: &str, span: Option<Span>) -> Self {
        Self {
            directive: directive.to_string(),
            code: DiagnosticCode::EntityBarEmptyResult,
            subject: "chart has nothing to draw",
            span,
        }
    }

    /// The diagnostic code this reports under — what a `.. noqa:` names to
    /// suppress it.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// The author-facing message.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "{}: the filter matched no entity, so the {}",
            self.directive, self.subject
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_an_empty_table_reports_under_the_tables_own_code() {
        // Given
        let error = EmptyListingError::table("needtable", None);

        // When
        let code = error.code();

        // Then
        assert_eq!(code, DiagnosticCode::EntityTableEmptyResult);
    }

    #[test]
    fn test_an_empty_chart_reports_under_the_charts_own_code() {
        // Given — a code names the construct, not the shared plumbing
        let error = EmptyListingError::pie("needpie", None);

        // When
        let code = error.code();

        // Then
        assert_eq!(code, DiagnosticCode::EntityPieEmptyResult);
    }

    #[test]
    fn test_an_empty_bar_chart_reports_under_its_own_code() {
        // Given
        let error = EmptyListingError::bar("needbar", None);

        // When
        let code = error.code();

        // Then
        assert_eq!(code, DiagnosticCode::EntityBarEmptyResult);
        assert!(error.message().starts_with("needbar:"));
    }

    #[test]
    fn test_the_message_names_the_spelling_the_author_wrote() {
        // Given — a `.. needtable::` author should not be told about a
        // directive they did not write
        let error = EmptyListingError::table("needtable", None);

        // When
        let message = error.message();

        // Then
        assert!(message.starts_with("needtable:"));
        assert!(message.contains("no entity"));
    }

    #[test]
    fn test_each_construct_describes_what_was_empty_in_its_own_terms() {
        // Given
        let table = EmptyListingError::table("entity-table", None);
        let pie = EmptyListingError::pie("entity-pie", None);

        // When
        let (table_message, pie_message) = (table.message(), pie.message());

        // Then
        assert!(
            table_message.ends_with("the table is empty"),
            "{table_message}"
        );
        assert!(
            pie_message.ends_with("the chart has nothing to draw"),
            "{pie_message}"
        );
    }
}

//! What a parse records on the side: the problems it found, and the
//! `.. noqa:` comments excusing some of them.

use rusty_sphinx_ast::{Diagnostic, Suppression};

/// The collector every block-level parser is handed.
///
/// Diagnostics and suppressions travel together because they are produced by
/// the same pass and are meaningless apart — a suppression is only ever read
/// to decide whether a diagnostic is shown. Bundling them is also what keeps
/// the parser's signatures unchanged: [`Self::push`] is deliberately named
/// and shaped like `Vec::push`, so the several dozen call sites that report a
/// diagnostic read exactly as they did when this was a plain `Vec`.
#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    entries: Vec<Diagnostic>,
    suppressions: Vec<Suppression>,
}

impl Diagnostics {
    /// Records a problem found while parsing.
    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        self.entries.push(diagnostic);
    }

    /// Records a `.. noqa:` comment's resolved range.
    pub(crate) fn suppress(&mut self, suppression: Suppression) {
        self.suppressions.push(suppression);
    }

    /// Everything recorded, for the caller that builds the `Document`.
    pub(crate) fn into_parts(self) -> (Vec<Diagnostic>, Vec<Suppression>) {
        (self.entries, self.suppressions)
    }

    /// The diagnostics recorded so far.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[Diagnostic] {
        &self.entries
    }

    /// Iterates the diagnostics recorded so far.
    #[cfg(test)]
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.entries.iter()
    }

    /// The suppressions recorded so far.
    #[cfg(test)]
    pub(crate) fn suppressions(&self) -> &[Suppression] {
        &self.suppressions
    }

    /// Whether nothing at all was recorded.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.suppressions.is_empty()
    }

    /// How many diagnostics were recorded.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
impl std::ops::Index<usize> for Diagnostics {
    type Output = Diagnostic;

    fn index(&self, index: usize) -> &Diagnostic {
        &self.entries[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{DiagnosticCode, SuppressionCodes};

    #[test]
    fn test_a_new_collector_is_empty() {
        // Given / When
        let diagnostics = Diagnostics::default();

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_push_records_a_diagnostic() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        diagnostics.push(Diagnostic::without_span(
            DiagnosticCode::CsvNoData,
            "no data",
        ));

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::CsvNoData);
    }

    #[test]
    fn test_suppress_records_a_suppression_without_touching_diagnostics() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        diagnostics.suppress(Suppression {
            start_line: 1,
            end_line: 2,
            codes: SuppressionCodes::All,
        });

        // Then — the two collections are independent
        assert_eq!(diagnostics.suppressions().len(), 1);
        assert_eq!(diagnostics.len(), 0);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_into_parts_yields_both_collections() {
        // Given one of each
        let mut diagnostics = Diagnostics::default();
        diagnostics.push(Diagnostic::without_span(
            DiagnosticCode::CsvNoData,
            "no data",
        ));
        diagnostics.suppress(Suppression {
            start_line: 1,
            end_line: 2,
            codes: SuppressionCodes::All,
        });

        // When
        let (entries, suppressions) = diagnostics.into_parts();

        // Then
        assert_eq!(entries.len(), 1);
        assert_eq!(suppressions.len(), 1);
    }

    #[test]
    fn test_entries_returns_them_in_the_order_they_were_pushed() {
        // Given
        let mut diagnostics = Diagnostics::default();
        diagnostics.push(Diagnostic::without_span(DiagnosticCode::CsvNoData, "first"));
        diagnostics.push(Diagnostic::without_span(
            DiagnosticCode::CsvMalformedData,
            "second",
        ));

        // When / Then — document order is what a reader expects of a report
        assert_eq!(diagnostics.entries()[0].message, "first");
        assert_eq!(diagnostics.entries()[1].message, "second");
    }
}

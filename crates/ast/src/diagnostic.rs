use serde::{Deserialize, Serialize};

use crate::diagnostic_code::DiagnosticCode;
use crate::diagnostic_subject::DiagnosticSubject;
use crate::span::Span;

/// One problem found in a document, recorded rather than raised.
///
/// The parser never fails on bad input — it degrades the construct and pushes
/// a `Diagnostic`, leaving the build step to decide whether that is fatal (see
/// `guidelines.md`'s diagnostics section). Diagnostics therefore accumulate in
/// a plain `Vec` threaded through the parse, and ride on
/// [`Document::diagnostics`](crate::Document) into the `.ast` file.
///
/// The three fields answer three different questions, and no consumer needs
/// all three: `code` is what a `.. noqa:` matches and what the benchmark
/// aggregates by, `span` is what an editor underlines, and `message` is the
/// prose a human reads. Keeping the code out of the message is what makes the
/// first two possible at all — before this type they were one formatted
/// string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    /// A human-readable explanation, written for the document's author. Does
    /// *not* repeat the code or the position — the reporting layer prints
    /// those around it.
    pub message: String,
    /// Where in the source this is about, when that is known.
    ///
    /// `None` for content with no source position at all: the rows a
    /// `.. csv-table::` generates from CSV data exist only after the parse, so
    /// nothing about them can point back into the `.rst`. Such a diagnostic
    /// can never be suppressed by a `.. noqa:`, having no line to match
    /// against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// What this is about, for the findings a front end treats by subject —
    /// see [`DiagnosticSubject`]. `None` for every other finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<DiagnosticSubject>,
}

impl Diagnostic {
    /// A diagnostic at a known place in the source.
    #[must_use]
    pub fn new(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span: Some(span),
            subject: None,
        }
    }

    /// A diagnostic about content that has no source position — see
    /// [`Self::span`].
    #[must_use]
    pub fn without_span(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            span: None,
            subject: None,
        }
    }

    /// A diagnostic at `span` when one is known, and an unpositioned one
    /// otherwise — the shape every parser call site has, since a nested parse
    /// of synthetic content yields `None` from
    /// [`ParseCtx`](../../rinx_parser/index.html)'s span helpers.
    #[must_use]
    pub fn at(code: DiagnosticCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            subject: None,
        }
    }

    /// This diagnostic, recorded as being about `subject`.
    #[must_use]
    pub fn about(self, subject: DiagnosticSubject) -> Self {
        Self {
            subject: Some(subject),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Position;

    fn a_span() -> Span {
        Span::new(Position::new(3, 1), Position::new(3, 12))
    }

    #[test]
    fn test_new_records_code_message_and_span() {
        // Given / When
        let diagnostic = Diagnostic::new(DiagnosticCode::CsvNoData, "no table data", a_span());

        // Then
        assert_eq!(diagnostic.code, DiagnosticCode::CsvNoData);
        assert_eq!(diagnostic.message, "no table data");
        assert_eq!(diagnostic.span, Some(a_span()));
    }

    #[test]
    fn test_without_span_records_no_position() {
        // Given / When
        let diagnostic = Diagnostic::without_span(DiagnosticCode::CsvMalformedData, "bad row");

        // Then
        assert_eq!(diagnostic.span, None);
    }

    #[test]
    fn test_at_keeps_a_present_span() {
        // Given / When
        let diagnostic = Diagnostic::at(DiagnosticCode::CsvNoData, "no data", Some(a_span()));

        // Then
        assert_eq!(diagnostic.span, Some(a_span()));
    }

    #[test]
    fn test_at_accepts_an_absent_span() {
        // Given — a nested parse of synthetic content yields no position
        let diagnostic = Diagnostic::at(DiagnosticCode::CsvNoData, "no data", None);

        // When / Then
        assert_eq!(diagnostic.span, None);
    }

    #[test]
    fn test_serialization_omits_an_absent_span() {
        // Given
        let diagnostic = Diagnostic::without_span(DiagnosticCode::CsvNoData, "no data");

        // When
        let json = serde_json::to_string(&diagnostic).expect("Failed to serialize");

        // Then — the key is absent rather than null, so the wire form stays
        // readable for the benchmark's aggregation.
        assert!(!json.contains("span"), "{json}");
    }

    #[test]
    fn test_serialization_roundtrip_with_a_span() {
        // Given
        let diagnostic = Diagnostic::new(DiagnosticCode::LinkBrokenRef, "broken ref", a_span());

        // When
        let json = serde_json::to_string(&diagnostic).expect("Failed to serialize");
        let deserialized: Diagnostic = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(diagnostic, deserialized);
    }

    #[test]
    fn test_about_records_the_subject() {
        // Given
        let diagnostic = Diagnostic::new(DiagnosticCode::DirectiveUnknown, "unknown", a_span());

        // When
        let diagnostic = diagnostic.about(DiagnosticSubject::Directive("automodule".to_string()));

        // Then
        assert_eq!(
            diagnostic.subject,
            Some(DiagnosticSubject::Directive("automodule".to_string()))
        );
    }

    #[test]
    fn test_serialization_omits_an_absent_subject() {
        // Given
        let diagnostic = Diagnostic::new(DiagnosticCode::LinkBrokenRef, "broken ref", a_span());

        // When
        let json = serde_json::to_string(&diagnostic).expect("Failed to serialize");

        // Then — an `.ast` holding no subject is byte-for-byte what it was
        assert!(!json.contains("subject"), "{json}");
    }

    #[test]
    fn test_serialization_roundtrip_with_a_subject() {
        // Given
        let diagnostic = Diagnostic::new(DiagnosticCode::DirectiveUnknown, "unknown", a_span())
            .about(DiagnosticSubject::Directive("automodule".to_string()));

        // When
        let json = serde_json::to_string(&diagnostic).expect("Failed to serialize");
        let deserialized: Diagnostic = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(diagnostic, deserialized);
    }

    #[test]
    fn test_deserialization_defaults_a_missing_span_to_none() {
        // Given JSON written without the optional field
        let json = r#"{"code":"csv.no-data","message":"no data"}"#;

        // When
        let diagnostic: Diagnostic = serde_json::from_str(json).expect("Failed to deserialize");

        // Then
        assert_eq!(diagnostic.span, None);
    }
}

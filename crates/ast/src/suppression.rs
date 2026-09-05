use serde::{Deserialize, Serialize};

use crate::diagnostic_code::DiagnosticCode;
use crate::span::{FileId, Span};

/// Which diagnostics a `.. noqa:` comment silences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuppressionCodes {
    /// A bare `.. noqa` — every code, for the block it precedes.
    ///
    /// Supported because it is occasionally what an author means, but the
    /// listed form is the one to reach for: a blanket suppression also hides
    /// the *next* problem to appear in that block, which nobody chose.
    All,
    /// `.. noqa: a, b` — only these codes.
    Only(Vec<DiagnosticCode>),
}

impl SuppressionCodes {
    /// Whether `code` is one of the codes this silences.
    #[must_use]
    pub fn covers(&self, code: DiagnosticCode) -> bool {
        match self {
            Self::All => true,
            Self::Only(codes) => codes.contains(&code),
        }
    }
}

/// One `.. noqa:` comment and the block it applies to.
///
/// The range is the *following* block's extent, not the comment's own: a
/// suppression is written above what it silences, and what the author means by
/// it is "not for this block". Resolving that to a line range at parse time is
/// what lets the range be checked later, in a different process, against a
/// diagnostic the parser never saw — every render-time broken link included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suppression {
    /// First line of the suppressed block, 1-based and inclusive.
    pub start_line: u32,
    /// Last line of the suppressed block, 1-based and inclusive.
    pub end_line: u32,
    pub codes: SuppressionCodes,
    /// The file whose lines [`Self::start_line`] and [`Self::end_line`] count,
    /// when that is not the document itself — i.e. a `.. noqa:` written inside
    /// a file spliced in by `.. include::`.
    ///
    /// Mirrors [`Span::file`](crate::Span::file), and must, because the two
    /// are compared: without it a `.. noqa:` on line 3 of a document would
    /// silence a diagnostic from line 3 of a fragment it includes, which is a
    /// different place entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<FileId>,
}

impl Suppression {
    /// Whether this silences `code` reported at `span`.
    ///
    /// Matching is on where the diagnostic *starts*: a construct spanning
    /// several lines belongs to the block it opens in, so a `.. noqa:` before
    /// that block covers it, and one before a block it merely spills into does
    /// not.
    ///
    /// The span's file must equal this suppression's — `None`, for the
    /// document being parsed, on both sides. A comment therefore only silences
    /// diagnostics from the file it was written in: line numbers from two
    /// different files are not comparable, so matching them would silence
    /// something the author never looked at.
    ///
    /// A diagnostic with no span is never suppressed — there is nothing to
    /// match against, and silencing it on the strength of its code alone would
    /// reach across the whole document.
    #[must_use]
    pub fn suppresses(&self, code: DiagnosticCode, span: Option<Span>) -> bool {
        let Some(span) = span else {
            return false;
        };
        self.file == span.file
            && self.codes.covers(code)
            && span.start.line >= self.start_line
            && span.start.line <= self.end_line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Position;

    fn span_on(line: u32) -> Span {
        Span::new(Position::new(line, 1), Position::new(line, 10))
    }

    fn only_broken_ref() -> Suppression {
        Suppression {
            start_line: 10,
            end_line: 12,
            codes: SuppressionCodes::Only(vec![DiagnosticCode::LinkBrokenRef]),
            file: None,
        }
    }

    /// The same suppression, written inside an included fragment instead.
    fn only_broken_ref_in(file: FileId) -> Suppression {
        Suppression {
            file: Some(file),
            ..only_broken_ref()
        }
    }

    /// A one-line span in an included fragment rather than in the document.
    fn span_on_in(line: u32, file: FileId) -> Span {
        span_on(line).with_file(Some(file))
    }

    #[test]
    fn test_all_covers_every_code() {
        // Given / When / Then
        let all = SuppressionCodes::All;
        assert!(all.covers(DiagnosticCode::LinkBrokenRef));
        assert!(all.covers(DiagnosticCode::CsvNoData));
    }

    #[test]
    fn test_only_covers_just_the_listed_codes() {
        // Given
        let codes = SuppressionCodes::Only(vec![DiagnosticCode::LinkBrokenRef]);

        // When / Then
        assert!(codes.covers(DiagnosticCode::LinkBrokenRef));
        assert!(!codes.covers(DiagnosticCode::LinkBrokenTerm));
    }

    #[test]
    fn test_suppresses_a_matching_code_inside_the_range() {
        // Given / When / Then
        assert!(only_broken_ref().suppresses(DiagnosticCode::LinkBrokenRef, Some(span_on(11))));
    }

    #[test]
    fn test_suppresses_on_the_range_boundaries() {
        // Given a range of lines 10 through 12
        let suppression = only_broken_ref();

        // When / Then — both ends are inclusive
        assert!(suppression.suppresses(DiagnosticCode::LinkBrokenRef, Some(span_on(10))));
        assert!(suppression.suppresses(DiagnosticCode::LinkBrokenRef, Some(span_on(12))));
    }

    #[test]
    fn test_does_not_suppress_outside_the_range() {
        // Given
        let suppression = only_broken_ref();

        // When / Then
        assert!(!suppression.suppresses(DiagnosticCode::LinkBrokenRef, Some(span_on(9))));
        assert!(!suppression.suppresses(DiagnosticCode::LinkBrokenRef, Some(span_on(13))));
    }

    #[test]
    fn test_does_not_suppress_an_unlisted_code() {
        // Given / When / Then
        assert!(!only_broken_ref().suppresses(DiagnosticCode::LinkBrokenTerm, Some(span_on(11))));
    }

    #[test]
    fn test_never_suppresses_a_diagnostic_without_a_span() {
        // Given a blanket suppression, which covers every code
        let suppression = Suppression {
            start_line: 1,
            end_line: 1000,
            codes: SuppressionCodes::All,
            file: None,
        };

        // When / Then — with nothing to match against, it still does not fire
        assert!(!suppression.suppresses(DiagnosticCode::CsvNoData, None));
    }

    #[test]
    fn test_matches_on_the_start_of_a_multi_line_span() {
        // Given a diagnostic that begins inside the range and ends past it
        let span = Some(Span::new(Position::new(12, 1), Position::new(40, 1)));

        // When / Then — it is attributed to where it begins
        assert!(only_broken_ref().suppresses(DiagnosticCode::LinkBrokenRef, span));
    }

    #[test]
    fn test_a_documents_noqa_does_not_reach_into_an_included_file() {
        // Given a `.. noqa:` written in the document itself
        let suppression = only_broken_ref();

        // When the same line number comes up in an included fragment
        let matches = suppression.suppresses(
            DiagnosticCode::LinkBrokenRef,
            Some(span_on_in(11, FileId::new(0))),
        );

        // Then — line 11 of the fragment is not line 11 of the document
        assert!(!matches);
    }

    #[test]
    fn test_an_included_files_noqa_does_not_reach_the_document() {
        // Given a `.. noqa:` written inside an included fragment
        let suppression = only_broken_ref_in(FileId::new(0));

        // When the same line number comes up in the including document
        let matches = suppression.suppresses(DiagnosticCode::LinkBrokenRef, Some(span_on(11)));

        // Then — the fragment's author cannot silence its includer
        assert!(!matches);
    }

    #[test]
    fn test_an_included_files_noqa_silences_its_own_file() {
        // Given
        let suppression = only_broken_ref_in(FileId::new(0));

        // When
        let matches = suppression.suppresses(
            DiagnosticCode::LinkBrokenRef,
            Some(span_on_in(11, FileId::new(0))),
        );

        // Then
        assert!(matches);
    }

    #[test]
    fn test_a_noqa_does_not_reach_a_different_included_file() {
        // Given two fragments included by the same document
        let suppression = only_broken_ref_in(FileId::new(0));

        // When
        let matches = suppression.suppresses(
            DiagnosticCode::LinkBrokenRef,
            Some(span_on_in(11, FileId::new(1))),
        );

        // Then
        assert!(!matches);
    }

    #[test]
    fn test_serialization_omits_an_absent_file() {
        // Given a suppression written in the document itself
        let suppression = only_broken_ref();

        // When
        let json = serde_json::to_string(&suppression).expect("Failed to serialize");

        // Then — the common case must not grow the `.ast` wire form
        assert!(!json.contains("file"), "{json}");
    }

    #[test]
    fn test_deserialization_defaults_a_missing_file_to_none() {
        // Given JSON written before this field existed
        let json = r#"{"start_line":10,"end_line":12,"codes":{"Only":["link.broken-ref"]}}"#;

        // When
        let suppression: Suppression = serde_json::from_str(json).expect("Failed to deserialize");

        // Then — an `.ast` from an older build still loads
        assert_eq!(suppression.file, None);
    }

    #[test]
    fn test_serialization_roundtrip_with_a_file() {
        // Given
        let suppression = only_broken_ref_in(FileId::new(3));

        // When
        let json = serde_json::to_string(&suppression).expect("Failed to serialize");
        let deserialized: Suppression = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(suppression, deserialized);
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Given
        let suppression = only_broken_ref();

        // When
        let json = serde_json::to_string(&suppression).expect("Failed to serialize");
        let deserialized: Suppression = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(suppression, deserialized);
    }
}

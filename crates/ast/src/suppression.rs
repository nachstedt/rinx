//! `.. noqa:` comments, and applying them to what a phase reported.
//!
//! Filtering happens at the reporting boundary, never where a diagnostic is
//! raised: a phase's job is to *find* problems and record them faithfully, and
//! a front end's job is to decide which of them a human should see. It is also
//! the only place that works — a broken link is found while rendering, in a
//! different process from the parse that read the comment excusing it, and
//! only the `.ast` in between carries both.
//!
//! The filter lives here rather than in either front end because there are
//! two — the build's worker and the language server — and they must agree on
//! what a comment silences, or the editor and CI would disagree.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::diagnostic::Diagnostic;
use crate::diagnostic_code::DiagnosticCode;
use crate::diagnostic_subject::DiagnosticSubject;
use crate::span::{FileId, Span};

/// Something a phase reported that a `.. noqa:` may silence: a parse
/// [`Diagnostic`], or one of the renderer's typed findings.
///
/// A trait rather than a conversion to [`Diagnostic`] because the renderer's
/// findings carry more than a message — the target of a broken link, which
/// `--strict-links` then fails over — and the survivors must keep it.
///
/// It also answers the message, so every front end words a finding the same
/// way: the build's warning line and the language server's diagnostic are
/// both written from it.
pub trait Reported {
    /// The code a `.. noqa:` names to silence this.
    fn code(&self) -> DiagnosticCode;
    /// Where this was found, when that is known.
    fn span(&self) -> Option<Span>;
    /// What was found, for the document's author — without the code or the
    /// position, which the reporting layer writes around it.
    fn message(&self) -> Cow<'_, str>;
    /// What this is about, when a front end may treat it by subject — see
    /// [`DiagnosticSubject`]. Most findings have none.
    fn subject(&self) -> Option<DiagnosticSubject> {
        None
    }

    /// This finding as a plain [`Diagnostic`], for a front end that needs
    /// nothing beyond what any diagnostic carries.
    fn to_diagnostic(&self) -> Diagnostic {
        let diagnostic = Diagnostic::at(self.code(), self.message(), self.span());
        match self.subject() {
            Some(subject) => diagnostic.about(subject),
            None => diagnostic,
        }
    }
}

impl Reported for Diagnostic {
    fn code(&self) -> DiagnosticCode {
        self.code
    }

    fn span(&self) -> Option<Span> {
        self.span
    }

    fn message(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.message)
    }

    fn subject(&self) -> Option<DiagnosticSubject> {
        self.subject.clone()
    }
}

/// Whether any of `suppressions` silences `code` reported at `span`.
#[must_use]
pub fn is_suppressed(
    suppressions: &[Suppression],
    code: DiagnosticCode,
    span: Option<Span>,
) -> bool {
    suppressions
        .iter()
        .any(|suppression| suppression.suppresses(code, span))
}

/// The items no comment in `suppressions` silences.
///
/// A survivor is everything a suppressed item must not be: shown, written to
/// the warning sidecar, and — for a broken link — failed over by
/// `--strict-links`. Filter once, here, and pass the survivors to all three,
/// or the comment would only hide the message while keeping the consequence.
pub fn retain_reportable<'a, T: Reported>(
    items: &'a [T],
    suppressions: &'a [Suppression],
) -> impl Iterator<Item = &'a T> {
    items
        .iter()
        .filter(|item| !is_suppressed(suppressions, item.code(), item.span()))
}

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
    fn test_diagnostic_reports_its_code_span_and_message() {
        // Given
        let diagnostic = Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span_on(4));

        // When / Then
        assert_eq!(Reported::code(&diagnostic), DiagnosticCode::CsvNoData);
        assert_eq!(Reported::span(&diagnostic), Some(span_on(4)));
        assert_eq!(Reported::message(&diagnostic), "no data");
    }

    #[test]
    fn test_to_diagnostic_keeps_code_message_and_span() {
        // Given
        let diagnostic = Diagnostic::new(DiagnosticCode::CsvNoData, "no data", span_on(4));

        // When / Then
        assert_eq!(diagnostic.to_diagnostic(), diagnostic);
    }

    #[test]
    fn test_to_diagnostic_keeps_the_subject() {
        // Given
        let diagnostic = Diagnostic::new(DiagnosticCode::DirectiveUnknown, "unknown", span_on(4))
            .about(DiagnosticSubject::Directive("todo".to_string()));

        // When / Then
        assert_eq!(diagnostic.to_diagnostic(), diagnostic);
    }

    #[test]
    fn test_is_suppressed_finds_a_matching_suppression() {
        // Given / When / Then
        assert!(is_suppressed(
            &[only_broken_ref()],
            DiagnosticCode::LinkBrokenRef,
            Some(span_on(11))
        ));
    }

    #[test]
    fn test_is_suppressed_is_false_when_nothing_matches() {
        // Given a suppression for another block
        // When / Then
        assert!(!is_suppressed(
            &[only_broken_ref()],
            DiagnosticCode::LinkBrokenRef,
            Some(span_on(20))
        ));
    }

    #[test]
    fn test_is_suppressed_is_false_with_no_suppressions() {
        // Given / When / Then
        assert!(!is_suppressed(
            &[],
            DiagnosticCode::LinkBrokenRef,
            Some(span_on(11))
        ));
    }

    #[test]
    fn test_retain_reportable_drops_only_the_suppressed_item() {
        // Given two diagnostics, one inside a suppressed block
        let diagnostics = [
            Diagnostic::new(DiagnosticCode::LinkBrokenRef, "inside", span_on(11)),
            Diagnostic::new(DiagnosticCode::LinkBrokenRef, "outside", span_on(20)),
        ];
        let suppressions = [only_broken_ref()];

        // When
        let reportable: Vec<_> = retain_reportable(&diagnostics, &suppressions).collect();

        // Then
        assert_eq!(reportable.len(), 1);
        assert_eq!(reportable[0].message, "outside");
    }

    #[test]
    fn test_retain_reportable_keeps_an_item_of_another_code() {
        // Given a diagnostic in the suppressed block, but of a code the
        // comment did not name
        let diagnostics = [Diagnostic::new(
            DiagnosticCode::LinkBrokenTerm,
            "term",
            span_on(11),
        )];
        let suppressions = [only_broken_ref()];

        // When / Then
        assert_eq!(retain_reportable(&diagnostics, &suppressions).count(), 1);
    }

    #[test]
    fn test_retain_reportable_keeps_an_item_with_no_span() {
        // Given a blanket suppression and a positionless diagnostic
        let diagnostics = [Diagnostic::without_span(
            DiagnosticCode::CsvNoData,
            "no data",
        )];
        let suppressions = [Suppression {
            start_line: 1,
            end_line: 1000,
            codes: SuppressionCodes::All,
            file: None,
        }];

        // When / Then — nothing to match on, so it is still reported
        assert_eq!(retain_reportable(&diagnostics, &suppressions).count(), 1);
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

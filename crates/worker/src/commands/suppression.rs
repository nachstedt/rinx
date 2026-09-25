//! Applying a document's `.. noqa:` comments to the warnings it would
//! otherwise produce.
//!
//! Filtering happens here, at the reporting boundary, rather than where each
//! diagnostic is raised. That is the same separation the parser already keeps:
//! a phase's job is to *find* problems and record them faithfully, and the
//! build step's job is to decide which of them a human should see. It is also
//! the only place that works — a broken link is found while rendering, in a
//! different process from the parse that read the comment excusing it, and
//! only the `.ast` in between carries both.

use rinx_ast::{Diagnostic, DiagnosticCode, Span, Suppression};
use rinx_renderer::{
    BrokenLink, DiagramError, EmptyListingError, HighlightError, ImageError, MathError,
    ObjectTypeMismatch,
};

/// Whether any of `suppressions` silences `code` reported at `span`.
pub(super) fn is_suppressed(
    suppressions: &[Suppression],
    code: DiagnosticCode,
    span: Option<Span>,
) -> bool {
    suppressions
        .iter()
        .any(|suppression| suppression.suppresses(code, span))
}

/// The parse diagnostics that survive `suppressions`.
pub(super) fn retain_reportable<'a>(
    diagnostics: &'a [Diagnostic],
    suppressions: &[Suppression],
) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| !is_suppressed(suppressions, diagnostic.code, diagnostic.span))
        .collect()
}

/// The broken links that survive `suppressions`.
///
/// Returns owned links rather than references because the survivors are what
/// `--strict-links` then fails the build over, and what the warning sidecar
/// records: a suppressed link must not do either, or the mechanism would only
/// be hiding the message while keeping the consequence.
pub(super) fn retain_reportable_links(
    links: &[BrokenLink],
    suppressions: &[Suppression],
) -> Vec<BrokenLink> {
    links
        .iter()
        .filter(|link| !is_suppressed(suppressions, link.code(), link.span))
        .cloned()
        .collect()
}

/// The math errors that survive `suppressions`.
///
/// Owned for the same reason [`retain_reportable_links`] returns owned links:
/// what survives here is what the warning sidecar records, so a suppressed
/// equation must leave no trace behind either.
pub(super) fn retain_reportable_math_errors(
    errors: &[MathError],
    suppressions: &[Suppression],
) -> Vec<MathError> {
    errors
        .iter()
        .filter(|error| !is_suppressed(suppressions, error.code(), error.span))
        .cloned()
        .collect()
}

/// The empty listings that survive `suppressions`.
///
/// Owned for the same reason [`retain_reportable_links`] returns owned links:
/// a suppressed table must leave no trace in the warning sidecar either.
pub(super) fn retain_reportable_empty_listing_errors(
    errors: &[EmptyListingError],
    suppressions: &[Suppression],
) -> Vec<EmptyListingError> {
    errors
        .iter()
        .filter(|error| !is_suppressed(suppressions, error.code(), error.span))
        .cloned()
        .collect()
}

/// The diagram failures that survive `suppressions`.
///
/// Owned for the same reason [`retain_reportable_links`] returns owned links:
/// a suppressed diagram must leave no trace in the warning sidecar either.
pub(super) fn retain_reportable_diagram_errors(
    errors: &[DiagramError],
    suppressions: &[Suppression],
) -> Vec<DiagramError> {
    errors
        .iter()
        .filter(|error| !is_suppressed(suppressions, error.code(), error.span))
        .cloned()
        .collect()
}

/// The highlighting failures that survive `suppressions`.
///
/// Owned for the same reason [`retain_reportable_links`] returns owned links:
/// a suppressed block must leave no trace in the warning sidecar either.
pub(super) fn retain_reportable_highlight_errors(
    errors: &[HighlightError],
    suppressions: &[Suppression],
) -> Vec<HighlightError> {
    errors
        .iter()
        .filter(|error| !is_suppressed(suppressions, error.code(), error.span))
        .cloned()
        .collect()
}

/// The image failures that survive `suppressions`.
///
/// Owned for the same reason [`retain_reportable_links`] returns owned links:
/// a suppressed image must leave no trace in the warning sidecar either.
pub(super) fn retain_reportable_image_errors(
    errors: &[ImageError],
    suppressions: &[Suppression],
) -> Vec<ImageError> {
    errors
        .iter()
        .filter(|error| !is_suppressed(suppressions, error.code(), error.span))
        .cloned()
        .collect()
}

/// The object-type mismatches that survive `suppressions`.
pub(super) fn retain_reportable_mismatches(
    mismatches: &[ObjectTypeMismatch],
    suppressions: &[Suppression],
) -> Vec<ObjectTypeMismatch> {
    mismatches
        .iter()
        .filter(|mismatch| {
            !is_suppressed(
                suppressions,
                DiagnosticCode::LinkTypeMismatch,
                mismatch.span,
            )
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Position, SuppressionCodes};
    use rinx_renderer::BrokenLinkKind;

    fn span_on(line: u32) -> Span {
        Span::new(Position::new(line, 1), Position::new(line, 10))
    }

    fn suppress_broken_ref(start_line: u32, end_line: u32) -> Suppression {
        Suppression {
            start_line,
            end_line,
            codes: SuppressionCodes::Only(vec![DiagnosticCode::LinkBrokenRef]),
            file: None,
        }
    }

    fn broken_ref(span: Option<Span>) -> BrokenLink {
        BrokenLink {
            kind: BrokenLinkKind::Reference,
            target: "missing".to_string(),
            span,
        }
    }

    #[test]
    fn test_is_suppressed_finds_a_matching_suppression() {
        // Given
        let suppressions = [suppress_broken_ref(5, 7)];

        // When / Then
        assert!(is_suppressed(
            &suppressions,
            DiagnosticCode::LinkBrokenRef,
            Some(span_on(6))
        ));
    }

    #[test]
    fn test_is_suppressed_is_false_when_nothing_matches() {
        // Given a suppression for another block
        let suppressions = [suppress_broken_ref(5, 7)];

        // When / Then
        assert!(!is_suppressed(
            &suppressions,
            DiagnosticCode::LinkBrokenRef,
            Some(span_on(9))
        ));
    }

    #[test]
    fn test_is_suppressed_is_false_with_no_suppressions() {
        // Given / When / Then
        assert!(!is_suppressed(
            &[],
            DiagnosticCode::LinkBrokenRef,
            Some(span_on(6))
        ));
    }

    #[test]
    fn test_retain_reportable_drops_only_the_suppressed_diagnostic() {
        // Given two diagnostics, one inside a suppressed block
        let diagnostics = [
            Diagnostic::at(DiagnosticCode::LinkBrokenRef, "inside", Some(span_on(6))),
            Diagnostic::at(DiagnosticCode::LinkBrokenRef, "outside", Some(span_on(20))),
        ];

        // When
        let reportable = retain_reportable(&diagnostics, &[suppress_broken_ref(5, 7)]);

        // Then
        assert_eq!(reportable.len(), 1);
        assert_eq!(reportable[0].message, "outside");
    }

    #[test]
    fn test_retain_reportable_keeps_a_diagnostic_of_another_code() {
        // Given a diagnostic in the suppressed block, but of a code the
        // comment did not name
        let diagnostics = [Diagnostic::at(
            DiagnosticCode::LinkBrokenTerm,
            "term",
            Some(span_on(6)),
        )];

        // When
        let reportable = retain_reportable(&diagnostics, &[suppress_broken_ref(5, 7)]);

        // Then
        assert_eq!(reportable.len(), 1);
    }

    #[test]
    fn test_retain_reportable_keeps_a_diagnostic_with_no_span() {
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

        // When
        let reportable = retain_reportable(&diagnostics, &suppressions);

        // Then — nothing to match on, so it is still reported
        assert_eq!(reportable.len(), 1);
    }

    #[test]
    fn test_retain_reportable_links_drops_a_suppressed_link() {
        // Given
        let links = [broken_ref(Some(span_on(6))), broken_ref(Some(span_on(20)))];

        // When
        let reportable = retain_reportable_links(&links, &[suppress_broken_ref(5, 7)]);

        // Then
        assert_eq!(reportable.len(), 1);
        assert_eq!(reportable[0].span, Some(span_on(20)));
    }

    #[test]
    fn test_retain_reportable_links_keeps_everything_without_suppressions() {
        // Given
        let links = [broken_ref(Some(span_on(6)))];

        // When / Then
        assert_eq!(retain_reportable_links(&links, &[]).len(), 1);
    }

    #[test]
    fn test_retain_reportable_mismatches_matches_on_the_type_mismatch_code() {
        // Given a mismatch inside a block suppressing exactly that code
        let mismatches = [ObjectTypeMismatch {
            name: "Fault".to_string(),
            requested_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Exception),
            resolved_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Class),
            span: Some(span_on(6)),
        }];
        let suppressions = [Suppression {
            start_line: 5,
            end_line: 7,
            codes: SuppressionCodes::Only(vec![DiagnosticCode::LinkTypeMismatch]),
            file: None,
        }];

        // When / Then
        assert!(retain_reportable_mismatches(&mismatches, &suppressions).is_empty());
        assert_eq!(retain_reportable_mismatches(&mismatches, &[]).len(), 1);
    }
}

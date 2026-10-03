//! The renderer's findings as [`Reported`] items, so a `.. noqa:` filters
//! them through the same [`rinx_ast::retain_reportable`] a parse diagnostic
//! goes through.
//!
//! Gathered in one file rather than beside each type because every impl is the
//! same two lines: each type already answers its code (an inherent `code()`,
//! which stays `const`) and carries its `span`.

use rinx_ast::{DiagnosticCode, Reported, Span};

use crate::{
    BrokenLink, DiagramError, EmptyListingError, HighlightError, ImageError, MathError,
    ObjectTypeMismatch,
};

/// Implements [`Reported`] for types with an inherent `code()` and a `span`
/// field.
macro_rules! reported_by_code_and_span {
    ($($finding:ty),+ $(,)?) => {$(
        impl Reported for $finding {
            fn code(&self) -> DiagnosticCode {
                <$finding>::code(self)
            }

            fn span(&self) -> Option<Span> {
                self.span
            }
        }
    )+};
}

reported_by_code_and_span!(
    BrokenLink,
    MathError,
    EmptyListingError,
    DiagramError,
    HighlightError,
    ImageError,
);

/// A mismatch has no code of its own to delegate to: there is one kind.
impl Reported for ObjectTypeMismatch {
    fn code(&self) -> DiagnosticCode {
        DiagnosticCode::LinkTypeMismatch
    }

    fn span(&self) -> Option<Span> {
        self.span
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BrokenLinkKind, HighlightErrorKind, HighlightedConstruct};
    use rinx_ast::{ObjectType, Position, PyObjectType};
    use rinx_uml::UmlError;

    const SPAN: Span = Span::new(Position::new(3, 1), Position::new(3, 9));

    /// The trait's answers, read through the trait rather than the inherent
    /// method of the same name.
    fn reported<T: Reported>(finding: &T) -> (DiagnosticCode, Option<Span>) {
        (finding.code(), finding.span())
    }

    #[test]
    fn test_a_broken_link_reports_its_kinds_code() {
        // Given
        let link = BrokenLink {
            kind: BrokenLinkKind::Reference,
            target: "missing".to_string(),
            span: Some(SPAN),
        };

        // When / Then
        assert_eq!(reported(&link), (DiagnosticCode::LinkBrokenRef, Some(SPAN)));
    }

    #[test]
    fn test_a_math_error_reports_the_invalid_latex_code() {
        // Given
        let error = MathError {
            message: "bad".to_string(),
            span: Some(SPAN),
        };

        // When / Then
        assert_eq!(
            reported(&error),
            (DiagnosticCode::MathInvalidLatex, Some(SPAN))
        );
    }

    #[test]
    fn test_an_empty_listing_reports_its_constructs_code() {
        // Given
        let error = EmptyListingError::pie("needpie", Some(SPAN));

        // When / Then
        assert_eq!(
            reported(&error),
            (DiagnosticCode::EntityPieEmptyResult, Some(SPAN))
        );
    }

    #[test]
    fn test_a_diagram_error_reports_its_failures_code() {
        // Given
        let error = DiagramError {
            directive: "needuml".to_string(),
            error: UmlError::UnknownEntity("REQ_404".to_string()).into(),
            span: Some(SPAN),
        };

        // When / Then
        assert_eq!(
            reported(&error),
            (DiagnosticCode::UmlUnknownEntity, Some(SPAN))
        );
    }

    #[test]
    fn test_a_highlight_error_reports_its_constructs_code() {
        // Given
        let error = HighlightError {
            message: "no grammar".to_string(),
            kind: HighlightErrorKind::UnknownLanguage,
            span: Some(SPAN),
            construct: HighlightedConstruct::CodeRole,
        };

        // When / Then
        assert_eq!(
            reported(&error),
            (DiagnosticCode::CodeRoleUnknownLanguage, Some(SPAN))
        );
    }

    #[test]
    fn test_an_image_error_reports_the_embed_unavailable_code() {
        // Given
        let error = ImageError {
            uri: "logo.png".to_string(),
            message: "unavailable".to_string(),
            span: Some(SPAN),
        };

        // When / Then
        assert_eq!(
            reported(&error),
            (DiagnosticCode::ImageEmbedUnavailable, Some(SPAN))
        );
    }

    #[test]
    fn test_a_type_mismatch_reports_the_type_mismatch_code() {
        // Given
        let mismatch = ObjectTypeMismatch {
            name: "Fault".to_string(),
            span: Some(SPAN),
            requested_type: ObjectType::Py(PyObjectType::Exception),
            resolved_type: ObjectType::Py(PyObjectType::Class),
        };

        // When / Then
        assert_eq!(
            reported(&mismatch),
            (DiagnosticCode::LinkTypeMismatch, Some(SPAN))
        );
    }
}

//! The renderer's findings as [`Reported`] items, so a `.. noqa:` filters
//! them through the same [`rinx_ast::retain_reportable`] a parse diagnostic
//! goes through — and so the build's warning line and the language server's
//! diagnostic word each one the same way.
//!
//! Gathered in one file rather than beside each type because every impl is the
//! same three lines: each type already answers its code (an inherent `code()`,
//! which stays `const`), carries its `span`, and words its message.

use std::borrow::Cow;

use rinx_ast::{Diagnostic, DiagnosticCode, DiagnosticSubject, Reported, Span};

use crate::{
    BrokenLink, DiagramError, EmptyListingError, HighlightError, ImageError, MathError,
    ObjectTypeMismatch,
};

/// Implements [`Reported`] for types with an inherent `code()` and a `span`
/// field, whose message is `$message` of the finding bound to `$finding`.
macro_rules! reported_by_code_and_span {
    ($($type:ty => |$finding:ident| $message:expr),+ $(,)?) => {$(
        impl Reported for $type {
            fn code(&self) -> DiagnosticCode {
                <$type>::code(self)
            }

            fn span(&self) -> Option<Span> {
                self.span
            }

            fn message(&self) -> Cow<'_, str> {
                let $finding = self;
                $message
            }
        }
    )+};
}

reported_by_code_and_span!(
    MathError => |error| Cow::Owned(format!("invalid math: {}", error.message)),
    EmptyListingError => |error| Cow::Owned(EmptyListingError::message(error)),
    DiagramError => |error| Cow::Owned(DiagramError::message(error)),
    HighlightError => |error| Cow::Borrowed(&error.message),
    ImageError => |error| Cow::Borrowed(&error.message),
);

/// A broken link is the one finding with a subject: the domain it missed.
impl Reported for BrokenLink {
    fn code(&self) -> DiagnosticCode {
        BrokenLink::code(self)
    }

    fn span(&self) -> Option<Span> {
        self.span
    }

    fn message(&self) -> Cow<'_, str> {
        Cow::Owned(BrokenLink::message(self))
    }

    fn subject(&self) -> Option<DiagnosticSubject> {
        self.kind.subject()
    }
}

/// A mismatch has no code of its own to delegate to: there is one kind.
impl Reported for ObjectTypeMismatch {
    fn code(&self) -> DiagnosticCode {
        DiagnosticCode::LinkTypeMismatch
    }

    fn span(&self) -> Option<Span> {
        self.span
    }

    fn message(&self) -> Cow<'_, str> {
        Cow::Owned(ObjectTypeMismatch::message(self))
    }
}

impl crate::RenderOutput {
    /// Every problem the render found that a document's author can act on, as
    /// plain diagnostics in source order — what the language server shows.
    ///
    /// The entity templates a site could not use are left out: they are a
    /// fault in the site, named by no document's span.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics: Vec<Diagnostic> = self
            .broken_links
            .iter()
            .map(Reported::to_diagnostic)
            .chain(
                self.object_type_mismatches
                    .iter()
                    .map(Reported::to_diagnostic),
            )
            .chain(self.math_errors.iter().map(Reported::to_diagnostic))
            .chain(
                self.empty_listing_errors
                    .iter()
                    .map(Reported::to_diagnostic),
            )
            .chain(self.diagram_errors.iter().map(Reported::to_diagnostic))
            .chain(self.highlight_errors.iter().map(Reported::to_diagnostic))
            .chain(self.image_errors.iter().map(Reported::to_diagnostic))
            .collect();
        diagnostics.sort_by_key(|diagnostic| diagnostic.span.map(|span| (span.file, span.start)));
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BrokenLinkKind, HighlightErrorKind, HighlightedConstruct};
    use rinx_ast::{ObjectType, Position, PyObjectType};
    use rinx_uml::UmlError;

    const SPAN: Span = Span::new(Position::new(3, 1), Position::new(3, 9));

    #[test]
    fn test_every_finding_words_its_message_as_the_build_prints_it() {
        // Given
        let math = MathError {
            message: "unknown command".to_string(),
            span: None,
        };
        let highlight = HighlightError {
            message: "no grammar".to_string(),
            kind: HighlightErrorKind::UnknownLanguage,
            span: None,
            construct: HighlightedConstruct::CodeRole,
        };
        let image = ImageError {
            uri: "logo.png".to_string(),
            message: "unavailable".to_string(),
            span: None,
        };
        let listing = EmptyListingError::pie("needpie", None);

        // When / Then
        assert_eq!(Reported::message(&math), "invalid math: unknown command");
        assert_eq!(Reported::message(&highlight), "no grammar");
        assert_eq!(Reported::message(&image), "unavailable");
        assert_eq!(
            Reported::message(&listing),
            EmptyListingError::message(&listing)
        );
    }

    #[test]
    fn test_diagnostics_lists_every_finding_in_source_order() {
        // Given — a broken `:ref:` on line 3 and an invalid equation on line 1,
        // which the render collects into separate lists
        let on_line = |line| Some(Span::new(Position::new(line, 1), Position::new(line, 9)));
        let document = rinx_ast::Document::new(
            "index.rst".to_string(),
            vec![
                rinx_ast::Node::Paragraph(vec![rinx_ast::InlineNode::Reference {
                    display: None,
                    target: "missing".to_string(),
                    span: on_line(3),
                    inventory: rinx_ast::InventorySelector::Any,
                }]),
                rinx_ast::Node::Paragraph(vec![rinx_ast::InlineNode::Math {
                    latex: "\\frac{".to_string(),
                    span: on_line(1),
                }]),
            ],
        );

        // When
        let diagnostics =
            crate::render(&document, &rinx_index::ProjectIndex::default(), "index.rst")
                .diagnostics();

        // Then
        let found: Vec<(DiagnosticCode, Option<Span>)> = diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.span))
            .collect();
        assert_eq!(
            found,
            [
                (DiagnosticCode::MathInvalidLatex, on_line(1)),
                (DiagnosticCode::LinkBrokenRef, on_line(3)),
            ]
        );
        assert_eq!(diagnostics[1].message, "broken ref 'missing'");
    }

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

//! The one way an image can still fail after it has parsed.
//!
//! Every other image problem — a malformed `:width:`, an `:align:` docutils
//! would not accept, a `:loading: embed` on an external URL — is decidable
//! from the source text alone and is reported by the parser. What is left is
//! the question only the renderer can ask: were the bytes this page asked to
//! embed actually handed to it?
//!
//! Modelled like [`crate::MathError`] and [`crate::HighlightError`], the other
//! two render-time failures, so the worker formats all three the same way.

use rinx_ast::{DiagnosticCode, Span};

/// An image whose `:loading: embed` could not be honoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageError {
    /// The image URI exactly as the author wrote it, so the message names
    /// something they can find in their document.
    pub uri: String,
    /// What went wrong, phrased for the document's author.
    pub message: String,
    /// Where the directive was written.
    pub span: Option<Span>,
}

impl ImageError {
    /// The diagnostic code this reports under — what a `.. noqa:` names to
    /// suppress it.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        DiagnosticCode::ImageEmbedUnavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_is_the_embed_unavailable_code() {
        // Given
        let error = ImageError {
            uri: "logo.png".to_string(),
            message: "not embedded".to_string(),
            span: None,
        };

        // When
        let code = error.code();

        // Then
        assert_eq!(code, DiagnosticCode::ImageEmbedUnavailable);
    }
}

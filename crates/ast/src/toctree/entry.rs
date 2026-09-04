use serde::{Deserialize, Serialize};

use crate::Span;

/// One line in a `.. toctree::` body, with its markup already interpreted.
///
/// The parser classifies each entry line rather than storing it verbatim, so
/// no later phase has to re-decide whether `Upstream <https://example.org>` is
/// a document path, and none of them can disagree about it. The four kinds
/// carry genuinely different data — a glob has no title, `self` has no
/// docname, an external link has a URL that is never resolved against the
/// source tree — so each gets its own variant rather than one struct holding
/// the union.
///
/// Every variant carries its own [`Span`], because the diagnostics that fault
/// an entry (`toctree.missing-document`, `toctree.glob-no-match`) are raised
/// during the project-wide *index* phase, long after the line was read. The
/// span is the only way that warning can still name the line the author wrote.
/// It is `Option` for the same reason [`crate::InlineNode`]'s is: content that
/// corresponds to no source line reports no position rather than a wrong one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TocEntry {
    /// Another document: `intro`, or `Getting started <intro>`.
    ///
    /// `docname` is stored exactly as written — relative to the referencing
    /// document, or absolute when it starts with `/`. Resolving it against the
    /// source tree is the analyzer's job, not the parser's.
    Document {
        title: Option<String>,
        docname: String,
        span: Option<Span>,
    },
    /// `self`, or `Overview <self>` — the document containing this toctree.
    SelfRef {
        title: Option<String>,
        span: Option<Span>,
    },
    /// An external link: `https://example.org`, or `Upstream <https://example.org>`.
    External {
        title: Option<String>,
        url: String,
        span: Option<Span>,
    },
    /// A wildcard pattern, produced only when the directive sets `:glob:`.
    ///
    /// It carries no title: Sphinx has no syntax for titling a pattern that
    /// expands to an arbitrary number of documents, and inventing one would
    /// make the same entry mean different things in the two tools.
    Glob { pattern: String, span: Option<Span> },
}

impl TocEntry {
    /// Where this entry was written, for a diagnostic raised about it in a
    /// later phase.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::Document { span, .. }
            | Self::SelfRef { span, .. }
            | Self::External { span, .. }
            | Self::Glob { span, .. } => *span,
        }
    }

    /// The author's explicit `Title <target>` text, when they wrote one.
    ///
    /// A [`Self::Glob`] never has one, so this is always `None` for it.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        match self {
            Self::Document { title, .. }
            | Self::SelfRef { title, .. }
            | Self::External { title, .. } => title.as_deref(),
            Self::Glob { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;

    fn span() -> Span {
        Span::new(Position::new(3, 4), Position::new(3, 9))
    }

    #[test]
    fn test_span_returns_the_span_of_every_variant() {
        // Given — one of each variant, all carrying the same span.
        let entries = vec![
            TocEntry::Document {
                title: None,
                docname: "intro".to_string(),
                span: Some(span()),
            },
            TocEntry::SelfRef {
                title: None,
                span: Some(span()),
            },
            TocEntry::External {
                title: None,
                url: "https://example.org".to_string(),
                span: Some(span()),
            },
            TocEntry::Glob {
                pattern: "api/*".to_string(),
                span: Some(span()),
            },
        ];

        // When / Then — no variant silently drops its position.
        for entry in &entries {
            assert_eq!(entry.span(), Some(span()), "{entry:?}");
        }
    }

    #[test]
    fn test_span_is_none_for_an_entry_with_no_source_line() {
        // Given
        let entry = TocEntry::Document {
            title: None,
            docname: "intro".to_string(),
            span: None,
        };

        // When / Then
        assert_eq!(entry.span(), None);
    }

    #[test]
    fn test_title_returns_the_explicit_title_when_written() {
        // Given
        let entry = TocEntry::Document {
            title: Some("Getting started".to_string()),
            docname: "intro".to_string(),
            span: None,
        };

        // When / Then
        assert_eq!(entry.title(), Some("Getting started"));
    }

    #[test]
    fn test_title_is_none_for_a_glob() {
        // Given — a glob can never carry a title.
        let entry = TocEntry::Glob {
            pattern: "api/*".to_string(),
            span: Some(span()),
        };

        // When / Then
        assert_eq!(entry.title(), None);
    }

    #[test]
    fn test_entry_round_trips_through_json() {
        // Given
        let entry = TocEntry::External {
            title: Some("Upstream".to_string()),
            url: "https://example.org".to_string(),
            span: Some(span()),
        };

        // When
        let json = serde_json::to_string(&entry).expect("serializes");
        let restored: TocEntry = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, entry);
    }
}

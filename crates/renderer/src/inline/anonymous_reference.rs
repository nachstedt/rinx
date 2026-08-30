//! Anonymous (`__`-suffixed) reference and hyperlink rendering.

use rusty_sphinx_ast::Span;
use std::fmt::Write as _;

use crate::{BrokenLink, BrokenLinkKind};

/// Renders an anonymous `__` reference by consuming the next URI from `anon_targets`.
/// Emits a broken-link fallback if the anonymous target list is exhausted.
pub(super) fn render_inline_anonymous_reference(
    html: &mut String,
    text: &str,
    span: Option<Span>,
    anon_targets: &[String],
    anon_index: &mut usize,
    broken_links: &mut Vec<BrokenLink>,
) {
    let text_escaped = html_escape::encode_text(text);
    if let Some(uri) = anon_targets.get(*anon_index) {
        let uri_attr = html_escape::encode_double_quoted_attribute(uri);
        let _ = write!(html, "<a href=\"{uri_attr}\">{text_escaped}</a>");
        *anon_index += 1;
    } else {
        let _ = write!(
            html,
            "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
        );
        broken_links.push(BrokenLink {
            kind: BrokenLinkKind::AnonymousReference,
            target: text.to_string(),
            span,
        });
    }
}

/// Renders an anonymous hyperlink (`` `text <target>`__ ``) — no index lookup,
/// since the target is written inline right there.
pub(super) fn render_inline_anonymous_hyperlink(html: &mut String, text: &str, target: &str) {
    let text_escaped = html_escape::encode_text(text);
    let target_attr = html_escape::encode_double_quoted_attribute(target);
    let _ = write!(html, "<a href=\"{target_attr}\">{text_escaped}</a>");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_inline_anonymous_reference_resolved() {
        // Given
        let anon_targets = vec!["https://example.com".to_string()];
        let mut anon_index = 0;
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_anonymous_reference(
            &mut html,
            "link text",
            None,
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"https://example.com\">link text</a>");
        assert_eq!(anon_index, 1);
    }

    #[test]
    fn test_render_inline_anonymous_reference_broken_when_index_exhausted() {
        // Given — no anonymous targets available
        let anon_targets: Vec<String> = vec![];
        let mut anon_index = 0;
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_anonymous_reference(
            &mut html,
            "broken",
            None,
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">broken</a>");
        assert_eq!(anon_index, 0);
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::AnonymousReference,
                target: "broken".to_string(),
                span: None,
            }]
        );
    }

    #[test]
    fn test_render_inline_anonymous_reference_advances_index_per_call() {
        // Given — two sequential calls consume targets in order
        let anon_targets = vec![
            "https://first.com".to_string(),
            "https://second.com".to_string(),
        ];
        let mut anon_index = 0;
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_anonymous_reference(
            &mut html,
            "first",
            None,
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );
        render_inline_anonymous_reference(
            &mut html,
            "second",
            None,
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );

        // Then
        assert_eq!(
            html,
            "<a href=\"https://first.com\">first</a><a href=\"https://second.com\">second</a>"
        );
        assert_eq!(anon_index, 2);
    }
}

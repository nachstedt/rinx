//! Anonymous (`__`-suffixed) reference and hyperlink rendering.

use rinx_ast::{LinkDestination, Span};
use std::fmt::Write as _;

use crate::hyperlink_target::DocumentHyperlinkTargets;
use crate::{BrokenLink, BrokenLinkKind};

/// Renders an anonymous `__` reference by consuming the next anonymous
/// target from `anon_targets`. Emits a broken-link fallback if the anonymous
/// target list is exhausted.
pub(super) fn render_inline_anonymous_reference(
    html: &mut String,
    text: &str,
    span: Option<Span>,
    anon_targets: &[LinkDestination],
    anon_index: &mut usize,
    document_targets: &DocumentHyperlinkTargets,
    broken_links: &mut Vec<BrokenLink>,
) {
    let Some(destination) = anon_targets.get(*anon_index) else {
        write_broken_link(html, text);
        broken_links.push(BrokenLink {
            kind: BrokenLinkKind::AnonymousReference,
            target: text.to_string(),
            span,
        });
        return;
    };
    *anon_index += 1;
    render_destination(
        html,
        text,
        destination,
        span,
        document_targets,
        broken_links,
    );
}

/// Renders an anonymous hyperlink (`` `text <destination>`__ ``), which
/// carries its destination and so consumes no anonymous target.
pub(super) fn render_inline_anonymous_hyperlink(
    html: &mut String,
    text: &str,
    destination: &LinkDestination,
    document_targets: &DocumentHyperlinkTargets,
    broken_links: &mut Vec<BrokenLink>,
) {
    render_destination(
        html,
        text,
        destination,
        None,
        document_targets,
        broken_links,
    );
}

/// Links `text` to `destination`, following an alias within the document —
/// reported as a broken hyperlink when the name it aliases is not there.
fn render_destination(
    html: &mut String,
    text: &str,
    destination: &LinkDestination,
    span: Option<Span>,
    document_targets: &DocumentHyperlinkTargets,
    broken_links: &mut Vec<BrokenLink>,
) {
    if let Some(href) = document_targets.destination_href(destination) {
        let text_escaped = html_escape::encode_text(text);
        let href_attr = html_escape::encode_double_quoted_attribute(href);
        let _ = write!(html, "<a href=\"{href_attr}\">{text_escaped}</a>");
        return;
    }
    write_broken_link(html, text);
    let target = match destination {
        LinkDestination::Uri(uri) => uri.clone(),
        LinkDestination::Name(name) => name.as_str().to_string(),
    };
    broken_links.push(BrokenLink {
        kind: BrokenLinkKind::Hyperlink,
        target,
        span,
    });
}

fn write_broken_link(html: &mut String, text: &str) {
    let text_escaped = html_escape::encode_text(text);
    let _ = write!(
        html,
        "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_inline_anonymous_reference_resolved() {
        // Given
        let anon_targets = vec![LinkDestination::Uri("https://example.com".to_string())];
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
            &DocumentHyperlinkTargets::default(),
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"https://example.com\">link text</a>");
        assert_eq!(anon_index, 1);
    }

    #[test]
    fn test_render_inline_anonymous_reference_broken_when_index_exhausted() {
        // Given — no anonymous targets available
        let anon_targets: Vec<LinkDestination> = vec![];
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
            &DocumentHyperlinkTargets::default(),
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
            LinkDestination::Uri("https://first.com".to_string()),
            LinkDestination::Uri("https://second.com".to_string()),
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
            &DocumentHyperlinkTargets::default(),
            &mut broken_links,
        );
        render_inline_anonymous_reference(
            &mut html,
            "second",
            None,
            &anon_targets,
            &mut anon_index,
            &DocumentHyperlinkTargets::default(),
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

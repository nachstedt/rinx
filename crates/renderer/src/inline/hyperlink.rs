//! Named hyperlink rendering.

use std::fmt::Write as _;

use super::RefText;
use rinx_ast::TargetName;

use crate::hyperlink_target::DocumentHyperlinkTargets;
use crate::{BrokenLink, BrokenLinkKind};

/// Renders a named hyperlink. Resolution order:
/// 1. Direct URI (http/https/mailto) — emitted as-is.
/// 2. The target this name stands for in this document — see
///    [`DocumentHyperlinkTargets`].
/// 3. No match — broken-link fallback.
pub(super) fn render_inline_hyperlink(
    html: &mut String,
    reference: RefText<'_>,
    document_targets: &DocumentHyperlinkTargets,
    broken_links: &mut Vec<BrokenLink>,
) {
    let RefText {
        display: text,
        target,
        span,
    } = reference;
    let text_escaped = html_escape::encode_text(text);
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
    {
        let target_attr = html_escape::encode_double_quoted_attribute(target);
        let _ = write!(html, "<a href=\"{target_attr}\">{text_escaped}</a>");
        return;
    }

    let target_name = TargetName::new(target);
    if let Some(href) = document_targets.href(&target_name) {
        let href_attr = html_escape::encode_double_quoted_attribute(href);
        let _ = write!(html, "<a href=\"{href_attr}\">{text_escaped}</a>");
    } else {
        let _ = write!(
            html,
            "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
        );
        broken_links.push(BrokenLink {
            kind: BrokenLinkKind::Hyperlink,
            target: target.to_string(),
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Document, Node};
    use rinx_index::ProjectIndex;

    /// Renders a reference to `target` shown as `display`, written in a
    /// document at `page.rst` holding `nodes`, against `index`.
    fn render(
        display: &str,
        target: &str,
        nodes: Vec<Node>,
        index: &ProjectIndex,
    ) -> (String, Vec<BrokenLink>) {
        let doc = Document::new("page.rst".to_string(), nodes);
        let section_ids = rinx_ast::allocate_section_ids(&doc.nodes);
        let document_targets = DocumentHyperlinkTargets::collect(&doc, index, &section_ids);
        let mut html = String::new();
        let mut broken_links = Vec::new();
        render_inline_hyperlink(
            &mut html,
            RefText {
                display,
                target,
                span: None,
            },
            &document_targets,
            &mut broken_links,
        );
        (html, broken_links)
    }

    #[test]
    fn test_render_inline_hyperlink_direct_http_uri() {
        // When
        let (html, _) = render(
            "Click here",
            "https://example.com",
            Vec::new(),
            &ProjectIndex::default(),
        );

        // Then
        assert_eq!(html, "<a href=\"https://example.com\">Click here</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_direct_mailto_uri() {
        // When
        let (html, _) = render(
            "Email us",
            "mailto:hello@example.com",
            Vec::new(),
            &ProjectIndex::default(),
        );

        // Then
        assert_eq!(html, "<a href=\"mailto:hello@example.com\">Email us</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_documents_own_external_target() {
        // Given
        let nodes = vec![Node::Target {
            name: TargetName::new("Python"),
            uri: Some("https://python.org".to_string()),
        }];

        // When
        let (html, _) = render("Python", "Python", nodes, &ProjectIndex::default());

        // Then
        assert_eq!(html, "<a href=\"https://python.org\">Python</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_label_of_this_document() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("my-label"), "page.rst".to_string());

        // When
        let (html, broken_links) = render("See below", "my-label", Vec::new(), &index);

        // Then
        assert_eq!(html, "<a href=\"#my-label\">See below</a>");
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_render_inline_hyperlink_section_title_of_this_document() {
        // Given
        let nodes = vec![Node::Heading {
            level: 1,
            text: vec![rinx_ast::InlineNode::Text("Getting Started".to_string())],
        }];

        // When
        let (html, _) = render(
            "Getting Started",
            "Getting Started",
            nodes,
            &ProjectIndex::default(),
        );

        // Then
        assert_eq!(html, "<a href=\"#getting-started\">Getting Started</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_breaks_on_a_label_of_another_document() {
        // Given — CPython's `hashlib.rst` writes `constants`_ for a label in
        // another document; docutils, and so Sphinx, reports it unknown.
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("constants"), "other.rst".to_string());

        // When
        let (html, broken_links) = render("constants", "constants", Vec::new(), &index);

        // Then
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">constants</a>");
        assert_eq!(broken_links.len(), 1);
    }

    #[test]
    fn test_render_inline_hyperlink_broken_link_when_not_found() {
        // When
        let (html, broken_links) = render(
            "No target",
            "no-target",
            Vec::new(),
            &ProjectIndex::default(),
        );

        // Then
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">No target</a>");
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::Hyperlink,
                target: "no-target".to_string(),
                span: None,
            }]
        );
    }
}

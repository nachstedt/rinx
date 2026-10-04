//! Named hyperlink rendering.

use std::fmt::Write as _;

use rinx_ast::{HyperlinkTarget, LinkDestination, Span};

use crate::hyperlink_target::DocumentHyperlinkTargets;
use crate::{BrokenLink, BrokenLinkKind};

/// Renders a named hyperlink: to the destination written in it
/// (`` `text <destination>`_ ``), or to the target its name stands for in this
/// document — see [`DocumentHyperlinkTargets`]. A name that leads nowhere is
/// a broken link.
pub(super) fn render_inline_hyperlink(
    html: &mut String,
    text: &str,
    target: &HyperlinkTarget,
    span: Option<Span>,
    document_targets: &DocumentHyperlinkTargets,
    broken_links: &mut Vec<BrokenLink>,
) {
    let text_escaped = html_escape::encode_text(text);
    let href = match target {
        HyperlinkTarget::Reference(name) => document_targets.href(name),
        HyperlinkTarget::Embedded(destination) => document_targets.destination_href(destination),
    };
    if let Some(href) = href {
        let href_attr = html_escape::encode_double_quoted_attribute(href);
        let _ = write!(html, "<a href=\"{href_attr}\">{text_escaped}</a>");
        return;
    }
    let _ = write!(
        html,
        "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
    );
    let missing = match target {
        HyperlinkTarget::Reference(name)
        | HyperlinkTarget::Embedded(LinkDestination::Name(name)) => name.as_str().to_string(),
        HyperlinkTarget::Embedded(LinkDestination::Uri(uri)) => uri.clone(),
    };
    broken_links.push(BrokenLink {
        kind: BrokenLinkKind::Hyperlink,
        target: missing,
        span,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Document, Node, TargetName};
    use rinx_index::ProjectIndex;

    /// Renders a reference to `target` shown as `display`, written in a
    /// document at `page.rst` holding `nodes`, against `index`.
    fn render(
        display: &str,
        target: &HyperlinkTarget,
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
            display,
            target,
            None,
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
            &HyperlinkTarget::Embedded(LinkDestination::Uri("https://example.com".to_string())),
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
            &HyperlinkTarget::Embedded(LinkDestination::Uri(
                "mailto:hello@example.com".to_string(),
            )),
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
            destination: Some(rinx_ast::LinkDestination::Uri(
                "https://python.org".to_string(),
            )),
        }];

        // When
        let (html, _) = render(
            "Python",
            &HyperlinkTarget::Reference(TargetName::new("Python")),
            nodes,
            &ProjectIndex::default(),
        );

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
        let (html, broken_links) = render(
            "See below",
            &HyperlinkTarget::Reference(TargetName::new("my-label")),
            Vec::new(),
            &index,
        );

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
            &HyperlinkTarget::Reference(TargetName::new("Getting Started")),
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
        let (html, broken_links) = render(
            "constants",
            &HyperlinkTarget::Reference(TargetName::new("constants")),
            Vec::new(),
            &index,
        );

        // Then
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">constants</a>");
        assert_eq!(broken_links.len(), 1);
    }

    #[test]
    fn test_render_inline_hyperlink_broken_link_when_not_found() {
        // When
        let (html, broken_links) = render(
            "No target",
            &HyperlinkTarget::Reference(TargetName::new("no-target")),
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

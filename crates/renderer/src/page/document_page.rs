//! A document's finished page: its rendered body in the site's template, with
//! the title, sidebar and page relations the project index gives it.
//!
//! The build's render action and the language server's preview both call
//! [`render_document_page`], so a preview cannot show a page the build would
//! not write.

use anyhow::Result;
use rinx_ast::{Document, Node, inline_plain_text};
use rinx_index::ProjectIndex;

use super::layout::{PageMeta, render_page};
use crate::config::SiteConfig;

/// The title of `document`'s page: its first level-1 heading as plain text,
/// or empty when it has none.
#[must_use]
pub fn page_title(document: &Document) -> String {
    document
        .nodes
        .iter()
        .find_map(|node| match node {
            Node::Heading { level: 1, text } => Some(inline_plain_text(text)),
            _ => None,
        })
        .unwrap_or_default()
}

/// The page of `document`, whose rendered body is `body`, as the page at
/// `doc_path` of a site rendered with `config` through `template`, its
/// stylesheet linked as `css_path`.
///
/// # Errors
///
/// Returns an error if the template cannot be parsed or rendered.
pub fn render_document_page(
    body: &str,
    document: &Document,
    index: &ProjectIndex,
    doc_path: &str,
    config: &SiteConfig,
    template: &str,
    css_path: &str,
) -> Result<String> {
    let page_title = page_title(document);
    render_page(
        body,
        template,
        config,
        &PageMeta {
            css_path,
            page_title: &page_title,
            doc_path,
            source_path: &document.path,
            has_genindex: index.has_genindex_entries(),
            ..PageMeta::default()
        }
        .with_navigation(index, config),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_title_is_the_first_level_one_heading() {
        // Given
        let document = document_of(vec![
            heading(2, "Sub"),
            heading(1, "Main"),
            heading(1, "Second"),
        ]);

        // When / Then
        assert_eq!(page_title(&document), "Main");
    }

    #[test]
    fn test_page_title_is_empty_without_a_level_one_heading() {
        // Given
        let document = document_of(vec![heading(2, "Sub")]);

        // When / Then
        assert_eq!(page_title(&document), "");
    }

    #[test]
    fn test_render_document_page_fills_the_template() {
        // Given
        let document = document_of(vec![heading(1, "Main")]);
        let template = "<title>{{ page_title }}</title><link href=\"{{ css_path }}\">{{ body }}";

        // When
        let page = render_document_page(
            "<p>body</p>",
            &document,
            &ProjectIndex::default(),
            "guide/intro.rst",
            &SiteConfig::default(),
            template,
            "../default.css",
        )
        .unwrap();

        // Then
        assert_eq!(
            page,
            "<title>Main</title><link href=\"../default.css\"><p>body</p>"
        );
    }

    #[test]
    fn test_render_document_page_reports_a_broken_template() {
        // Given
        let document = document_of(Vec::new());

        // When
        let result = render_document_page(
            "",
            &document,
            &ProjectIndex::default(),
            "index.rst",
            &SiteConfig::default(),
            "{% if %}",
            "default.css",
        );

        // Then
        assert!(result.is_err());
    }

    fn heading(level: u8, text: &str) -> Node {
        Node::Heading {
            level,
            text: vec![rinx_ast::InlineNode::Text(text.to_string())],
        }
    }

    fn document_of(nodes: Vec<Node>) -> Document {
        Document::new("index.rst".to_string(), nodes)
    }
}

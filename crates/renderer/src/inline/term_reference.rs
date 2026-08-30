//! Glossary term (`:term:`) reference rendering.

use std::fmt::Write as _;

use super::RefText;
use rusty_sphinx_ast::TargetName;
use rusty_sphinx_index::ProjectIndex;

use crate::{BrokenLink, BrokenLinkKind};

/// Renders a glossary term reference (`:term:`). Resolves the term via the project
/// index and emits a relative link with the appropriate CSS classes, or a
/// broken-link fallback if the term is not found in the index.
pub(super) fn render_inline_term_reference(
    html: &mut String,
    reference: RefText<'_>,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let RefText {
        display,
        target: term,
        span,
    } = reference;
    let display_escaped = html_escape::encode_text(display);
    let term_name = TargetName::new(term);
    if let Some(glossary_doc_path) = index.glossary_terms.get(&term_name) {
        let current_dir = std::path::Path::new(doc_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let target_html_path = std::path::Path::new(glossary_doc_path).with_extension("html");
        let relative_path =
            pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
        let anchor = rusty_sphinx_ast::term_id(term);
        let href = format!("{}#{}", relative_path.display(), anchor);
        let href_attr = html_escape::encode_double_quoted_attribute(&href);
        let _ = write!(
            html,
            "<a class=\"reference internal\" href=\"{href_attr}\"><span class=\"xref std std-term\">{display_escaped}</span></a>"
        );
    } else {
        let _ = write!(
            html,
            "<a href=\"#\" class=\"broken-link\"><span class=\"xref std std-term\">{display_escaped}</span></a>"
        );
        broken_links.push(BrokenLink {
            kind: BrokenLinkKind::TermReference,
            target: term.to_string(),
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_inline_term_reference_resolved_with_css_classes() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("widget"), "glossary.rst".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_term_reference(
            &mut html,
            RefText {
                display: "widget",
                target: "widget",
                span: None,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert!(html.contains("class=\"reference internal\""));
        assert!(html.contains("href=\"glossary.html#term-widget\""));
        assert!(html.contains("class=\"xref std std-term\""));
        assert!(html.contains(">widget<"));
    }

    #[test]
    fn test_render_inline_term_reference_broken_link_when_term_not_found() {
        // Given
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_term_reference(
            &mut html,
            RefText {
                display: "unknown term",
                target: "unknown",
                span: None,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert!(html.contains("class=\"xref std std-term\""));
        assert!(html.contains(">unknown term<"));
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::TermReference,
                target: "unknown".to_string(),
                span: None,
            }]
        );
    }

    #[test]
    fn test_render_inline_term_reference_resolves_cross_directory_path() {
        // Given — document is two levels deep, glossary at root
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("api"), "reference/glossary.rst".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_term_reference(
            &mut html,
            RefText {
                display: "API",
                target: "api",
                span: None,
            },
            &index,
            "guide/intro.rst",
            &mut broken_links,
        );

        // Then
        assert!(html.contains("href=\"../reference/glossary.html#term-api\""));
        assert!(html.contains(">API<"));
    }

    #[test]
    fn test_render_inline_term_reference_custom_display_differs_from_term() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_term_reference(
            &mut html,
            RefText {
                display: "the env",
                target: "environment",
                span: None,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert!(html.contains("href=\"glossary.html#term-environment\""));
        assert!(html.contains(">the env<"));
    }
}

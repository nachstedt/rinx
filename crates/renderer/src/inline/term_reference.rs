//! Glossary term (`:term:`) reference rendering.

use std::fmt::Write as _;

use super::RefText;
use rinx_ast::{InventorySelector, TargetName};
use rinx_index::ProjectIndex;

use super::external_link::write_external_link;
use crate::resolution::{resolve_external, unresolved_kind};
use crate::{BrokenLink, BrokenLinkKind};

/// Renders a glossary term reference (`:term:`). Resolves the term via the project
/// index and emits a relative link with the appropriate CSS classes, then —
/// if no glossary of this site defines it — through the other sites'
/// inventories, and only then falls back to a broken link.
pub(super) fn render_inline_term_reference(
    html: &mut String,
    reference: RefText<'_>,
    inventory: &InventorySelector,
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
    let local = if inventory.allows_local() {
        index.glossary_terms.get(&term_name)
    } else {
        None
    };
    if let Some(glossary_doc_path) = local {
        let current_dir = std::path::Path::new(doc_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let target_html_path = std::path::Path::new(glossary_doc_path).with_extension("html");
        let relative_path =
            pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
        let anchor = rinx_ast::term_id(term);
        let href = format!("{}#{}", relative_path.display(), anchor);
        let href_attr = html_escape::encode_double_quoted_attribute(&href);
        let _ = write!(
            html,
            "<a class=\"reference internal\" href=\"{href_attr}\"><span class=\"xref std std-term\">{display_escaped}</span></a>"
        );
    } else if let Some(hit) = resolve_external(
        &index.external_inventories,
        &["std:term".to_string()],
        term,
        inventory,
    ) {
        let inner = format!("<span class=\"xref std std-term\">{display_escaped}</span>");
        write_external_link(html, &hit, doc_path, &inner);
    } else {
        let _ = write!(
            html,
            "<a href=\"#\" class=\"broken-link\"><span class=\"xref std std-term\">{display_escaped}</span></a>"
        );
        broken_links.push(BrokenLink {
            kind: unresolved_kind(
                inventory,
                &index.external_inventories,
                BrokenLinkKind::TermReference,
            ),
            target: term.to_string(),
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_inline_term_reference_links_a_term_another_site_defines() {
        // Given
        let index = crate::test_support::index_linking_into_python();
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_term_reference(
            &mut html,
            RefText {
                display: "bytecode",
                target: "bytecode",
                span: None,
            },
            &rinx_ast::InventorySelector::Any,
            &index,
            "index.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(
            html,
            "<a class=\"reference external\" \
             href=\"https://docs.python.org/3/glossary.html#term-bytecode\" \
             title=\"(in Python v3.12)\"><span class=\"xref std std-term\">bytecode</span></a>"
        );
        assert!(broken_links.is_empty());
    }

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
            &rinx_ast::InventorySelector::Any,
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
            &rinx_ast::InventorySelector::Any,
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
            &rinx_ast::InventorySelector::Any,
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
            &rinx_ast::InventorySelector::Any,
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert!(html.contains("href=\"glossary.html#term-environment\""));
        assert!(html.contains(">the env<"));
    }
}

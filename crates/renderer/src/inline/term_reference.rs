//! Glossary term (`:term:`) reference rendering.

use std::fmt::Write as _;

use super::RefText;
use rinx_ast::{InventorySelector, TargetName};
use rinx_index::ProjectIndex;

use super::external_link::write_external_link;
use crate::resolution::{ExternalHit, resolve_external, unresolved_kind};
use crate::{BrokenLink, BrokenLinkKind, ReferenceTarget};

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
    match resolve_term(index, term, inventory) {
        TermResolution::Local(glossary_doc_path) => {
            let href = term_href(glossary_doc_path, term, doc_path);
            let href_attr = html_escape::encode_double_quoted_attribute(&href);
            let _ = write!(
                html,
                "<a class=\"reference internal\" href=\"{href_attr}\"><span class=\"xref std std-term\">{display_escaped}</span></a>"
            );
        }
        TermResolution::External(hit) => {
            let inner = format!("<span class=\"xref std std-term\">{display_escaped}</span>");
            write_external_link(html, &hit, doc_path, &inner);
        }
        TermResolution::Unresolved(kind) => {
            let _ = write!(
                html,
                "<a href=\"#\" class=\"broken-link\"><span class=\"xref std std-term\">{display_escaped}</span></a>"
            );
            broken_links.push(BrokenLink {
                kind,
                target: term.to_string(),
                span,
            });
        }
    }
}

/// What a `:term:` resolved to.
enum TermResolution<'a> {
    /// A term a glossary of this site defines, in the document at this path.
    Local(&'a str),
    /// A term another site's inventory lists.
    External(ExternalHit<'a>),
    /// Nothing the reference may link to; why.
    Unresolved(BrokenLinkKind),
}

/// Resolves the glossary term `term` against this site, then — when no
/// glossary of it defines the term — against the other sites' inventories.
fn resolve_term<'a>(
    index: &'a ProjectIndex,
    term: &str,
    inventory: &InventorySelector,
) -> TermResolution<'a> {
    let term_name = TargetName::new(term);
    if let Some(glossary_doc_path) = index
        .glossary_terms
        .get(&term_name)
        .filter(|_| inventory.allows_local())
    {
        return TermResolution::Local(glossary_doc_path);
    }
    // A term several glossaries define is this site's, twice: refused rather
    // than looked up in another site.
    let contested = index
        .ambiguous_definitions
        .glossary_terms
        .get(&term_name)
        .filter(|_| inventory.allows_local());
    if contested.is_none()
        && let Some(hit) = resolve_external(
            &index.external_inventories,
            &["std:term".to_string()],
            term,
            inventory,
        )
    {
        return TermResolution::External(hit);
    }
    TermResolution::Unresolved(
        unresolved_kind(
            inventory,
            &index.external_inventories,
            BrokenLinkKind::TermReference,
        )
        .unless_contested(contested),
    )
}

/// Where the `:term:` to `term`, written in the page at `doc_path`, leads —
/// `None` when [`render_inline_term_reference`] would draw it broken.
pub(super) fn term_target(
    index: &ProjectIndex,
    term: &str,
    inventory: &InventorySelector,
    doc_path: &str,
) -> Option<ReferenceTarget> {
    match resolve_term(index, term, inventory) {
        TermResolution::Local(glossary_doc) => Some(term_reference_target(term, glossary_doc)),
        TermResolution::External(hit) => Some(ReferenceTarget::external(
            None,
            hit.inventory,
            hit.target,
            doc_path,
        )),
        TermResolution::Unresolved(_) => None,
    }
}

/// The glossary term `term`, defined in `glossary_doc`, as a reference target
/// — shared with `:any:`.
pub(super) fn term_reference_target(term: &str, glossary_doc: &str) -> ReferenceTarget {
    ReferenceTarget::in_document(term, glossary_doc, Some(rinx_ast::term_id(term)))
}

/// The href a page at `doc_path` links `term`, defined by a glossary in
/// `glossary_doc`, by.
pub(super) fn term_href(glossary_doc: &str, term: &str, doc_path: &str) -> String {
    format!(
        "{}#{}",
        rinx_index::relative_doc_href(glossary_doc, doc_path),
        rinx_ast::term_id(term)
    )
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

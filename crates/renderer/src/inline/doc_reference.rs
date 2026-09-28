//! `:doc:` reference rendering, and the link to a whole document that
//! `:any:` draws the same way.

use std::fmt::Write as _;

use rinx_ast::{InventorySelector, Span};
use rinx_index::{ProjectIndex, relative_doc_href};

use super::external_link::write_external_link;
use crate::resolution::{resolve_document, resolve_external, unresolved_kind};
use crate::{BrokenLink, BrokenLinkKind};

/// What Sphinx shows for a document that has no title.
const NO_TITLE: &str = "<no title>";

/// A `:doc:` as the author wrote it.
///
/// Not a [`super::RefText`], for the reason [`super::reference::LabelRef`]
/// is not: a bare `:doc:` shows the target document's title, which only the
/// index knows.
#[derive(Debug, Clone, Copy)]
pub(super) struct DocRef<'a> {
    /// The explicit title of the `Title <doc>` form, if one was written.
    pub title: Option<&'a str>,
    /// The document name as written, relative or `/`-absolute.
    pub target: &'a str,
    /// `false` for the `!` form, which is never looked up.
    pub link: bool,
    /// Where the role was written, when the parser could place it.
    pub span: Option<Span>,
    /// Which sites may hold the document — see [`InventorySelector`].
    pub inventory: &'a InventorySelector,
}

/// Renders a `:doc:` reference: a link to the document it names, showing the
/// explicit title, else the document's own title, else `<no title>`.
///
/// Another site's inventory is searched only when the role asks for it with
/// an `:external:` prefix. A bare `:doc:` is this site's alone, as it is in
/// Sphinx, whose `intersphinx_disabled_reftypes` leaves `std:doc` out by
/// default — a document name like `index` is too generic to guess at.
pub(super) fn render_inline_doc_reference(
    html: &mut String,
    reference: DocRef<'_>,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let DocRef {
        title,
        target,
        link,
        span,
        inventory,
    } = reference;
    if !link {
        let _ = write!(
            html,
            "<span class=\"xref std std-doc\">{}</span>",
            html_escape::encode_text(target)
        );
        return;
    }
    let local = inventory
        .allows_local()
        .then(|| resolve_document(index, doc_path, target))
        .flatten();
    if let Some(target_doc) = local {
        write_doc_link(html, index, title, target_doc, doc_path);
        return;
    }
    if !inventory.is_any()
        && let Some(hit) = resolve_external(
            &index.external_inventories,
            &["std:doc".to_string()],
            target,
            inventory,
        )
    {
        let display = title.unwrap_or_else(|| hit.target.display_text());
        write_external_link(html, &hit, doc_path, &html_escape::encode_text(display));
        return;
    }
    let _ = write!(
        html,
        "<a href=\"#\" class=\"broken-link\">{}</a>",
        html_escape::encode_text(title.unwrap_or(target))
    );
    broken_links.push(BrokenLink {
        kind: unresolved_kind(
            inventory,
            &index.external_inventories,
            BrokenLinkKind::DocReference,
        ),
        target: target.to_string(),
        span,
    });
}

/// Writes the link to `target_doc`, a document of this site, from the page at
/// `doc_path`: Sphinx's `reference internal` anchor around a `doc` span
/// showing `title`, else the document's own title, else `<no title>`.
///
/// Shared with `:any:`, so a document it finds is drawn exactly as `:doc:`
/// draws one.
pub(super) fn write_doc_link(
    html: &mut String,
    index: &ProjectIndex,
    title: Option<&str>,
    target_doc: &str,
    doc_path: &str,
) {
    let text = title
        .or_else(|| index.document_title(target_doc))
        .unwrap_or(NO_TITLE);
    let href = relative_doc_href(target_doc, doc_path);
    let _ = write!(
        html,
        "<a class=\"reference internal\" href=\"{}\"><span class=\"doc\">{}</span></a>",
        html_escape::encode_double_quoted_attribute(&href),
        html_escape::encode_text(text)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::InventoryName;

    /// A site holding a titled page, a titleless one and a page in a
    /// subdirectory, and linking into Python's documentation.
    fn index() -> ProjectIndex {
        let mut index = crate::test_support::index_linking_into_python();
        for doc in ["index.rst", "notes.rst", "guide/intro.rst"] {
            index.documents.insert(doc.to_string());
        }
        index
            .document_titles
            .insert("index.rst".to_string(), "Home & away".to_string());
        index
            .document_titles
            .insert("guide/intro.rst".to_string(), "Introduction".to_string());
        index
    }

    /// Renders `reference` from `doc_path`, returning the HTML and what broke.
    fn render_from(doc_path: &str, reference: DocRef<'_>) -> (String, Vec<BrokenLink>) {
        let mut html = String::new();
        let mut broken_links = Vec::new();
        render_inline_doc_reference(&mut html, reference, &index(), doc_path, &mut broken_links);
        (html, broken_links)
    }

    fn doc_ref<'a>(target: &'a str, inventory: &'a InventorySelector) -> DocRef<'a> {
        DocRef {
            title: None,
            target,
            link: true,
            span: None,
            inventory,
        }
    }

    #[test]
    fn test_render_links_a_document_showing_its_title() {
        // Given / When
        let (html, broken) =
            render_from("index.rst", doc_ref("guide/intro", &InventorySelector::Any));

        // Then
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"guide/intro.html\">\
             <span class=\"doc\">Introduction</span></a>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_render_resolves_relative_to_the_referencing_document() {
        // Given / When
        let (up, _) = render_from(
            "guide/intro.rst",
            doc_ref("../notes", &InventorySelector::Any),
        );
        let (root, _) = render_from(
            "guide/intro.rst",
            doc_ref("/index", &InventorySelector::Any),
        );

        // Then
        assert!(up.contains("href=\"../notes.html\""), "{up}");
        assert!(root.contains("href=\"../index.html\""), "{root}");
        assert!(root.contains(">Home &amp; away<"), "{root}");
    }

    #[test]
    fn test_render_shows_no_title_for_a_titleless_document() {
        // Given / When
        let (html, broken) = render_from("index.rst", doc_ref("notes", &InventorySelector::Any));

        // Then
        assert!(
            html.contains("<span class=\"doc\">&lt;no title&gt;</span>"),
            "{html}"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_render_prefers_an_explicit_title() {
        // Given
        let reference = DocRef {
            title: Some("the notes"),
            ..doc_ref("notes", &InventorySelector::Any)
        };

        // When
        let (html, _) = render_from("index.rst", reference);

        // Then
        assert!(
            html.contains("<span class=\"doc\">the notes</span>"),
            "{html}"
        );
    }

    #[test]
    fn test_render_writes_an_unlinked_bang_form_as_text() {
        // Given
        let reference = DocRef {
            link: false,
            ..doc_ref("Notes <notes>", &InventorySelector::Any)
        };

        // When
        let (html, broken) = render_from("index.rst", reference);

        // Then
        assert_eq!(
            html,
            "<span class=\"xref std std-doc\">Notes &lt;notes&gt;</span>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_render_reports_an_unknown_document() {
        // Given
        let at = Span::new(
            rinx_ast::Position::new(4, 1),
            rinx_ast::Position::new(4, 16),
        );
        let reference = DocRef {
            span: Some(at),
            ..doc_ref("missing", &InventorySelector::Any)
        };

        // When
        let (html, broken) = render_from("index.rst", reference);

        // Then
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">missing</a>");
        assert_eq!(
            broken,
            vec![BrokenLink {
                kind: BrokenLinkKind::DocReference,
                target: "missing".to_string(),
                span: Some(at),
            }]
        );
    }

    #[test]
    fn test_render_searches_no_inventory_without_an_external_prefix() {
        // Given — Python's inventory lists `tutorial/index`, this site does not
        // When
        let (html, broken) = render_from(
            "index.rst",
            doc_ref("tutorial/index", &InventorySelector::Any),
        );

        // Then
        assert!(html.contains("broken-link"), "{html}");
        assert_eq!(broken.len(), 1);
    }

    #[test]
    fn test_render_links_into_an_inventory_with_an_external_prefix() {
        // Given
        let named = InventorySelector::Named(InventoryName::new("python").unwrap());

        // When
        let (external, _) = render_from(
            "guide/intro.rst",
            doc_ref("tutorial/index", &InventorySelector::ExternalOnly),
        );
        let (by_name, _) = render_from("index.rst", doc_ref("tutorial/index", &named));

        // Then — the target is looked up as written, not made relative
        assert_eq!(
            external,
            "<a class=\"reference external\" \
             href=\"https://docs.python.org/3/tutorial/index.html\" \
             title=\"(in Python v3.12)\">The Python Tutorial</a>"
        );
        assert_eq!(by_name, external);
    }

    #[test]
    fn test_render_never_links_a_local_document_for_an_external_role() {
        // Given — `index` is a document of this site, and of no inventory
        // When
        let (html, broken) = render_from(
            "index.rst",
            doc_ref("index", &InventorySelector::ExternalOnly),
        );

        // Then
        assert!(html.contains("broken-link"), "{html}");
        assert_eq!(broken[0].kind, BrokenLinkKind::DocReference);
    }

    #[test]
    fn test_render_reports_an_undeclared_inventory_by_name() {
        // Given
        let nope = InventoryName::new("nope").unwrap();
        let selector = InventorySelector::Named(nope.clone());

        // When
        let (_, broken) = render_from("index.rst", doc_ref("tutorial/index", &selector));

        // Then
        assert_eq!(broken[0].kind, BrokenLinkKind::UnknownInventory(nope));
    }
}

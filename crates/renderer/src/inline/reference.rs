//! Named `:ref:` reference rendering.

use std::fmt::Write as _;

use rinx_ast::{InventorySelector, Span, TargetName};
use rinx_index::relative_doc_href;
use rinx_index::{ProjectIndex, SpecialPage};

use super::external_link::write_external_link;
use crate::resolution::{ExternalHit, resolve_external, unresolved_kind};
use crate::{BrokenLink, BrokenLinkKind, Destination, ReferenceTarget};

/// A `:ref:` as the author wrote it.
///
/// Not a [`super::RefText`], because the one thing that type takes for
/// granted — that the link text is known before the index is consulted — is
/// false here: a bare `:ref:` shows the title of the section its label
/// points at, which only the index knows.
#[derive(Debug, Clone, Copy)]
pub(super) struct LabelRef<'a> {
    /// The explicit title of the `Title <label>` form, if one was written.
    pub title: Option<&'a str>,
    /// The label to look up in the project index.
    pub target: &'a str,
    /// Where the role was written, when the parser could place it.
    pub span: Option<Span>,
    /// Which sites may define the label — see [`InventorySelector`].
    pub inventory: &'a InventorySelector,
}

/// What a `:ref:` label resolved to.
enum LabelResolution<'a> {
    /// A label a document of this site defines, in `doc_path`.
    Local { name: TargetName, doc_path: &'a str },
    /// A page the build writes that no document is.
    SpecialPage(SpecialPage),
    /// A label another site's inventory lists.
    External(ExternalHit<'a>),
    /// Nothing the reference may link to; why.
    Unresolved(BrokenLinkKind),
}

/// Resolves the label `target` against this site, then — when no document of
/// it defines the label — against the other sites' inventories.
fn resolve_label<'a>(
    index: &'a ProjectIndex,
    target: &str,
    inventory: &InventorySelector,
) -> LabelResolution<'a> {
    let name = TargetName::new(target);
    if let Some(doc_path) = index
        .targets
        .get(&name)
        .filter(|_| inventory.allows_local())
    {
        return LabelResolution::Local { name, doc_path };
    }
    // A label several documents define is this site's, twice: refused here
    // rather than looked up in another site, which would link it elsewhere.
    let contested = index
        .ambiguous_definitions
        .targets
        .get(&name)
        .filter(|_| inventory.allows_local());
    // A page the build writes that no document is (`genindex`, and the module
    // index where enabled): local, so ahead of every inventory, but behind a
    // label a document defines.
    if inventory.allows_local()
        && let Some(page) = index.special_page(&name)
    {
        return LabelResolution::SpecialPage(page);
    }
    if contested.is_none()
        && let Some(hit) = resolve_external(
            &index.external_inventories,
            &["std:label".to_string()],
            target,
            inventory,
        )
    {
        return LabelResolution::External(hit);
    }
    LabelResolution::Unresolved(
        unresolved_kind(
            inventory,
            &index.external_inventories,
            BrokenLinkKind::Reference,
        )
        .unless_contested(contested),
    )
}

/// Renders a named `:ref:` reference. Resolves `target` via the project
/// index and emits a relative HTML link, or a broken-link fallback if
/// `target` is not found.
///
/// The link text is the explicit title when one was written, else the title
/// of the section the label sits above (as Sphinx shows it), else the label
/// itself — a label on anything but a heading has no title to show.
///
/// A label no document of this site defines is looked up in the other sites'
/// inventories before it is reported broken; one found there shows the
/// title that inventory gives it.
pub(super) fn render_inline_reference(
    html: &mut String,
    reference: LabelRef<'_>,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let LabelRef {
        title,
        target,
        span,
        inventory,
    } = reference;
    match resolve_label(index, target, inventory) {
        LabelResolution::Local {
            name,
            doc_path: target_path,
        } => {
            let display_escaped =
                html_escape::encode_text(label_link_text(index, title, &name, target));
            let href = label_href(index, &name, target_path, doc_path);
            let href_attr = html_escape::encode_double_quoted_attribute(&href);
            let _ = write!(html, "<a href=\"{href_attr}\">{display_escaped}</a>");
        }
        LabelResolution::SpecialPage(page) => {
            write_special_page_link(html, page, title, doc_path);
        }
        LabelResolution::External(hit) => {
            let display = title.unwrap_or_else(|| hit.target.display_text());
            write_external_link(html, &hit, doc_path, &html_escape::encode_text(display));
        }
        LabelResolution::Unresolved(kind) => {
            let name = TargetName::new(target);
            let display_escaped =
                html_escape::encode_text(label_link_text(index, title, &name, target));
            let target_escaped = html_escape::encode_text(target);
            let _ = write!(
                html,
                "<a href=\"#{target_escaped}\" class=\"broken-link\">{display_escaped}</a>"
            );
            broken_links.push(BrokenLink {
                kind,
                target: target.to_string(),
                span,
            });
        }
    }
}

/// Where the `:ref:` to `target`, written in the page at `doc_path`, leads —
/// `None` when [`render_inline_reference`] would draw it broken.
pub(super) fn label_target(
    index: &ProjectIndex,
    target: &str,
    inventory: &InventorySelector,
    doc_path: &str,
) -> Option<ReferenceTarget> {
    match resolve_label(index, target, inventory) {
        LabelResolution::Local {
            name,
            doc_path: target_doc,
        } => Some(label_reference_target(index, &name, target, target_doc)),
        LabelResolution::SpecialPage(page) => Some(special_page_target(page)),
        LabelResolution::External(hit) => Some(ReferenceTarget::external(
            None,
            hit.inventory,
            hit.target,
            doc_path,
        )),
        LabelResolution::Unresolved(_) => None,
    }
}

/// The label `name`, written `target` and defined in `target_doc`, as a
/// reference target — shared with `:any:`, which finds labels the same way.
pub(super) fn label_reference_target(
    index: &ProjectIndex,
    name: &TargetName,
    target: &str,
    target_doc: &str,
) -> ReferenceTarget {
    ReferenceTarget::in_document(
        label_link_text(index, None, name, target),
        target_doc,
        Some(index.target_anchor(name).to_string()),
    )
}

/// A page the build writes as a reference target — shared with `:any:`.
pub(super) fn special_page_target(page: SpecialPage) -> ReferenceTarget {
    ReferenceTarget {
        title: page.title.to_string(),
        destination: Destination::GeneratedPage {
            path: page.path.to_string(),
        },
    }
}

/// Writes the link to a special page — shown by the explicit `title` when
/// one was written, else by the title Sphinx gives the page. The page lives
/// at the site root, so the href climbs out of `doc_path`'s directory.
pub(super) fn write_special_page_link(
    html: &mut String,
    page: SpecialPage,
    title: Option<&str>,
    doc_path: &str,
) {
    let href = crate::css_relative_path(doc_path, page.path);
    let _ = write!(
        html,
        "<a href=\"{}\">{}</a>",
        html_escape::encode_double_quoted_attribute(&href),
        html_escape::encode_text(title.unwrap_or(page.title))
    );
}

/// The text a link to the label `name` shows: the explicit `title` when one
/// was written, else the title of the section the label sits above, else the
/// label as written — a label on anything but a heading has no title to show.
pub(super) fn label_link_text<'a>(
    index: &'a ProjectIndex,
    title: Option<&'a str>,
    name: &TargetName,
    target: &'a str,
) -> &'a str {
    title
        .or_else(|| index.target_titles.get(name).map(String::as_str))
        .unwrap_or(target)
}

/// The href a page at `doc_path` links the label `name`, defined in
/// `target_doc`, by.
pub(super) fn label_href(
    index: &ProjectIndex,
    name: &TargetName,
    target_doc: &str,
    doc_path: &str,
) -> String {
    format!(
        "{}#{}",
        relative_doc_href(target_doc, doc_path),
        index.target_anchor(name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_inline_reference_shows_the_section_title_for_a_bare_label() {
        // Given — a label written above a heading titled "Installing"
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "guide.rst".to_string());
        index
            .target_titles
            .insert(TargetName::new("install"), "Installing".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: None,
                target: "install",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"guide.html#install\">Installing</a>");
    }

    #[test]
    fn test_render_inline_reference_prefers_an_explicit_title_over_the_section_title() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "guide.rst".to_string());
        index
            .target_titles
            .insert(TargetName::new("install"), "Installing".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: Some("the setup"),
                target: "install",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"guide.html#install\">the setup</a>");
    }

    #[test]
    fn test_render_inline_reference_links_to_a_recorded_anchor() {
        // Given — an entity, whose anchor is `entity-<id>` rather than its name
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("REQ_001"), "reqs.rst".to_string());
        index
            .target_anchors
            .insert(TargetName::new("REQ_001"), "entity-REQ_001".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: None,
                target: "REQ_001",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"reqs.html#entity-REQ_001\">REQ_001</a>");
    }

    fn render_against(
        index: &ProjectIndex,
        title: Option<&str>,
        target: &str,
    ) -> (String, Vec<BrokenLink>) {
        render_selecting(index, title, target, &InventorySelector::Any)
    }

    fn render_selecting(
        index: &ProjectIndex,
        title: Option<&str>,
        target: &str,
        inventory: &InventorySelector,
    ) -> (String, Vec<BrokenLink>) {
        let mut html = String::new();
        let mut broken_links = Vec::new();
        render_inline_reference(
            &mut html,
            LabelRef {
                title,
                target,
                span: None,
                inventory,
            },
            index,
            "guide/page.rst",
            &mut broken_links,
        );
        (html, broken_links)
    }

    #[test]
    fn test_render_inline_reference_links_a_label_another_site_defines() {
        // Given
        let index = crate::test_support::index_linking_into_python();

        // When
        let (html, broken_links) = render_against(&index, None, "tut-intro");

        // Then — the title comes from that site's inventory
        assert_eq!(
            html,
            "<a class=\"reference external\" \
             href=\"https://docs.python.org/3/tutorial/introduction.html#tut-intro\" \
             title=\"(in Python v3.12)\">An Informal Introduction to Python</a>"
        );
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_render_inline_reference_keeps_an_explicit_title_for_an_external_label() {
        // Given
        let index = crate::test_support::index_linking_into_python();

        // When
        let (html, _) = render_against(&index, Some("the tutorial"), "tut-intro");

        // Then
        assert!(html.contains(">the tutorial</a>"), "{html}");
    }

    #[test]
    fn test_render_inline_reference_resolves_an_inventory_prefix() {
        // Given
        let index = crate::test_support::index_linking_into_python();

        // When
        let (html, broken_links) = render_against(&index, None, "python:tut-intro");

        // Then
        assert!(
            html.contains("tutorial/introduction.html#tut-intro"),
            "{html}"
        );
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_render_inline_reference_prefers_a_local_label_over_an_external_one() {
        // Given — this site defines `tut-intro` too
        let mut index = crate::test_support::index_linking_into_python();
        index
            .targets
            .insert(TargetName::new("tut-intro"), "tutorial.rst".to_string());

        // When
        let (html, _) = render_against(&index, None, "tut-intro");

        // Then
        assert_eq!(html, "<a href=\"../tutorial.html#tut-intro\">tut-intro</a>");
    }

    #[test]
    fn test_render_inline_reference_skips_a_local_label_when_external() {
        // Given — this site defines `tut-intro`, but the role says `:external:`
        let mut index = crate::test_support::index_linking_into_python();
        index
            .targets
            .insert(TargetName::new("tut-intro"), "tutorial.rst".to_string());

        // When
        let (html, _) =
            render_selecting(&index, None, "tut-intro", &InventorySelector::ExternalOnly);

        // Then
        assert!(html.contains("docs.python.org"), "{html}");
    }

    #[test]
    fn test_render_inline_reference_reports_an_external_label_as_broken_without_local_fallback() {
        // Given — only this site defines `local-only`
        let mut index = crate::test_support::index_linking_into_python();
        index
            .targets
            .insert(TargetName::new("local-only"), "here.rst".to_string());

        // When
        let (_, broken_links) =
            render_selecting(&index, None, "local-only", &InventorySelector::ExternalOnly);

        // Then
        assert_eq!(broken_links[0].kind, BrokenLinkKind::Reference);
    }

    #[test]
    fn test_render_inline_reference_reports_an_undeclared_inventory_by_name() {
        // Given
        let index = crate::test_support::index_linking_into_python();
        let selector = InventorySelector::Named(rinx_ast::InventoryName::new("numpy").unwrap());

        // When
        let (_, broken_links) = render_selecting(&index, None, "tut-intro", &selector);

        // Then
        assert_eq!(
            broken_links[0].kind,
            BrokenLinkKind::UnknownInventory(rinx_ast::InventoryName::new("numpy").unwrap())
        );
    }

    #[test]
    fn test_render_inline_reference_reports_a_label_no_site_defines() {
        // Given
        let index = crate::test_support::index_linking_into_python();

        // When
        let (_, broken_links) = render_against(&index, None, "nowhere");

        // Then
        assert_eq!(broken_links.len(), 1);
    }

    #[test]
    fn test_render_inline_reference_escapes_a_section_title() {
        // Given — a title holding markup-significant characters
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("cmp"), "guide.rst".to_string());
        index
            .target_titles
            .insert(TargetName::new("cmp"), "a < b".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: None,
                target: "cmp",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert!(html.contains(">a &lt; b</a>"));
    }

    #[test]
    fn test_render_inline_reference_resolved_internal_target() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("my-section"), "other.rst".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: None,
                target: "my-section",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"other.html#my-section\">my-section</a>");
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_render_inline_reference_broken_link_when_target_missing() {
        // Given
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: None,
                target: "missing",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(
            html,
            "<a href=\"#missing\" class=\"broken-link\">missing</a>"
        );
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::Reference,
                target: "missing".to_string(),
                span: None,
            }]
        );
    }

    #[test]
    fn test_render_inline_reference_resolves_cross_directory_path() {
        // Given — document in a subdir, target in another subdir
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("target-a"), "team_a/index.rst".to_string());
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: None,
                target: "target-a",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "team_b/index.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(
            html,
            "<a href=\"../team_a/index.html#target-a\">target-a</a>"
        );
    }

    #[test]
    fn test_render_inline_reference_custom_display_differs_from_target() {
        // Given — an explicit-title `:ref:`, mirroring CPython's
        // `:ref:`GenericAlias <types-genericalias>``
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("types-genericalias"),
            "stdtypes.rst".to_string(),
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: Some("GenericAlias"),
                target: "types-genericalias",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(
            html,
            "<a href=\"stdtypes.html#types-genericalias\">GenericAlias</a>"
        );
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_render_inline_reference_custom_display_broken_link_reports_real_target() {
        // Given — the display text and target both appear only via their
        // respective fields, not concatenated together in the warning.
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_reference(
            &mut html,
            LabelRef {
                title: Some("GenericAlias"),
                target: "types-genericalias",
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(
            html,
            "<a href=\"#types-genericalias\" class=\"broken-link\">GenericAlias</a>"
        );
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::Reference,
                target: "types-genericalias".to_string(),
                span: None,
            }]
        );
    }

    /// Renders `:ref:` to `target`, with an optional explicit `title`, from
    /// the page `doc_path`, returning the HTML and what was reported broken.
    fn render_label(
        index: &ProjectIndex,
        title: Option<&str>,
        target: &str,
        doc_path: &str,
    ) -> (String, Vec<BrokenLink>) {
        let mut html = String::new();
        let mut broken_links = Vec::new();
        render_inline_reference(
            &mut html,
            LabelRef {
                title,
                target,
                span: None,
                inventory: &rinx_ast::InventorySelector::Any,
            },
            index,
            doc_path,
            &mut broken_links,
        );
        (html, broken_links)
    }

    #[test]
    fn test_render_inline_reference_links_the_general_index_by_its_builtin_label() {
        // Given — nothing defines `genindex`; Sphinx predefines it
        let index = ProjectIndex::default();

        // When — from a page one directory deep
        let (html, broken) = render_label(&index, None, "genindex", "guide/intro.rst");

        // Then
        assert_eq!(html, "<a href=\"../genindex.html\">Index</a>");
        assert!(broken.is_empty());
    }

    #[test]
    fn test_render_inline_reference_links_the_module_index_when_the_site_writes_it() {
        // Given
        let index = ProjectIndex {
            domain_indices: [rinx_index::DomainIndex::PyModindex].into(),
            ..ProjectIndex::default()
        };

        // When
        let (short, _) = render_label(&index, None, "modindex", "intro.rst");
        let (long, _) = render_label(&index, Some("all modules"), "py-modindex", "intro.rst");

        // Then
        assert_eq!(short, "<a href=\"py-modindex.html\">Module Index</a>");
        assert_eq!(long, "<a href=\"py-modindex.html\">all modules</a>");
    }

    #[test]
    fn test_render_inline_reference_reports_a_module_index_the_site_does_not_write() {
        // Given
        let index = ProjectIndex::default();

        // When
        let (_, broken) = render_label(&index, None, "py-modindex", "intro.rst");

        // Then — broken, where Sphinx would link a page it never wrote
        assert_eq!(broken.len(), 1);
    }

    #[test]
    fn test_render_inline_reference_prefers_a_label_a_document_defines() {
        // Given — a document defines its own `genindex` label
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("genindex"), "guide.rst".to_string());

        // When
        let (html, _) = render_label(&index, None, "genindex", "intro.rst");

        // Then
        assert_eq!(html, "<a href=\"guide.html#genindex\">genindex</a>");
    }
}

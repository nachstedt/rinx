//! Named `:ref:` reference rendering.

use std::fmt::Write as _;

use rinx_ast::{InventorySelector, Span, TargetName};
use rinx_index::relative_doc_href;
use rinx_index::{ProjectIndex, TargetLocation};

use super::external_link::write_external_link;
use crate::resolution::{resolve_external, unresolved_kind};
use crate::{BrokenLink, BrokenLinkKind};

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
    let target_name = TargetName::new(target);
    let local = match index.targets.get(&target_name) {
        Some(TargetLocation::Internal(target_path)) if inventory.allows_local() => {
            Some(target_path)
        }
        _ => None,
    };
    if local.is_none()
        && let Some(hit) = resolve_external(
            &index.external_inventories,
            &["std:label".to_string()],
            target,
            inventory,
        )
    {
        let display = title.unwrap_or_else(|| hit.target.display_text());
        write_external_link(html, &hit, doc_path, &html_escape::encode_text(display));
        return;
    }
    let display = label_link_text(index, title, &target_name, target);
    let display_escaped = html_escape::encode_text(display);
    if let Some(target_path) = local {
        let href = label_href(index, &target_name, target_path, doc_path);
        let href_attr = html_escape::encode_double_quoted_attribute(&href);
        let _ = write!(html, "<a href=\"{href_attr}\">{display_escaped}</a>");
    } else {
        let target_escaped = html_escape::encode_text(target);
        let _ = write!(
            html,
            "<a href=\"#{target_escaped}\" class=\"broken-link\">{display_escaped}</a>"
        );
        broken_links.push(BrokenLink {
            kind: unresolved_kind(
                inventory,
                &index.external_inventories,
                BrokenLinkKind::Reference,
            ),
            target: target.to_string(),
            span,
        });
    }
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
        index.targets.insert(
            TargetName::new("install"),
            TargetLocation::Internal("guide.rst".to_string()),
        );
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
        index.targets.insert(
            TargetName::new("install"),
            TargetLocation::Internal("guide.rst".to_string()),
        );
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
        index.targets.insert(
            TargetName::new("REQ_001"),
            TargetLocation::Internal("reqs.rst".to_string()),
        );
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
        index.targets.insert(
            TargetName::new("tut-intro"),
            TargetLocation::Internal("tutorial.rst".to_string()),
        );

        // When
        let (html, _) = render_against(&index, None, "tut-intro");

        // Then
        assert_eq!(html, "<a href=\"../tutorial.html#tut-intro\">tut-intro</a>");
    }

    #[test]
    fn test_render_inline_reference_skips_a_local_label_when_external() {
        // Given — this site defines `tut-intro`, but the role says `:external:`
        let mut index = crate::test_support::index_linking_into_python();
        index.targets.insert(
            TargetName::new("tut-intro"),
            TargetLocation::Internal("tutorial.rst".to_string()),
        );

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
        index.targets.insert(
            TargetName::new("local-only"),
            TargetLocation::Internal("here.rst".to_string()),
        );

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
        index.targets.insert(
            TargetName::new("cmp"),
            TargetLocation::Internal("guide.rst".to_string()),
        );
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
        index.targets.insert(
            TargetName::new("my-section"),
            TargetLocation::Internal("other.rst".to_string()),
        );
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
        index.targets.insert(
            TargetName::new("target-a"),
            TargetLocation::Internal("team_a/index.rst".to_string()),
        );
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
            TargetLocation::Internal("stdtypes.rst".to_string()),
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
}

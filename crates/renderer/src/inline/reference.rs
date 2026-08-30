//! Named `:ref:` reference rendering.

use std::fmt::Write as _;

use super::RefText;
use rusty_sphinx_ast::TargetName;
use rusty_sphinx_index::{ProjectIndex, TargetLocation};

use crate::{BrokenLink, BrokenLinkKind};

/// Renders a named `:ref:` reference. Resolves `target` via the project
/// index and emits a relative HTML link showing `display` as the link text
/// (equal to `target` unless the role used the explicit-title syntax,
/// e.g. `` :ref:`Display text <target>` ``), or a broken-link fallback if
/// `target` is not found.
pub(super) fn render_inline_reference(
    html: &mut String,
    reference: RefText<'_>,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let RefText {
        display,
        target,
        span,
    } = reference;
    let display_escaped = html_escape::encode_text(display);
    let target_name = TargetName::new(target);
    if let Some(TargetLocation::Internal(target_path)) = index.targets.get(&target_name) {
        let current_dir = std::path::Path::new(doc_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let target_html_path = std::path::Path::new(target_path).with_extension("html");
        let relative_path =
            pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
        let href = format!("{}#{}", relative_path.display(), target_name.as_str());
        let href_attr = html_escape::encode_double_quoted_attribute(&href);
        let _ = write!(html, "<a href=\"{href_attr}\">{display_escaped}</a>");
    } else {
        let target_escaped = html_escape::encode_text(target);
        let _ = write!(
            html,
            "<a href=\"#{target_escaped}\" class=\"broken-link\">{display_escaped}</a>"
        );
        broken_links.push(BrokenLink {
            kind: BrokenLinkKind::Reference,
            target: target.to_string(),
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            RefText {
                display: "my-section",
                target: "my-section",
                span: None,
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
            RefText {
                display: "missing",
                target: "missing",
                span: None,
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
            RefText {
                display: "target-a",
                target: "target-a",
                span: None,
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
            RefText {
                display: "GenericAlias",
                target: "types-genericalias",
                span: None,
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
            RefText {
                display: "GenericAlias",
                target: "types-genericalias",
                span: None,
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

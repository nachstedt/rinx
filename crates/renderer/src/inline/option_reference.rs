//! `:option:` cross-reference rendering.

use std::fmt::Write as _;

use super::RefText;
use rinx_ast::{InventorySelector, ObjectType, StdObjectType};

use super::domain_object_reference::{domain_object_href, domain_object_reference_target};
use super::external_link::write_external_link;
use crate::resolution::{OptionResolution, OptionResolver, unresolved_kind};
use crate::{BrokenLink, BrokenLinkKind, ReferenceTarget};

/// Renders a `:option:` cross-reference.
///
/// Delegates the search to [`crate::resolution::option`] (its module doc
/// comment documents the ambient-program/global-fallback/embedded-program
/// search order) and only decides here what each outcome looks like on the
/// page — the same split [`super::domain_object_reference::render_inline_domain_object_reference`]
/// makes for `:func:`/`:py:func:`/etc.
pub(super) fn render_inline_option_reference(
    html: &mut String,
    reference: RefText<'_>,
    inventory: &InventorySelector,
    resolver: &OptionResolver<'_>,
    ambient_program: Option<&str>,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let RefText {
        display,
        target,
        span,
    } = reference;
    let display_escaped = html_escape::encode_text(display);
    let literal =
        format!("<code class=\"xref std cmdoption docutils literal\">{display_escaped}</code>");

    match resolver.resolve(ambient_program, target, inventory) {
        OptionResolution::Resolved {
            qualified_name,
            doc_path: target_doc_path,
        } => {
            let href = domain_object_href(
                ObjectType::Std(StdObjectType::Cmdoption),
                &qualified_name,
                target_doc_path,
                doc_path,
            );
            let href_attr = html_escape::encode_double_quoted_attribute(&href);
            let _ = write!(
                html,
                "<a class=\"reference internal\" href=\"{href_attr}\">{literal}</a>"
            );
        }
        OptionResolution::External(hit) => {
            write_external_link(html, &hit, doc_path, &literal);
        }
        OptionResolution::Contested { documents } => {
            let _ = write!(html, "<a href=\"#\" class=\"broken-link\">{literal}</a>");
            broken_links.push(BrokenLink {
                kind: BrokenLinkKind::AmbiguousTarget { documents },
                target: target.to_string(),
                span,
            });
        }
        OptionResolution::NotFound => {
            let _ = write!(html, "<a href=\"#\" class=\"broken-link\">{literal}</a>");
            broken_links.push(BrokenLink {
                kind: unresolved_kind(
                    inventory,
                    resolver.external_inventories(),
                    BrokenLinkKind::OptionReference,
                ),
                target: target.to_string(),
                span,
            });
        }
    }
}

/// Where the `:option:` to `target`, written under `ambient_program` in the
/// page at `doc_path`, leads — `None` whenever
/// [`render_inline_option_reference`] would draw it broken.
pub(super) fn option_target(
    target: &str,
    inventory: &InventorySelector,
    resolver: &OptionResolver<'_>,
    ambient_program: Option<&str>,
    doc_path: &str,
) -> Option<ReferenceTarget> {
    match resolver.resolve(ambient_program, target, inventory) {
        OptionResolution::Resolved {
            qualified_name,
            doc_path: target_doc,
        } => Some(domain_object_reference_target(
            ObjectType::Std(StdObjectType::Cmdoption),
            &qualified_name,
            target_doc,
        )),
        OptionResolution::External(hit) => Some(ReferenceTarget::external(
            None,
            hit.inventory,
            hit.target,
            doc_path,
        )),
        OptionResolution::Contested { .. } | OptionResolution::NotFound => None,
    }
}

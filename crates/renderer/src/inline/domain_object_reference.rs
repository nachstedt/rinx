//! Domain object cross-reference (`:func:`, `:py:func:`, `:c:func:`, ...) rendering.

#[cfg(test)]
mod resolution_tests;
#[cfg(test)]
mod scope_tests;

use std::fmt::Write as _;

use rusty_sphinx_ast::ObjectType;

use crate::resolution::{DomainObjectResolution, DomainObjectResolver};
use crate::{BrokenLink, BrokenLinkKind, ObjectTypeMismatch};

/// The fields of `InlineNode::DomainObjectReference` needed to render it,
/// bundled to keep [`render_inline_domain_object_reference`] within clippy's
/// argument-count limit.
#[derive(Clone, Copy)]
pub(super) struct DomainObjectRef<'a> {
    pub object_type: ObjectType,
    pub name: &'a str,
    pub display: &'a str,
    pub link: bool,
    pub search_order: rusty_sphinx_ast::TargetSearchOrder,
}

/// Mutable diagnostic sinks for [`render_inline_domain_object_reference`],
/// bundled (like `DomainObjectRef` bundles its inputs) to keep the function
/// within clippy's argument-count limit.
pub(super) struct DomainObjectDiagnostics<'a> {
    pub broken_links: &'a mut Vec<BrokenLink>,
    pub object_type_mismatches: &'a mut Vec<ObjectTypeMismatch>,
}

/// Renders a domain object cross-reference (`:func:`, `:py:func:`, `:c:func:`).
///
/// Delegates the search itself to [`crate::resolution::domain_object`] — which order
/// names are tried in, what counts as a type match, and how a dot-prefixed
/// target falls back to a suffix search are all documented there — and only
/// decides here what each outcome looks like on the page:
///
/// - resolved: a relative link whose anchor uses the *matched* object type
///   and the qualified name that actually matched, not the text the author
///   wrote. A type that differs from the requested one additionally records
///   an [`ObjectTypeMismatch`]; the reference still works and never fails
///   `--strict-links`.
/// - ambiguous or not found: the broken-link fallback plus a [`BrokenLink`],
///   the ambiguous case carrying the candidates it could not choose between.
///
/// When `link` is `false` (the role target used a `!` prefix), the index is
/// never consulted — the target is rendered as plain text with no hyperlink
/// and no broken-link fallback, matching Sphinx's "suppress cross-reference"
/// semantics.
pub(super) fn render_inline_domain_object_reference(
    html: &mut String,
    obj_ref: DomainObjectRef<'_>,
    resolver: &DomainObjectResolver<'_>,
    doc_path: &str,
    diagnostics: &mut DomainObjectDiagnostics<'_>,
    scope: &rusty_sphinx_scope::Scope,
) {
    let DomainObjectRef {
        object_type,
        name,
        display,
        link,
        search_order,
    } = obj_ref;
    let display_escaped = html_escape::encode_text(display);
    let domain_str = object_type.domain().as_str();
    let objtype_str = object_type.as_str();
    let literal = format!(
        "<code class=\"xref {domain_str} {objtype_str} docutils literal\">{display_escaped}</code>"
    );

    if !link {
        html.push_str(&literal);
        return;
    }

    let mut render_unresolved = |kind| {
        let _ = write!(html, "<a href=\"#\" class=\"broken-link\">{literal}</a>");
        diagnostics.broken_links.push(BrokenLink {
            kind,
            target: name.to_string(),
        });
    };

    match resolver.resolve(scope, object_type, name, search_order) {
        DomainObjectResolution::Resolved {
            object_type: matched_type,
            qualified_name,
            doc_path: target_doc_path,
        } => {
            if matched_type != object_type {
                diagnostics.object_type_mismatches.push(ObjectTypeMismatch {
                    name: qualified_name.clone(),
                    requested_type: object_type,
                    resolved_type: matched_type,
                });
            }
            let anchor = rusty_sphinx_ast::build_domain_object_key(matched_type, &qualified_name);
            let current_dir = std::path::Path::new(doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let target_html_path = std::path::Path::new(target_doc_path).with_extension("html");
            let relative_path =
                pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
            let href = format!("{}#{}", relative_path.display(), anchor.as_str());
            let href_attr = html_escape::encode_double_quoted_attribute(&href);
            let _ = write!(
                html,
                "<a class=\"reference internal\" href=\"{href_attr}\">{literal}</a>"
            );
        }
        DomainObjectResolution::Ambiguous { candidates } => {
            render_unresolved(BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type,
                candidates,
            });
        }
        DomainObjectResolution::NotFound => {
            render_unresolved(BrokenLinkKind::DomainObjectReference(object_type));
        }
    }
}

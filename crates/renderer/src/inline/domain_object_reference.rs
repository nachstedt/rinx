//! Domain object cross-reference (`:func:`, `:py:func:`, `:c:func:`, ...) rendering.

#[cfg(test)]
mod resolution_tests;
#[cfg(test)]
mod scope_tests;

use std::fmt::Write as _;

use rinx_ast::{InventorySelector, ObjectType};

use super::external_link::write_external_link;
use crate::resolution::{DomainObjectResolution, DomainObjectResolver, unresolved_kind};
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
    pub search_order: rinx_ast::TargetSearchOrder,
    /// Where the role was written, carried through so a broken or ambiguous
    /// reference can name its own line rather than just the document.
    pub span: Option<rinx_ast::Span>,
    /// Which sites may define the object — see [`InventorySelector`].
    pub inventory: &'a InventorySelector,
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
    scope: &rinx_scope::Scope,
) {
    let DomainObjectRef {
        object_type,
        name,
        display,
        link,
        search_order,
        span,
        inventory,
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
            span,
        });
    };

    match resolver.resolve(scope, object_type, name, search_order, inventory) {
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
                    span,
                });
            }
            let href = domain_object_href(matched_type, &qualified_name, target_doc_path, doc_path);
            let href_attr = html_escape::encode_double_quoted_attribute(&href);
            let title_attr =
                module_title_attribute(resolver.index(), matched_type, &qualified_name);
            let _ = write!(
                html,
                "<a class=\"reference internal\" href=\"{href_attr}\"{title_attr}>{literal}</a>"
            );
        }
        DomainObjectResolution::External(hit) => {
            write_external_link(html, &hit, doc_path, &literal);
        }
        DomainObjectResolution::Ambiguous { candidates } => {
            render_unresolved(BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type,
                candidates,
            });
        }
        DomainObjectResolution::NotFound => {
            render_unresolved(unresolved_kind(
                inventory,
                resolver.external_inventories(),
                BrokenLinkKind::DomainObjectReference(object_type),
            ));
        }
    }
}

/// The href a page at `doc_path` links the object `qualified_name` of
/// `object_type`, defined in `target_doc`, by — `:option:` included, whose
/// definitions are domain objects of their own.
pub(super) fn domain_object_href(
    object_type: ObjectType,
    qualified_name: &str,
    target_doc: &str,
    doc_path: &str,
) -> String {
    format!(
        "{}#{}",
        rinx_index::relative_doc_href(target_doc, doc_path),
        rinx_ast::build_domain_object_key(object_type, qualified_name).as_str()
    )
}

/// The ` title="…"` attribute a link to the object `qualified_name` of
/// `object_type` carries: a module's tooltip (see [`module_link_title`]) for a
/// module the index records, and nothing for anything else — Sphinx titles
/// only the links `_make_module_refnode` builds. Shared by `:mod:` and by an
/// `:any:` landing on a module, as Sphinx's `resolve_any_xref` shares it.
pub(super) fn module_title_attribute(
    index: &rinx_index::ProjectIndex,
    object_type: ObjectType,
    qualified_name: &str,
) -> String {
    if object_type != ObjectType::Py(rinx_ast::PyObjectType::Module) {
        return String::new();
    }
    let key = rinx_ast::TargetName::new(qualified_name);
    index.modules.get(&key).map_or_else(String::new, |entry| {
        let title = module_link_title(index.domain_object_spelling(&key), entry);
        format!(
            " title=\"{}\"",
            html_escape::encode_double_quoted_attribute(&title)
        )
    })
}

/// A module link's tooltip, exactly as Sphinx's `_make_module_refnode` builds
/// it: the name, then `: synopsis`, ` (deprecated)` and ` (platform)` for
/// whichever of them the module wrote, in that order.
pub(super) fn module_link_title(name: &str, entry: &rinx_index::ModuleEntry) -> String {
    let mut title = name.to_string();
    if let Some(synopsis) = &entry.synopsis {
        let _ = write!(title, ": {synopsis}");
    }
    if entry.deprecated {
        title.push_str(" (deprecated)");
    }
    if let Some(platform) = &entry.platform {
        let _ = write!(title, " ({platform})");
    }
    title
}

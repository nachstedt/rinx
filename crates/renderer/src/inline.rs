//! Inline node rendering helpers.

use crate::{BrokenLink, BrokenLinkKind, ObjectTypeMismatch, RenderCtx};
use rusty_sphinx_analyzer::{ProjectIndex, TargetLocation};
use rusty_sphinx_ast::{ObjectType, TargetName};
use std::fmt::Write as _;

/// Renders a single inline node into `html`.
pub(super) fn render_inline(
    html: &mut String,
    inline: &rusty_sphinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    match inline {
        rusty_sphinx_ast::InlineNode::Text(text) => {
            let _ = write!(html, "{}", html_escape::encode_text(text));
        }
        rusty_sphinx_ast::InlineNode::Reference { display, target } => {
            render_inline_reference(
                html,
                display,
                target,
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::Hyperlink { text, target } => {
            render_inline_hyperlink(
                html,
                text,
                target,
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::AnonymousReference(text) => {
            render_inline_anonymous_reference(
                html,
                text,
                ctx.anon_targets,
                ctx.anon_index,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::AnonymousHyperlink { text, target } => {
            let text_escaped = html_escape::encode_text(text);
            let target_attr = html_escape::encode_double_quoted_attribute(target);
            let _ = write!(html, "<a href=\"{target_attr}\">{text_escaped}</a>");
        }
        rusty_sphinx_ast::InlineNode::Emphasis(text) => {
            let _ = write!(html, "<em>{}</em>", html_escape::encode_text(text));
        }
        rusty_sphinx_ast::InlineNode::Strong(text) => {
            let _ = write!(html, "<strong>{}</strong>", html_escape::encode_text(text));
        }
        rusty_sphinx_ast::InlineNode::Literal(text) => {
            let _ = write!(html, "<code>{}</code>", html_escape::encode_text(text));
        }
        rusty_sphinx_ast::InlineNode::Program(text) => {
            let _ = write!(
                html,
                "<strong class=\"program\">{}</strong>",
                html_escape::encode_text(text)
            );
        }
        rusty_sphinx_ast::InlineNode::TermReference { display, term } => {
            render_inline_term_reference(
                html,
                display,
                term,
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::DomainObjectReference {
            object_type,
            name,
            display,
            link,
        } => {
            render_inline_domain_object_reference(
                html,
                DomainObjectRef {
                    object_type: *object_type,
                    name,
                    display,
                    link: *link,
                },
                ctx.index,
                ctx.doc_path,
                &mut DomainObjectDiagnostics {
                    broken_links: ctx.broken_links,
                    object_type_mismatches: ctx.object_type_mismatches,
                },
                &ctx.python_scope,
            );
        }
    }
}

/// Renders a named `:ref:` reference. Resolves `target` via the project
/// index and emits a relative HTML link showing `display` as the link text
/// (equal to `target` unless the role used the explicit-title syntax,
/// e.g. `` :ref:`Display text <target>` ``), or a broken-link fallback if
/// `target` is not found.
pub(super) fn render_inline_reference(
    html: &mut String,
    display: &str,
    target: &str,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
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
        });
    }
}

/// Renders a named hyperlink. Resolution order:
/// 1. Direct URI (http/https/mailto) — emitted as-is.
/// 2. External target in the project index — emitted as an external link.
/// 3. Internal target in the project index — converted to a relative HTML href.
/// 4. No match — broken-link fallback.
pub(super) fn render_inline_hyperlink(
    html: &mut String,
    text: &str,
    target: &str,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let text_escaped = html_escape::encode_text(text);
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
    {
        let target_attr = html_escape::encode_double_quoted_attribute(target);
        let _ = write!(html, "<a href=\"{target_attr}\">{text_escaped}</a>");
        return;
    }

    let target_name = TargetName::new(target);
    match index.targets.get(&target_name) {
        Some(TargetLocation::External(url)) => {
            let url_attr = html_escape::encode_double_quoted_attribute(url);
            let _ = write!(html, "<a href=\"{url_attr}\">{text_escaped}</a>");
        }
        Some(TargetLocation::Internal(target_path)) => {
            let current_dir = std::path::Path::new(doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let target_html_path = std::path::Path::new(target_path).with_extension("html");
            let relative_path =
                pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
            let href = format!("{}#{}", relative_path.display(), target_name.as_str());
            let href_attr = html_escape::encode_double_quoted_attribute(&href);
            let _ = write!(html, "<a href=\"{href_attr}\">{text_escaped}</a>");
        }
        None => {
            let _ = write!(
                html,
                "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
            );
            broken_links.push(BrokenLink {
                kind: BrokenLinkKind::Hyperlink,
                target: target.to_string(),
            });
        }
    }
}

/// Renders an anonymous `__` reference by consuming the next URI from `anon_targets`.
/// Emits a broken-link fallback if the anonymous target list is exhausted.
pub(super) fn render_inline_anonymous_reference(
    html: &mut String,
    text: &str,
    anon_targets: &[String],
    anon_index: &mut usize,
    broken_links: &mut Vec<BrokenLink>,
) {
    let text_escaped = html_escape::encode_text(text);
    if let Some(uri) = anon_targets.get(*anon_index) {
        let uri_attr = html_escape::encode_double_quoted_attribute(uri);
        let _ = write!(html, "<a href=\"{uri_attr}\">{text_escaped}</a>");
        *anon_index += 1;
    } else {
        let _ = write!(
            html,
            "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
        );
        broken_links.push(BrokenLink {
            kind: BrokenLinkKind::AnonymousReference,
            target: text.to_string(),
        });
    }
}

/// Renders a glossary term reference (`:term:`). Resolves the term via the project
/// index and emits a relative link with the appropriate CSS classes, or a
/// broken-link fallback if the term is not found in the index.
pub(super) fn render_inline_term_reference(
    html: &mut String,
    display: &str,
    term: &str,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let display_escaped = html_escape::encode_text(display);
    let term_name = rusty_sphinx_ast::TargetName::new(term);
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
        });
    }
}

/// Yields the qualified names to try, in order, when resolving a domain
/// object cross-reference against `scope`: qualified by the reference's full
/// enclosing scope first (nearest `py:class`/`py:exception`, plus the
/// current `py:module` — only for the `py` domain, real Sphinx's module
/// context never qualifies non-`py` objects), then by the module alone, then
/// the bare, unqualified name as the final global fallback.
///
/// Three tiers, not the two a *definition*'s own qualification needs: a
/// reference can be written from *inside* a narrower scope than the object
/// it targets, e.g. a bare `:exc:` role reading just `ZipImportError`,
/// written inside `.. class:: zipimporter`'s own body, referring to an
/// exception that was itself only ever module-qualified (a sibling of
/// `.. module:: zipimport`, never nested in the class) — the class-qualified
/// guess (`zipimport.zipimporter.ZipImportError`) misses, so resolution must
/// still fall through to the module-qualified one
/// (`zipimport.ZipImportError`) before giving up and trying the bare name.
fn domain_object_scope_candidates(
    scope: &rusty_sphinx_scope::PythonScope,
    domain: rusty_sphinx_ast::Domain,
    name: &str,
) -> [String; 3] {
    let full = scope.qualify(domain, name).qualified_name;
    let mut module_only = scope.clone();
    module_only.truncate_classes(0);
    let module_qualified = module_only.qualify(domain, name).qualified_name;
    let bare = rusty_sphinx_scope::PythonScope::default()
        .qualify(domain, name)
        .qualified_name;
    [full, module_qualified, bare]
}

/// Renders a domain object cross-reference (`:func:`, `:py:func:`, `:c:func:`).
/// Resolves the domain-qualified key via the project index and emits a
/// relative link, or a broken-link fallback if the object is not found.
///
/// Resolution tries the reference's enclosing scope(s) first, from most to
/// least specific (see [`domain_object_scope_candidates`]) — the same
/// `PythonScope` state `render_domain_object` uses to qualify a definition's
/// own anchor `id` — before falling back to the
/// literal, unqualified name. This mirrors real Sphinx: a bare `:meth:` role
/// reading just `find_spec`, written inside `.. class:: zipimporter` (itself
/// a sibling of `.. module:: zipimport`), must resolve against that scope,
/// since it was indexed as `zipimport.zipimporter.find_spec`, not
/// `find_spec`. When no enclosing scope applies (or the reference is
/// already written fully qualified), every qualified attempt is identical to
/// the bare one, so this is a no-op for every previously-working case.
///
/// For each scope candidate, every object type [`ObjectType::role_alias_candidates`]
/// accepts for the requested type is also tried (most-preferred, i.e. the
/// requested type itself, first) — e.g. `CPython` documents `Fault` via
/// `.. class::` but references it via `:exc:`, which real Sphinx resolves
/// because it treats `class`/`exception` as mutually aliasable role targets.
/// The rendered anchor always matches the *matched* object type (what the
/// definition actually rendered), not the originally requested one. When the
/// matched type differs from the requested one, an [`ObjectTypeMismatch`] is
/// recorded in `diagnostics` — the reference still resolves and is not a
/// [`BrokenLink`], but the mismatch is surfaced to the author as a
/// non-fatal warning (never fails `--strict-links`).
///
/// When `link` is `false` (the role target used a `!` prefix), the index is
/// never consulted — the target is rendered as plain text with no hyperlink
/// and no broken-link fallback, matching Sphinx's "suppress cross-reference"
/// semantics.
/// The fields of `InlineNode::DomainObjectReference` needed to render it,
/// bundled to keep [`render_inline_domain_object_reference`] within clippy's
/// argument-count limit.
#[derive(Clone, Copy)]
pub(super) struct DomainObjectRef<'a> {
    pub object_type: ObjectType,
    pub name: &'a str,
    pub display: &'a str,
    pub link: bool,
}

/// Mutable diagnostic sinks for [`render_inline_domain_object_reference`],
/// bundled (like `DomainObjectRef` bundles its inputs) to keep the function
/// within clippy's argument-count limit.
pub(super) struct DomainObjectDiagnostics<'a> {
    pub broken_links: &'a mut Vec<BrokenLink>,
    pub object_type_mismatches: &'a mut Vec<ObjectTypeMismatch>,
}

pub(super) fn render_inline_domain_object_reference(
    html: &mut String,
    obj_ref: DomainObjectRef<'_>,
    index: &ProjectIndex,
    doc_path: &str,
    diagnostics: &mut DomainObjectDiagnostics<'_>,
    scope: &rusty_sphinx_scope::PythonScope,
) {
    let DomainObjectRef {
        object_type,
        name,
        display,
        link,
    } = obj_ref;
    let display_escaped = html_escape::encode_text(display);
    let domain_str = object_type.domain().as_str();
    let objtype_str = object_type.as_str();

    if !link {
        let _ = write!(
            html,
            "<code class=\"xref {domain_str} {objtype_str} docutils literal\">{display_escaped}</code>"
        );
        return;
    }

    let resolved = domain_object_scope_candidates(scope, object_type.domain(), name)
        .into_iter()
        .find_map(|qualified_name| {
            let entries = index
                .domain_objects
                .get(&TargetName::new(&qualified_name))?;
            let matched_type = object_type
                .role_alias_candidates()
                .iter()
                .find(|candidate| entries.contains_key(candidate))?;
            let target_doc_path = entries.get(matched_type)?;
            Some((*matched_type, qualified_name, target_doc_path))
        });

    if let Some((matched_type, qualified_name, target_doc_path)) = resolved {
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
            "<a class=\"reference internal\" href=\"{href_attr}\"><code class=\"xref {domain_str} {objtype_str} docutils literal\">{display_escaped}</code></a>"
        );
    } else {
        let _ = write!(
            html,
            "<a href=\"#\" class=\"broken-link\"><code class=\"xref {domain_str} {objtype_str} docutils literal\">{display_escaped}</code></a>"
        );
        diagnostics.broken_links.push(BrokenLink {
            kind: BrokenLinkKind::DomainObjectReference(object_type),
            target: name.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_analyzer::ProjectIndex;
    use rusty_sphinx_ast::TargetName;

    #[test]
    fn test_domain_object_scope_candidates_yields_class_then_module_then_bare_for_py() {
        // Given
        let mut scope = rusty_sphinx_scope::PythonScope::default();
        scope.set_module("zipimport");
        scope.push_classes(&["zipimporter".to_string()]);

        // When
        let candidates =
            domain_object_scope_candidates(&scope, rusty_sphinx_ast::Domain::Py, "find_spec");

        // Then
        assert_eq!(
            candidates,
            [
                "zipimport.zipimporter.find_spec".to_string(),
                "zipimport.find_spec".to_string(),
                "find_spec".to_string(),
            ]
        );
    }

    #[test]
    fn test_domain_object_scope_candidates_skips_module_for_c_domain_with_no_class_scope() {
        // Given — a `c` domain object referenced with no enclosing class:
        // real Sphinx's module context is `py`-domain-only, so the
        // module-qualified and bare candidates must coincide.
        let mut scope = rusty_sphinx_scope::PythonScope::default();
        scope.set_module("zipimport");

        // When
        let candidates =
            domain_object_scope_candidates(&scope, rusty_sphinx_ast::Domain::C, "PyList_Append");

        // Then
        assert_eq!(
            candidates,
            [
                "PyList_Append".to_string(),
                "PyList_Append".to_string(),
                "PyList_Append".to_string(),
            ]
        );
    }

    #[test]
    fn test_domain_object_scope_candidates_yields_only_bare_with_no_enclosing_scope() {
        // Given / When
        let candidates = domain_object_scope_candidates(
            &rusty_sphinx_scope::PythonScope::default(),
            rusty_sphinx_ast::Domain::Py,
            "greet",
        );

        // Then
        assert_eq!(
            candidates,
            [
                "greet".to_string(),
                "greet".to_string(),
                "greet".to_string()
            ]
        );
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
            "my-section",
            "my-section",
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
            "missing",
            "missing",
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
            "target-a",
            "target-a",
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
            "GenericAlias",
            "types-genericalias",
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
            "GenericAlias",
            "types-genericalias",
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
            }]
        );
    }

    #[test]
    fn test_render_inline_hyperlink_direct_http_uri() {
        // Given
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_hyperlink(
            &mut html,
            "Click here",
            "https://example.com",
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"https://example.com\">Click here</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_direct_mailto_uri() {
        // Given
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_hyperlink(
            &mut html,
            "Email us",
            "mailto:hello@example.com",
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"mailto:hello@example.com\">Email us</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_external_index_target() {
        // Given
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("Python"),
            TargetLocation::External("https://python.org".to_string()),
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_hyperlink(
            &mut html,
            "Python",
            "Python",
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"https://python.org\">Python</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_internal_index_target() {
        // Given
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("my-label"),
            TargetLocation::Internal("other.rst".to_string()),
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_hyperlink(
            &mut html,
            "See other",
            "my-label",
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"other.html#my-label\">See other</a>");
    }

    #[test]
    fn test_render_inline_hyperlink_broken_link_when_not_found() {
        // Given
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_hyperlink(
            &mut html,
            "No target",
            "no-target",
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">No target</a>");
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::Hyperlink,
                target: "no-target".to_string(),
            }]
        );
    }

    #[test]
    fn test_render_inline_anonymous_reference_resolved() {
        // Given
        let anon_targets = vec!["https://example.com".to_string()];
        let mut anon_index = 0;
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_anonymous_reference(
            &mut html,
            "link text",
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"https://example.com\">link text</a>");
        assert_eq!(anon_index, 1);
    }

    #[test]
    fn test_render_inline_anonymous_reference_broken_when_index_exhausted() {
        // Given — no anonymous targets available
        let anon_targets: Vec<String> = vec![];
        let mut anon_index = 0;
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_anonymous_reference(
            &mut html,
            "broken",
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );

        // Then
        assert_eq!(html, "<a href=\"#\" class=\"broken-link\">broken</a>");
        assert_eq!(anon_index, 0);
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::AnonymousReference,
                target: "broken".to_string(),
            }]
        );
    }

    #[test]
    fn test_render_inline_anonymous_reference_advances_index_per_call() {
        // Given — two sequential calls consume targets in order
        let anon_targets = vec![
            "https://first.com".to_string(),
            "https://second.com".to_string(),
        ];
        let mut anon_index = 0;
        let mut html = String::new();
        let mut broken_links = Vec::new();

        // When
        render_inline_anonymous_reference(
            &mut html,
            "first",
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );
        render_inline_anonymous_reference(
            &mut html,
            "second",
            &anon_targets,
            &mut anon_index,
            &mut broken_links,
        );

        // Then
        assert_eq!(
            html,
            "<a href=\"https://first.com\">first</a><a href=\"https://second.com\">second</a>"
        );
        assert_eq!(anon_index, 2);
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
            "widget",
            "widget",
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
            "unknown term",
            "unknown",
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
            "API",
            "api",
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
            "the env",
            "environment",
            &index,
            "doc.rst",
            &mut broken_links,
        );

        // Then
        assert!(html.contains("href=\"glossary.html#term-environment\""));
        assert!(html.contains(">the env<"));
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolved_py_domain() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            "greet",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "greet",
                display: "greet",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("class=\"reference internal\""));
        assert!(html.contains("href=\"api.html#py:function:greet\""));
        assert!(html.contains("class=\"xref py function docutils literal\""));
        assert!(html.contains(">greet<"));
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolved_c_domain() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
            "add",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "add",
                display: "add",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("href=\"api.html#c:function:add\""));
        assert!(html.contains("class=\"xref c function docutils literal\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolved_c_macro() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
            "MAX",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "MAX",
                display: "MAX",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("href=\"api.html#c:macro:max\""));
        assert!(html.contains("class=\"xref c macro docutils literal\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolves_exc_role_to_class_definition() {
        // Given — CPython's `xmlrpc.client.rst` defines `Fault` via
        // `.. class::` but references it via `:exc:`.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Class),
            "Fault",
            "xmlrpc.client.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "Fault",
                display: "Fault",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then — resolved, and the anchor matches the actual definition's
        // object type (`class`), not the role that referenced it (`exc`).
        assert!(broken_links.is_empty());
        assert!(html.contains("class=\"reference internal\""));
        assert!(html.contains("href=\"xmlrpc.client.html#py:class:fault\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_does_not_alias_unrelated_object_types() {
        // Given — `Fault` is defined only as a `py:function`, which has no
        // role-alias relationship with `py:exception`.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            "Fault",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "Fault",
                display: "Fault",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert_eq!(broken_links.len(), 1);
    }

    #[test]
    fn test_render_inline_domain_object_reference_broken_link_for_c_macro() {
        // Given
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "MISSING",
                display: "MISSING",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert!(html.contains(">MISSING<"));
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::DomainObjectReference(ObjectType::C(
                    rusty_sphinx_ast::CObjectType::Macro
                )),
                target: "MISSING".to_string(),
            }]
        );
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolved_py_module() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
            "greetings",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings",
                display: "greetings",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("class=\"reference internal\""));
        assert!(html.contains("href=\"api.html#py:module:greetings\""));
        assert!(html.contains("class=\"xref py module docutils literal\""));
        assert!(html.contains(">greetings<"));
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolved_py_data_via_data_and_const_roles() {
        // Given — a single `.. py:data::` definition registered under its
        // canonical `ObjectType::Py(PyObjectType::Data)` key.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
            "DEFAULT_TIMEOUT",
            "api.rst",
        );

        // When — both `:py:data:` and `:py:const:` roles parse down to the
        // same `ObjectType`, so rendering either must resolve identically.
        let mut data_html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();
        render_inline_domain_object_reference(
            &mut data_html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT",
                display: "DEFAULT_TIMEOUT",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );
        let mut const_html = String::new();
        render_inline_domain_object_reference(
            &mut const_html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT",
                display: "DEFAULT_TIMEOUT",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert_eq!(data_html, const_html);
        assert!(data_html.contains("class=\"reference internal\""));
        assert!(data_html.contains("href=\"api.html#py:data:default_timeout\""));
        assert!(data_html.contains("class=\"xref py data docutils literal\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_broken_link_when_missing() {
        // Given
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "missing",
                display: "missing",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert!(html.contains(">missing<"));
        assert_eq!(
            broken_links,
            vec![BrokenLink {
                kind: BrokenLinkKind::DomainObjectReference(ObjectType::Py(
                    rusty_sphinx_ast::PyObjectType::Function
                )),
                target: "missing".to_string(),
            }]
        );
    }

    #[test]
    fn test_render_inline_domain_object_reference_bare_name_resolves_via_current_module() {
        // Given — an exception indexed under its module-qualified name, the
        // shape `.. exception:: ZipImportError` gets when it's a sibling
        // after `.. module:: zipimport` (see zipimport.rst in the CPython
        // benchmark).
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
            "zipimport.ZipImportError",
            "library/zipimport.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        let mut scope = rusty_sphinx_scope::PythonScope::default();
        scope.set_module("zipimport");

        // When — the reference is written bare, as real Sphinx docs do,
        // relying on `zipimport` being the current module.
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "ZipImportError",
                display: "ZipImportError",
                link: true,
            },
            &index,
            "library/zipimport.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &scope,
        );

        // Then
        assert!(broken_links.is_empty());
        assert!(html.contains("class=\"reference internal\""));
        assert!(html.contains("href=\"zipimport.html#py:exception:zipimport.zipimporterror\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_bare_name_resolves_via_python_scope() {
        // Given — a method indexed under its class-qualified name, the shape
        // `.. method:: find_spec` gets when nested inside
        // `.. class:: zipimporter` (itself a sibling after
        // `.. module:: zipimport`).
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
            "zipimport.zipimporter.find_spec",
            "library/zipimport.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();
        let mut scope = rusty_sphinx_scope::PythonScope::default();
        scope.set_module("zipimport");
        scope.push_classes(&["zipimporter".to_string()]);

        // When — the reference is written bare, resolved against the
        // innermost enclosing class, which wins over the current module.
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
                name: "find_spec",
                display: "find_spec",
                link: true,
            },
            &index,
            "library/zipimport.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &scope,
        );

        // Then
        assert!(broken_links.is_empty());
        assert!(html.contains("href=\"zipimport.html#py:method:zipimport.zipimporter.find_spec\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_falls_back_from_class_to_module_scope() {
        // Given — an exception indexed under its *module*-qualified name
        // only (it's a sibling of `.. module:: zipimport`, never nested in
        // any class), but referenced bare from *inside* a narrower class
        // scope — the real shape of `:exc:`ZipImportError`` written inside
        // `.. class:: zipimporter`'s own body in zipimport.rst. The
        // class-qualified guess (`zipimport.zipimporter.ZipImportError`)
        // must miss and fall through to the module-qualified one, not go
        // straight to the bare global name.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
            "zipimport.ZipImportError",
            "library/zipimport.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();
        let mut scope = rusty_sphinx_scope::PythonScope::default();
        scope.set_module("zipimport");
        scope.push_classes(&["zipimporter".to_string()]);

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
                name: "ZipImportError",
                display: "ZipImportError",
                link: true,
            },
            &index,
            "library/zipimport.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &scope,
        );

        // Then
        assert!(broken_links.is_empty());
        assert!(html.contains("href=\"zipimport.html#py:exception:zipimport.zipimporterror\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_falls_back_to_bare_key_when_scope_unrelated() {
        // Given — a function indexed under its own bare, unqualified name
        // (documented before any `py:module` was in effect), and a reference
        // to it written from within a document whose current module is
        // unrelated.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            "greet",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        let mut scope = rusty_sphinx_scope::PythonScope::default();
        scope.set_module("other_module");

        // When — the qualified attempt ("other_module.greet") misses, so
        // resolution must fall back to the bare key.
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "greet",
                display: "greet",
                link: true,
            },
            &index,
            "api.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &scope,
        );

        // Then
        assert!(broken_links.is_empty());
        assert!(html.contains("href=\"api.html#py:function:greet\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_already_qualified_name_unaffected_by_scope() {
        // Given — a function indexed under its module-qualified name, and a
        // reference that already spells out that qualifier explicitly. The
        // module-qualified candidate ("types.types.coroutine") misses since
        // the module is never absorbed against a repeat in the reference
        // text, but the bare-name candidate ("types.coroutine", the literal
        // text) still resolves — no different from real Sphinx trying the
        // literal name first.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            "types.coroutine",
            "library/types.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();
        let mut scope = rusty_sphinx_scope::PythonScope::default();
        scope.set_module("types");

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "types.coroutine",
                display: "types.coroutine",
                link: true,
            },
            &index,
            "library/types.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &scope,
        );

        // Then
        assert!(broken_links.is_empty());
        assert!(html.contains("href=\"types.html#py:function:types.coroutine\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolves_cross_directory_path() {
        // Given — document is nested, object defined at root
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            "greet",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "greet",
                display: "greet",
                link: true,
            },
            &index,
            "guide/intro.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("href=\"../api.html#py:function:greet\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_suppressed_link_renders_plain_text() {
        // Given — an empty index; a real `!`-suppressed reference never
        // performs a lookup, so this also proves no lookup is attempted.
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "curses",
                display: "curses",
                link: false,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert_eq!(
            html,
            "<code class=\"xref py module docutils literal\">curses</code>"
        );
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_render_inline_domain_object_reference_shortened_display_resolves_via_full_name() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            "greetings.shout",
            "api.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "greetings.shout",
                display: "shout",
                link: true,
            },
            &index,
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::PythonScope::default(),
        );

        // Then
        assert!(html.contains("href=\"api.html#py:function:greetings.shout\""));
        assert!(html.contains(">shout<"));
        assert!(!html.contains("greetings.shout<"));
    }
}

//! Domain object cross-reference (`:func:`, `:py:func:`, `:c:func:`, ...) rendering.

#[cfg(test)]
mod scope_tests;

use std::fmt::Write as _;

use rusty_sphinx_ast::ObjectType;

use crate::domain_resolution::{DomainObjectResolution, DomainObjectResolver};
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
/// Delegates the search itself to [`crate::domain_resolution`] — which order
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::TargetSearchOrder;
    use rusty_sphinx_index::ProjectIndex;

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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
        );

        // Then
        assert!(html.contains("href=\"api.html#c:function:add\""));
        assert!(html.contains("class=\"xref c function docutils literal\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_keeps_call_parens_in_the_text_only() {
        // Given — the two halves the parser split apart for
        // `known_bugs.md` #1's `` :c:func:`Py_TYPE()` ``: a paren-free `name`
        // to key the lookup by, and a `display` that still reads as a call.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
            "Py_TYPE",
            "c-api/object.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "Py_TYPE",
                display: "Py_TYPE()",
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "c-api/refcounting.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
        );

        // Then — the anchor is built from the resolved, paren-free name,
        // while the reader still sees the parens.
        assert!(html.contains("href=\"object.html#c:function:py_type\""));
        assert!(html.contains(">Py_TYPE()</code>"));
        assert!(broken_links.is_empty());
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
        );

        // Then — resolved, and the anchor matches the actual definition's
        // object type (`class`), not the role that referenced it (`exc`).
        assert!(broken_links.is_empty());
        assert!(html.contains("class=\"reference internal\""));
        assert!(html.contains("href=\"xmlrpc.client.html#py:class:fault\""));
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolves_c_func_role_to_macro_definition() {
        // Given — CPython's `c-api/gcsupport.rst` defines the function-like
        // macro `Py_VISIT` via `.. c:macro::` but references it via `:c:func:`.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
            "Py_VISIT",
            "gcsupport.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "Py_VISIT",
                display: "Py_VISIT",
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
        );

        // Then — resolved, anchored on the definition's own object type, and
        // the role/definition disagreement recorded as a soft mismatch.
        assert!(broken_links.is_empty());
        assert!(html.contains("href=\"gcsupport.html#c:macro:py_visit\""));
        assert_eq!(object_type_mismatches.len(), 1);
        assert_eq!(
            object_type_mismatches[0].requested_type,
            ObjectType::C(rusty_sphinx_ast::CObjectType::Function)
        );
        assert_eq!(
            object_type_mismatches[0].resolved_type,
            ObjectType::C(rusty_sphinx_ast::CObjectType::Macro)
        );
    }

    #[test]
    fn test_render_inline_domain_object_reference_resolves_c_macro_role_to_function_definition() {
        // Given — the reverse direction: `Py_REFCNT` is defined
        // `.. c:function::` in `c-api/refcounting.rst` and referenced via
        // `:c:macro:` from `c-api/structures.rst`.
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
            "Py_REFCNT",
            "refcounting.rst",
        );
        let mut html = String::new();
        let mut broken_links = Vec::new();
        let mut object_type_mismatches = Vec::new();

        // When
        render_inline_domain_object_reference(
            &mut html,
            DomainObjectRef {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Macro),
                name: "Py_REFCNT",
                display: "Py_REFCNT",
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
        );

        // Then
        assert!(broken_links.is_empty());
        assert!(html.contains("href=\"refcounting.html#c:function:py_refcnt\""));
        assert_eq!(object_type_mismatches.len(), 1);
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
        );
        let mut const_html = String::new();
        render_inline_domain_object_reference(
            &mut const_html,
            DomainObjectRef {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT",
                display: "DEFAULT_TIMEOUT",
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
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
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            },
            &DomainObjectResolver::new(&index),
            "doc.rst",
            &mut DomainObjectDiagnostics {
                broken_links: &mut broken_links,
                object_type_mismatches: &mut object_type_mismatches,
            },
            &rusty_sphinx_scope::Scope::default(),
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
}

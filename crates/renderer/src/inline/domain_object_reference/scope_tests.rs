//! Scope-fallback and other continuation tests for
//! [`super::render_inline_domain_object_reference`] — split out from
//! `domain_object_reference.rs`'s own `mod tests` purely to keep file size down.

use super::*;
use rusty_sphinx_ast::TargetSearchOrder;
use rusty_sphinx_index::ProjectIndex;

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

    let mut scope = rusty_sphinx_scope::Scope::default();
    scope.python.set_module("zipimport");

    // When — the reference is written bare, as real Sphinx docs do,
    // relying on `zipimport` being the current module.
    render_inline_domain_object_reference(
        &mut html,
        DomainObjectRef {
            object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
            name: "ZipImportError",
            display: "ZipImportError",
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
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
    let mut scope = rusty_sphinx_scope::Scope::default();
    scope.python.set_module("zipimport");
    scope.python.push_classes(&["zipimporter".to_string()]);

    // When — the reference is written bare, resolved against the
    // innermost enclosing class, which wins over the current module.
    render_inline_domain_object_reference(
        &mut html,
        DomainObjectRef {
            object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
            name: "find_spec",
            display: "find_spec",
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
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
    let mut scope = rusty_sphinx_scope::Scope::default();
    scope.python.set_module("zipimport");
    scope.python.push_classes(&["zipimporter".to_string()]);

    // When
    render_inline_domain_object_reference(
        &mut html,
        DomainObjectRef {
            object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Exception),
            name: "ZipImportError",
            display: "ZipImportError",
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
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

    let mut scope = rusty_sphinx_scope::Scope::default();
    scope.python.set_module("other_module");

    // When — the qualified attempt ("other_module.greet") misses, so
    // resolution must fall back to the bare key.
    render_inline_domain_object_reference(
        &mut html,
        DomainObjectRef {
            object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            name: "greet",
            display: "greet",
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
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
    let mut scope = rusty_sphinx_scope::Scope::default();
    scope.python.set_module("types");

    // When
    render_inline_domain_object_reference(
        &mut html,
        DomainObjectRef {
            object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
            name: "types.coroutine",
            display: "types.coroutine",
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
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
fn test_render_inline_domain_object_reference_dot_prefixed_target_displays_without_its_dot() {
    // Given — the CPython `datetime` shape: inside `.. module:: datetime`
    // a `:class:`.datetime`` reference means the module's own class. The
    // parser has already stripped the dot into `search_order`.
    let mut index = ProjectIndex::default();
    index.insert_domain_object(
        ObjectType::Py(rusty_sphinx_ast::PyObjectType::Class),
        "datetime.datetime",
        "library/datetime.rst",
    );
    let mut html = String::new();
    let mut broken_links = Vec::new();
    let mut object_type_mismatches = Vec::new();
    let mut scope = rusty_sphinx_scope::Scope::default();
    scope.python.set_module("datetime");

    // When
    render_inline_domain_object_reference(
        &mut html,
        DomainObjectRef {
            object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Class),
            name: "datetime",
            display: "datetime",
            link: true,
            search_order: TargetSearchOrder::MostQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
        "library/datetime.rst",
        &mut DomainObjectDiagnostics {
            broken_links: &mut broken_links,
            object_type_mismatches: &mut object_type_mismatches,
        },
        &scope,
    );

    // Then — linked to the module-qualified class, and no dot is shown.
    assert!(broken_links.is_empty());
    assert!(html.contains("href=\"datetime.html#py:class:datetime.datetime\""));
    assert!(html.contains(">datetime</code>"));
}

#[test]
fn test_render_inline_domain_object_reference_ambiguous_suffix_reports_its_candidates() {
    // Given — two classes documenting a `close` method, and a
    // dot-prefixed reference that names neither of them unambiguously.
    let mut index = ProjectIndex::default();
    index.insert_domain_object(
        ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
        "tarfile.TarFile.close",
        "library/tarfile.rst",
    );
    index.insert_domain_object(
        ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
        "zipfile.ZipFile.close",
        "library/zipfile.rst",
    );
    let mut html = String::new();
    let mut broken_links = Vec::new();
    let mut object_type_mismatches = Vec::new();

    // When
    render_inline_domain_object_reference(
        &mut html,
        DomainObjectRef {
            object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
            name: "close",
            display: "close",
            link: true,
            search_order: TargetSearchOrder::MostQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
        "library/shutil.rst",
        &mut DomainObjectDiagnostics {
            broken_links: &mut broken_links,
            object_type_mismatches: &mut object_type_mismatches,
        },
        &rusty_sphinx_scope::Scope::default(),
    );

    // Then — nothing is linked, and the diagnostic names both options.
    assert!(html.contains("class=\"broken-link\""));
    assert_eq!(
        broken_links,
        vec![BrokenLink {
            kind: BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Method),
                candidates: vec![
                    "tarfile.tarfile.close".to_string(),
                    "zipfile.zipfile.close".to_string(),
                ],
            },
            target: "close".to_string(),
            span: None,
        }]
    );
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
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
        },
        &DomainObjectResolver::new(&index),
        "guide/intro.rst",
        &mut DomainObjectDiagnostics {
            broken_links: &mut broken_links,
            object_type_mismatches: &mut object_type_mismatches,
        },
        &rusty_sphinx_scope::Scope::default(),
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
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
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
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
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
    assert!(html.contains("href=\"api.html#py:function:greetings.shout\""));
    assert!(html.contains(">shout<"));
    assert!(!html.contains("greetings.shout<"));
}

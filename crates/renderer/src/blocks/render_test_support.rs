//! A [`RenderCtx`] for the renderer tests that call a node renderer directly
//! rather than going through [`crate::render`].
//!
//! Its own module because several sibling test modules need it — the
//! code-block, doctest, admonition, image and figure renderers — and a context
//! is a dozen owned values threaded together by reference, which is a lot to
//! restate in each of them.
#![cfg(test)]

use rinx_ast::{Directive, Node, ResolvedLanguage};
use rinx_index::ProjectIndex;

use crate::{EmbeddedAssets, RenderCtx};

/// Runs `body` with a default rendering context over an empty project index.
///
/// The closure receives the context rather than the helper returning one:
/// every field of a [`RenderCtx`] borrows something that has to outlive it, so
/// handing one back would mean handing back its ten owners too. Anything a
/// test needs to assert on afterwards — `highlight_errors`, say — is readable
/// inside the closure and returned from it.
pub(super) fn with_ctx<R>(body: impl FnOnce(&mut RenderCtx<'_>) -> R) -> R {
    with_highlight_language(ResolvedLanguage::default(), body)
}

/// As [`with_ctx`], with the inherited highlight language set — what a
/// `.. highlight::` earlier in the document would have left behind.
pub(super) fn with_highlight_language<R>(
    language: ResolvedLanguage,
    body: impl FnOnce(&mut RenderCtx<'_>) -> R,
) -> R {
    let index = ProjectIndex::default();
    with_ctx_for(&index, "test.rst", language, &EmbeddedAssets::new(), body)
}

/// As [`with_ctx`], over a caller-supplied index, page path and asset table —
/// what a test needs when the thing under test is a cross-reference, a
/// page-relative href, or an embedded image.
pub(super) fn with_ctx_for<R>(
    index: &ProjectIndex,
    doc_path: &str,
    language: ResolvedLanguage,
    embedded_assets: &EmbeddedAssets,
    body: impl FnOnce(&mut RenderCtx<'_>) -> R,
) -> R {
    let anon_targets = Vec::new();
    let mut anon_index = 0;
    let domain_resolver = crate::resolution::DomainObjectResolver::new(index);
    let option_resolver = crate::resolution::OptionResolver::new(index);
    // These tests render body constructs, none of which is an entity, so the
    // empty schema is the honest input rather than a stub.
    let schema = rinx_entity::EntitySchema::empty_ref();
    let entity_resolver = crate::resolution::EntityResolver::new(index, schema);
    let math = crate::math::MathRenderer::new();
    let highlighter = crate::highlight::Highlighter::new();
    let section_ids = std::collections::BTreeMap::new();
    // No test here renders a numbered caption through this context: numbering
    // needs the document, and these tests render one construct at a time.
    let config = crate::config::SiteConfig::default();
    let numbering = crate::numbering::Numbering::new(
        &rinx_ast::Document::new(doc_path.to_string(), Vec::new()),
        index,
        &config,
    );

    let mut ctx = RenderCtx {
        numbering: &numbering,
        index,
        domain_resolver: &domain_resolver,
        option_resolver: &option_resolver,
        entity_resolver: &entity_resolver,
        schema,
        entity_templates: &crate::blocks::EntityTemplates::new(),
        entity_template_errors: &mut Vec::new(),
        doc_path,
        anon_targets: &anon_targets,
        anon_index: &mut anon_index,
        original_doc_path: doc_path,
        broken_links: &mut Vec::new(),
        object_type_mismatches: &mut Vec::new(),
        math_errors: &mut Vec::new(),
        empty_listing_errors: &mut Vec::new(),
        diagram_errors: &mut Vec::new(),
        diagram_sources: &mut Vec::new(),
        math: &math,
        highlight_errors: &mut Vec::new(),
        image_errors: &mut Vec::new(),
        embedded_assets,
        highlighter: &highlighter,
        highlight_language: language,
        collapse_entities: true,
        show_entity_updates: crate::blocks::EntityUpdateVisibility::Show,
        uml_configs: &std::collections::BTreeMap::new(),
        pep_base_url: &config.pep_base_url,
        rfc_base_url: &config.rfc_base_url,
        linenothreshold: None,
        highlight_force: false,
        section_ids: &section_ids,
        at_top_level: true,
        contents_backlinks: &mut std::collections::HashMap::new(),
        contents_id_allocator: &mut rinx_ast::SectionIdAllocator::new(),
        scope: rinx_scope::Scope::default(),
    };

    body(&mut ctx)
}

/// Renders one directive on a page at `doc_path`, resolving references
/// against `index`.
///
/// Goes through the node dispatcher rather than calling a renderer directly,
/// so a test also proves its directive is actually wired into the dispatch.
pub(super) fn render_directive_html(
    directive: &Directive,
    index: &ProjectIndex,
    doc_path: &str,
) -> String {
    render_directive_with_assets(directive, index, doc_path, &EmbeddedAssets::new())
}

/// As [`render_directive_html`], with the embedded-asset table a
/// `:loading: embed` image is resolved against.
pub(super) fn render_directive_with_assets(
    directive: &Directive,
    index: &ProjectIndex,
    doc_path: &str,
    embedded_assets: &EmbeddedAssets,
) -> String {
    let nodes = vec![Node::Directive(directive.clone())];
    with_ctx_for(
        index,
        doc_path,
        ResolvedLanguage::default(),
        embedded_assets,
        |ctx| {
            let mut html = String::new();
            crate::blocks::render_nodes(&mut html, &nodes, ctx);
            html
        },
    )
}

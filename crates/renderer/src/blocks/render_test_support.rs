//! A [`RenderCtx`] for the renderer tests that call a node renderer directly
//! rather than going through [`crate::render`].
//!
//! Its own module because three sibling test modules need it — the code-block,
//! doctest and admonition renderers — and a context is a dozen owned values
//! threaded together by reference, which is a lot to restate three times.
#![cfg(test)]

use rusty_sphinx_ast::ResolvedLanguage;
use rusty_sphinx_index::ProjectIndex;

use crate::RenderCtx;

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
    let anon_targets = Vec::new();
    let mut anon_index = 0;
    let domain_resolver = crate::resolution::DomainObjectResolver::new(&index);
    let option_resolver = crate::resolution::OptionResolver::new(&index);
    let math = crate::math::MathRenderer::new();
    let highlighter = crate::highlight::Highlighter::new();
    let section_ids = std::collections::BTreeMap::new();

    let mut ctx = RenderCtx {
        index: &index,
        domain_resolver: &domain_resolver,
        option_resolver: &option_resolver,
        doc_path: "test.rst",
        anon_targets: &anon_targets,
        anon_index: &mut anon_index,
        original_doc_path: "test.rst",
        broken_links: &mut Vec::new(),
        object_type_mismatches: &mut Vec::new(),
        math_errors: &mut Vec::new(),
        math: &math,
        highlight_errors: &mut Vec::new(),
        highlighter: &highlighter,
        highlight_language: language,
        linenothreshold: None,
        highlight_force: false,
        section_ids: &section_ids,
        at_top_level: true,
        scope: rusty_sphinx_scope::Scope::default(),
    };

    body(&mut ctx)
}

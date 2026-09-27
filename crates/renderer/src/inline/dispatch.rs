//! The inline-node dispatcher: matches on an [`rinx_ast::InlineNode`]
//! and delegates to the module that renders that kind.

use std::fmt::Write as _;

use rinx_ast::ImageAlign;

use super::anonymous_reference::{
    render_inline_anonymous_hyperlink, render_inline_anonymous_reference,
};
use super::any_reference::{AnyRef, render_inline_any_reference};
use super::doc_reference::{DocRef, render_inline_doc_reference};
use super::domain_object_reference::{
    DomainObjectDiagnostics, DomainObjectRef, render_inline_domain_object_reference,
};
use super::hyperlink::render_inline_hyperlink;
use super::math::{render_equation_reference, render_inline_math};
use super::option_reference::render_inline_option_reference;
use super::reference::{LabelRef, render_inline_reference};
use super::term_reference::render_inline_term_reference;

use super::RefText;
use crate::RenderCtx;
use crate::blocks::render_linked_image;
use crate::resolution::AnyResolver;

/// Renders a single inline node into `html`.
///
/// The plain-markup variants render here; everything that resolves against
/// the project index — and so can report a broken link — goes to
/// [`render_cross_reference`]. The split is by *what the arm needs*: these
/// arms need nothing but the text, those need the resolvers, the scope and
/// the diagnostic sinks.
pub(crate) fn render_inline(
    html: &mut String,
    inline: &rinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    match inline {
        rinx_ast::InlineNode::Text(text) => {
            let _ = write!(html, "{}", html_escape::encode_text(text));
        }
        rinx_ast::InlineNode::Emphasis(text) => {
            let _ = write!(html, "<em>{}</em>", html_escape::encode_text(text));
        }
        rinx_ast::InlineNode::Strong(text) => {
            let _ = write!(html, "<strong>{}</strong>", html_escape::encode_text(text));
        }
        rinx_ast::InlineNode::Literal(text) => {
            let _ = write!(html, "<code>{}</code>", html_escape::encode_text(text));
        }
        rinx_ast::InlineNode::Program(text) => {
            let _ = write!(
                html,
                "<strong class=\"program\">{}</strong>",
                html_escape::encode_text(text)
            );
        }
        rinx_ast::InlineNode::AnonymousHyperlink { text, target } => {
            render_inline_anonymous_hyperlink(html, text, target);
        }
        // Self-contained despite carrying a span: rendering an equation needs
        // the math backend, not the index.
        rinx_ast::InlineNode::Math { latex, span } => {
            render_inline_math(html, latex, *span, ctx.math, ctx.math_errors);
        }
        // What a `.. |name| image::` substitution reference resolves to —
        // built the same way a standalone `.. image::`'s `<img>` is, minus
        // the `:name:` anchor span it can never carry (see
        // `DiagnosticCode::SubstitutionImageNameNotAllowed`).
        rinx_ast::InlineNode::InlineImage(options) => {
            render_inline_image(html, options, ctx);
        }
        // Never reaches a well-formed document by the time it is rendered:
        // `resolve_substitutions` replaces every reference with its
        // definition's content (or a literal-text fallback) before parsing
        // returns. Rendered as the written source rather than panicking, so
        // an `.ast` from a differently-behaved parser degrades instead of
        // crashing the render.
        rinx_ast::InlineNode::SubstitutionReference { name, .. } => {
            let _ = write!(html, "|{}|", html_escape::encode_text(name));
        }
        // Listed rather than caught by a `_`, so a variant added later is a
        // compile error here and in `render_cross_reference` instead of
        // silently rendering as nothing.
        rinx_ast::InlineNode::Reference { .. }
        | rinx_ast::InlineNode::AnyReference { .. }
        | rinx_ast::InlineNode::DocReference { .. }
        | rinx_ast::InlineNode::Hyperlink { .. }
        | rinx_ast::InlineNode::AnonymousReference { .. }
        | rinx_ast::InlineNode::TermReference { .. }
        | rinx_ast::InlineNode::DomainObjectReference { .. }
        | rinx_ast::InlineNode::OptionReference { .. }
        | rinx_ast::InlineNode::EntityReference { .. }
        | rinx_ast::InlineNode::EquationReference { .. } => {
            render_cross_reference(html, inline, ctx);
        }
    }
}

/// Renders a substitution-defined inline image.
///
/// Reuses [`render_linked_image`] verbatim — the same `<img>`, the same
/// `:target:` wrapping, the same `:align:` handling (vertical alignments
/// included, since [`ImageAlign::css_class`] treats all six identically) —
/// because nothing about *how* an image is rendered differs here; only which
/// options the parser let an author write differs, and that was already
/// enforced by the time this node exists.
fn render_inline_image(html: &mut String, options: &rinx_ast::ImageOptions, ctx: &mut RenderCtx) {
    let align_class: Vec<String> = options
        .align
        .map(ImageAlign::css_class)
        .into_iter()
        .collect();
    render_linked_image(html, options, &align_class, ctx);
}

/// Renders the inline variants that resolve against the project index, each
/// of which carries the source position its diagnostic is reported at.
fn render_cross_reference(
    html: &mut String,
    inline: &rinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    match inline {
        rinx_ast::InlineNode::Reference {
            display,
            target,
            span,
            inventory,
        } => {
            render_inline_reference(
                html,
                LabelRef {
                    title: display.as_deref(),
                    target,
                    span: *span,
                    inventory,
                },
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rinx_ast::InlineNode::Hyperlink { text, target, span } => {
            render_inline_hyperlink(
                html,
                RefText {
                    display: text,
                    target,
                    span: *span,
                },
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rinx_ast::InlineNode::AnonymousReference { text, span } => {
            render_inline_anonymous_reference(
                html,
                text,
                *span,
                ctx.anon_targets,
                ctx.anon_index,
                ctx.broken_links,
            );
        }
        rinx_ast::InlineNode::DomainObjectReference { .. } => {
            render_domain_object(html, inline, ctx);
        }
        rinx_ast::InlineNode::AnyReference {
            display,
            target,
            link,
            span,
            inventory,
        } => {
            render_inline_any_reference(
                html,
                AnyRef {
                    title: display.as_deref(),
                    target,
                    link: *link,
                    span: *span,
                    inventory,
                },
                &AnyResolver {
                    index: ctx.index,
                    domains: ctx.domain_resolver,
                    options: ctx.option_resolver,
                },
                &ctx.scope,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        // Grouped into their own function purely to keep this match's total
        // line count from growing past a readable length as roles
        // accumulate — see [`render_indexed_cross_reference`].
        rinx_ast::InlineNode::DocReference { .. }
        | rinx_ast::InlineNode::TermReference { .. }
        | rinx_ast::InlineNode::OptionReference { .. }
        | rinx_ast::InlineNode::EntityReference { .. }
        | rinx_ast::InlineNode::EquationReference { .. } => {
            render_indexed_cross_reference(html, inline, ctx);
        }
        rinx_ast::InlineNode::Text(_)
        | rinx_ast::InlineNode::AnonymousHyperlink { .. }
        | rinx_ast::InlineNode::Emphasis(_)
        | rinx_ast::InlineNode::Strong(_)
        | rinx_ast::InlineNode::Literal(_)
        | rinx_ast::InlineNode::Math { .. }
        | rinx_ast::InlineNode::InlineImage(_)
        | rinx_ast::InlineNode::SubstitutionReference { .. }
        | rinx_ast::InlineNode::Program(_) => {
            unreachable!("render_inline routes only cross-reference variants here")
        }
    }
}

/// Renders the cross-reference roles [`render_cross_reference`] groups into
/// one arm purely to stay under a readable line count — nothing else ties
/// `:doc:`, `:term:`, `:option:`, the entity roles and `:eq:` together the
/// way domain objects share a resolver.
fn render_indexed_cross_reference(
    html: &mut String,
    inline: &rinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    match inline {
        rinx_ast::InlineNode::DocReference {
            display,
            target,
            link,
            span,
            inventory,
        } => {
            render_inline_doc_reference(
                html,
                DocRef {
                    title: display.as_deref(),
                    target,
                    link: *link,
                    span: *span,
                    inventory,
                },
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rinx_ast::InlineNode::TermReference {
            display,
            term,
            span,
            inventory,
        } => {
            render_inline_term_reference(
                html,
                RefText {
                    display,
                    target: term,
                    span: *span,
                },
                inventory,
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rinx_ast::InlineNode::OptionReference {
            display,
            target,
            span,
            inventory,
        } => {
            render_inline_option_reference(
                html,
                RefText {
                    display,
                    target,
                    span: *span,
                },
                inventory,
                ctx.option_resolver,
                ctx.scope.program.current(),
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rinx_ast::InlineNode::EntityReference {
            role,
            target,
            display,
            span,
        } => {
            super::entity_reference::render_inline_entity_reference(
                html,
                role,
                RefText {
                    display,
                    target,
                    span: *span,
                },
                ctx.entity_resolver,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rinx_ast::InlineNode::EquationReference { label, span } => {
            render_equation_reference(
                html,
                label,
                *span,
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        _ => unreachable!(
            "render_cross_reference routes only TermReference/OptionReference/EquationReference here"
        ),
    }
}

/// Renders a domain-object role.
///
/// Split out of [`render_cross_reference`] because it is the one reference
/// kind that needs more than the index: its own resolver, the enclosing
/// scope, and a second diagnostic sink for object-type mismatches.
fn render_domain_object(html: &mut String, inline: &rinx_ast::InlineNode, ctx: &mut RenderCtx<'_>) {
    let rinx_ast::InlineNode::DomainObjectReference {
        object_type,
        name,
        display,
        link,
        search_order,
        span,
        inventory,
    } = inline
    else {
        unreachable!("render_cross_reference routes only domain-object roles here")
    };
    render_inline_domain_object_reference(
        html,
        DomainObjectRef {
            object_type: *object_type,
            name,
            display,
            link: *link,
            search_order: *search_order,
            span: *span,
            inventory,
        },
        ctx.domain_resolver,
        ctx.doc_path,
        &mut DomainObjectDiagnostics {
            broken_links: ctx.broken_links,
            object_type_mismatches: ctx.object_type_mismatches,
        },
        &ctx.scope,
    );
}

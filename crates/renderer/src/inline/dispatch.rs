//! The inline-node dispatcher: matches on an [`rusty_sphinx_ast::InlineNode`]
//! and delegates to the module that renders that kind.

use std::fmt::Write as _;

use super::anonymous_reference::{
    render_inline_anonymous_hyperlink, render_inline_anonymous_reference,
};
use super::domain_object_reference::{
    DomainObjectDiagnostics, DomainObjectRef, render_inline_domain_object_reference,
};
use super::hyperlink::render_inline_hyperlink;
use super::math::{render_equation_reference, render_inline_math};
use super::option_reference::render_inline_option_reference;
use super::reference::render_inline_reference;
use super::term_reference::render_inline_term_reference;

use super::RefText;
use crate::RenderCtx;

/// Renders a single inline node into `html`.
///
/// The plain-markup variants render here; everything that resolves against
/// the project index — and so can report a broken link — goes to
/// [`render_cross_reference`]. The split is by *what the arm needs*: these
/// arms need nothing but the text, those need the resolvers, the scope and
/// the diagnostic sinks.
pub(crate) fn render_inline(
    html: &mut String,
    inline: &rusty_sphinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    match inline {
        rusty_sphinx_ast::InlineNode::Text(text) => {
            let _ = write!(html, "{}", html_escape::encode_text(text));
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
        rusty_sphinx_ast::InlineNode::AnonymousHyperlink { text, target } => {
            render_inline_anonymous_hyperlink(html, text, target);
        }
        // Self-contained despite carrying a span: rendering an equation needs
        // the math backend, not the index.
        rusty_sphinx_ast::InlineNode::Math { latex, span } => {
            render_inline_math(html, latex, *span, ctx.math, ctx.math_errors);
        }
        // Listed rather than caught by a `_`, so a variant added later is a
        // compile error here and in `render_cross_reference` instead of
        // silently rendering as nothing.
        rusty_sphinx_ast::InlineNode::Reference { .. }
        | rusty_sphinx_ast::InlineNode::Hyperlink { .. }
        | rusty_sphinx_ast::InlineNode::AnonymousReference { .. }
        | rusty_sphinx_ast::InlineNode::TermReference { .. }
        | rusty_sphinx_ast::InlineNode::DomainObjectReference { .. }
        | rusty_sphinx_ast::InlineNode::OptionReference { .. }
        | rusty_sphinx_ast::InlineNode::EquationReference { .. } => {
            render_cross_reference(html, inline, ctx);
        }
    }
}

/// Renders the inline variants that resolve against the project index, each
/// of which carries the source position its diagnostic is reported at.
fn render_cross_reference(
    html: &mut String,
    inline: &rusty_sphinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    match inline {
        rusty_sphinx_ast::InlineNode::Reference {
            display,
            target,
            span,
        } => {
            render_inline_reference(
                html,
                RefText {
                    display,
                    target,
                    span: *span,
                },
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::Hyperlink { text, target, span } => {
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
        rusty_sphinx_ast::InlineNode::AnonymousReference { text, span } => {
            render_inline_anonymous_reference(
                html,
                text,
                *span,
                ctx.anon_targets,
                ctx.anon_index,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::TermReference {
            display,
            term,
            span,
        } => {
            render_inline_term_reference(
                html,
                RefText {
                    display,
                    target: term,
                    span: *span,
                },
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::DomainObjectReference { .. } => {
            render_domain_object(html, inline, ctx);
        }
        rusty_sphinx_ast::InlineNode::OptionReference {
            display,
            target,
            span,
        } => {
            render_inline_option_reference(
                html,
                RefText {
                    display,
                    target,
                    span: *span,
                },
                ctx.option_resolver,
                ctx.scope.program.current(),
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::EquationReference { label, span } => {
            render_equation_reference(
                html,
                label,
                *span,
                ctx.index,
                ctx.doc_path,
                ctx.broken_links,
            );
        }
        rusty_sphinx_ast::InlineNode::Text(_)
        | rusty_sphinx_ast::InlineNode::AnonymousHyperlink { .. }
        | rusty_sphinx_ast::InlineNode::Emphasis(_)
        | rusty_sphinx_ast::InlineNode::Strong(_)
        | rusty_sphinx_ast::InlineNode::Literal(_)
        | rusty_sphinx_ast::InlineNode::Math { .. }
        | rusty_sphinx_ast::InlineNode::Program(_) => {
            unreachable!("render_inline routes only cross-reference variants here")
        }
    }
}

/// Renders a domain-object role.
///
/// Split out of [`render_cross_reference`] because it is the one reference
/// kind that needs more than the index: its own resolver, the enclosing
/// scope, and a second diagnostic sink for object-type mismatches.
fn render_domain_object(
    html: &mut String,
    inline: &rusty_sphinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    let rusty_sphinx_ast::InlineNode::DomainObjectReference {
        object_type,
        name,
        display,
        link,
        search_order,
        span,
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

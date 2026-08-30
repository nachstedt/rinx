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
use super::option_reference::render_inline_option_reference;
use super::reference::render_inline_reference;
use super::term_reference::render_inline_term_reference;

use crate::RenderCtx;

/// Renders a single inline node into `html`.
pub(crate) fn render_inline(
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
            render_inline_anonymous_hyperlink(html, text, target);
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
            search_order,
        } => {
            render_inline_domain_object_reference(
                html,
                DomainObjectRef {
                    object_type: *object_type,
                    name,
                    display,
                    link: *link,
                    search_order: *search_order,
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
        rusty_sphinx_ast::InlineNode::OptionReference { display, target } => {
            render_inline_option_reference(
                html,
                display,
                target,
                ctx.option_resolver,
                ctx.scope.program.current(),
                ctx.doc_path,
                ctx.broken_links,
            );
        }
    }
}

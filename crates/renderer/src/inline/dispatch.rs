//! The inline-node dispatcher: matches on an [`rinx_ast::InlineNode`]
//! and delegates to the module that renders that kind.

use std::fmt::Write as _;

use rinx_ast::ImageAlign;

use super::anonymous_reference::{
    render_inline_anonymous_hyperlink, render_inline_anonymous_reference,
};
use super::any_reference::{AnyRef, render_inline_any_reference};
use super::code::{CodeRef, render_inline_code};
use super::doc_reference::{DocRef, render_inline_doc_reference};
use super::docutils_pep_reference::render_inline_docutils_pep_reference;
use super::docutils_rfc_reference::render_inline_docutils_rfc_reference;
use super::domain_object_reference::{
    DomainObjectDiagnostics, DomainObjectRef, render_inline_domain_object_reference,
};
use super::download_reference::{DownloadRef, render_inline_download_reference};
use super::hyperlink::render_inline_hyperlink;
use super::index_reference::render_inline_index_reference;
use super::math::{render_equation_reference, render_inline_math};
use super::number_reference::render_number_reference_node;
use super::option_reference::render_inline_option_reference;
use super::reference::{LabelRef, render_inline_reference};
use super::registry_reference::{RegistryRef, registry_base_url, render_inline_registry_reference};
use super::script::render_inline_script;
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
        rinx_ast::InlineNode::Script {
            position,
            text,
            classes,
        } => render_inline_script(html, *position, text, classes),
        // docutils' HTML writers and Sphinx's alike draw a title as `<cite>`.
        rinx_ast::InlineNode::TitleReference(text) => {
            let _ = write!(html, "<cite>{}</cite>", html_escape::encode_text(text));
        }
        rinx_ast::InlineNode::AnonymousHyperlink { text, target } => {
            render_inline_anonymous_hyperlink(html, text, target);
        }
        // Self-contained despite carrying a span: rendering an equation needs
        // the math backend, not the index.
        rinx_ast::InlineNode::Math { latex, span } => {
            render_inline_math(html, latex, *span, ctx.math, ctx.math_errors);
        }
        // Self-contained like `:math:`: highlighting needs the backend, not
        // the index.
        rinx_ast::InlineNode::Code {
            text,
            language,
            classes,
            span,
        } => {
            render_inline_code(
                html,
                &CodeRef {
                    text,
                    language,
                    classes,
                    span: *span,
                },
                ctx.highlighter,
                ctx.highlight_errors,
            );
        }
        // What a `.. |name| image::` substitution reference resolves to —
        // built the same way a standalone `.. image::`'s `<img>` is, minus
        // the `:name:` anchor span it can never carry (see
        // `DiagnosticCode::SubstitutionImageNameNotAllowed`).
        rinx_ast::InlineNode::InlineImage(options) => {
            render_inline_image(html, options, ctx);
        }
        // A file, not something a document defines: its href follows from the
        // page's path alone, so it cannot be broken while rendering — the
        // site's validation action checks the declaration instead.
        rinx_ast::InlineNode::DownloadReference {
            display,
            target,
            link,
            ..
        } => {
            render_inline_download_reference(
                html,
                DownloadRef {
                    title: display.as_deref(),
                    target,
                    link: *link,
                },
                ctx.doc_path,
            );
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
        // Never reaches a well-formed document either: the parser reports
        // every refusal and lowers it to what its refusal says. Rendered as
        // that would render, for the same reason as above.
        rinx_ast::InlineNode::RefusedRole { text, refusal, .. } => {
            render_refused_role(html, text, refusal);
        }
        // A page outside the site: its href follows from the target and the
        // registry's address alone, so it cannot be broken while rendering.
        rinx_ast::InlineNode::RegistryReference { .. }
        | rinx_ast::InlineNode::DocutilsPepReference { .. }
        | rinx_ast::InlineNode::DocutilsRfcReference { .. } => {
            render_registry_role(html, inline, ctx);
        }
        // Links nowhere: the general index links here.
        rinx_ast::InlineNode::IndexReference {
            title, index_id, ..
        } => render_inline_index_reference(html, title, index_id),
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
        | rinx_ast::InlineNode::NumberReference { .. }
        | rinx_ast::InlineNode::EquationReference { .. } => {
            render_cross_reference(html, inline, ctx);
        }
    }
}

/// Renders the roles linking a numbered document in a registry outside the
/// site — Sphinx's `:pep:`/`:rfc:`/`:cve:`/`:cwe:` and docutils'
/// `:pep-reference:`/`:rfc-reference:` — which need only the site's
/// registry addresses.
fn render_registry_role(html: &mut String, inline: &rinx_ast::InlineNode, ctx: &RenderCtx<'_>) {
    match inline {
        rinx_ast::InlineNode::RegistryReference {
            target,
            display,
            index_id,
            ..
        } => render_inline_registry_reference(
            html,
            RegistryRef::new(display.as_deref(), target, index_id),
            registry_base_url(target.registry(), ctx.pep_base_url, ctx.rfc_base_url),
        ),
        // The same, without the anchor: docutils' roles make no index entry.
        rinx_ast::InlineNode::DocutilsPepReference { number, .. } => {
            render_inline_docutils_pep_reference(html, number, ctx.pep_base_url);
        }
        rinx_ast::InlineNode::DocutilsRfcReference { number, .. } => {
            render_inline_docutils_rfc_reference(html, number, ctx.rfc_base_url);
        }
        _ => unreachable!("render_inline routes only registry roles here"),
    }
}

/// Renders a refused role as the parser would have lowered it: an unlinked
/// `:numref:`, an `:index:`'s title, or any other refused role's source
/// text.
fn render_refused_role(html: &mut String, text: &str, refusal: &rinx_ast::RoleRefusal) {
    match refusal {
        rinx_ast::RoleRefusal::NumberReference(_) => {
            let _ = write!(
                html,
                "<span class=\"xref std std-numref\">{}</span>",
                html_escape::encode_text(text)
            );
        }
        rinx_ast::RoleRefusal::RegistryTarget { .. }
        | rinx_ast::RoleRefusal::DocutilsPepNumber { .. }
        | rinx_ast::RoleRefusal::DocutilsRfcNumber { .. }
        | rinx_ast::RoleRefusal::MultipleRoles
        | rinx_ast::RoleRefusal::RoleAndReference => {
            let _ = write!(html, "{}", html_escape::encode_text(text));
        }
        rinx_ast::RoleRefusal::IndexEntry { title, .. } => {
            let _ = write!(html, "{}", html_escape::encode_text(title));
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

/// Renders a hyperlink reference — `` `text`_ `` or `` `text`__ `` — which
/// links a target the document or the project names, rather than a role's.
fn render_hyperlink_reference(
    html: &mut String,
    inline: &rinx_ast::InlineNode,
    ctx: &mut RenderCtx<'_>,
) {
    match inline {
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
        _ => unreachable!("render_cross_reference routes only hyperlink references here"),
    }
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
        rinx_ast::InlineNode::Hyperlink { .. }
        | rinx_ast::InlineNode::AnonymousReference { .. } => {
            render_hyperlink_reference(html, inline, ctx);
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
        | rinx_ast::InlineNode::NumberReference { .. }
        | rinx_ast::InlineNode::EquationReference { .. } => {
            render_indexed_cross_reference(html, inline, ctx);
        }
        rinx_ast::InlineNode::Text(_)
        | rinx_ast::InlineNode::AnonymousHyperlink { .. }
        | rinx_ast::InlineNode::Emphasis(_)
        | rinx_ast::InlineNode::Strong(_)
        | rinx_ast::InlineNode::Literal(_)
        | rinx_ast::InlineNode::Math { .. }
        | rinx_ast::InlineNode::Code { .. }
        | rinx_ast::InlineNode::InlineImage(_)
        | rinx_ast::InlineNode::SubstitutionReference { .. }
        | rinx_ast::InlineNode::RefusedRole { .. }
        | rinx_ast::InlineNode::RegistryReference { .. }
        | rinx_ast::InlineNode::IndexReference { .. }
        | rinx_ast::InlineNode::DocutilsPepReference { .. }
        | rinx_ast::InlineNode::DocutilsRfcReference { .. }
        | rinx_ast::InlineNode::DownloadReference { .. }
        | rinx_ast::InlineNode::Program(_)
        | rinx_ast::InlineNode::Script { .. }
        | rinx_ast::InlineNode::TitleReference(_) => {
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
        rinx_ast::InlineNode::NumberReference { .. } => {
            render_number_reference_node(html, inline, ctx);
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
        _ => unreachable!("render_cross_reference routes only the index-resolved roles here"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_refused_role_shows_a_numref_unlinked() {
        // Given
        let mut html = String::new();

        // When
        render_refused_role(
            &mut html,
            "see <this>",
            &rinx_ast::RoleRefusal::NumberReference(rinx_ast::NumberReferenceRefusal::InvalidTitle),
        );

        // Then
        assert_eq!(
            html,
            "<span class=\"xref std std-numref\">see &lt;this&gt;</span>"
        );
    }

    #[test]
    fn test_render_refused_role_shows_a_pep_as_its_source() {
        // Given
        let mut html = String::new();

        // When
        render_refused_role(
            &mut html,
            ":pep:`<x>`",
            &rinx_ast::RoleRefusal::RegistryTarget {
                registry: rinx_ast::Registry::Pep,
                target: "<x>".to_string(),
            },
        );

        // Then
        assert_eq!(html, ":pep:`&lt;x&gt;`");
    }

    #[test]
    fn test_render_refused_role_shows_an_rfc_reference_as_its_source() {
        // Given
        let mut html = String::new();

        // When
        render_refused_role(
            &mut html,
            ":rfc-reference:`<x>`",
            &rinx_ast::RoleRefusal::DocutilsRfcNumber {
                target: "<x>".to_string(),
            },
        );

        // Then
        assert_eq!(html, ":rfc-reference:`&lt;x&gt;`");
    }

    #[test]
    fn test_render_refused_role_shows_a_pep_reference_as_its_source() {
        // Given
        let mut html = String::new();

        // When
        render_refused_role(
            &mut html,
            ":pep-reference:`<x>`",
            &rinx_ast::RoleRefusal::DocutilsPepNumber {
                target: "<x>".to_string(),
            },
        );

        // Then
        assert_eq!(html, ":pep-reference:`&lt;x&gt;`");
    }

    #[test]
    fn test_render_refused_role_shows_an_index_role_as_its_title() {
        // Given
        let mut html = String::new();

        // When
        render_refused_role(
            &mut html,
            ":index:`a <b> <pair: x>`",
            &rinx_ast::RoleRefusal::IndexEntry {
                title: "a <b>".to_string(),
                entry: rinx_ast::InvalidIndexEntry {
                    entry_type: rinx_ast::IndexEntryType::Pair,
                    value: "x".to_string(),
                },
            },
        );

        // Then
        assert_eq!(html, "a &lt;b&gt;");
    }

    fn render_paragraph(inlines: Vec<rinx_ast::InlineNode>) -> String {
        let doc = rinx_ast::Document::new(
            "guide.rst".to_string(),
            vec![rinx_ast::Node::Paragraph(inlines)],
        );
        crate::render(&doc, &rinx_index::ProjectIndex::default(), &doc.path).html
    }

    #[test]
    fn test_render_inline_writes_a_script_inside_its_word() {
        // Given / When — `H\ :sub:`2`\ O`
        let html = render_paragraph(vec![
            rinx_ast::InlineNode::Text("H".to_string()),
            rinx_ast::InlineNode::Script {
                position: rinx_ast::ScriptPosition::Subscript,
                text: "2".to_string(),
                classes: Vec::new(),
            },
            rinx_ast::InlineNode::Text("O".to_string()),
        ]);

        // Then
        assert!(html.contains("H<sub>2</sub>O"), "{html}");
    }

    #[test]
    fn test_render_inline_writes_a_title_reference_as_a_citation() {
        // Given / When — `` `Dune & Co` ``
        let html = render_paragraph(vec![
            rinx_ast::InlineNode::Text("Read ".to_string()),
            rinx_ast::InlineNode::TitleReference("Dune & Co".to_string()),
        ]);

        // Then
        assert!(html.contains("Read <cite>Dune &amp; Co</cite>"), "{html}");
    }

    #[test]
    fn test_render_inline_writes_an_index_role_anchor_and_text() {
        // Given / When
        let html = render_paragraph(vec![
            rinx_ast::InlineNode::Text("The ".to_string()),
            rinx_ast::InlineNode::IndexReference {
                title: "loop".to_string(),
                entries: Vec::new(),
                index_id: "index-2".to_string(),
                span: None,
            },
            rinx_ast::InlineNode::Text(" statement.".to_string()),
        ]);

        // Then
        assert!(
            html.contains("The <span class=\"target\" id=\"index-2\"></span>loop statement."),
            "{html}"
        );
    }

    #[test]
    fn test_render_registry_role_links_each_registry_role_below_its_base_url() {
        // Given
        let inlines = vec![
            rinx_ast::InlineNode::RegistryReference {
                target: rinx_ast::RegistryTarget::parse(rinx_ast::Registry::Pep, "8").unwrap(),
                display: None,
                index_id: "index-0".to_string(),
                span: None,
            },
            rinx_ast::InlineNode::DocutilsPepReference {
                number: rinx_ast::DocutilsPepNumber::parse("20").unwrap(),
                span: None,
            },
            rinx_ast::InlineNode::DocutilsRfcReference {
                number: rinx_ast::DocutilsRfcNumber::parse("2822").unwrap(),
                span: None,
            },
        ];

        // When
        let html = render_paragraph(inlines);

        // Then
        assert!(html.contains("pep-0008/"), "{html}");
        assert!(html.contains("pep-0020\""), "{html}");
        assert!(html.contains("rfc2822.html"), "{html}");
    }
}

//! The domain-object directive dispatcher (`.. py:function::`,
//! `.. c:function::`, etc.) and its type-specific option rendering.

use std::fmt::Write as _;

use rinx_scope::DefinitionNames;

use super::c::render_cmdoption;
use super::labels::{domain_object_prefix_labels, is_decorator_signature};
use crate::RenderCtx;

/// Renders a domain object directive (e.g. `.. py:function::`, `.. c:function::`,
/// `.. py:module::`, `.. py:data::`) as a Sphinx-style object description
/// (`<dl class="{domain} {objtype}">`), using the same qualified key as the
/// analyzer for the anchor `id`.
///
/// The shared `<dl>`/`<dt>` wrapper and cross-reference key are built
/// generically via `obj`'s accessors; any option specific to one object type
/// (`py:data`'s `type`/`value`, rendered inline as leading `<dd>`
/// paragraphs) is matched explicitly, so adding a new object type with its
/// own options can't be forgotten here. `py:module`'s
/// `platform`/`synopsis`/`deprecated` are deliberately *not* printed, as
/// Sphinx never prints them: they reach the reader through the Python Module
/// Index and the `:mod:` link tooltip instead (ADR-032).
///
/// Each `<dt>`'s `id` is built from the names [`rinx_scope::DocumentScopes`]
/// qualified the object to — the same names the analyzer's
/// `index_domain_object` indexes it under, so index keys and anchor `id`s
/// cannot disagree.
pub(crate) fn render_domain_object(
    html: &mut String,
    obj: &rinx_ast::DomainObjectBody,
    ctx: &mut RenderCtx<'_>,
) {
    // Copied out of the context, so the names borrow the table rather than
    // `ctx`, which the body renders through below.
    let scopes = ctx.scopes;
    let definition = scopes.definition(obj);
    let names = match &*definition {
        DefinitionNames::Options(lines) => {
            render_cmdoption(html, obj, lines, ctx);
            return;
        }
        DefinitionNames::Object(names) => names,
    };

    let object_type = obj.object_type();
    let domain_str = object_type.domain().as_str();
    let objtype_str = object_type.as_str();

    let _ = writeln!(html, "<dl class=\"{domain_str} {objtype_str}\">");
    // One `<dt>` per declared signature, all sharing the single `<dd>` below —
    // the shape real Sphinx renders a multi-signature object description in.
    for (qualified_name, signature_text) in names.as_slice().iter().zip(obj.signature_texts()) {
        let sig_escaped = html_escape::encode_text(signature_text);
        // `no_index` means no cross-reference target — omit the `id`
        // entirely rather than emitting a dangling anchor.
        if obj.no_index() {
            let _ = write!(html, "  <dt>");
        } else {
            let key = rinx_ast::build_domain_object_key(object_type, qualified_name);
            let id_attr = html_escape::encode_double_quoted_attribute(key.as_str());
            let _ = write!(html, "  <dt id=\"{id_attr}\">");
        }
        for label in domain_object_prefix_labels(obj) {
            let _ = write!(html, "<em class=\"property\">{label}</em> ");
        }
        // Real Sphinx's `PyDecoratorFunction`/`PyDecoratorMethod` insert a
        // literal `@` (`desc_addname('@', '@')`) directly before the
        // signature name — unlike the `<em class="property">` badges above,
        // which are separate flag annotations, this is fused onto the name
        // itself, so it's prepended inside the same `<code class="sig-name">`
        // element rather than rendered as its own node.
        let decorator_prefix = if is_decorator_signature(obj) { "@" } else { "" };
        let _ = writeln!(
            html,
            "<code class=\"sig-name\">{decorator_prefix}{sig_escaped}</code></dt>"
        );
    }
    let _ = write!(html, "  <dd>");
    render_domain_object_options(html, obj);
    crate::render_nodes(html, obj.body(), ctx);
    let _ = writeln!(html, "</dd>");
    let _ = writeln!(html, "</dl>");
}

/// Renders a domain object's type-specific options (`py:data`'s `type`/`value`,
/// `py:attribute`'s `type`/`value`/`canonical`) as leading `<dd>` paragraphs.
/// Object types with no such options (`py:function`, `c:function`,
/// `c:macro`, `py:method`, `py:class`, `py:exception`) render nothing here,
/// and neither does `py:module`, whose options Sphinx never prints (ADR-032).
fn render_domain_object_options(html: &mut String, obj: &rinx_ast::DomainObjectBody) {
    match obj {
        rinx_ast::DomainObjectBody::PyData { type_, value, .. } => {
            if let Some(type_) = type_ {
                let _ = write!(
                    html,
                    "<p class=\"type\">Type: {}</p>",
                    html_escape::encode_text(type_)
                );
            }
            if let Some(value) = value {
                let _ = write!(
                    html,
                    "<p class=\"value\">Value: {}</p>",
                    html_escape::encode_text(value)
                );
            }
        }
        rinx_ast::DomainObjectBody::PyAttribute {
            type_,
            value,
            canonical,
            ..
        } => {
            if let Some(type_) = type_ {
                let _ = write!(
                    html,
                    "<p class=\"type\">Type: {}</p>",
                    html_escape::encode_text(type_)
                );
            }
            if let Some(value) = value {
                let _ = write!(
                    html,
                    "<p class=\"value\">Value: {}</p>",
                    html_escape::encode_text(value)
                );
            }
            if let Some(canonical) = canonical {
                let _ = write!(
                    html,
                    "<p class=\"canonical\">Canonical: {}</p>",
                    html_escape::encode_text(canonical)
                );
            }
        }
        rinx_ast::DomainObjectBody::PyModule { .. }
        | rinx_ast::DomainObjectBody::PyFunction { .. }
        | rinx_ast::DomainObjectBody::CFunction { .. }
        | rinx_ast::DomainObjectBody::CMacro { .. }
        | rinx_ast::DomainObjectBody::CStruct { .. }
        | rinx_ast::DomainObjectBody::CUnion { .. }
        | rinx_ast::DomainObjectBody::CMember { .. }
        | rinx_ast::DomainObjectBody::CType { .. }
        | rinx_ast::DomainObjectBody::PyMethod { .. }
        | rinx_ast::DomainObjectBody::PyClass { .. }
        | rinx_ast::DomainObjectBody::PyException { .. }
        | rinx_ast::DomainObjectBody::StdCmdoption { .. } => {}
    }
}

#[cfg(test)]
mod python_scoping_tests;
#[cfg(test)]
mod python_tests;
#[cfg(test)]
mod signature_tests;

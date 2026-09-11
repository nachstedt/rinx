//! Rendering an entity and its sections.
//!
//! The built-in presentation, which every type gets without configuring
//! anything: a titled box carrying the entity's id anchor, a table of its
//! attribute values, its prose sections in the order they were written, and
//! its outgoing and incoming links.
//!
//! Sections render as ordinary body content, because that is what they are —
//! the whole point of the attribute/section split is that this one goes
//! through `render_nodes` and an attribute never can.

use std::fmt::Write as _;

use rusty_sphinx_ast::{EntityBody, EntitySection};

use crate::RenderCtx;
use crate::inline::entity_reference::entity_href;

/// The anchor an entity is linked by.
///
/// Re-exported from `rusty_sphinx_index` rather than defined here: a
/// templated diagram's generated node links have to land on exactly this
/// anchor, and that text is built by a phase that cannot depend on the
/// renderer.
pub(crate) use rusty_sphinx_index::entity_anchor;

mod template;

pub use template::EntityTemplates;

/// Renders one entity, through its type's template when it declares one.
pub(super) fn render_entity(html: &mut String, entity: &EntityBody, ctx: &mut RenderCtx<'_>) {
    match template::render_with_template(entity, ctx) {
        Some(Ok(rendered)) => {
            html.push_str(&rendered);
            return;
        }
        // A named-but-broken template is a build misconfiguration, not a
        // document fault: report it and fall back, so the page keeps the entity
        // rather than losing it to a typo in a template file.
        Some(Err(error)) => ctx.entity_template_errors.push(error),
        None => {}
    }
    render_builtin(html, entity, ctx);
}

/// The built-in rendering, which every type gets without configuring anything.
fn render_builtin(html: &mut String, entity: &EntityBody, ctx: &mut RenderCtx<'_>) {
    let entity_type = ctx.schema.entity_type(&entity.type_name);
    let type_label = entity_type.map_or(entity.type_name.as_str(), |t| t.display_label());

    let _ = write!(
        html,
        "<div class=\"entity entity-{}\" id=\"{}\">",
        html_escape::encode_double_quoted_attribute(&entity.type_name),
        html_escape::encode_double_quoted_attribute(&entity_anchor(entity.id.as_str()))
    );

    render_header(html, entity, type_label);

    // Collapsed, an entity shows its header and its leading prose; everything
    // else is folded away. The split is forced rather than chosen: a closed
    // `<details>` hides *every* child but its `<summary>` and no rule exempts
    // one, so what stays visible has to sit outside the element. That is also
    // why the prose moves above the attribute table here — the visible part
    // must be contiguous at the top — while the flat rendering keeps the
    // table first.
    let ordered = sections_in_render_order(entity, ctx.schema);
    if ctx.collapse_entities {
        let (visible, folded) = split_at_first_named_section(&ordered);
        render_sections(html, visible.iter().copied(), ctx);
        html.push_str(
            "<details class=\"entity-more\">\
             <summary class=\"entity-more-toggle\">Details</summary>",
        );
        render_attributes(html, entity, ctx);
        render_sections(html, folded.iter().copied(), ctx);
        render_links(html, entity, ctx);
        html.push_str("</details>");
    } else {
        render_attributes(html, entity, ctx);
        render_sections(html, ordered.into_iter(), ctx);
        render_links(html, entity, ctx);
    }

    html.push_str("</div>");
}

/// An entity's sections in the order they render: its unnamed prose first, then
/// its named sections in the order the *schema* declares them.
///
/// Declared order rather than document order, so two entities of one type are
/// comparable however their authors happened to write them — the same reason
/// attributes and relations follow the declaration. Within one name, document
/// order is kept, since that is the only order a `multiple` section has.
///
/// A section the schema does not declare cannot normally reach here, but is
/// appended rather than dropped: losing an author's prose to a lookup miss
/// would be far worse than showing it last.
fn sections_in_render_order<'a>(
    entity: &'a EntityBody,
    schema: &rusty_sphinx_entity::EntitySchema,
) -> Vec<&'a EntitySection> {
    let declared: Vec<&str> = schema
        .entity_type(&entity.type_name)
        .map(|t| t.sections.iter().map(|s| s.name.as_str()).collect())
        .unwrap_or_default();

    let mut ordered: Vec<&EntitySection> = entity
        .sections
        .iter()
        .filter(|s| s.name().is_none())
        .collect();
    for name in &declared {
        ordered.extend(entity.sections.iter().filter(|s| s.name() == Some(*name)));
    }
    ordered.extend(
        entity
            .sections
            .iter()
            .filter(|s| s.name().is_some_and(|name| !declared.contains(&name))),
    );
    ordered
}

/// Splits sections already in render order into the prose a collapsed entity
/// keeps visible and the named sections it folds away.
fn split_at_first_named_section<'a>(
    sections: &'a [&'a EntitySection],
) -> (&'a [&'a EntitySection], &'a [&'a EntitySection]) {
    let first_named = sections
        .iter()
        .position(|section| section.name().is_some())
        .unwrap_or(sections.len());
    sections.split_at(first_named)
}

/// The type label, the title and the id.
fn render_header(html: &mut String, entity: &EntityBody, type_label: &str) {
    let _ = write!(
        html,
        "<p class=\"entity-header\"><span class=\"entity-type\">{}</span>",
        html_escape::encode_text(type_label)
    );
    if let Some(title) = entity.title() {
        let _ = write!(
            html,
            "<span class=\"entity-title\">{}</span>",
            html_escape::encode_text(&title)
        );
    }
    let _ = write!(
        html,
        "<span class=\"entity-id\">{}</span></p>",
        html_escape::encode_text(entity.id.as_str())
    );
}

/// The attribute table, skipping the title already shown in the header.
fn render_attributes(html: &mut String, entity: &EntityBody, ctx: &RenderCtx<'_>) {
    let entity_type = ctx.schema.entity_type(&entity.type_name);

    // Declared order, not the map's key order, so two entities of one type put
    // the same field in the same place. An attribute the schema does not
    // declare is appended rather than dropped, for the same reason a section is.
    let declared: Vec<&str> = entity_type
        .map(|t| t.attributes.iter().map(|a| a.name.as_str()).collect())
        .unwrap_or_default();
    let ordered = declared
        .iter()
        .filter_map(|name| Some((*name, entity.attributes.get(*name)?)))
        .chain(
            entity
                .attributes
                .iter()
                .filter(|(name, _)| !declared.contains(&name.as_str()))
                .map(|(name, value)| (name.as_str(), value)),
        );

    let rows: Vec<(&str, String)> = ordered
        .filter(|(name, _)| *name != "title")
        .map(|(name, value)| {
            let label = entity_type
                .and_then(|t| t.attribute(name))
                .map_or(name, |a| a.display_label());
            (label, value.to_string())
        })
        .collect();
    if rows.is_empty() {
        return;
    }

    html.push_str("<table class=\"entity-attributes\"><tbody>");
    for (label, value) in rows {
        let _ = write!(
            html,
            "<tr><th>{}</th><td>{}</td></tr>",
            html_escape::encode_text(label),
            html_escape::encode_text(&value)
        );
    }
    html.push_str("</tbody></table>");
}

/// Every section, in the order it was written.
fn render_sections<'a>(
    html: &mut String,
    sections: impl Iterator<Item = &'a EntitySection>,
    ctx: &mut RenderCtx<'_>,
) {
    for section in sections {
        match section.name() {
            None => {
                html.push_str("<div class=\"entity-content\">");
                super::render_nodes(html, &section.body, ctx);
                html.push_str("</div>");
            }
            Some(name) => {
                let label = ctx
                    .schema
                    .types()
                    .iter()
                    .find_map(|t| t.section(name))
                    .map_or(name, |s| s.display_label());
                let _ = write!(
                    html,
                    "<div class=\"entity-section entity-section-{}\"><p class=\"entity-section-label\">{}</p>",
                    html_escape::encode_double_quoted_attribute(name),
                    html_escape::encode_text(label)
                );
                super::render_nodes(html, &section.body, ctx);
                html.push_str("</div>");
            }
        }
    }
}

/// The outgoing relations and the derived incoming ones.
fn render_links(html: &mut String, entity: &EntityBody, ctx: &RenderCtx<'_>) {
    let Some(entity_type) = ctx.schema.entity_type(&entity.type_name) else {
        return;
    };

    let mut lists = String::new();
    for relation in &entity_type.relations {
        let targets = entity.relation_targets(&relation.name);
        if targets.is_empty() {
            continue;
        }
        render_link_list(
            &mut lists,
            relation.display_label(),
            targets.iter().map(rusty_sphinx_ast::EntityId::as_str),
            ctx,
        );
    }

    // Incoming links come from the index, not from this node: they are a fact
    // about the whole project, which is exactly why they are derived there.
    if let Some(incoming) = ctx.index.entity_backlinks.get(&entity.id) {
        for backlink in ctx.schema.backlinks_for(&entity.type_name) {
            let Some(sources) = incoming.get(&backlink.name) else {
                continue;
            };
            render_link_list(
                &mut lists,
                &backlink.label,
                sources.iter().map(rusty_sphinx_ast::EntityId::as_str),
                ctx,
            );
        }
    }

    if !lists.is_empty() {
        let _ = write!(html, "<div class=\"entity-links\">{lists}</div>");
    }
}

/// One labelled list of links to other entities.
fn render_link_list<'t>(
    html: &mut String,
    label: &str,
    targets: impl Iterator<Item = &'t str>,
    ctx: &RenderCtx<'_>,
) {
    let mut items = String::new();
    for target in targets {
        // A target that resolves links; one that does not is shown as plain
        // text. The diagnostic for it was already raised by the index phase,
        // which is the only place that can see the whole graph — reporting it
        // again here would double every message.
        match rusty_sphinx_ast::EntityId::new(target)
            .ok()
            .and_then(|id| ctx.index.entities.get(&id))
        {
            Some(record) => {
                let _ = write!(
                    items,
                    "<li><a class=\"reference internal\" href=\"{}\">{}</a></li>",
                    html_escape::encode_double_quoted_attribute(&entity_href(
                        record,
                        ctx.doc_path,
                        target
                    )),
                    html_escape::encode_text(target)
                );
            }
            None => {
                let _ = write!(
                    items,
                    "<li><span class=\"broken-link\">{}</span></li>",
                    html_escape::encode_text(target)
                );
            }
        }
    }
    let _ = write!(
        html,
        "<p class=\"entity-link-label\">{}</p><ul class=\"entity-link-list\">{items}</ul>",
        html_escape::encode_text(label)
    );
}

#[cfg(test)]
mod tests;

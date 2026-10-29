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

use rinx_ast::{EntityBody, EntityId, EntitySection};
use rinx_index::{EntityFields, OwnFields, ShownLinks};

use crate::RenderCtx;
use crate::inline::entity_reference::entity_href;

/// What `entity` holds as written, for the index's effective values to fall
/// back to.
fn own_fields(entity: &EntityBody) -> OwnFields<'_> {
    OwnFields {
        attributes: &entity.attributes,
        relations: &entity.relations,
    }
}

/// The value to show for one attribute of `entity` — the merged project
/// index's *effective* value (the entity's own value, or a
/// `.. entity-update::`/`.. needextend::`'s current value when one touched
/// this field) rather than `entity.attributes` directly.
///
/// This is the fix that makes an update's effect visible on the entity's own
/// page at all: `entity-table`/`entity-flow`/`entity-pie`/a diagram's
/// `filter()` already read exclusively through
/// [`rinx_index::EntitySubject`], which already does this, but
/// nothing about the built-in rendering read the index before this existed —
/// there was nothing else to read. See `docs/decisions/019-entity-update.md`.
///
/// An entity the index does not hold shows its written value: one whose id
/// two documents declare is in `ambiguous_definitions` instead, and each of
/// its declarations is still drawn.
fn effective_attribute<'a>(
    entity: &'a EntityBody,
    name: &str,
    ctx: &'a RenderCtx<'_>,
) -> Option<&'a rinx_ast::AttributeValue> {
    own_fields(entity).effective_attribute(entity.id.value(), name, ctx.index)
}

/// The targets to show for one outgoing relation of `entity`, same rule as
/// [`effective_attribute`].
fn effective_relation_targets<'a>(
    entity: &'a EntityBody,
    relation: &str,
    ctx: &'a RenderCtx<'_>,
) -> &'a [EntityId] {
    own_fields(entity).effective_relation_targets(entity.id.value(), relation, ctx.index)
}

/// Wraps `value` in the visible conflict marker when `conflict` is set: the
/// counterpart, for a reader of the page, to the
/// `entity-update.conflicting-update` diagnostic.
fn mark_if_conflicting(value: &str, conflict: bool) -> String {
    let escaped = html_escape::encode_text(value);
    if conflict {
        format!(
            "<span class=\"entity-conflict\" title=\"Set by conflicting .. entity-update:: directives\">{escaped}</span>"
        )
    } else {
        escaped.into_owned()
    }
}

/// The anchor an entity is linked by.
///
/// Re-exported from `rinx_index` rather than defined here: a
/// templated diagram's generated node links have to land on exactly this
/// anchor, and that text is built by a phase that cannot depend on the
/// renderer.
pub(crate) use rinx_index::entity_anchor;

mod template;

pub use template::{EntityTemplates, load_entity_templates};

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
    let fields = EntityFields::of(
        entity.id.value(),
        &entity.type_name,
        own_fields(entity),
        ctx.index,
        ctx.schema,
    );

    let _ = write!(
        html,
        "<div class=\"entity entity-{}\" id=\"{}\">",
        html_escape::encode_double_quoted_attribute(&entity.type_name),
        html_escape::encode_double_quoted_attribute(&entity_anchor(entity.id.value().as_str()))
    );

    render_header(html, entity, fields.type_label);

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
        render_attributes(html, &fields);
        render_sections(html, folded.iter().copied(), ctx);
        render_links(html, &fields, ctx);
        html.push_str("</details>");
    } else {
        render_attributes(html, &fields);
        render_sections(html, ordered.into_iter(), ctx);
        render_links(html, &fields, ctx);
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
    schema: &rinx_entity::EntitySchema,
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
        html_escape::encode_text(entity.id.value().as_str())
    );
}

/// The attribute table; the title is already shown in the header.
fn render_attributes(html: &mut String, fields: &EntityFields<'_>) {
    if fields.attributes.is_empty() {
        return;
    }
    html.push_str("<table class=\"entity-attributes\"><tbody>");
    for attribute in &fields.attributes {
        let _ = write!(
            html,
            "<tr><th>{}</th><td>{}</td></tr>",
            html_escape::encode_text(attribute.label),
            mark_if_conflicting(&attribute.value.to_string(), attribute.conflict)
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
fn render_links(html: &mut String, fields: &EntityFields<'_>, ctx: &RenderCtx<'_>) {
    let mut lists = String::new();
    for links in fields.relations.iter().chain(&fields.backlinks) {
        render_link_list(&mut lists, links, ctx);
    }
    if !lists.is_empty() {
        let _ = write!(html, "<div class=\"entity-links\">{lists}</div>");
    }
}

/// One labelled list of links to other entities. A conflict marks the whole
/// list — a relation conflict changes which targets are on it, not one
/// specific link — the same way [`mark_if_conflicting`] marks an attribute's
/// value.
fn render_link_list(html: &mut String, links: &ShownLinks<'_>, ctx: &RenderCtx<'_>) {
    let mut items = String::new();
    for target in links.targets.iter().map(EntityId::as_str) {
        // A target that resolves links; one that does not is shown as plain
        // text. The diagnostic for it was already raised by the index phase,
        // which is the only place that can see the whole graph — reporting it
        // again here would double every message.
        match rinx_ast::EntityId::new(target)
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
    let label_html = mark_if_conflicting(links.label, links.conflict);
    let _ = write!(
        html,
        "<p class=\"entity-link-label\">{label_html}</p><ul class=\"entity-link-list\">{items}</ul>",
    );
}

#[cfg(test)]
mod tests;

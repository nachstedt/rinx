//! The table markup, over the same shell every other table directive uses.

use std::fmt::Write as _;

use rinx_ast::EntityTable;
use rinx_filter::FieldName;

use crate::RenderCtx;
use crate::blocks::table_shell::{
    render_table_colgroup, render_table_name_anchor, render_table_open_tag,
};
use crate::empty_listing_error::EmptyListingError;
use crate::inline::entity_reference::entity_href;

use super::rows::{Cell, Row, select_rows};

/// The class every entity table carries, whichever name it was written under.
///
/// One class for both spellings deliberately: two documents using different
/// names for one directive should not need two stylesheets.
const TABLE_CLASS: &str = "entity-table";

/// Renders a `.. entity-table::` / `.. needtable::`.
pub(in crate::blocks) fn render_entity_table(
    html: &mut String,
    table: &EntityTable,
    ctx: &mut RenderCtx<'_>,
) {
    let rows = select_rows(table, ctx.index, ctx.schema);
    if rows.is_empty() {
        ctx.empty_listing_errors
            .push(EmptyListingError::table(table.source.as_str(), table.span));
    }

    render_table_name_anchor(html, table.name.as_ref());

    let mut classes = vec![TABLE_CLASS.to_string()];
    classes.extend(table.classes.iter().cloned());
    render_table_open_tag(html, &classes, table.align, table.width.as_deref());
    render_table_colgroup(html, table.widths.as_ref());

    render_header(html, &table.columns, ctx);

    let _ = writeln!(html, "<tbody>");
    for (_, row) in &rows {
        render_row(html, row, ctx);
    }
    let _ = writeln!(html, "</tbody>");
    let _ = writeln!(html, "</table>");
}

/// The heading row, labelled the way the schema declares.
///
/// A column's heading is never a second copy of a label held here: an
/// attribute's is its `label`, a relation's is its own, and a back-link's is
/// the `incoming_label` declared on the type at the *other* end. Only the
/// built-in fields, which no schema declares, are named here.
fn render_header(html: &mut String, columns: &[FieldName], ctx: &RenderCtx<'_>) {
    let _ = writeln!(html, "<thead>");
    let _ = writeln!(html, "<tr>");
    for column in columns {
        let label = column_label(column, ctx);
        let _ = writeln!(
            html,
            "<th class=\"head\">{}</th>",
            html_escape::encode_text(&label)
        );
    }
    let _ = writeln!(html, "</tr>");
    let _ = writeln!(html, "</thead>");
}

/// The heading `column` is shown under.
fn column_label(column: &FieldName, ctx: &RenderCtx<'_>) -> String {
    let name = column.as_str();
    if let Some(builtin) = builtin_label(name) {
        return builtin.to_string();
    }
    for entity_type in ctx.schema.types() {
        if let Some(attribute) = entity_type.attribute(name) {
            return attribute.display_label().to_string();
        }
        if let Some(relation) = entity_type.relation(name) {
            return relation.display_label().to_string();
        }
        if let Some(backlink) = ctx
            .schema
            .backlinks_for(&entity_type.name)
            .iter()
            .find(|backlink| backlink.name == name)
        {
            return backlink.label.clone();
        }
    }
    name.to_string()
}

/// The heading for a field every entity has.
///
/// The one place these are spelled in prose, since no schema declares them.
/// `type` and `type_name` share a heading on purpose: to a reader both columns
/// are "the type", and they differ only in whether the cell shows the
/// directive name or the schema's label — see `rinx_entity::field`.
fn builtin_label(name: &str) -> Option<&'static str> {
    match name {
        "id" => Some("ID"),
        "type" | "type_name" => Some("Type"),
        "title" => Some("Title"),
        "docname" => Some("Document"),
        _ => None,
    }
}

/// One entity's row.
fn render_row(html: &mut String, row: &Row, ctx: &RenderCtx<'_>) {
    let _ = writeln!(html, "<tr>");
    for cell in &row.cells {
        let _ = writeln!(html, "<td>{}</td>", cell_html(cell, ctx));
    }
    let _ = writeln!(html, "</tr>");
}

/// One cell's inner HTML.
fn cell_html(cell: &Cell, ctx: &RenderCtx<'_>) -> String {
    match cell {
        Cell::Text(text) => html_escape::encode_text(text).to_string(),
        Cell::IdLink(id) => link_html(id, LinkText::Id, ctx),
        Cell::Links(ids) => ids
            .iter()
            .map(|id| link_html(id, LinkText::Title, ctx))
            .collect::<Vec<String>>()
            .join(", "),
    }
}

/// What a link to an entity shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkText {
    /// The entity's id, verbatim.
    Id,
    /// Its title, falling back to its id when it has none.
    Title,
}

/// One link to an entity.
///
/// Goes through the same [`entity_href`] the `:entity:` role and the entity
/// box already use, so no two phases can disagree about where a link points.
/// An id naming no entity is shown as plain text rather than as a dead link.
fn link_html(id: &str, shows: LinkText, ctx: &RenderCtx<'_>) -> String {
    let Some((entity_id, record)) = rinx_ast::EntityId::new(id).ok().and_then(|parsed| {
        ctx.index
            .entities
            .get(&parsed)
            .map(|record| (parsed, record))
    }) else {
        return html_escape::encode_text(id).to_string();
    };
    let text = match shows {
        LinkText::Id => entity_id.as_str(),
        LinkText::Title => record.display_text(&entity_id),
    };
    format!(
        "<a class=\"reference internal\" href=\"{}\">{}</a>",
        html_escape::encode_double_quoted_attribute(&entity_href(record, ctx.doc_path, id)),
        html_escape::encode_text(text)
    )
}

#[cfg(test)]
mod tests;

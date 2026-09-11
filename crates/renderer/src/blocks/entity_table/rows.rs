//! Turning the project index into the rows a table shows.
//!
//! Split from the markup for the reason the toctree renderer splits expansion
//! from `write_nav_list`: selecting and ordering is where every decision is,
//! and it is testable without reading a byte of HTML.

use rusty_sphinx_ast::{EntityId, EntityTable};
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_filter::{FieldName, FieldValue, FilterSubject};
use rusty_sphinx_index::{EntityRecord, EntitySubject, ProjectIndex};

/// One cell of a rendered row.
///
/// A cell knows whether its content is text or links, and nothing about how
/// either is marked up. The two link kinds are separate variants rather than
/// one carrying a flag, because they differ in the only thing a link cell
/// decides: what text it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Cell {
    /// Plain text — an attribute value, a type name, a document path.
    Text(String),
    /// A link to one entity, shown by its **id**. The `id` column: a reader
    /// asking for ids wants to read ids, however well-titled the entity is.
    IdLink(String),
    /// Links to entities, each shown by its title and falling back to its id.
    /// Empty renders as an empty cell rather than as an empty list.
    Links(Vec<String>),
}

/// One entity's row.
pub(super) struct Row {
    pub cells: Vec<Cell>,
}

/// The rows `table` shows, in the order it shows them.
///
/// Iterating `index.entities` gives id order for free — it is a `BTreeMap` —
/// so an unsorted table is already deterministic rather than merely
/// consistent, which matters because a page is a cached build artefact.
pub(super) fn select_rows(
    table: &EntityTable,
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> Vec<(EntityId, Row)> {
    let mut selected: Vec<(&EntityId, &EntityRecord)> = index
        .entities
        .iter()
        .filter(|(id, record)| {
            let subject = subject_for(id, record, index, schema);
            table
                .filter
                .as_ref()
                .is_none_or(|filter| filter.matches(&subject))
        })
        .collect();

    if let Some(sort) = &table.sort {
        sort_rows(&mut selected, sort, index, schema);
    }

    selected
        .into_iter()
        .map(|(id, record)| {
            let subject = subject_for(id, record, index, schema);
            let cells = table
                .columns
                .iter()
                .map(|column| cell_for(column, &subject))
                .collect();
            (id.clone(), Row { cells })
        })
        .collect()
}

/// Orders `selected` by `sort`, keeping id order among equal keys.
///
/// Sorting on the *rendered* text rather than the stored value: a column
/// showing `open` before `closed` while claiming to be sorted would be a
/// worse answer than sorting the way the reader sees it. Entities missing the
/// field sort last, since an absent value has no place in an order.
fn sort_rows(
    selected: &mut [(&EntityId, &EntityRecord)],
    sort: &FieldName,
    index: &ProjectIndex,
    schema: &EntitySchema,
) {
    selected.sort_by_cached_key(|(id, record)| {
        let subject = subject_for(id, record, index, schema);
        let value = subject.field(sort);
        let missing = value == FieldValue::Missing;
        (missing, sort_key(&value), (*id).clone())
    });
}

/// The text a value orders by.
fn sort_key(value: &FieldValue) -> String {
    match value {
        FieldValue::Missing => String::new(),
        FieldValue::List(items) => items.join(", "),
        other => other.as_text().unwrap_or_default(),
    }
}

/// One cell's content.
fn cell_for(column: &FieldName, subject: &EntitySubject<'_>) -> Cell {
    // The id and the title both link to the entity, as sphinx-needs' own table
    // does: a listing whose rows cannot be reached is a report, not navigation.
    // They differ in what they show — the id column shows the id.
    if column.as_str() == "id" {
        return Cell::IdLink(subject.id.to_string());
    }
    if column.as_str() == "title" {
        return Cell::Links(vec![subject.id.to_string()]);
    }
    match subject.field(column) {
        FieldValue::Missing => Cell::Text(String::new()),
        FieldValue::List(items) => links_or_text(items, subject),
        other => Cell::Text(other.as_text().unwrap_or_default()),
    }
}

/// A list-valued cell: links when every item names an entity, text otherwise.
///
/// Deciding per cell rather than per column, because the two list-shaped
/// fields are indistinguishable by name: `tags` holds words and `verifies`
/// holds ids, and only the index can tell them apart. An id that names no
/// entity stays text rather than becoming a dead link — the index phase has
/// already reported it as `entity.unknown-target`, and reporting it again from
/// every table that lists it would multiply one fault by its readers.
fn links_or_text(items: Vec<String>, subject: &EntitySubject<'_>) -> Cell {
    let all_resolve = !items.is_empty()
        && items.iter().all(|item| {
            EntityId::new(item).is_ok_and(|id| subject.index.entities.contains_key(&id))
        });
    if all_resolve {
        Cell::Links(items)
    } else {
        Cell::Text(items.join(", "))
    }
}

/// The subject one entity presents to a filter and to the cell builder.
fn subject_for<'a>(
    id: &'a EntityId,
    record: &'a EntityRecord,
    index: &'a ProjectIndex,
    schema: &'a EntitySchema,
) -> EntitySubject<'a> {
    EntitySubject {
        id,
        record,
        index,
        schema,
    }
}

#[cfg(test)]
mod tests;

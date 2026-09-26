//! The listing directive's node: what it selects, what it shows, how it looks.

use rinx_filter::{Expr, FieldName};
use serde::{Deserialize, Serialize};

use crate::entity_table::source::EntityTableSource;
use crate::span::Span;
use crate::table::{TableAlign, TableWidths};
use crate::target_name::TargetName;

/// A table of the entities matching a filter — `.. entity-table::`, and its
/// sphinx-needs spelling `.. needtable::`.
///
/// The node carries a *question*, not an answer: which entities to show and
/// which of their fields, with the rows resolved against the project index
/// while rendering. It cannot be otherwise — the entities a table lists are
/// written in documents this one has never heard of, and only the index knows
/// them all.
///
/// The filter arrives here already parsed, which is the whole reason
/// `rinx_filter` exists as a crate. Parsing it in the *parser* is what
/// lets a broken expression be reported at the column it breaks at, since that
/// is the only phase still holding the option line's own position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityTable {
    /// Which of the directive's two names was written.
    pub source: EntityTableSource,
    /// `:filter:` — which entities to list. `None` lists every entity in the
    /// project, which is what an omitted filter means in sphinx-needs too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Expr>,
    /// `:columns:` — the fields to show, in the order written.
    ///
    /// Never empty: an omitted option is resolved to the default set while
    /// parsing, so no later phase has a second copy of what the default is.
    pub columns: Vec<FieldName>,
    /// `:sort:` — the field to order rows by. `None` orders by entity id,
    /// which the index's own `BTreeMap` ordering already provides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<FieldName>,
    /// `:widths:`, or sphinx-needs' `:colwidths:` spelling of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widths: Option<TableWidths>,
    /// `:width:` — an opaque CSS length, re-emitted as a `style` attribute the
    /// way every other table directive does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TableAlign>,
    /// `:class:` — space-separated class names, already split.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// `:name:` — reuses [`TargetName`] like every other table directive, so
    /// it registers in `ProjectIndex::targets` with no conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<TargetName>,
    /// Where the directive was written, so the renderer — which is where a
    /// table's rows are actually resolved — has a position to report against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// The columns shown when `:columns:` is omitted.
///
/// Deliberately shorter than sphinx-needs' default, which also shows status,
/// tags and outgoing links: those are attributes only *some* schemas declare,
/// and a column that is empty for every row of a project's first table teaches
/// the wrong thing about the feature. These three exist for every entity of
/// every schema.
pub const DEFAULT_COLUMNS: [&str; 3] = ["id", "type", "title"];

impl EntityTable {
    /// A table with the default columns, no filter and no options.
    ///
    /// # Panics
    ///
    /// Never: [`DEFAULT_COLUMNS`] are compile-time constants that satisfy
    /// [`FieldName`]'s rules, and a test pins that.
    #[must_use]
    pub fn new(source: EntityTableSource) -> Self {
        Self {
            source,
            filter: None,
            columns: Self::default_columns(),
            sort: None,
            widths: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: None,
            span: None,
        }
    }

    /// [`DEFAULT_COLUMNS`] as field names.
    ///
    /// # Panics
    ///
    /// Never, for the reason [`Self::new`] gives.
    #[must_use]
    pub fn default_columns() -> Vec<FieldName> {
        DEFAULT_COLUMNS
            .iter()
            .map(|name| FieldName::new(name).expect("default column names are valid field names"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_new_table_shows_the_default_columns() {
        // Given
        let source = EntityTableSource::EntityTable;

        // When
        let table = EntityTable::new(source);

        // Then
        let shown: Vec<String> = table.columns.iter().map(ToString::to_string).collect();
        assert_eq!(shown, ["id", "type", "title"]);
    }

    #[test]
    fn test_a_new_table_selects_everything_and_carries_no_options() {
        // Given
        let source = EntityTableSource::NeedTable;

        // When
        let table = EntityTable::new(source);

        // Then
        assert_eq!(table.filter, None);
        assert_eq!(table.sort, None);
        assert_eq!(table.widths, None);
        assert!(table.classes.is_empty());
    }

    #[test]
    fn test_every_default_column_is_a_legal_field_name() {
        // Given — `new` would panic otherwise, so this pins the promise its
        // doc comment makes
        let names = DEFAULT_COLUMNS;

        // When
        let parsed: Vec<bool> = names
            .iter()
            .map(|name| FieldName::new(name).is_ok())
            .collect();

        // Then
        assert_eq!(parsed, [true, true, true]);
    }

    #[test]
    fn test_a_table_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let mut table = EntityTable::new(EntityTableSource::NeedTable);
        table.filter = Some(rinx_filter::parse_filter(r#"type == "req""#).unwrap());
        table.sort = Some(FieldName::new("id").unwrap());
        table.classes = vec!["wide".to_string()];

        // When
        let json = serde_json::to_string(&table).unwrap();
        let decoded: EntityTable = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, table);
    }

    #[test]
    fn test_absent_options_are_left_out_of_the_serialized_form() {
        // Given — a `.ast` is a build artefact stored per document, so an
        // unset option should cost nothing
        let table = EntityTable::new(EntityTableSource::EntityTable);

        // When
        let json = serde_json::to_string(&table).unwrap();

        // Then
        assert!(!json.contains("filter"));
        assert!(!json.contains("widths"));
        assert!(!json.contains("span"));
    }
}

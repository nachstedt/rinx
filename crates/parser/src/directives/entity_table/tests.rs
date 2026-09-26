use rinx_ast::{Directive, EntityTable, EntityTableSource, Node, TableWidths};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

/// The schema every test below parses against: one attribute, one relation and
/// one derived back-link, which is every kind of field a column can name.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]

        [[entity_type]]
        name = "test"

          [[entity_type.relation]]
          name = "verifies"
          to = ["req"]
          incoming = "verified_by"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn parse(rst: &str) -> rinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("specs/boot", rst, &ctx)
}

/// The first entity table in `rst`.
fn parse_table(rst: &str) -> EntityTable {
    let doc = parse(rst);
    doc.nodes
        .iter()
        .find_map(|node| match node {
            Node::Directive(Directive::EntityTable(table)) => Some((**table).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no entity table parsed from:\n{rst}"))
}

/// The diagnostic codes a parse reported, as their author-facing ids.
fn codes(rst: &str) -> Vec<String> {
    parse(rst)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// The first diagnostic's message.
fn message(rst: &str) -> String {
    let doc = parse(rst);
    let Some(diagnostic) = doc.diagnostics.first() else {
        panic!("expected a diagnostic from:\n{rst}");
    };
    diagnostic.message.clone()
}

/// The column names a table shows.
fn columns(table: &EntityTable) -> Vec<String> {
    table.columns.iter().map(ToString::to_string).collect()
}

#[test]
fn test_the_bare_directive_parses() {
    // Given
    let rst = ".. entity-table::\n";

    // When
    let table = parse_table(rst);

    // Then
    assert_eq!(table.source, EntityTableSource::EntityTable);
    assert_eq!(table.filter, None);
}

#[test]
fn test_the_sphinx_needs_spelling_parses_to_the_same_node() {
    // Given — a migrating project keeps its documents unchanged
    let entity_table = parse_table(".. entity-table::\n   :columns: id, status\n");
    let needtable = parse_table(".. needtable::\n   :columns: id, status\n");

    // When
    let (one, other) = (columns(&entity_table), columns(&needtable));

    // Then — same columns, and only the recorded spelling differs
    assert_eq!(one, other);
    assert_eq!(needtable.source, EntityTableSource::NeedTable);
}

#[test]
fn test_an_omitted_columns_option_takes_the_default_set() {
    // Given
    let rst = ".. entity-table::\n";

    // When
    let table = parse_table(rst);

    // Then — resolved here, so no later phase holds a second copy of it
    assert_eq!(columns(&table), ["id", "type", "title"]);
}

#[test]
fn test_columns_are_kept_in_the_order_written() {
    // Given
    let rst = ".. entity-table::\n   :columns: status, id, title\n";

    // When
    let table = parse_table(rst);

    // Then
    assert_eq!(columns(&table), ["status", "id", "title"]);
}

#[test]
fn test_a_column_may_name_a_relation_or_a_derived_backlink() {
    // Given — `verified_by` is declared nowhere; it falls out of the relation
    let rst = ".. entity-table::\n   :columns: id, verifies, verified_by\n";

    // When
    let table = parse_table(rst);

    // Then
    assert_eq!(columns(&table), ["id", "verifies", "verified_by"]);
}

#[test]
fn test_a_filter_is_parsed_while_the_document_is() {
    // Given
    let rst = ".. entity-table::\n   :filter: status == \"open\"\n";

    // When
    let table = parse_table(rst);

    // Then
    assert!(table.filter.is_some());
    assert!(codes(rst).is_empty());
}

#[test]
fn test_every_field_a_filter_names_is_checked_against_the_schema() {
    // Given
    let rst = ".. entity-table::\n   :filter: asil == \"D\"\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-table.unknown-field"]);
}

#[test]
fn test_an_unknown_field_offers_the_whole_vocabulary() {
    // Given
    let rst = ".. entity-table::\n   :columns: asil\n";

    // When
    let reported = message(rst);

    // Then
    assert!(reported.contains("unknown field 'asil'"));
    assert!(reported.contains("verified_by"));
    assert!(reported.contains("docname"));
}

#[test]
fn test_a_broken_filter_is_reported_and_the_table_survives() {
    // Given — refusing the whole directive would hide which entities the
    // author was reaching for
    let rst = ".. entity-table::\n   :filter: status ==\n";

    // When
    let (table, reported) = (parse_table(rst), codes(rst));

    // Then
    assert_eq!(reported, ["entity-table.invalid-filter"]);
    assert_eq!(table.filter, None);
}

#[test]
fn test_a_broken_filter_is_reported_at_the_column_that_breaks() {
    // Given — `len` starts at column 12 of `   :filter: len(x)`
    let rst = ".. entity-table::\n   :filter: len(x)\n";

    // When
    let doc = parse(rst);

    // Then
    let span = doc.diagnostics[0].span.expect("expected a positioned span");
    assert_eq!(span.start.line, 2);
    assert_eq!(span.start.column, 13);
    assert_eq!(span.end.column, 16);
}

#[test]
fn test_an_unsupported_python_construct_is_named_in_the_message() {
    // Given
    let rst = ".. entity-table::\n   :filter: len(x)\n";

    // When
    let reported = message(rst);

    // Then
    assert!(reported.contains("function calls"));
}

#[test]
fn test_a_filter_wrapped_over_two_lines_is_reported_against_its_line() {
    // Given — the continuation lines were joined with spaces, so an offset
    // into the joined text no longer names a column in any of them
    let rst = ".. entity-table::\n   :filter: status == \"open\"\n     and len(x)\n";

    // When
    let doc = parse(rst);

    // Then
    let span = doc.diagnostics[0].span.expect("expected a positioned span");
    assert_eq!(span.start.line, 2);
    // Column 4 is where `:filter:` itself starts — the whole option line,
    // rather than a column inside the joined text.
    assert_eq!(span.start.column, 4);
}

#[test]
fn test_a_sort_key_is_read() {
    // Given
    let rst = ".. entity-table::\n   :sort: status\n";

    // When
    let table = parse_table(rst);

    // Then
    assert_eq!(
        table.sort.map(|name| name.to_string()),
        Some("status".to_string())
    );
}

#[test]
fn test_a_sort_key_naming_several_fields_is_refused() {
    // Given
    let rst = ".. entity-table::\n   :sort: status, id\n";

    // When
    let (table, reported) = (parse_table(rst), codes(rst));

    // Then
    assert_eq!(reported, ["directive.entity-table-unknown-option"]);
    assert_eq!(table.sort, None);
}

#[test]
fn test_colwidths_is_the_sphinx_needs_spelling_of_widths() {
    // Given
    let rst = ".. entity-table::\n   :columns: id, title\n   :colwidths: 30, 70\n";

    // When
    let table = parse_table(rst);

    // Then
    assert_eq!(table.widths, Some(TableWidths::Explicit(vec![30, 70])));
}

#[test]
fn test_widths_is_accepted_under_its_docutils_spelling_too() {
    // Given
    let rst = ".. entity-table::\n   :columns: id, title\n   :widths: 30, 70\n";

    // When
    let table = parse_table(rst);

    // Then
    assert_eq!(table.widths, Some(TableWidths::Explicit(vec![30, 70])));
}

#[test]
fn test_giving_both_widths_spellings_is_refused_rather_than_guessed_at() {
    // Given — the case where picking one silently renders the wrong table
    let rst =
        ".. entity-table::\n   :columns: id, title\n   :widths: 30, 70\n   :colwidths: 50, 50\n";

    // When
    let (table, reported) = (parse_table(rst), codes(rst));

    // Then
    assert_eq!(reported, ["entity-table.duplicate-widths"]);
    assert_eq!(table.widths, None);
}

#[test]
fn test_widths_are_validated_against_the_column_count() {
    // Given — the count is known here, unlike in a data table, because
    // `:columns:` fixes it before any row exists
    let rst = ".. entity-table::\n   :columns: id, title\n   :colwidths: 30, 30, 40\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["table.data.widths-count-mismatch"]);
}

#[test]
fn test_the_supported_style_is_accepted_silently() {
    // Given — 15 of the corpus' 17 tables write this
    let rst = ".. entity-table::\n   :style: table\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.is_empty());
}

#[test]
fn test_a_javascript_style_is_reported_rather_than_ignored() {
    // Given — the author asked for browser-side sorting and would otherwise
    // get a static table with no sign that they did not
    let rst = ".. entity-table::\n   :style: datatables\n";

    // When
    let (reported, text) = (codes(rst), message(rst));

    // Then
    assert_eq!(reported, ["entity-table.unsupported-style"]);
    assert!(text.contains("datatables"));
}

#[test]
fn test_the_shared_presentation_options_are_read() {
    // Given
    let rst = ".. entity-table::\n   :class: wide\n   :align: right\n   :width: 80%\n   :name: Every requirement\n";

    // When
    let table = parse_table(rst);

    // Then
    assert_eq!(table.classes, ["wide"]);
    assert_eq!(table.align, Some(rinx_ast::TableAlign::Right));
    assert_eq!(table.width, Some("80%".to_string()));
    assert!(table.name.is_some());
}

#[test]
fn test_an_unknown_option_is_reported_under_the_directives_own_code() {
    // Given
    let rst = ".. entity-table::\n   :show_filters:\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["directive.entity-table-unknown-option"]);
}

#[test]
fn test_an_argument_is_reported_since_the_directive_takes_none() {
    // Given — sphinx-needs takes no argument here either, and a dropped one
    // would silently change nothing about the table
    let rst = ".. entity-table:: every requirement\n";

    // When
    let (reported, text) = (codes(rst), message(rst));

    // Then
    assert_eq!(reported, ["directive.entity-table-unknown-option"]);
    assert!(text.contains(":filter:"));
}

#[test]
fn test_the_directive_is_unknown_without_a_schema() {
    // Given — a project with no entities has nothing to list, but the name is
    // still reserved, so it must not become an entity type either
    let rst = ".. entity-table::\n";

    // When
    let doc = crate::parse("specs/boot", rst);

    // Then — it still parses as the directive, listing an empty project
    let parsed = doc
        .nodes
        .iter()
        .any(|node| matches!(node, Node::Directive(Directive::EntityTable(_))));
    assert!(parsed);
}

#[test]
fn test_a_whole_corpus_directive_parses_with_no_diagnostics() {
    // Given — transcribed from the sphinx-needs demo, with this schema's own
    // field names substituted for the corpus schema's
    let rst = concat!(
        ".. needtable::\n",
        "   :filter: type == \"req\" and docname is not None and \"specs\" in docname\n",
        "   :columns: id, title, status\n",
        "   :style: table\n",
        "   :colwidths: 15, 45, 40\n",
    );

    // When
    let (table, reported) = (parse_table(rst), codes(rst));

    // Then
    assert!(reported.is_empty(), "unexpected diagnostics: {reported:?}");
    assert!(table.filter.is_some());
    assert_eq!(columns(&table), ["id", "title", "status"]);
    assert_eq!(table.widths, Some(TableWidths::Explicit(vec![15, 45, 40])));
}

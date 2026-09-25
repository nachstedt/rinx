//! `.. table::` directive rendering — wraps a grid or simple table with the
//! title/caption and presentation options neither ASCII-art syntax has
//! notation of its own for, sharing [`super::table_shell`]'s presentation
//! helpers with [`super::data_table`] and cell rendering with
//! [`super::tables`].

use rinx_ast::{TableAlign, TableRow, TableWidths, TargetName};
use std::fmt::Write as _;

use super::table_shell::{
    render_table_caption, render_table_colgroup, render_table_name_anchor, render_table_open_tag,
};
use crate::RenderCtx;

/// The fields `render_table_directive` needs, borrowed straight from
/// [`rinx_ast::Directive::Table`] — grouped into one struct (rather
/// than eight separate parameters) purely to keep the function's arity
/// reasonable, mirroring [`super::data_table::DataTableParams`].
#[derive(Clone, Copy)]
pub(super) struct TableDirectiveParams<'a> {
    pub title: Option<&'a str>,
    pub widths: Option<&'a TableWidths>,
    pub width: Option<&'a str>,
    pub align: Option<TableAlign>,
    pub classes: &'a [String],
    pub name: Option<&'a TargetName>,
    pub header_rows: &'a [TableRow],
    pub body_rows: &'a [TableRow],
}

/// Renders a `.. table::` directive as HTML. Unlike
/// [`super::data_table::render_data_table`], there is no directive-name class
/// to lead with — a bare grid/simple table has no name of its own — so the
/// `<table>` carries a `class` attribute only when `:class:`/`:align:`
/// actually contributed one, matching a bare wrapped table's plain
/// `<table>`.
pub(super) fn render_table_directive(
    html: &mut String,
    params: TableDirectiveParams<'_>,
    ctx: &mut RenderCtx<'_>,
) {
    let TableDirectiveParams {
        title,
        widths,
        width,
        align,
        classes,
        name,
        header_rows,
        body_rows,
    } = params;

    render_table_name_anchor(html, name);
    render_table_open_tag(html, classes, align, width);
    render_table_caption(html, title);
    render_table_colgroup(html, widths);

    if !header_rows.is_empty() {
        let _ = writeln!(html, "<thead>");
        for row in header_rows {
            super::tables::render_table_row(html, row, "th", ctx);
        }
        let _ = writeln!(html, "</thead>");
    }
    let _ = writeln!(html, "<tbody>");
    for row in body_rows {
        super::tables::render_table_row(html, row, "td", ctx);
    }
    let _ = writeln!(html, "</tbody>");
    let _ = writeln!(html, "</table>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::table_test_support::{render_doc, table_row};
    use rinx_ast::{Directive, Document, Node};

    /// A minimal `.. table::` wrapping one header row and one body row, for
    /// the tests that care about nothing else.
    fn table_directive() -> Document {
        Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: None,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                header_rows: vec![table_row(&["Fruit", "Colour"])],
                body_rows: vec![table_row(&["Apple", "Red"])],
            })],
        )
    }

    #[test]
    fn test_render_table_directive_with_no_options_matches_a_bare_table() {
        // Given — no `:class:`/`:align:`, so the `<table>` should be exactly
        // as classless as a bare grid/simple `Node::Table` renders.
        let doc = table_directive();

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(
            result,
            "<table>\n\
             <thead>\n<tr>\n<th><p>Fruit</p>\n</th>\n<th><p>Colour</p>\n</th>\n</tr>\n</thead>\n\
             <tbody>\n<tr>\n<td><p>Apple</p>\n</td>\n<td><p>Red</p>\n</td>\n</tr>\n</tbody>\n\
             </table>\n"
        );
    }

    #[test]
    fn test_render_table_directive_without_header_rows_omits_thead() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: None,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                header_rows: vec![],
                body_rows: vec![table_row(&["only"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(!result.contains("<thead>"));
    }

    #[test]
    fn test_render_table_directive_title_produces_caption() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: Some("Truth table for \"not\"".to_string()),
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                header_rows: vec![],
                body_rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<caption>Truth table for \"not\"</caption>"));
    }

    #[test]
    fn test_render_table_directive_class_option_produces_class_attribute() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: None,
                widths: None,
                width: None,
                align: None,
                classes: vec!["custom".to_string()],
                name: None,
                header_rows: vec![],
                body_rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<table class=\"custom\">"), "{result}");
    }

    #[test]
    fn test_render_table_directive_align_option_produces_align_class() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: None,
                widths: None,
                width: None,
                align: Some(TableAlign::Center),
                classes: vec![],
                name: None,
                header_rows: vec![],
                body_rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(
            result.contains("<table class=\"align-center\">"),
            "{result}"
        );
    }

    #[test]
    fn test_render_table_directive_width_option_produces_inline_style() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: None,
                widths: None,
                width: Some("50%".to_string()),
                align: None,
                classes: vec![],
                name: None,
                header_rows: vec![],
                body_rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("style=\"width: 50%\""));
    }

    #[test]
    fn test_render_table_directive_widths_option_produces_colgroup() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: None,
                widths: Some(TableWidths::Explicit(vec![30, 70])),
                width: None,
                align: None,
                classes: vec![],
                name: None,
                header_rows: vec![],
                body_rows: vec![table_row(&["A", "B"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<colgroup>"));
        assert!(result.contains("<col style=\"width: 30.00%\" />"));
        assert!(result.contains("<col style=\"width: 70.00%\" />"));
    }

    #[test]
    fn test_render_table_directive_name_option_produces_anchor() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Table {
                title: None,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: Some(TargetName::new("wrapped-table")),
                header_rows: vec![],
                body_rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<a id=\"wrapped-table\"></a>"));
    }

    #[test]
    fn test_render_table_directive_uses_th_only_from_the_wrapped_tables_own_header_rows() {
        // Given — `.. table::` has no `:stub-columns:` of its own, so every
        // body-row cell must render as `<td>`, even the first column.
        let doc = table_directive();

        // When
        let result = render_doc(&doc);

        // Then
        assert!(!result.contains("scope=\"row\""));
        assert_eq!(result.matches("<td>").count(), 2);
    }
}

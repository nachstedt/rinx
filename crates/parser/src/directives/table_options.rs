//! The presentation options shared by every option-bearing table directive —
//! `.. list-table::`, `.. csv-table::` and `.. table::` alike.
//!
//! All three open with a run of `:name: value` lines before their real body.
//! [`super::options::scan_option_lines`] turns that run into name/value pairs
//! without judging them, and [`TableOption`] is the group of the five
//! presentation options all three share (`widths`, `width`, `align`, `class`,
//! `name`), read by [`read_table_option`] — each directive's own option enum
//! takes the group beside its own options (`.. list-table::`/`.. csv-table::`
//! add `header-rows`/`stub-columns`, see `data_table::options`), keyed by
//! directive name rather than `TableSource` so `.. table::` — which produces
//! no `Directive::DataTable` at all — can use it too.
//!
//! This lives as a flat sibling of `data_table` rather than inside it
//! precisely because it is no longer a data-table concept: `.. table::`
//! wraps an existing grid/simple table and needs these same five options
//! without either of `data_table`'s own two extras.

use super::option_value::{Classes, Located};
use super::options::{OptionLine, directive_options};
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rinx_ast::{Named, TableAlign, TableWidths, TargetName};

/// The five presentation options every option-bearing table directive shares:
/// `.. list-table::`, `.. csv-table::` and `.. table::` alike.
///
/// `widths` is kept as its raw string (not yet resolved to a
/// [`rinx_ast::TableWidths`]) because validating it needs the table's
/// column count, which isn't known until the rows have been built.
pub(in crate::directives) struct CommonTableOptions {
    pub widths: Option<Located<TableWidths>>,
    pub width: Option<String>,
    pub align: Option<TableAlign>,
    pub classes: Vec<String>,
    pub name: Option<Named<TargetName>>,
}

impl CommonTableOptions {
    pub(in crate::directives) fn empty() -> Self {
        Self {
            widths: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: None,
        }
    }
}

directive_options! {
    /// The five presentation options every option-bearing table directive
    /// takes: docutils' `Table.option_spec`.
    pub(in crate::directives) enum TableOption {
        /// `:widths:`.
        Widths(Located<TableWidths>) => (["widths"], TableDataWidthsInvalid, "The columns' relative widths, or `auto` or `grid`."),
        /// `:width:`.
        Width => (["width"], Length, "The table's width."),
        /// `:align:`.
        Align(TableAlign) => (["align"], TableDataAlignInvalid, "Where the table sits."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the table."),
        /// `:name:`.
        Name(Named<TargetName>) => (["name"], "A label to reference the table by."),
    }
}

/// Records one of the five presentation options.
pub(in crate::directives) fn read_table_option(
    option: TableOption,
    line: &OptionLine,
    options: &mut CommonTableOptions,
    _directive: &str,
    _diagnostics: &mut Diagnostics,
    _ctx: &ParseCtx<'_>,
) {
    match option {
        TableOption::Widths(widths) => options.widths = Some(widths),
        TableOption::Width => options.width = Some(line.value.clone()),
        TableOption::Align(align) => options.align = Some(align),
        TableOption::Class(classes) => options.classes = classes.0,
        TableOption::Name(name) => options.name = Some(name),
    }
}

#[cfg(test)]
mod tests {
    use crate::directives::options::{DirectiveOptions, OptionRead};
    /// The five presentation options read off `option_lines`, and the lines
    /// that are none of them.
    fn parse_common_table_options<'a>(
        option_lines: &[&'a OptionLine],
        directive: &str,
        diagnostics: &mut Diagnostics,
        ctx: &ParseCtx<'_>,
    ) -> (CommonTableOptions, Vec<&'a OptionLine>) {
        let mut options = CommonTableOptions::empty();
        let mut unrecognized = Vec::new();
        for &line in option_lines {
            match TableOption::read(line, directive, diagnostics, ctx) {
                OptionRead::Read(option) => {
                    read_table_option(option, line, &mut options, directive, diagnostics, ctx);
                }
                OptionRead::Refused => {}
                OptionRead::NotTaken => unrecognized.push(line),
            }
        }
        (options, unrecognized)
    }

    use super::*;

    use crate::directives::options::scan_option_lines;
    use rinx_ast::Domain;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_parse_common_table_options_reads_every_common_option() {
        // Given — the five options `.. table::` has, called directly rather
        // than through `parse_shared_table_options`'s header-rows/stub-columns
        // layer.
        let body = lines(&[
            ":widths: 30 70",
            ":width: 50%",
            ":align: right",
            ":class: foo bar",
            ":name: My Table",
        ]);
        let (option_lines, _) = scan_option_lines(&body);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        let (options, unrecognized) = parse_common_table_options(
            &refs,
            "table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(diagnostics.is_empty());
        assert!(unrecognized.is_empty());
        assert_eq!(
            options.widths.map(|widths| widths.value),
            Some(TableWidths::Explicit(vec![30, 70]))
        );
        assert_eq!(options.width.as_deref(), Some("50%"));
        assert_eq!(options.align, Some(TableAlign::Right));
        assert_eq!(options.classes, vec!["foo".to_string(), "bar".to_string()]);
        assert_eq!(options.name, Some(TargetName::new("My Table").into()));
    }

    #[test]
    fn test_parse_common_table_options_hands_back_a_header_rows_line_as_unrecognized() {
        // Given — `.. table::` has no `:header-rows:` option, unlike
        // `list-table`/`csv-table`, so the common parser must not claim it.
        let body = lines(&[":header-rows: 1"]);
        let (option_lines, _) = scan_option_lines(&body);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        let (_, unrecognized) = parse_common_table_options(
            &refs,
            "table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(unrecognized.len(), 1);
        assert_eq!(unrecognized[0].name, "header-rows");
    }

    #[test]
    fn test_parse_common_table_options_rejects_an_invalid_align_value() {
        // Given
        let body = lines(&[":align: diagonal"]);
        let (option_lines, _) = scan_option_lines(&body);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        let (options, _) = parse_common_table_options(
            &refs,
            "csv-table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(options.align, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .starts_with("csv-table: :align: expects one of"),
            "{}",
            diagnostics[0].message
        );
    }
}

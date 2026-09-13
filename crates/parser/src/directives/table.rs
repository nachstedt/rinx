//! `.. table::` — wraps an existing grid or simple table with a title/caption
//! and the presentation options neither ASCII-art table syntax has notation
//! of its own for.
//!
//! Unlike `.. list-table::`/`.. csv-table::` (see [`super::data_table`]),
//! this directive's content isn't data to lower into rows itself — it's
//! ordinary block content that must parse down to exactly one
//! [`rusty_sphinx_ast::Node::Table`], produced by the same grid- or
//! simple-table parsing every other table in the document goes through.

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::directives::error_node::malformed_directive;
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::directives::table_options::parse_common_table_options;
use crate::directives::table_widths::parse_widths_option;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{DiagnosticCode, Directive, Node};

/// The directive name, used throughout this module's diagnostics.
const DIRECTIVE: &str = "table";

/// Parses a `.. table::` directive. `body_lines` is the raw, still-indented
/// body collected by `collect_directive_body`, exactly as every other
/// content-bearing directive parser receives it.
pub(in crate::directives) fn parse_table_directive(
    argument: &str,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let title = if argument.is_empty() {
        None
    } else {
        Some(argument.to_string())
    };

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, opt_idx) = scan_option_lines(&unindented_lines);
    let option_line_refs: Vec<&OptionLine> = option_lines.iter().collect();
    let (options, unrecognized) =
        parse_common_table_options(&option_line_refs, DIRECTIVE, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    let body_content: Vec<&str> = unindented_lines[opt_idx..]
        .iter()
        .map(String::as_str)
        .collect();
    // The option lines scanned above are real lines this directive's content
    // occupies — unlike list-table's rows (lowered from already-parsed nodes
    // that carry no position of their own), a wrapped table's cells are full
    // nested parses whose diagnostics must point at the right source line, so
    // `opt_idx` must be rebased into `ctx` before parsing the remainder.
    let body_ctx = ctx.nested(opt_idx, 0);
    let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, &body_ctx);

    let span = body_span(body_lines, ctx);
    let node_count = body_nodes.len();
    let [
        Node::Table {
            header_rows,
            body_rows,
        },
    ] = body_nodes.as_slice()
    else {
        return malformed_table_directive(node_count, argument, body_lines, diagnostics, span);
    };

    let ncols = body_rows
        .first()
        .or_else(|| header_rows.first())
        .map_or(0, |row| row.cells.len());
    let widths = options
        .widths_raw
        .and_then(|raw| parse_widths_option(&raw, ncols, DIRECTIVE, diagnostics, span));

    Directive::Table {
        title,
        widths,
        width: options.width,
        align: options.align,
        classes: options.classes,
        name: options.name,
        header_rows: header_rows.clone(),
        body_rows: body_rows.clone(),
    }
}

/// Degrades the directive, saying why its content wasn't exactly one wrapped
/// table: no content at all, content that isn't a table, or more than one
/// block.
///
/// The diagnostic and the node are built from one message so the build log and
/// the rendered error block cannot disagree about the reason.
fn malformed_table_directive(
    node_count: usize,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    span: Option<rusty_sphinx_ast::Span>,
) -> Directive {
    let (code, message) = match node_count {
        0 => (
            DiagnosticCode::TableDirectiveNoContent,
            format!("{DIRECTIVE}: directive requires content — a single grid or simple table"),
        ),
        1 => (
            DiagnosticCode::TableDirectiveNotATable,
            format!("{DIRECTIVE}: directive content must be a single grid or simple table"),
        ),
        _ => (
            DiagnosticCode::TableDirectiveMultipleBlocks,
            format!(
                "{DIRECTIVE}: directive content must be exactly one table, found {node_count} blocks"
            ),
        ),
    };
    malformed_directive(
        DIRECTIVE,
        argument,
        body_lines,
        code,
        message,
        span,
        diagnostics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Domain, TableAlign, TableWidths, TargetName};

    fn parse(body_lines: &[&str]) -> (Directive, Diagnostics) {
        parse_with("", body_lines)
    }

    fn parse_with(argument: &str, body_lines: &[&str]) -> (Directive, Diagnostics) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let directive = parse_table_directive(
            argument,
            body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        (directive, diagnostics)
    }

    const GRID_TABLE: &[&str] = &[
        "   +-------+-------+",
        "   | A     | B     |",
        "   +=======+=======+",
        "   | 1     | 2     |",
        "   +-------+-------+",
    ];

    const SIMPLE_TABLE: &[&str] = &[
        "   =====  =====",
        "     A      B",
        "   =====  =====",
        "     1      2",
        "   =====  =====",
    ];

    #[test]
    fn test_parse_table_directive_wraps_a_grid_table() {
        // Given / When
        let (directive, diagnostics) = parse(GRID_TABLE);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Table {
            header_rows,
            body_rows,
            ..
        } = directive
        else {
            panic!("Expected Table directive, got {directive:?}");
        };
        assert_eq!(header_rows.len(), 1);
        assert_eq!(body_rows.len(), 1);
    }

    #[test]
    fn test_parse_table_directive_wraps_a_simple_table() {
        // Given / When
        let (directive, diagnostics) = parse(SIMPLE_TABLE);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Table {
            header_rows,
            body_rows,
            ..
        } = directive
        else {
            panic!("Expected Table directive, got {directive:?}");
        };
        assert_eq!(header_rows.len(), 1);
        assert_eq!(body_rows.len(), 1);
    }

    #[test]
    fn test_parse_table_directive_with_title() {
        // Given / When
        let (directive, diagnostics) = parse_with("Truth Table", GRID_TABLE);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Table { title, .. } = directive else {
            panic!("Expected Table directive");
        };
        assert_eq!(title, Some("Truth Table".to_string()));
    }

    #[test]
    fn test_parse_table_directive_without_title_is_none() {
        // Given / When
        let (directive, _) = parse(GRID_TABLE);

        // Then
        let Directive::Table { title, .. } = directive else {
            panic!("Expected Table directive");
        };
        assert_eq!(title, None);
    }

    #[test]
    fn test_parse_table_directive_reads_every_common_option() {
        // Given
        let mut body_lines = vec![
            "   :widths: 30 70",
            "   :width: 50%",
            "   :align: right",
            "   :class: compact",
            "   :name: my-table",
            "",
        ];
        body_lines.extend_from_slice(GRID_TABLE);

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Table {
            widths,
            width,
            align,
            classes,
            name,
            ..
        } = directive
        else {
            panic!("Expected Table directive");
        };
        assert_eq!(widths, Some(TableWidths::Explicit(vec![30, 70])));
        assert_eq!(width, Some("50%".to_string()));
        assert_eq!(align, Some(TableAlign::Right));
        assert_eq!(classes, vec!["compact".to_string()]);
        assert_eq!(name, Some(TargetName::new("my-table")));
    }

    #[test]
    fn test_parse_table_directive_rejects_header_rows_as_unknown() {
        // Given — `.. table::` has no `:header-rows:` option, unlike
        // `list-table`/`csv-table`.
        let mut body_lines = vec!["   :header-rows: 1", ""];
        body_lines.extend_from_slice(GRID_TABLE);

        // When
        let (_, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("Invalid or non-standard Sphinx table option"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_table_directive_validates_widths_against_actual_column_count() {
        // Given — the wrapped table has 2 columns, but :widths: names 3.
        let mut body_lines = vec!["   :widths: 30 30 40", ""];
        body_lines.extend_from_slice(GRID_TABLE);

        // When
        let (_, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains(":widths:"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_table_directive_rejects_empty_content() {
        // Given
        let body_lines: Vec<&str> = Vec::new();

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("requires content"),
            "{}",
            diagnostics[0].message
        );
        assert!(matches!(directive, Directive::Malformed { name, .. } if name == "table"));
    }

    #[test]
    fn test_parse_table_directive_rejects_content_that_is_not_a_table() {
        // Given
        let body_lines = vec!["   Just a paragraph."];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("must be a single"),
            "{}",
            diagnostics[0].message
        );
        assert!(matches!(directive, Directive::Malformed { .. }));
    }

    #[test]
    fn test_parse_table_directive_rejects_more_than_one_block() {
        // Given
        let mut body_lines = vec!["   A stray paragraph.", ""];
        body_lines.extend_from_slice(GRID_TABLE);

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("exactly one table"),
            "{}",
            diagnostics[0].message
        );
        assert!(matches!(directive, Directive::Malformed { .. }));
    }

    #[test]
    fn test_parse_table_directive_positions_a_nested_ref_at_its_real_source_line() {
        // Given — a `:name:` option line and a blank separator sit above the
        // wrapped table, so the `:ref:` role's line inside it is not line 1
        // of the directive's body. Resolving this correctly requires
        // rebasing `ctx` by the option lines' count before parsing the
        // wrapped table (see the comment on that rebase above).
        let input = "\
.. table::
   :name: outer

   +----------------+
   | :ref:`missing` |
   +----------------+
";
        let expected_line = u32::try_from(
            input
                .lines()
                .position(|line| line.contains(":ref:`missing`"))
                .expect("fixture must contain the :ref: role")
                + 1,
        )
        .expect("line number fits in u32");

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        let Node::Directive(Directive::Table { body_rows, .. }) = &doc.nodes[0] else {
            panic!("Expected Table directive, got {:?}", doc.nodes[0]);
        };
        let [Node::Paragraph(inlines)] = body_rows[0].cells[0].content.as_slice() else {
            panic!(
                "Expected a single paragraph cell, got {:?}",
                body_rows[0].cells[0].content
            );
        };
        let [rusty_sphinx_ast::InlineNode::Reference { span, .. }] = inlines.as_slice() else {
            panic!("Expected a single Reference inline node, got {inlines:?}");
        };
        let span = span.expect("a :ref: role parsed from real source must carry a span");
        assert_eq!(span.start.line, expected_line);
    }

    #[test]
    fn test_parse_table_directive_via_full_parse_pipeline() {
        // Given
        let input = "\
.. table:: Truth table for \"not\"
   :widths: auto

   =====  =====
     A    not A
   =====  =====
   False  True
   True   False
   =====  =====
";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::Table { title, widths, header_rows, body_rows, .. })
                if title.as_deref() == Some("Truth table for \"not\"")
                    && *widths == Some(TableWidths::Auto)
                    && header_rows.len() == 1
                    && body_rows.len() == 2
        ));
    }

    #[test]
    fn test_malformed_table_directive_carries_argument_body_and_reason() {
        // Given
        let body_lines = vec!["   Just a paragraph."];
        let mut diagnostics = Diagnostics::default();

        // When
        let directive =
            malformed_table_directive(1, "My Title", &body_lines, &mut diagnostics, None);

        // Then
        assert_eq!(
            directive,
            Directive::Malformed {
                name: "table".to_string(),
                argument: "My Title".to_string(),
                body: "Just a paragraph.".to_string(),
                message: "table: directive content must be a single grid or simple table"
                    .to_string(),
            }
        );
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, DiagnosticCode::TableDirectiveNotATable);
    }
}

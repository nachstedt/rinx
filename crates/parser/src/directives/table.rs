//! `.. table::` — wraps an existing grid or simple table with a title/caption
//! and the presentation options neither ASCII-art table syntax has notation
//! of its own for.
//!
//! Unlike `.. list-table::`/`.. csv-table::` (see [`super::data_table`]),
//! this directive's content isn't data to lower into rows itself — it's
//! ordinary block content that must parse down to exactly one
//! [`rinx_ast::Node::Table`], produced by the same grid- or
//! simple-table parsing every other table in the document goes through.

use crate::diagnostics::Diagnostics;
use crate::directives::error_node::malformed_directive;
use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::options::{DirectiveOptions, classify_options};
use crate::directives::spec::{Arguments, DirectiveSpec};
use crate::directives::table_options::{CommonTableOptions, TableOption, read_table_option};
use crate::directives::table_widths::widths_for_columns;
use crate::headings::Adornment;
use rinx_ast::{DiagnosticCode, Directive, Node};

/// The directive name, used throughout this module's diagnostics.
const DIRECTIVE: &str = "table";

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["table"],
    doc: "A title and options for the table in its body.",
    arguments: Arguments::OPTIONAL_TEXT,
    takes_content: true,
    content_hint: None,
    options: <TableOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. table::`.
pub(in crate::directives) fn interpret_table(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_table_directive(
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses a `.. table::` directive, its argument the table's title.
pub(in crate::directives) fn parse_table_directive(
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let argument = invocation.argument(0);
    let body_lines = invocation.source_body;
    let ctx = &invocation.ctx;
    let title = if argument.is_empty() {
        None
    } else {
        Some(argument.to_string())
    };

    let mut options = CommonTableOptions::empty();
    for (option, line) in
        classify_options::<TableOption>(&invocation.options, DIRECTIVE, diagnostics, ctx)
    {
        read_table_option(option, line, &mut options, DIRECTIVE, diagnostics, ctx);
    }

    // Unlike list-table's rows (lowered from already-parsed nodes that carry
    // no position of their own), a wrapped table's cells are full nested
    // parses whose diagnostics must point at the right source line, which the
    // content's own context does.
    let body_nodes = invocation.content().parse(adornment_order, diagnostics);

    let span = invocation.span;
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
        .widths
        .and_then(|widths| widths_for_columns(widths, ncols, DIRECTIVE, diagnostics, ctx));

    Directive::Table {
        title,
        widths,
        width: options.width,
        align: options.align,
        classes: options.classes,
        name: options.name,
        header_rows: header_rows.clone(),
        body_rows: body_rows.clone(),
        span,
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
    span: Option<rinx_ast::Span>,
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
    use crate::context::ParseCtx;
    use rinx_ast::{Domain, TableAlign, TableWidths, TargetName};

    #[test]
    fn test_parse_records_the_tables_marker_line_as_its_span() {
        // Given — a named table below a paragraph
        let input = "Intro\n\n.. table:: Sizes\n   :name: sizes\n\n   =  =\n   a  b\n   =  =\n";

        // When
        let doc = crate::parse_document("test.rst", input);

        // Then
        let span = doc.nodes.iter().find_map(|node| match node {
            Node::Directive(Directive::Table { span, .. }) => Some(*span),
            _ => None,
        });
        assert_eq!(
            span,
            Some(Some(rinx_ast::Span::new(
                rinx_ast::Position::new(3, 1),
                rinx_ast::Position::new(3, 17)
            )))
        );
    }

    fn parse(body_lines: &[&str]) -> (Directive, Diagnostics) {
        parse_with("", body_lines)
    }

    fn parse_with(argument: &str, body_lines: &[&str]) -> (Directive, Diagnostics) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let written = crate::directives::reader::written_after_blank(body_lines);
        let invocation = crate::directives::reader::test_invocation(
            "table",
            &SPEC,
            argument,
            &written,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_table_directive(&invocation, &mut adornment_order, &mut diagnostics);
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
        assert_eq!(name, Some(TargetName::new("my-table").into()));
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
        let doc = crate::parse_document("test.rst", input);

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
        let [rinx_ast::InlineNode::Reference { span, .. }] = inlines.as_slice() else {
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
        let doc = crate::parse_document("test.rst", input);

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

    #[test]
    fn test_content_that_is_not_a_table_is_reported_on_the_marker_line() {
        // Given — the shape written in practice: a blank line, then content
        let input = "Intro.\n\n.. table::\n\n   Just a paragraph.\n";

        // When / Then — on the directive, not the blank line below it
        assert_eq!(
            crate::diagnostics::lines_reported(input, DiagnosticCode::TableDirectiveNotATable),
            [3]
        );
    }

    #[test]
    fn test_missing_content_is_reported_on_the_marker_line() {
        // Given — a titled table with nothing below it but the next paragraph
        let input = "Intro.\n\n.. table:: Title\n\nAfter.\n";

        // When / Then — not on the line after the directive
        assert_eq!(
            crate::diagnostics::lines_reported(input, DiagnosticCode::TableDirectiveNoContent),
            [3]
        );
    }

    #[test]
    fn test_several_blocks_are_reported_on_the_marker_line() {
        // Given — options, then two paragraphs
        let input = "Intro.\n\n.. table::\n   :align: left\n\n   One.\n\n   Two.\n";

        // When / Then — not on the option line
        assert_eq!(
            crate::diagnostics::lines_reported(input, DiagnosticCode::TableDirectiveMultipleBlocks),
            [3]
        );
    }
}

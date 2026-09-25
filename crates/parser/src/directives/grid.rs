//! `.. grid::` and `.. grid-item::` — sphinx-design's responsive row.
//!
//! sphinx-design's `GridDirective` and `GridItemDirective` are the authority
//! for every option name and value below; its `_media_option`,
//! `margin_option`, `padding_option` and `make_choice` validators are why each
//! is matched against a closed domain.
//!
//! Two rules govern this file, and both exist because of the same failure.
//!
//! An unreadable option value is dropped and reported, leaving the container
//! itself intact — the policy [`super::dropdown`] states, and the reason both
//! directives are here at all: an *unknown* directive never parses its body,
//! so before this existed a `.. uml::` written inside a `.. grid-item::` was
//! invisible to every later phase, diagram pipeline included. Refusing a grid
//! over a misspelled `:gutter:` would put it straight back.
//!
//! For the same reason a misplaced *child* is reported and kept rather than
//! dropped, which is also what sphinx-design does: a grid holding something
//! that is not a grid-item, and a grid-item written outside a grid, both warn
//! and both still render.

use rinx_ast::{
    ChildAlign, ChildDirection, ColumnSpec, Diagnostic, DiagnosticCode, Directive, Grid, GridItem,
    Gutter, Node, Spacing, SpacingKind, Span,
};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

use super::options::{OptionLine, report_unknown_options, scan_option_lines};

const GRID: &str = "grid";
const GRID_ITEM: &str = "grid-item";

/// Parses a `.. grid::` into a [`Directive::Grid`].
pub(super) fn parse_grid(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let mut grid = Grid {
        columns: parse_argument_columns(argument, directive_span, diagnostics, ctx),
        span: directive_span,
        ..Grid::new()
    };
    let unrecognized = read_grid_options(&mut grid, &option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        GRID,
        DiagnosticCode::DirectiveGridUnknownOption,
        diagnostics,
        ctx,
    );

    // The body starts below the option block, so every position inside it is
    // short by that many lines unless the context is rebased first. This is
    // what keeps a diagram's span — and the `.. noqa:` that might silence it —
    // pointing at the line it was written on.
    let body_ctx = ctx.nested(body_start, 0).inside_grid_row();
    let content: Vec<&str> = unindented_lines[body_start..]
        .iter()
        .map(String::as_str)
        .collect();
    grid.body = parse_blocks(&content, adornment_order, diagnostics, &body_ctx);
    report_unexpected_children(&grid.body, directive_span, diagnostics);

    Directive::Grid(Box::new(grid))
}

/// Parses a `.. grid-item::` into a [`Directive::GridItem`].
pub(super) fn parse_grid_item(
    directive_span: Option<Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    if !ctx.in_grid_row {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::GridItemOutsideGrid,
            format!("{GRID_ITEM}: the parent of a grid-item should be a grid"),
            directive_span,
        ));
    }

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let mut item = GridItem {
        span: directive_span,
        ..GridItem::new()
    };
    let unrecognized = read_item_options(&mut item, &option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        GRID_ITEM,
        DiagnosticCode::DirectiveGridItemUnknownOption,
        diagnostics,
        ctx,
    );

    let body_ctx = ctx.nested(body_start, 0).outside_grid_row();
    let content: Vec<&str> = unindented_lines[body_start..]
        .iter()
        .map(String::as_str)
        .collect();
    item.body = parse_blocks(&content, adornment_order, diagnostics, &body_ctx);

    Directive::GridItem(Box::new(item))
}

/// Reads the `.. grid::` argument, which is a column count.
///
/// An absent argument is legal — sphinx-design then emits no column classes at
/// all and the row falls back to the stylesheet's default.
fn parse_argument_columns(
    argument: &str,
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<ColumnSpec> {
    let _ = ctx;
    let written = argument.trim();
    if written.is_empty() {
        return None;
    }
    match ColumnSpec::parse(written) {
        Ok(columns) => Some(columns),
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::GridInvalidColumns,
                format!("{GRID}: argument {error}"),
                directive_span,
            ));
            None
        }
    }
}

/// Reads every `.. grid::` option onto `grid`, returning the lines nobody
/// claimed.
fn read_grid_options<'a>(
    grid: &mut Grid,
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let mut unrecognized = Vec::new();
    for line in option_lines {
        match line.name.as_str() {
            "gutter" => match Gutter::parse(&line.value) {
                Ok(gutter) => grid.gutter = Some(gutter),
                Err(error) => report_invalid_value(
                    line,
                    GRID,
                    "gutter",
                    &error.to_string(),
                    DiagnosticCode::GridInvalidGutter,
                    diagnostics,
                    ctx,
                ),
            },
            "margin" => {
                grid.margin = parse_spacing(line, GRID, SpacingKind::Margin, diagnostics, ctx);
            }
            "padding" => {
                grid.padding = parse_spacing(line, GRID, SpacingKind::Padding, diagnostics, ctx);
            }
            // Flags: `directives.flag` accepts no value at all, so what
            // matters is only that the option was written.
            "outline" => grid.outline = true,
            "reverse" => grid.reverse = true,
            "class-container" => grid.class_container = split_classes(&line.value),
            "class-row" => grid.class_row = split_classes(&line.value),
            _ => unrecognized.push(line),
        }
    }
    unrecognized
}

/// Reads every `.. grid-item::` option onto `item`, returning the lines nobody
/// claimed.
fn read_item_options<'a>(
    item: &mut GridItem,
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let mut unrecognized = Vec::new();
    for line in option_lines {
        match line.name.as_str() {
            "columns" => match ColumnSpec::parse(&line.value) {
                Ok(columns) => item.columns = Some(columns),
                Err(error) => report_invalid_value(
                    line,
                    GRID_ITEM,
                    "columns",
                    &error.to_string(),
                    DiagnosticCode::GridInvalidColumns,
                    diagnostics,
                    ctx,
                ),
            },
            "margin" => {
                item.margin = parse_spacing(line, GRID_ITEM, SpacingKind::Margin, diagnostics, ctx);
            }
            "padding" => {
                item.padding =
                    parse_spacing(line, GRID_ITEM, SpacingKind::Padding, diagnostics, ctx);
            }
            "child-direction" => match ChildDirection::parse(&line.value) {
                Some(direction) => item.child_direction = direction,
                None => report_invalid_choice(
                    line,
                    "child-direction",
                    &names(ChildDirection::ALL, ChildDirection::as_str),
                    DiagnosticCode::GridInvalidChildDirection,
                    diagnostics,
                    ctx,
                ),
            },
            "child-align" => match ChildAlign::parse(&line.value) {
                Some(align) => item.child_align = Some(align),
                None => report_invalid_choice(
                    line,
                    "child-align",
                    &names(ChildAlign::ALL, ChildAlign::as_str),
                    DiagnosticCode::GridInvalidChildAlign,
                    diagnostics,
                    ctx,
                ),
            },
            "outline" => item.outline = true,
            "class" => item.classes = split_classes(&line.value),
            _ => unrecognized.push(line),
        }
    }
    unrecognized
}

/// Reads a `:margin:` or `:padding:` value for either directive.
fn parse_spacing(
    line: &OptionLine,
    directive: &str,
    kind: SpacingKind,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Spacing> {
    let option = match kind {
        SpacingKind::Margin => "margin",
        SpacingKind::Padding => "padding",
    };
    if line.value.is_empty() {
        report_empty_value(line, directive, option, diagnostics, ctx);
        return None;
    }
    match Spacing::parse(&line.value, kind) {
        Ok(spacing) => Some(spacing),
        Err(error) => {
            report_invalid_value(
                line,
                directive,
                option,
                &error.to_string(),
                DiagnosticCode::GridInvalidSpacing,
                diagnostics,
                ctx,
            );
            None
        }
    }
}

/// Reports content written directly inside a `.. grid::` that is not a
/// `.. grid-item::`.
///
/// Once, not once per stray node: sphinx-design breaks out of its own loop
/// after the first, and a grid written with prose in it would otherwise
/// produce one warning per paragraph. The content is kept either way.
fn report_unexpected_children(
    body: &[Node],
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    let stray = body.iter().any(|node| !is_grid_item(node));
    if stray {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::GridUnexpectedChild,
            format!("{GRID}: all children of a grid should be grid-items"),
            directive_span,
        ));
    }
}

/// Whether a node is a `.. grid-item::`.
fn is_grid_item(node: &Node) -> bool {
    matches!(node, Node::Directive(Directive::GridItem(_)))
}

/// Splits a class-list option value, as docutils' `class_option` does.
fn split_classes(value: &str) -> Vec<String> {
    value.split_whitespace().map(str::to_string).collect()
}

/// The written names of a closed option vocabulary, for a diagnostic.
fn names<T: Copy>(values: &[T], name_of: fn(T) -> &'static str) -> String {
    values
        .iter()
        .copied()
        .map(name_of)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reports an option value the option's own type refused, quoting its reason.
fn report_invalid_value(
    line: &OptionLine,
    directive: &str,
    option: &str,
    reason: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        code,
        format!("{directive}: :{option}: {reason}: {}", line.raw),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports a value that is not one of the ones the option accepts, listing
/// them in the order sphinx-design declares them.
fn report_invalid_choice(
    line: &OptionLine,
    option: &str,
    accepted: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        code,
        format!(
            "{GRID_ITEM}: :{option}: expects one of {accepted}, found '{}'",
            line.value
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an option written without the value it needs.
fn report_empty_value(
    line: &OptionLine,
    directive: &str,
    option: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::GridEmptyOptionValue,
        format!(
            "{directive}: a :{option}: option needs a value: {}",
            line.raw
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{InlineNode, SpacingValue, Uml, walk_nodes};

    /// Parses a whole document and returns its single `.. grid::` with the
    /// document's diagnostics — the dispatcher is what positions the parse
    /// context, so spans are only meaningful this way.
    fn parse_grid_document(input: &str) -> (Grid, Vec<Diagnostic>) {
        let doc = parse("test.rst", input);
        let grid = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive(Directive::Grid(grid)) => Some((**grid).clone()),
                _ => None,
            })
            .expect("document should contain a grid directive");
        (grid, doc.diagnostics)
    }

    /// As [`parse_grid_document`], for the item directive.
    fn parse_item_document(input: &str) -> (GridItem, Vec<Diagnostic>) {
        let doc = parse("test.rst", input);
        let item = find_item(&doc.nodes).expect("document should contain a grid-item directive");
        (item, doc.diagnostics)
    }

    /// The first `.. grid-item::` anywhere in `nodes`, however deeply nested.
    fn find_item(nodes: &[Node]) -> Option<GridItem> {
        let mut found = None;
        walk_nodes(nodes, &mut |node| {
            if found.is_none()
                && let Node::Directive(Directive::GridItem(item)) = node
            {
                found = Some((**item).clone());
            }
        });
        found
    }

    /// The first `.. uml::` anywhere in `nodes`.
    fn find_uml(nodes: &[Node]) -> Option<Uml> {
        let mut found = None;
        walk_nodes(nodes, &mut |node| {
            if found.is_none()
                && let Node::Directive(Directive::Uml(uml)) = node
            {
                found = Some((**uml).clone());
            }
        });
        found
    }

    /// The codes reported, in order, for asserting about diagnostics without
    /// pinning their wording.
    fn codes(diagnostics: &[Diagnostic]) -> Vec<DiagnosticCode> {
        diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn test_parses_a_grid_with_an_argument_and_one_item() {
        // Given
        let input = ".. grid:: 2\n\n   .. grid-item::\n\n      Some prose.\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(
            grid.columns,
            Some(ColumnSpec::parse("2").expect("a valid column spec"))
        );
        assert_eq!(grid.body.len(), 1);
        assert!(matches!(
            grid.body[0],
            Node::Directive(Directive::GridItem(_))
        ));
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn test_parses_the_four_value_argument_the_corpus_writes() {
        // Given
        let input = ".. grid:: 1 1 2 2\n\n   .. grid-item::\n\n      Cell\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(
            grid.columns,
            Some(ColumnSpec::parse("1 1 2 2").expect("a valid column spec"))
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn test_parses_a_grid_with_no_argument_at_all() {
        // Given — sphinx-design's argument is optional
        let input = ".. grid::\n\n   .. grid-item::\n\n      Cell\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(grid.columns, None);
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn test_keeps_a_diagram_written_inside_an_item() {
        // Given — the shape the sphinx-needs demo writes, and the reason this
        // directive exists: an unknown `grid` never parsed its body, so the
        // diagram inside it reached no later phase at all
        let input = concat!(
            ".. grid:: 2\n",
            "\n",
            "   .. grid-item::\n",
            "\n",
            "      .. uml::\n",
            "\n",
            "         node A\n",
            "         node B\n",
            "         A --> B\n",
        );

        // When
        let doc = parse("test.rst", input);

        // Then
        let uml = find_uml(&doc.nodes).expect("the diagram should have been parsed");
        assert!(uml.template.contains("A --> B"), "got: {}", uml.template);
    }

    #[test]
    fn test_rebases_positions_inside_an_item_onto_the_document() {
        // Given — the diagram sits on line 7, below an option block in the
        // grid and two levels of nesting
        let input = concat!(
            ".. grid:: 2\n",       // 1
            "   :gutter: 3\n",     // 2
            "\n",                  // 3
            "   .. grid-item::\n", // 4
            "\n",                  // 5
            "      .. uml::\n",    // 6
            "\n",                  // 7
            "         A -> B\n",   // 8
        );

        // When
        let doc = parse("test.rst", input);

        // Then — the span points at the directive's own line, which is what a
        // `.. noqa:` and every later diagnostic are matched against
        let uml = find_uml(&doc.nodes).expect("the diagram should have been parsed");
        let span = uml.span.expect("a diagram should carry a span");
        assert_eq!(span.start.line, 6);
    }

    #[test]
    fn test_reads_every_grid_option() {
        // Given
        let input = concat!(
            ".. grid:: 2\n",
            "   :gutter: 3\n",
            "   :margin: 1\n",
            "   :padding: 2\n",
            "   :outline:\n",
            "   :reverse:\n",
            "   :class-container: mine\n",
            "   :class-row: row-mine\n",
            "\n",
            "   .. grid-item::\n",
            "\n",
            "      Cell\n",
        );

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(grid.gutter, Some(Gutter::parse("3").expect("valid")));
        assert_eq!(grid.margin, Some(Spacing::All(SpacingValue::One)));
        assert_eq!(grid.padding, Some(Spacing::All(SpacingValue::Two)));
        assert!(grid.outline);
        assert!(grid.reverse);
        assert_eq!(grid.class_container, vec!["mine".to_string()]);
        assert_eq!(grid.class_row, vec!["row-mine".to_string()]);
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn test_reads_every_item_option() {
        // Given — `:columns: 12 12 8 8` is the corpus' own spelling
        let input = concat!(
            ".. grid:: 1 1 2 2\n",
            "\n",
            "   .. grid-item::\n",
            "      :columns: 12 12 8 8\n",
            "      :margin: 1\n",
            "      :padding: 2\n",
            "      :child-direction: row\n",
            "      :child-align: center\n",
            "      :outline:\n",
            "      :class: mine\n",
            "\n",
            "      Cell\n",
        );

        // When
        let (item, diagnostics) = parse_item_document(input);

        // Then
        assert_eq!(
            item.columns,
            Some(ColumnSpec::parse("12 12 8 8").expect("valid"))
        );
        assert_eq!(item.margin, Some(Spacing::All(SpacingValue::One)));
        assert_eq!(item.padding, Some(Spacing::All(SpacingValue::Two)));
        assert_eq!(item.child_direction, ChildDirection::Row);
        assert_eq!(item.child_align, Some(ChildAlign::Center));
        assert!(item.outline);
        assert_eq!(item.classes, vec!["mine".to_string()]);
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn test_an_unreadable_argument_leaves_the_grid_standing() {
        // Given — thirteen of twelve columns
        let input = ".. grid:: 13\n\n   .. grid-item::\n\n      Cell\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then — reported, and the body still parsed
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridInvalidColumns]
        );
        assert_eq!(grid.columns, None);
        assert_eq!(grid.body.len(), 1);
    }

    #[test]
    fn test_an_unreadable_gutter_leaves_the_grid_standing() {
        // Given — the policy this file is written around: a misspelled option
        // must never cost the content written inside the container
        let input = ".. grid:: 2\n   :gutter: 9\n\n   .. grid-item::\n\n      Cell\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::GridInvalidGutter]);
        assert_eq!(grid.gutter, None);
        assert_eq!(grid.body.len(), 1);
    }

    #[test]
    fn test_a_gutter_of_auto_is_refused() {
        // Given — `auto` is a column value, never a gutter step
        let input = ".. grid:: 2\n   :gutter: auto\n\n   .. grid-item::\n\n      Cell\n";

        // When
        let (_, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::GridInvalidGutter]);
    }

    #[test]
    fn test_a_padding_of_auto_is_refused() {
        // Given — `margin_option` accepts `auto` and `padding_option` does not
        let input = ".. grid:: 2\n   :padding: auto\n\n   .. grid-item::\n\n      Cell\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridInvalidSpacing]
        );
        assert_eq!(grid.padding, None);
    }

    #[test]
    fn test_an_unreadable_child_align_leaves_the_item_standing() {
        // Given
        let input = concat!(
            ".. grid:: 2\n",
            "\n",
            "   .. grid-item::\n",
            "      :child-align: middle\n",
            "\n",
            "      Cell\n",
        );

        // When
        let (item, diagnostics) = parse_item_document(input);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridInvalidChildAlign]
        );
        assert_eq!(item.child_align, None);
        assert_eq!(item.body.len(), 1);
    }

    #[test]
    fn test_an_unreadable_child_direction_leaves_the_item_standing() {
        // Given
        let input = concat!(
            ".. grid:: 2\n",
            "\n",
            "   .. grid-item::\n",
            "      :child-direction: diagonal\n",
            "\n",
            "      Cell\n",
        );

        // When
        let (item, diagnostics) = parse_item_document(input);

        // Then — the default survives rather than the item being refused
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridInvalidChildDirection]
        );
        assert_eq!(item.child_direction, ChildDirection::Column);
    }

    #[test]
    fn test_an_option_written_without_a_value_is_reported() {
        // Given
        let input = ".. grid:: 2\n   :margin:\n\n   .. grid-item::\n\n      Cell\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridEmptyOptionValue]
        );
        assert_eq!(grid.margin, None);
    }

    #[test]
    fn test_an_unknown_option_is_reported_per_directive() {
        // Given — one on each, so the two codes can be told apart
        let input = concat!(
            ".. grid:: 2\n",
            "   :nonsense: x\n",
            "\n",
            "   .. grid-item::\n",
            "      :also-nonsense: y\n",
            "\n",
            "      Cell\n",
        );

        // When
        let (_, diagnostics) = parse_grid_document(input);

        // Then — the grid's own options are read before its body is parsed,
        // so the outer directive reports first
        assert_eq!(
            codes(&diagnostics),
            vec![
                DiagnosticCode::DirectiveGridUnknownOption,
                DiagnosticCode::DirectiveGridItemUnknownOption,
            ]
        );
    }

    #[test]
    fn test_content_that_is_not_an_item_is_reported_and_kept() {
        // Given
        let input = ".. grid:: 2\n\n   Stray prose.\n";

        // When
        let (grid, diagnostics) = parse_grid_document(input);

        // Then — sphinx-design warns and still renders; dropping the prose
        // would be the very bug this directive was added to fix
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridUnexpectedChild]
        );
        assert_eq!(
            grid.body,
            vec![Node::Paragraph(vec![InlineNode::Text(
                "Stray prose.".to_string()
            )])]
        );
    }

    #[test]
    fn test_stray_content_is_reported_once_however_much_there_is() {
        // Given — two stray paragraphs, as sphinx-design's own loop breaks
        // after the first offender
        let input = ".. grid:: 2\n\n   One.\n\n   Two.\n";

        // When
        let (_, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridUnexpectedChild]
        );
    }

    #[test]
    fn test_an_item_outside_a_grid_is_reported_and_kept() {
        // Given
        let input = ".. grid-item::\n\n   Orphan.\n";

        // When
        let (item, diagnostics) = parse_item_document(input);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridItemOutsideGrid]
        );
        assert_eq!(item.body.len(), 1);
    }

    #[test]
    fn test_an_item_nested_in_an_item_is_reported() {
        // Given — a grid-item's body is content, not a row
        let input = concat!(
            ".. grid:: 2\n",
            "\n",
            "   .. grid-item::\n",
            "\n",
            "      .. grid-item::\n",
            "\n",
            "         Inner.\n",
        );

        // When
        let (_, diagnostics) = parse_grid_document(input);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::GridItemOutsideGrid]
        );
    }

    #[test]
    fn test_split_classes_splits_on_any_whitespace() {
        // Given
        let value = "  one\ttwo   three ";

        // When
        let classes = split_classes(value);

        // Then
        assert_eq!(classes, vec!["one", "two", "three"]);
    }

    #[test]
    fn test_names_lists_a_vocabulary_in_declaration_order() {
        // Given / When
        let listed = names(ChildAlign::ALL, ChildAlign::as_str);

        // Then
        assert_eq!(listed, "start, end, center, justify, spaced");
    }

    #[test]
    fn test_is_grid_item_distinguishes_an_item_from_other_content() {
        // Given
        let item = Node::Directive(Directive::GridItem(Box::new(GridItem::new())));
        let prose = Node::Paragraph(vec![InlineNode::Text("x".to_string())]);

        // When / Then
        assert!(is_grid_item(&item));
        assert!(!is_grid_item(&prose));
    }
}

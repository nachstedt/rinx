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

use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::option_value::{Margin, Padding};
use crate::directives::spec::{Arguments, DirectiveSpec};
use rinx_ast::{
    ChildAlign, ChildDirection, ColumnSpec, Diagnostic, DiagnosticCode, Directive, Grid, GridItem,
    Gutter, Node, Span,
};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;

use super::option_value::Classes;
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};

const GRID: &str = "grid";
const GRID_ITEM: &str = "grid-item";

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const GRID_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["grid"],
    doc: "A responsive row of grid items.",
    arguments: Arguments::OPTIONAL_TEXT,
    takes_content: true,
    content_hint: None,
    options: <GridOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Parses a `.. grid::` into a [`Directive::Grid`].
pub(super) fn parse_grid(
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let directive_span = invocation.span;
    let mut grid = Grid {
        columns: parse_argument_columns(invocation.argument(0), directive_span, diagnostics, ctx),
        span: directive_span,
        ..Grid::new()
    };
    read_grid_options(&mut grid, &invocation.options, diagnostics, ctx);

    // The content's own context keeps a diagram's span — and the `.. noqa:`
    // that might silence it — pointing at the line it was written on.
    let content = invocation.content();
    let lines: Vec<&str> = content.lines.iter().map(String::as_str).collect();
    let body_ctx = content.ctx.inside_grid_row();
    grid.body = parse_blocks(&lines, adornment_order, diagnostics, &body_ctx);
    report_unexpected_children(&grid.body, directive_span, diagnostics);

    Directive::Grid(Box::new(grid))
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const GRID_ITEM_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["grid-item"],
    doc: "One cell of a grid.",
    arguments: Arguments::NONE,
    takes_content: true,
    content_hint: None,
    options: <GridItemOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. grid::`.
pub(in crate::directives) fn interpret_grid(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_grid(input, state.adornment_order, state.diagnostics))
}

/// Interprets `.. grid-item::`, written inside a grid or not: outside one its
/// content survives to be warned about rather than lost.
pub(in crate::directives) fn interpret_grid_item(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_grid_item(
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses a `.. grid-item::` into a [`Directive::GridItem`].
pub(super) fn parse_grid_item(
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let directive_span = invocation.span;
    if !ctx.in_grid_row {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::GridItemOutsideGrid,
            format!("{GRID_ITEM}: the parent of a grid-item should be a grid"),
            directive_span,
        ));
    }

    let mut item = GridItem {
        span: directive_span,
        ..GridItem::new()
    };
    read_item_options(&mut item, &invocation.options, diagnostics, ctx);

    let content = invocation.content();
    let lines: Vec<&str> = content.lines.iter().map(String::as_str).collect();
    let body_ctx = content.ctx.outside_grid_row();
    item.body = parse_blocks(&lines, adornment_order, diagnostics, &body_ctx);

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

directive_options! {
    /// The options `.. grid::` takes: sphinx-design's seven.
    enum GridOption {
        /// `:gutter:`.
        Gutter(Gutter) => (["gutter"], GridInvalidGutter, "The space between items, by spacing level, per breakpoint."),
        /// `:margin:`.
        Margin(Margin) => (["margin"], GridInvalidSpacing, "The margin around the grid, by spacing level."),
        /// `:padding:`.
        Padding(Padding) => (["padding"], GridInvalidSpacing, "The padding inside the grid, by spacing level."),
        /// `:outline:`.
        Outline => (["outline"], Flag, "Draws an outline around the grid."),
        /// `:reverse:`.
        Reverse => (["reverse"], Flag, "Reverses the order of the items."),
        /// `:class-container:`.
        ClassContainer(Classes) => (["class-container"], "Classes for the grid's container."),
        /// `:class-row:`.
        ClassRow(Classes) => (["class-row"], "Classes for the grid's row."),
    }
}

/// Reads every `.. grid::` option onto `grid`, returning the lines nobody
/// claimed.
fn read_grid_options(
    grid: &mut Grid,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for (option, _) in classify_options::<GridOption>(option_lines, GRID, diagnostics, ctx) {
        match option {
            GridOption::Gutter(gutter) => grid.gutter = Some(gutter),
            GridOption::Margin(margin) => grid.margin = Some(margin.0),
            GridOption::Padding(padding) => grid.padding = Some(padding.0),
            // Flags: `directives.flag` accepts no value at all, so what
            // matters is only that the option was written.
            GridOption::Outline => grid.outline = true,
            GridOption::Reverse => grid.reverse = true,
            GridOption::ClassContainer(classes) => grid.class_container = classes.0,
            GridOption::ClassRow(classes) => grid.class_row = classes.0,
        }
    }
}

directive_options! {
    /// The options `.. grid-item::` takes: sphinx-design's seven.
    enum GridItemOption {
        /// `:columns:`.
        Columns(ColumnSpec) => (["columns"], GridInvalidColumns, "How many of the twelve columns the item spans, per breakpoint."),
        /// `:margin:`.
        Margin(Margin) => (["margin"], GridInvalidSpacing, "The margin around the item, by spacing level."),
        /// `:padding:`.
        Padding(Padding) => (["padding"], GridInvalidSpacing, "The padding inside the item, by spacing level."),
        /// `:child-direction:`.
        ChildDirection(ChildDirection) => (["child-direction"], GridInvalidChildDirection, "How the item's children are laid out."),
        /// `:child-align:`.
        ChildAlign(ChildAlign) => (["child-align"], GridInvalidChildAlign, "How the item's children are aligned."),
        /// `:outline:`.
        Outline => (["outline"], Flag, "Draws an outline around the item."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the item."),
    }
}

/// Reads every `.. grid-item::` option onto `item`, returning the lines nobody
/// claimed.
fn read_item_options(
    item: &mut GridItem,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for (option, _) in classify_options::<GridItemOption>(option_lines, GRID_ITEM, diagnostics, ctx)
    {
        match option {
            GridItemOption::Columns(columns) => item.columns = Some(columns),
            GridItemOption::Margin(margin) => item.margin = Some(margin.0),
            GridItemOption::Padding(padding) => item.padding = Some(padding.0),
            GridItemOption::ChildDirection(direction) => item.child_direction = direction,
            GridItemOption::ChildAlign(align) => item.child_align = Some(align),
            GridItemOption::Outline => item.outline = true,
            GridItemOption::Class(classes) => item.classes = classes.0,
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
    matches!(
        node,
        Node::Directive {
            directive: Directive::GridItem(_),
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_document;
    use rinx_ast::Spacing;
    use rinx_ast::{InlineNode, SpacingValue, Uml, walk_nodes};

    /// Parses a whole document and returns its single `.. grid::` with the
    /// document's diagnostics — the dispatcher is what positions the parse
    /// context, so spans are only meaningful this way.
    fn parse_grid_document(input: &str) -> (Grid, Vec<Diagnostic>) {
        let doc = parse_document("test.rst", input);
        let grid = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive {
                    directive: Directive::Grid(grid),
                    ..
                } => Some((**grid).clone()),
                _ => None,
            })
            .expect("document should contain a grid directive");
        (grid, doc.diagnostics)
    }

    /// As [`parse_grid_document`], for the item directive.
    fn parse_item_document(input: &str) -> (GridItem, Vec<Diagnostic>) {
        let doc = parse_document("test.rst", input);
        let item = find_item(&doc.nodes).expect("document should contain a grid-item directive");
        (item, doc.diagnostics)
    }

    /// The first `.. grid-item::` anywhere in `nodes`, however deeply nested.
    fn find_item(nodes: &[Node]) -> Option<GridItem> {
        let mut found = None;
        walk_nodes(nodes, &mut |node| {
            if found.is_none()
                && let Node::Directive {
                    directive: Directive::GridItem(item),
                    ..
                } = node
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
                && let Node::Directive {
                    directive: Directive::Uml(uml),
                    ..
                } = node
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
            Node::Directive {
                directive: Directive::GridItem(_),
                ..
            }
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
        let doc = parse_document("test.rst", input);

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
        let doc = parse_document("test.rst", input);

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
            vec![DiagnosticCode::DirectiveEmptyOptionValue]
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
                DiagnosticCode::DirectiveUnknownOption,
                DiagnosticCode::DirectiveUnknownOption,
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
            vec![Node::paragraph(vec![InlineNode::Text(
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
    fn test_a_choice_lists_its_vocabulary_in_declaration_order() {
        // Given / When
        let listed = <ChildAlign as crate::directives::option_value::ChoiceValue>::choices();

        // Then
        assert_eq!(listed, ["start", "end", "center", "justify", "spaced"]);
    }

    #[test]
    fn test_is_grid_item_distinguishes_an_item_from_other_content() {
        // Given
        let item = Node::directive(Directive::GridItem(Box::new(GridItem::new())));
        let prose = Node::paragraph(vec![InlineNode::Text("x".to_string())]);

        // When / Then
        assert!(is_grid_item(&item));
        assert!(!is_grid_item(&prose));
    }
}

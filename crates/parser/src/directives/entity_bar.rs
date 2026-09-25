//! `.. entity-bar::`, and its sphinx-needs spelling `.. needbar::` — a bar
//! chart of how many entities each cell of a grid of filters selects.
//!
//! The pie's sibling (`entity_pie.rs`), with the one difference that shapes
//! this whole module: its body is **two-dimensional**. Each content line is a
//! row — one *series*, drawn in one colour — and each cell of it, split on
//! `:separator:`, is one *category*. `:xlabels: FROM_DATA` and
//! `:ylabels: FROM_DATA` take a header row and a header column out of that
//! grid before anything is counted, and `:transpose:` swaps the two axes; all
//! three are this document's own text, so they are settled here and the node
//! carries the grid already in the orientation it is drawn in.
//!
//! Each cell is read where its own column is still in hand, through the same
//! [`read_filter_text`] a pie's content line and every `:filter:` option go
//! through, so a broken expression is reported at the character it breaks at
//! and a filter cannot select differently here than in a table.
//!
//! Where sphinx-needs raises — rows of different lengths, labels that do not
//! match the grid — this build reports and keeps drawing: a short row is
//! padded with zeros and surplus labels are dropped, because losing the whole
//! chart over one mistake would be the larger failure. The options it cannot
//! honour are refused **by name**, as the pie's are.

use rusty_sphinx_ast::{
    BarArrangement, BarGrid, BarOrientation, ChartValue, Diagnostic, DiagnosticCode, Directive,
    EntityBar, EntityBarSource, LabelRotation, Span,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;

use super::chart_options::{
    ChartCodes, ChartOwner, FigureOption, FigureRead, read_color, read_colors, read_figure_option,
    report_empty_value, report_unsupported, report_unusable_scale, unsupported_advice,
};
use super::filter_option::{
    FilterCodes, FilterOwner, FilterSite, read_filter_option, read_filter_text,
};
use super::options::{OptionLine, report_unknown_options, scan_option_lines};

/// The options sphinx-needs' `needbar` accepts that this build does not, each
/// with what an author should reach for instead.
///
/// Written with underscores, sphinx-needs' spelling; an option name is
/// normalised to it before it is looked up, so a hyphenated spelling is
/// refused with the same sentence.
const UNSUPPORTED_OPTIONS: [(&str, &str); 5] = [
    (
        "style",
        "that names a matplotlib stylesheet, which this build does not use; \
         use :colors: and :text_color:",
    ),
    (
        "status",
        "write the selection as :filter:, e.g. status == \"open\"",
    ),
    (
        "tags",
        "write the selection as :filter:, e.g. \"security\" in tags",
    ),
    (
        "types",
        "write the selection as :filter:, e.g. type == \"req\"",
    ),
    (
        "cypher",
        "that is a ubCode graph query, which this build does not evaluate; \
         write the selection as :filter:",
    ),
];

/// The separator sphinx-needs splits a content line on when none is written.
const DEFAULT_SEPARATOR: &str = ",";

/// The value of `:xlabels:`/`:ylabels:` that takes the labels from the body.
const FROM_DATA: &str = "FROM_DATA";

/// Parses a `.. entity-bar::` / `.. needbar::` into a [`Directive::EntityBar`].
pub(super) fn parse_entity_bar(
    source: EntityBarSource,
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.as_str();
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let mut bar = EntityBar {
        span: directive_span,
        title: (!argument.trim().is_empty()).then(|| argument.trim().to_string()),
        ..EntityBar::new(source, BarGrid::default())
    };
    let mut layout = Layout::default();

    let unrecognized = read_options(
        &mut bar,
        &mut layout,
        &option_lines,
        &unindented_lines,
        diagnostics,
        ctx,
    );
    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveEntityBarUnknownOption,
        diagnostics,
        ctx,
    );

    let body = BodyLines {
        lines: &unindented_lines,
        start: body_start,
    };
    bar.grid = read_grid(&layout, body, directive, directive_span, diagnostics, ctx);

    if bar.has_unusable_scale() {
        report_unusable_scale(&option_lines, owner(directive), diagnostics, ctx);
    }

    Directive::EntityBar(Box::new(bar))
}

/// The options that decide how the body is *read*, rather than how the chart
/// looks — so they are kept apart from the node, which never stores them.
struct Layout<'a> {
    /// `:separator:` — what a content line is split on.
    separator: String,
    /// `:xlabels:` — the categories' names.
    categories: LabelSpec<'a>,
    /// `:ylabels:` — the series' names.
    series: LabelSpec<'a>,
    /// `:transpose:` — swap series and categories once everything is read.
    transpose: bool,
}

impl Default for Layout<'_> {
    fn default() -> Self {
        Self {
            separator: DEFAULT_SEPARATOR.to_string(),
            categories: LabelSpec::Numbered,
            series: LabelSpec::Numbered,
            transpose: false,
        }
    }
}

/// How one axis' labels were given.
enum LabelSpec<'a> {
    /// Not at all: sphinx-needs numbers them `1`, `2`, ….
    Numbered,
    /// `FROM_DATA`: a header row or column of the body.
    FromData,
    /// A comma-separated list, kept with its line for a count mismatch to
    /// point at.
    Written {
        labels: Vec<Option<String>>,
        line: &'a OptionLine,
    },
}

impl LabelSpec<'_> {
    const fn is_from_data(&self) -> bool {
        matches!(self, Self::FromData)
    }
}

/// Reads every option onto `bar` and `layout`, returning the lines nobody
/// claimed.
fn read_options<'a>(
    bar: &mut EntityBar,
    layout: &mut Layout<'a>,
    option_lines: &'a [OptionLine],
    unindented_lines: &[String],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let directive = bar.source.as_str();
    let owner = owner(directive);
    let mut unrecognized = Vec::new();
    for line in option_lines {
        let name = line.name.replace('-', "_");
        match name.as_str() {
            "filter" => {
                let source_line = unindented_lines.get(line.line_index).map(String::as_str);
                bar.filter = read_filter_option(
                    line,
                    source_line,
                    directive,
                    FILTER_CODES,
                    diagnostics,
                    ctx,
                );
            }
            "legend" => bar.legend = true,
            "stacked" => bar.arrangement = BarArrangement::Stacked,
            "show_sum" => bar.value_labels.inside = true,
            "show_top_sum" => bar.value_labels.at_end = true,
            "horizontal" => bar.orientation = BarOrientation::Horizontal,
            "transpose" => layout.transpose = true,
            "separator" => {
                if line.value.is_empty() {
                    report_empty_value(line, owner, diagnostics, ctx);
                } else {
                    layout.separator.clone_from(&line.value);
                }
            }
            "xlabels" => layout.categories = read_label_spec(line, owner, diagnostics, ctx),
            "ylabels" => layout.series = read_label_spec(line, owner, diagnostics, ctx),
            "colors" => bar.colors = read_colors(line, owner, diagnostics, ctx),
            "text_color" => {
                bar.text_color = read_color(&line.value, line, owner, diagnostics, ctx);
            }
            "x_axis_title" => bar.x_axis_title = read_text(line, owner, diagnostics, ctx),
            "y_axis_title" => bar.y_axis_title = read_text(line, owner, diagnostics, ctx),
            "xlabels_rotation" => {
                bar.xlabels_rotation = read_rotation(line, directive, diagnostics, ctx);
            }
            "ylabels_rotation" => {
                bar.ylabels_rotation = read_rotation(line, directive, diagnostics, ctx);
            }
            "sum_rotation" => bar.sum_rotation = read_rotation(line, directive, diagnostics, ctx),
            _ => match read_figure_option(line, owner, diagnostics, ctx) {
                FigureRead::Read(option) => store_figure_option(bar, option),
                FigureRead::Reported => {}
                FigureRead::Unclaimed => match unsupported_advice(&UNSUPPORTED_OPTIONS, &name) {
                    Some(advice) => report_unsupported(line, advice, owner, diagnostics, ctx),
                    None => unrecognized.push(line),
                },
            },
        }
    }
    unrecognized
}

/// Reads `:xlabels:`/`:ylabels:` — `FROM_DATA`, or a list pairing by position.
///
/// `FROM_DATA` is recognised as the list's first entry, which is exactly the
/// test sphinx-needs makes.
fn read_label_spec<'a>(
    line: &'a OptionLine,
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> LabelSpec<'a> {
    if line.value.is_empty() {
        report_empty_value(line, owner, diagnostics, ctx);
        return LabelSpec::Numbered;
    }
    let labels: Vec<&str> = line.value.split(',').map(str::trim).collect();
    if labels.first() == Some(&FROM_DATA) {
        return LabelSpec::FromData;
    }
    LabelSpec::Written {
        labels: labels.into_iter().map(written_label).collect(),
        line,
    }
}

/// A label as written, where an empty one falls back to its ordinal.
fn written_label(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Reads an option whose value is free text that must not be empty.
fn read_text(
    line: &OptionLine,
    owner: ChartOwner<'_>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<String> {
    if line.value.is_empty() {
        report_empty_value(line, owner, diagnostics, ctx);
        return None;
    }
    Some(line.value.clone())
}

/// Reads one of the three rotation options.
///
/// A value that is not whole degrees is reported and the text drawn
/// unrotated. sphinx-needs draws it unrotated too, but silently — which leaves
/// the author no way to find out why their `-45` did nothing.
fn read_rotation(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<LabelRotation> {
    match LabelRotation::parse(&line.value) {
        Ok(rotation) => Some(rotation),
        Err(problem) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityBarInvalidRotation,
                format!("{directive}: :{}: {problem}", line.name),
                ctx.line_span(line.line_index, &line.raw),
            ));
            None
        }
    }
}

/// The directive body below its options.
#[derive(Clone, Copy)]
struct BodyLines<'a> {
    lines: &'a [String],
    start: usize,
}

/// One content line, split into cells.
struct WrittenRow<'a> {
    /// The line's index within the directive body.
    line_index: usize,
    /// The whole line, for spans that cover it.
    raw: &'a str,
    /// The cells, each `None` where the row was padded to the widest one.
    cells: Vec<Option<WrittenCell<'a>>>,
}

/// One cell, with the column its text starts at.
#[derive(Clone, Copy)]
struct WrittenCell<'a> {
    text: &'a str,
    column: usize,
}

/// Reads the body into a labelled grid, in drawing orientation.
fn read_grid(
    layout: &Layout<'_>,
    body: BodyLines<'_>,
    directive: &str,
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> BarGrid {
    let mut rows = split_rows(body, &layout.separator);
    pad_ragged_rows(&mut rows, directive, diagnostics, ctx);

    let mut categories = match &layout.categories {
        LabelSpec::FromData if !rows.is_empty() => {
            let header = rows.remove(0);
            let skip = usize::from(layout.series.is_from_data());
            header
                .cells
                .iter()
                .skip(skip)
                .map(|cell| cell.and_then(|cell| written_label(cell.text)))
                .collect()
        }
        _ => Vec::new(),
    };
    let mut series = Vec::new();
    if layout.series.is_from_data() {
        for row in &mut rows {
            let first = if row.cells.is_empty() {
                None
            } else {
                row.cells.remove(0)
            };
            series.push(first.and_then(|cell| written_label(cell.text)));
        }
    }

    let values: Vec<Vec<ChartValue>> = rows
        .iter()
        .map(|row| read_row_values(row, directive, diagnostics, ctx))
        .collect();
    // Rectangular by construction — every row was padded to the widest — so
    // the empty fallback is unreachable rather than a policy.
    let grid = BarGrid::new(values).unwrap_or_default();
    if grid.series_count() == 0 || grid.category_count() == 0 {
        report_no_data(directive, directive_span, diagnostics);
        return grid;
    }

    if let LabelSpec::Written { labels, line } = &layout.categories {
        check_label_count(
            labels,
            grid.category_count(),
            line,
            directive,
            diagnostics,
            ctx,
        );
        categories.clone_from(labels);
    }
    if let LabelSpec::Written { labels, line } = &layout.series {
        check_label_count(
            labels,
            grid.series_count(),
            line,
            directive,
            diagnostics,
            ctx,
        );
        series.clone_from(labels);
    }

    let grid = grid
        .with_category_labels(categories)
        .with_series_labels(series);
    if layout.transpose {
        grid.transposed()
    } else {
        grid
    }
}

/// Splits every non-blank content line on `separator`.
///
/// Split naïvely, as sphinx-needs does: a separator inside a filter's string
/// literal still splits it, which is exactly what `:separator:` is for.
fn split_rows<'a>(body: BodyLines<'a>, separator: &str) -> Vec<WrittenRow<'a>> {
    body.lines
        .iter()
        .enumerate()
        .skip(body.start)
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line_index, line)| WrittenRow {
            line_index,
            raw: line,
            cells: split_cells(line, separator).into_iter().map(Some).collect(),
        })
        .collect()
}

/// Splits one line into trimmed cells, each with the column it starts at.
fn split_cells<'a>(line: &'a str, separator: &str) -> Vec<WrittenCell<'a>> {
    let mut column = 0;
    let mut cells = Vec::new();
    for piece in line.split(separator) {
        let leading = piece.chars().count() - piece.trim_start().chars().count();
        cells.push(WrittenCell {
            text: piece.trim(),
            column: column + leading,
        });
        column += piece.chars().count() + separator.chars().count();
    }
    cells
}

/// Pads every row to the widest, reporting each one that was shorter.
///
/// Widest rather than first, so a long row loses nothing either: the grid
/// widens to hold it and every other row is the one reported.
fn pad_ragged_rows(
    rows: &mut [WrittenRow<'_>],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let widest = rows.iter().map(|row| row.cells.len()).max().unwrap_or(0);
    for row in rows {
        let written = row.cells.len();
        if written == widest {
            continue;
        }
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityBarRaggedRow,
            format!(
                "{directive}: this line has {written} cell{} but the widest has {widest}; the \
                 missing ones are drawn as 0",
                plural(written, "", "s"),
            ),
            ctx.line_span(row.line_index, row.raw),
        ));
        row.cells.resize(widest, None);
    }
}

/// Reads a row's cells into values, a padded cell counting zero.
fn read_row_values(
    row: &WrittenRow<'_>,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<ChartValue> {
    row.cells
        .iter()
        .map(|cell| match cell {
            None => ChartValue::Count(0),
            Some(cell) => read_cell(*cell, row, directive, diagnostics, ctx),
        })
        .collect()
}

/// Reads one cell as a number or a filter, tried in that order for the pie's
/// reason: a bare `12` is also a filter, and reading it as one would count the
/// whole project.
fn read_cell(
    cell: WrittenCell<'_>,
    row: &WrittenRow<'_>,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> ChartValue {
    if let Ok(count) = cell.text.parse::<u64>() {
        return ChartValue::Count(count);
    }
    ChartValue::Filter(read_filter_text(
        cell.text,
        FilterSite {
            line_index: row.line_index,
            column: Some(cell.column),
            raw: row.raw,
        },
        FilterOwner {
            directive,
            option: None,
            codes: FILTER_CODES,
        },
        diagnostics,
        ctx,
    ))
}

/// Reports a written label list whose length disagrees with its axis.
///
/// They pair by position, so a surplus or shortfall means at least one bar is
/// named wrongly and the author cannot see which from the list alone.
fn check_label_count(
    labels: &[Option<String>],
    expected: usize,
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    if labels.len() == expected {
        return;
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityBarLabelCountMismatch,
        format!(
            "{directive}: :{}: has {} entr{} but the chart has {expected} to name; they pair by \
             position, so at least one bar is named wrongly",
            line.name,
            labels.len(),
            plural(labels.len(), "y", "ies"),
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports a chart whose body held no values once its labels were taken out.
fn report_no_data(directive: &str, directive_span: Option<Span>, diagnostics: &mut Diagnostics) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityBarNoData,
        format!(
            "{directive}: has no values, so there is nothing to chart; write one line per series \
             below the options, one filter or number per category, separated by commas"
        ),
        directive_span,
    ));
}

/// `singular` for exactly one, `plural` otherwise.
const fn plural(count: usize, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 { singular } else { plural }
}

/// Stores a figure option where a bar chart keeps it.
fn store_figure_option(bar: &mut EntityBar, option: FigureOption) {
    match option {
        FigureOption::Caption(caption) => bar.caption = Some(caption),
        FigureOption::Align(align) => bar.align = Some(align),
        FigureOption::Scale(scale) => bar.scale = Some(scale),
        FigureOption::Width(width) => bar.width = Some(width),
        FigureOption::Classes(classes) => bar.classes = classes,
        FigureOption::Name(name) => bar.name = Some(name),
    }
}

/// The bar chart as the owner of the options every chart shares.
const fn owner(directive: &str) -> ChartOwner<'_> {
    ChartOwner {
        directive,
        codes: CHART_CODES,
    }
}

/// The codes a bar chart reports its filters' failures under.
const FILTER_CODES: FilterCodes = FilterCodes {
    invalid: DiagnosticCode::EntityBarInvalidFilter,
    unknown_field: DiagnosticCode::EntityBarUnknownField,
};

/// The codes a bar chart reports the shared chart options' failures under.
const CHART_CODES: ChartCodes = ChartCodes {
    invalid_color: DiagnosticCode::EntityBarInvalidColor,
    invalid_align: DiagnosticCode::EntityBarInvalidAlign,
    invalid_scale: DiagnosticCode::EntityBarInvalidScale,
    invalid_width: DiagnosticCode::EntityBarInvalidWidth,
    unusable_scale: DiagnosticCode::EntityBarUnusableScale,
    empty_option_value: DiagnosticCode::EntityBarEmptyOptionValue,
    unsupported_option: DiagnosticCode::EntityBarUnsupportedOption,
};

#[cfg(test)]
mod tests;

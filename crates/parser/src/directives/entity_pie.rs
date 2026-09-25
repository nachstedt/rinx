//! `.. entity-pie::`, and its sphinx-needs spelling `.. needpie::` — a pie
//! chart of how many entities each of several filters selects.
//!
//! Its shape is unlike either directive it is a sibling of. An
//! `.. entity-table::` refuses an argument and an `.. entity-flow::` refuses
//! content; a pie takes **both** — the argument is the chart's title, and each
//! content line is one wedge. That is sphinx-needs' own shape, and every
//! `.. needpie::` in the benchmark corpus writes it.
//!
//! The body is therefore where most of the work is. Each line is parsed here,
//! while its own position is still in hand, so a broken expression is reported
//! at the character it breaks at — the same reason a `:filter:` option is
//! parsed here rather than where it is evaluated, reached through the same
//! [`read_filter_text`] the option goes through so the two cannot select
//! differently.
//!
//! What is left for the renderer is the counting, which needs the whole
//! project, and the drawing.
//!
//! sphinx-needs' `needpie` has a wider option set than this build draws. The
//! ones it does not are refused **by name**, not ignored: an author who asked
//! for exploded wedges and silently got none has no way to find out why.

use rinx_ast::{Diagnostic, DiagnosticCode, Directive, EntityPie, EntityPieSource, PieSlice, Span};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;

use super::chart_options::{
    ChartCodes, ChartOwner, FigureOption, FigureRead, read_color, read_colors, read_figure_option,
    report_unsupported, report_unusable_scale, unsupported_advice,
};
use super::filter_option::{
    FilterCodes, FilterOwner, FilterSite, read_filter_option, read_filter_text,
};
use super::options::{OptionLine, report_unknown_options, scan_option_lines};

/// The options sphinx-needs' `needpie` accepts that this build does not, each
/// with what an author should reach for instead.
///
/// A table rather than a match arm apiece for the reason `entity_flow.rs`'s
/// own table gives: the option is being refused, so the sentence beside it is
/// the entire feature. Both spellings of every multiword name are listed,
/// since sphinx-needs writes them with underscores and this build with
/// hyphens, and an author migrating a document will write either.
const UNSUPPORTED_OPTIONS: [(&str, &str); 6] = [
    (
        "explode",
        "every wedge is drawn in place; use :colors: to tell them apart",
    ),
    ("shadow", "wedges are drawn flat"),
    (
        "style",
        "that names a matplotlib stylesheet, which this build does not use",
    ),
    ("filter-func", "write the selection as :filter:"),
    ("filter_func", "write the selection as :filter:"),
    (
        "filter_warning",
        "an empty chart is reported as entity-pie.empty-result",
    ),
];

/// Parses a `.. entity-pie::` / `.. needpie::` into a [`Directive::EntityPie`].
pub(super) fn parse_entity_pie(
    source: EntityPieSource,
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.as_str();
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let mut pie = EntityPie {
        span: directive_span,
        title: (!argument.trim().is_empty()).then(|| argument.trim().to_string()),
        ..EntityPie::new(source)
    };

    let unrecognized = read_options(&mut pie, &option_lines, &unindented_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveEntityPieUnknownOption,
        diagnostics,
        ctx,
    );

    pie.slices = read_slices(&unindented_lines, body_start, directive, diagnostics, ctx);
    if pie.slices.is_empty() {
        report_no_slices(directive, directive_span, diagnostics);
    }
    apply_labels(&mut pie, &option_lines, directive, diagnostics, ctx);

    if pie.has_unusable_scale() {
        report_unusable_scale(&option_lines, owner(directive), diagnostics, ctx);
    }

    Directive::EntityPie(Box::new(pie))
}

/// Reads the body into one wedge per non-blank line.
///
/// A line is a number or a filter, tried in that order because a bare `12` is
/// also a perfectly good filter — a [`Truthy`] test on an integer literal —
/// and reading it as one would count the whole project instead of drawing a
/// wedge of twelve.
///
/// Blank lines are skipped rather than becoming empty wedges, so the option
/// block and the content may be separated the way every directive's are.
///
/// [`Truthy`]: rinx_filter::Expr::Truthy
fn read_slices(
    unindented_lines: &[String],
    body_start: usize,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<PieSlice> {
    let mut slices = Vec::new();
    for (offset, line) in unindented_lines[body_start..].iter().enumerate() {
        let written = line.trim();
        if written.is_empty() {
            continue;
        }
        let line_index = body_start + offset;
        slices.push(match written.parse::<u64>() {
            Ok(count) => PieSlice::from_count(count),
            Err(_) => PieSlice::from_filter(read_slice_filter(
                written,
                line,
                line_index,
                directive,
                diagnostics,
                ctx,
            )),
        });
    }
    slices
}

/// Parses one body line as a filter, under this directive's own codes.
///
/// The column the filter starts at is whatever indentation survived
/// `unindent_body_lines`, which strips only the block's *common* indent — so a
/// line indented further than its siblings still reports at the right column.
fn read_slice_filter(
    written: &str,
    line: &str,
    line_index: usize,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<rinx_filter::Expr> {
    let column = line.chars().count() - line.trim_start().chars().count();
    read_filter_text(
        written,
        FilterSite {
            line_index,
            column: Some(column),
            raw: line,
        },
        FilterOwner {
            directive,
            option: None,
            codes: FILTER_CODES,
        },
        diagnostics,
        ctx,
    )
}

/// Pairs `:labels:` with the wedges by position.
///
/// Position is the whole interface between the two, which is what makes a
/// count mismatch worth reporting: the author cannot see from the labels alone
/// that one of them has slid onto the wrong wedge. The wedges are still drawn
/// and the surplus labels dropped — losing the data over a naming mistake
/// would be the larger failure.
fn apply_labels(
    pie: &mut EntityPie,
    option_lines: &[OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let Some(line) = option_lines.iter().rev().find(|line| line.name == "labels") else {
        return;
    };
    let labels: Vec<&str> = line.value.split(',').map(str::trim).collect();
    if labels.len() != pie.slices.len() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityPieLabelCountMismatch,
            format!(
                "{directive}: :labels: has {} entr{} but the chart has {} wedge{}; they pair by \
                 position, so at least one wedge is named wrongly",
                labels.len(),
                if labels.len() == 1 { "y" } else { "ies" },
                pie.slices.len(),
                if pie.slices.len() == 1 { "" } else { "s" },
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
    }
    for (slice, label) in pie.slices.iter_mut().zip(labels) {
        if !label.is_empty() {
            slice.label = Some(label.to_string());
        }
    }
}

/// Reads every option onto `pie`, returning the lines nobody claimed.
fn read_options<'a>(
    pie: &mut EntityPie,
    option_lines: &'a [OptionLine],
    unindented_lines: &[String],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let directive = pie.source.as_str();
    let mut unrecognized = Vec::new();
    for line in option_lines {
        match line.name.as_str() {
            "filter" => {
                let source_line = unindented_lines.get(line.line_index).map(String::as_str);
                pie.filter = read_filter_option(
                    line,
                    source_line,
                    directive,
                    FILTER_CODES,
                    diagnostics,
                    ctx,
                );
            }
            // Read after the body, since it pairs with the wedges by position
            // and they do not exist yet.
            "labels" => {}
            "legend" => pie.legend = true,
            "colors" => pie.colors = read_colors(line, owner(directive), diagnostics, ctx),
            "text_color" | "text-color" => {
                pie.text_color = read_color(&line.value, line, owner(directive), diagnostics, ctx);
            }
            name => match read_figure_option(line, owner(directive), diagnostics, ctx) {
                FigureRead::Read(option) => store_figure_option(pie, option),
                FigureRead::Reported => {}
                FigureRead::Unclaimed => match unsupported_advice(&UNSUPPORTED_OPTIONS, name) {
                    Some(advice) => {
                        report_unsupported(line, advice, owner(directive), diagnostics, ctx);
                    }
                    None => unrecognized.push(line),
                },
            },
        }
    }
    unrecognized
}

/// The codes a pie chart reports its filters' failures under.
const FILTER_CODES: FilterCodes = FilterCodes {
    invalid: DiagnosticCode::EntityPieInvalidFilter,
    unknown_field: DiagnosticCode::EntityPieUnknownField,
};

/// Reports a chart whose body held nothing to count.
///
/// Reported while parsing, unlike `entity-pie.empty-result`, because the body
/// is this document's own text: no index is needed to see that there is
/// nothing there. The message names the shape, since a `.. needpie::` whose
/// filters were written as options rather than content is the likely mistake.
fn report_no_slices(directive: &str, directive_span: Option<Span>, diagnostics: &mut Diagnostics) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityPieNoSlices,
        format!(
            "{directive}: has no content, so there is nothing to chart; write one filter per line \
             below the options, and name them with :labels:"
        ),
        directive_span,
    ));
}

/// Stores a figure option where a pie keeps it.
fn store_figure_option(pie: &mut EntityPie, option: FigureOption) {
    match option {
        FigureOption::Caption(caption) => pie.caption = Some(caption),
        FigureOption::Align(align) => pie.align = Some(align),
        FigureOption::Scale(scale) => pie.scale = Some(scale),
        FigureOption::Width(width) => pie.width = Some(width),
        FigureOption::Classes(classes) => pie.classes = classes,
        FigureOption::Name(name) => pie.name = Some(name),
    }
}

/// The pie as the owner of the options every chart shares.
const fn owner(directive: &str) -> ChartOwner<'_> {
    ChartOwner {
        directive,
        codes: CHART_CODES,
    }
}

/// The codes a pie chart reports the shared chart options' failures under.
const CHART_CODES: ChartCodes = ChartCodes {
    invalid_color: DiagnosticCode::EntityPieInvalidColor,
    invalid_align: DiagnosticCode::EntityPieInvalidAlign,
    invalid_scale: DiagnosticCode::EntityPieInvalidScale,
    invalid_width: DiagnosticCode::EntityPieInvalidWidth,
    unusable_scale: DiagnosticCode::EntityPieUnusableScale,
    empty_option_value: DiagnosticCode::EntityPieEmptyOptionValue,
    unsupported_option: DiagnosticCode::EntityPieUnsupportedOption,
};

#[cfg(test)]
mod tests;

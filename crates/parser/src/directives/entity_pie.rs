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

use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::spec::{Arguments, DirectiveShape, DirectiveSpec};
use rinx_ast::{Diagnostic, DiagnosticCode, Directive, EntityPie, EntityPieSource, PieSlice, Span};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::chart_options::{
    ChartCodes, ChartFigureOption, ChartOwner, FigureOption, read_color, read_colors,
    read_figure_option, report_unusable_scale,
};
use super::filter_option::{
    FilterCodes, FilterOwner, FilterSite, ShorthandFilterOption, read_filter_option,
    read_filter_text,
};
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};

directive_options! {
    /// The options `.. entity-pie::` takes: sphinx-needs' `needpie`'s
    /// fourteen, of which this build reads five, and the figure options. The
    /// ones it does not draw are refused by name, each with what to reach for
    /// instead.
    enum EntityPieOption {
        /// `:filter:`.
        Filter => (["filter"], Filter, "The entities every wedge counts among, by a filter."),
        /// `:labels:`.
        Labels => (["labels"], Text, "The wedges' names, comma-separated, in content order."),
        /// `:legend:`.
        Legend => (["legend"], Flag, "Draws a legend of the wedges."),
        /// `:colors:`.
        Colors => (["colors"], Text, "The wedges' colours, comma-separated, in content order."),
        /// `:text_color:`.
        TextColor => (["text_color", "text-color"], Text, "The colour of the chart's text."),
        _ => (["explode"], Text, "How far each wedge is pulled out.", "every wedge is drawn in place; use :colors: to tell them apart"),
        _ => (["shadow"], Flag, "Draws a shadow below the pie.", "wedges are drawn flat"),
        _ => (["style"], Text, "A matplotlib stylesheet.", "that names a matplotlib stylesheet, which this build does not use"),
        _ => (["filter-func", "filter_func"], Text, "The entities counted, by a Python function.", "write the selection as :filter:"),
        _ => (["filter_warning"], Text, "The text shown when nothing is counted.", "an empty chart is reported as entity-pie.empty-result"),
        _ => (["cypher"], Text, "The entities counted, by a Cypher query.", "write the selection as :filter:");
        Figure(ChartFigureOption),
        Shorthand(ShorthandFilterOption),
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["entity-pie", "needpie"],
    doc: "A pie chart of how many entities each filter selects.",
    shape: DirectiveShape {
        arguments: Arguments::OPTIONAL_TEXT,
        takes_options: true,
        takes_content: true,
        content_hint: None,
    },
    options: <EntityPieOption as DirectiveOptions>::GROUPS,
};

/// Interprets `.. entity-pie::` or `.. needpie::`, recording which was
/// written.
pub(in crate::directives) fn interpret_entity_pie(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(source) = input.name.parse::<rinx_ast::EntityPieSource>() else {
        return unknown(input, state);
    };
    single(parse_entity_pie(source, input, state.diagnostics))
}

/// Parses a `.. entity-pie::` / `.. needpie::` into a [`Directive::EntityPie`].
pub(super) fn parse_entity_pie(
    source: EntityPieSource,
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let directive = source.as_str();
    let argument = invocation.argument(0);
    let directive_span = invocation.span;
    let ctx = &invocation.ctx;
    let unindented_lines = &invocation.lines;
    let option_lines = &invocation.options;
    let body_start = invocation.content_start;

    let mut pie = EntityPie {
        span: directive_span,
        title: (!argument.trim().is_empty()).then(|| argument.trim().to_string()),
        ..EntityPie::new(source)
    };

    read_options(&mut pie, option_lines, unindented_lines, diagnostics, ctx);

    pie.slices = read_slices(unindented_lines, body_start, directive, diagnostics, ctx);
    if pie.slices.is_empty() {
        report_no_slices(directive, directive_span, diagnostics);
    }
    apply_labels(&mut pie, option_lines, directive, diagnostics, ctx);

    if pie.has_unusable_scale() {
        report_unusable_scale(option_lines, owner(directive), diagnostics, ctx);
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

/// Reads every option onto `pie`, reporting the ones it does not take.
fn read_options(
    pie: &mut EntityPie,
    option_lines: &[OptionLine],
    unindented_lines: &[String],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let directive = pie.source.as_str();
    for (option, line) in
        classify_options::<EntityPieOption>(option_lines, directive, diagnostics, ctx)
    {
        match option {
            EntityPieOption::Filter => {
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
            EntityPieOption::Labels => {}
            EntityPieOption::Legend => pie.legend = true,
            EntityPieOption::Colors => {
                pie.colors = read_colors(line, owner(directive), diagnostics, ctx);
            }
            EntityPieOption::TextColor => {
                pie.text_color = read_color(&line.value, line, owner(directive), diagnostics, ctx);
            }
            EntityPieOption::Figure(option) => {
                if let Some(option) =
                    read_figure_option(option, line, owner(directive), diagnostics, ctx)
                {
                    store_figure_option(pie, option);
                }
            }
            EntityPieOption::Shorthand(option) => match option {},
        }
    }
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
};

#[cfg(test)]
mod tests;

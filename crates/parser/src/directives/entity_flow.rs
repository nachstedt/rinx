//! `.. entity-flow::`, and its sphinx-needs spelling `.. needflow::` — a
//! picture of the entities matching a filter and the relations between them.
//!
//! Everything that can only happen here happens here: parsing the filter
//! expression, checking every field it names against the entity schema, and
//! checking every `:relations:` entry is a relation some type declares — all
//! while the option line's own position is still in hand. What is left for the
//! renderer is the question itself, which needs the whole project to answer.
//!
//! sphinx-needs' `needflow` has a wider option set than this build draws. The
//! ones it does not are refused **by name**, not ignored (see
//! [`EntityFlowOption`]).
//!
//! An unreadable option is dropped and reported, leaving the flowchart itself
//! intact — the error-resilience every directive here follows, and the same
//! sharper reason a diagram has for it: a picture refused over a misspelled
//! `:align:` is a page with a hole in it.

use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::option_value::{Classes, NonEmptyText, OptionValueType, Percentage};
use crate::directives::spec::OptionValue;
use crate::directives::spec::{Arguments, DirectiveSpec};
use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityFlow, EntityFlowSource, FlowDirection, ImageAlign,
    LengthOrPercentage, TargetName,
};
use rinx_filter::Expr;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::filter_option::{FilterCodes, ShorthandFilterOption, read_filter_option};
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};

/// The advice for the options choosing the entities from a root.
const SELECT_WITH_FILTER: &str = "select the entities to draw with :filter:";

/// The advice for the options styling single entities.
const DRAWN_ALIKE: &str = "every entity is drawn the same way";

directive_options! {
    /// The options `.. entity-flow::` takes: sphinx-needs' `needflow`'s thirty,
    /// of which this build reads eleven, and its own `:caption:`.
    ///
    /// The ones this build does not draw are refused **by name**, each with
    /// what to reach for instead: an author who asked for a legend and
    /// silently got none has no way to find out why. Both spellings of every
    /// multiword name are listed, since sphinx-needs writes them with
    /// underscores and this build with hyphens.
    enum EntityFlowOption {
        /// `:filter:`.
        Filter => (["filter"], Filter, "The entities drawn, by a filter."),
        /// `:relations:`.
        Relations => (["relations", "link_types", "link-types"], Text, "The relations drawn as edges, comma-separated."),
        /// `:show-link-names:`.
        ShowLinkNames => (["show-link-names", "show_link_names"], Flag, "Labels each edge with its relation."),
        /// `:direction:`.
        Direction(FlowDirection) => (["direction"], EntityFlowInvalidDirection, "Which way the chart is laid out."),
        /// `:config:`.
        Config(NonEmptyText) => (["config"], "A PlantUML configuration the chart is drawn with."),
        /// `:debug:`.
        Debug => (["debug"], Flag, "Shows the generated PlantUML source below the chart."),
        /// `:caption:`.
        Caption(String) => (["caption"], "A caption shown below the chart."),
        /// `:align:`.
        Align(ImageAlign) => (["align"], EntityFlowInvalidAlign, "Where the chart sits."),
        /// `:scale:`.
        Scale(Percentage) => (["scale"], EntityFlowInvalidScale, "The chart's size, as a percentage of its `:width:`."),
        /// `:width:`.
        Width(LengthOrPercentage) => (["width"], EntityFlowInvalidWidth, "The chart's width."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the chart."),
        /// `:name:`.
        Name(TargetName) => (["name"], "A label to reference the chart by."),
        _ => (["show_filters", "show-filters"], Flag, "Shows the filter below the chart.", "the filter is visible in the document source"),
        _ => (["show_legend", "show-legend"], Flag, "Draws a legend of the entity types.", "no legend is drawn"),
        _ => (["filter-func", "filter_func"], Text, "The entities drawn, by a Python function.", "write the selection as :filter:"),
        _ => (["cypher"], Text, "The entities drawn, by a Cypher query.", "write the selection as :filter:"),
        _ => (["filter_warning"], Text, "The text shown when nothing is drawn."),
        _ => (["export_id"], Text, "The id the filter is exported under."),
        _ => (["max_items"], Integer, "How many entities are drawn at most."),
        _ => (["sort_by"], Text, "The field the entities are ordered by."),
        _ => (["highlight"], Filter, "The entities drawn highlighted, by a filter.", DRAWN_ALIKE),
        _ => (["border_color", "border-color"], Text, "The colour of each entity's border.", DRAWN_ALIKE),
        _ => (["engine"], Text, "The program the chart is laid out by.", "every diagram here is compiled by PlantUML"),
        _ => (["root_id", "root-id"], Text, "The entity the chart is drawn from.", SELECT_WITH_FILTER),
        _ => (["root_direction"], Text, "Which relations are followed from the root.", SELECT_WITH_FILTER),
        _ => (["root_depth"], Integer, "How far relations are followed from the root.", SELECT_WITH_FILTER),
        _ => (["alt"], Text, "Text describing the chart."),
        _ => (["height"], Length, "The chart's height.");
        Shorthand(ShorthandFilterOption),
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["entity-flow", "needflow"],
    doc: "A flowchart of the entities a filter selects and their relations.",
    arguments: Arguments::NONE,
    takes_content: false,
    content_hint: Some(
        "its picture is generated from the entity graph: select the entities with :filter:, or write `.. entity-diagram::` to draw a diagram of your own",
    ),
    options: <EntityFlowOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. entity-flow::` or `.. needflow::`, recording which was
/// written.
pub(in crate::directives) fn interpret_entity_flow(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(source) = input.name.parse::<rinx_ast::EntityFlowSource>() else {
        return unknown(input, state);
    };
    single(parse_entity_flow(source, input, state.diagnostics))
}

/// Parses a `.. entity-flow::` / `.. needflow::` into a
/// [`Directive::EntityFlow`].
pub(super) fn parse_entity_flow(
    source: EntityFlowSource,
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let directive = source.as_str();
    let directive_span = invocation.span;
    let ctx = &invocation.ctx;
    let unindented_lines = &invocation.lines;
    let option_lines = &invocation.options;

    let mut flow = EntityFlow {
        span: directive_span,
        ..EntityFlow::new(source)
    };

    read_options(&mut flow, option_lines, unindented_lines, diagnostics, ctx);
    if flow.has_unusable_scale() {
        report_unusable_scale(option_lines, directive, diagnostics, ctx);
    }

    Directive::EntityFlow(Box::new(flow))
}

/// Reads every option onto `flow`, reporting the ones it does not take.
fn read_options(
    flow: &mut EntityFlow,
    option_lines: &[OptionLine],
    unindented_lines: &[String],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let directive = flow.source.as_str();
    for (option, line) in
        classify_options::<EntityFlowOption>(option_lines, directive, diagnostics, ctx)
    {
        match option {
            EntityFlowOption::Filter => {
                let source_line = unindented_lines.get(line.line_index).map(String::as_str);
                flow.filter = read_filter(line, source_line, directive, diagnostics, ctx);
            }
            EntityFlowOption::Relations => {
                flow.relations = read_relations(line, directive, diagnostics, ctx);
            }
            EntityFlowOption::ShowLinkNames => flow.show_link_names = true,
            EntityFlowOption::Direction(direction) => flow.direction = direction,
            EntityFlowOption::Config(config) => flow.config = Some(config.0),
            EntityFlowOption::Debug => flow.debug = true,
            EntityFlowOption::Caption(caption) => flow.caption = Some(caption),
            EntityFlowOption::Align(align) => flow.align = Some(align),
            EntityFlowOption::Scale(scale) => flow.scale = Some(scale.0),
            EntityFlowOption::Width(width) => flow.width = Some(width),
            EntityFlowOption::Class(classes) => flow.classes = classes.0,
            EntityFlowOption::Name(name) => flow.name = Some(name),
            EntityFlowOption::Shorthand(option) => match option {},
        }
    }
}

/// Reads `:filter:` under this directive's own diagnostic codes.
///
/// The reading itself is [`read_filter_option`], shared with
/// `.. entity-table::` so a question asked of the entity graph means the same
/// thing whichever way its answer is drawn. A flowchart whose filter could not
/// be parsed draws *everything* rather than nothing, for the reason a table
/// lists everything: the diagnostic already says what is wrong, and an empty
/// picture on top of it would hide which entities the author was reaching for
/// — and would then be reported a second time, as `entity-flow.empty-result`.
fn read_filter(
    line: &OptionLine,
    source_line: Option<&str>,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Expr> {
    read_filter_option(line, source_line, directive, FILTER_CODES, diagnostics, ctx)
}

/// The codes a flowchart reports its filter's failures under.
const FILTER_CODES: FilterCodes = FilterCodes {
    invalid: DiagnosticCode::EntityFlowInvalidFilter,
    unknown_field: DiagnosticCode::EntityFlowUnknownField,
};

/// Reads `:relations:` — which relations become edges.
///
/// An entry no type declares is dropped and reported while the rest are kept,
/// unlike `.. entity-table::`'s all-or-nothing `:columns:`. The two differ
/// because the failures do: a table missing one column of several is hard to
/// notice, while a flowchart missing one kind of edge is the picture the author
/// is looking at.
///
/// An option naming nothing usable at all leaves the flowchart drawing every
/// declared relation, which is what omitting it means.
fn read_relations(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Vec<String>> {
    let mut relations = Vec::new();
    for written in line.value.split(',').map(str::trim) {
        if written.is_empty() {
            continue;
        }
        if ctx.schema.relation(written).is_some() {
            relations.push(written.to_string());
        } else {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityFlowUnknownRelation,
                format!(
                    "{directive}: :{}: unknown relation '{written}'; the schema declares {}",
                    line.name,
                    declared_relations(ctx)
                ),
                ctx.line_span(line.line_index, &line.raw),
            ));
        }
    }
    (!relations.is_empty()).then_some(relations)
}

/// The relations a schema declares, for an unknown-relation diagnostic.
fn declared_relations(ctx: &ParseCtx<'_>) -> String {
    let names = ctx.schema.relation_names();
    if names.is_empty() {
        return "none".to_string();
    }
    names.join(", ")
}

/// A `:direction:`: one of the two layouts `PlantUML` has, in either spelling;
/// any other keeps the default.
impl OptionValueType for FlowDirection {
    const VALUE: OptionValue<'static> = OptionValue::Choice(FlowDirection::spellings);

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        FlowDirection::new(&line.value).map_err(|problem| format!("{problem}; {DIRECTION_NOTE}"))
    }
}

/// Why a direction that reads perfectly sensibly is still refused.
///
/// Worth a clause of its own: `RL` and `BT` are graphviz' own spellings, and
/// an author writing one is not making a typo — they are asking for something
/// `PlantUML` has no statement for.
const DIRECTION_NOTE: &str =
    "PlantUML lays a diagram out top-to-bottom or left-to-right and has no other direction";

/// Reports a `:scale:` that has no `:width:` to apply to.
///
/// Pointed at the `:scale:` line rather than at the directive, since that is
/// the line the author would have to change — the choice `.. image::` and
/// every diagram directive already make for their own version of this.
fn report_unusable_scale(
    option_lines: &[OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let span = option_lines
        .iter()
        .rev()
        .find(|line| line.name == "scale")
        .and_then(|line| ctx.line_span(line.line_index, &line.raw));
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityFlowUnusableScale,
        format!(
            "{directive}: :scale: has no :width: to apply to, so it was ignored — the compiled \
             diagram is never opened while rendering, so it has no size of its own to scale"
        ),
        span,
    ));
}

#[cfg(test)]
mod tests;

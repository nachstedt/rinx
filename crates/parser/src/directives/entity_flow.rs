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
//! ones it does not are refused **by name**, not ignored: an author who asked
//! for a legend and silently got none has no way to find out why.
//!
//! An unreadable option is dropped and reported, leaving the flowchart itself
//! intact — the error-resilience every directive here follows, and the same
//! sharper reason a diagram has for it: a picture refused over a misspelled
//! `:align:` is a page with a hole in it.

use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityFlow, EntityFlowSource, FlowDirection, ImageAlign,
    LengthOrPercentage, Span, TargetName,
};
use rinx_filter::Expr;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;

use super::filter_option::{FilterCodes, read_filter_option};
use super::options::{OptionLine, parse_percentage, report_unknown_options, scan_option_lines};

/// The options sphinx-needs' `needflow` accepts that this build does not,
/// each with what an author should reach for instead.
///
/// A table rather than a match arm apiece because the *point* of each entry is
/// the sentence beside it: the option is being refused, so the diagnostic is
/// the entire feature. Both spellings of every multiword name are listed,
/// since sphinx-needs writes them with underscores and this build with
/// hyphens, and an author migrating a document will write either.
const UNSUPPORTED_OPTIONS: [(&str, &str); 16] = [
    (
        "show_filters",
        "the filter is visible in the document source",
    ),
    (
        "show-filters",
        "the filter is visible in the document source",
    ),
    ("show_legend", "no legend is drawn"),
    ("show-legend", "no legend is drawn"),
    ("filter-func", "write the selection as :filter:"),
    ("filter_func", "write the selection as :filter:"),
    ("highlight", "every entity is drawn the same way"),
    ("border_color", "every entity is drawn the same way"),
    ("border-color", "every entity is drawn the same way"),
    ("engine", "every diagram here is compiled by PlantUML"),
    ("root_id", "select the entities to draw with :filter:"),
    ("root-id", "select the entities to draw with :filter:"),
    (
        "root_direction",
        "select the entities to draw with :filter:",
    ),
    ("root_depth", "select the entities to draw with :filter:"),
    (
        "tags",
        "write the selection as :filter:, e.g. \"api\" in tags",
    ),
    (
        "status",
        "write the selection as :filter:, e.g. status == \"open\"",
    ),
];

/// Parses a `.. entity-flow::` / `.. needflow::` into a
/// [`Directive::EntityFlow`].
pub(super) fn parse_entity_flow(
    source: EntityFlowSource,
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.as_str();
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let mut flow = EntityFlow {
        span: directive_span,
        ..EntityFlow::new(source)
    };

    report_argument(argument, directive, directive_span, diagnostics);
    report_content(
        &unindented_lines[body_start..],
        directive,
        directive_span,
        diagnostics,
    );

    let unrecognized = read_options(
        &mut flow,
        &option_lines,
        &unindented_lines,
        diagnostics,
        ctx,
    );
    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveEntityFlowUnknownOption,
        diagnostics,
        ctx,
    );
    if flow.has_unusable_scale() {
        report_unusable_scale(&option_lines, directive, diagnostics, ctx);
    }

    Directive::EntityFlow(Box::new(flow))
}

/// Reports an argument written on the directive's own line.
///
/// The directive takes none, and the one thing an author is likely to write
/// there is the selection — so the message names the option that would have
/// worked, exactly as `.. entity-table::`'s does.
fn report_argument(
    argument: &str,
    directive: &str,
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    if argument.trim().is_empty() {
        return;
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveEntityFlowUnknownOption,
        format!(
            "{directive}: takes no argument; write the selection as :filter: instead of '{}'",
            argument.trim()
        ),
        directive_span,
    ));
}

/// Reports body content below the option block.
///
/// A flowchart's `PlantUML` is generated, so there is nowhere for written text
/// to go. Saying so is worth a line: the directive sits one letter away from
/// `.. entity-diagram::`, whose body *is* the picture.
fn report_content(
    content: &[String],
    directive: &str,
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    if content.iter().all(|line| line.trim().is_empty()) {
        return;
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveEntityFlowUnknownOption,
        format!(
            "{directive}: takes no content, and its picture is generated from the entity graph; \
             write `.. entity-diagram::` to draw a diagram of your own"
        ),
        directive_span,
    ));
}

/// Reads every option onto `flow`, returning the lines nobody claimed.
fn read_options<'a>(
    flow: &mut EntityFlow,
    option_lines: &'a [OptionLine],
    unindented_lines: &[String],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let directive = flow.source.as_str();
    let mut unrecognized = Vec::new();
    for line in option_lines {
        match line.name.as_str() {
            "filter" => {
                let source_line = unindented_lines.get(line.line_index).map(String::as_str);
                flow.filter = read_filter(line, source_line, directive, diagnostics, ctx);
            }
            "relations" | "link_types" | "link-types" => {
                flow.relations = read_relations(line, directive, diagnostics, ctx);
            }
            "show-link-names" | "show_link_names" => flow.show_link_names = true,
            "direction" => {
                if let Some(direction) = read_direction(line, directive, diagnostics, ctx) {
                    flow.direction = direction;
                }
            }
            "config" => {
                if line.value.is_empty() {
                    report_empty_value(line, directive, diagnostics, ctx);
                } else {
                    flow.config = Some(line.value.clone());
                }
            }
            "debug" => flow.debug = true,
            "caption" => flow.caption = Some(line.value.clone()),
            "align" => match ImageAlign::parse(&line.value) {
                Some(align) => flow.align = Some(align),
                None => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::EntityFlowInvalidAlign,
                    format!(
                        "{directive}: :align: expects one of center, left, right, found '{}'",
                        line.value
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "scale" => match parse_percentage(&line.value) {
                Some(scale) => flow.scale = Some(scale),
                None => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::EntityFlowInvalidScale,
                    format!(
                        "{directive}: :scale: expects a non-negative percentage, found '{}'",
                        line.value
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "width" => match LengthOrPercentage::new(&line.value) {
                Ok(width) => flow.width = Some(width),
                Err(problem) => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::EntityFlowInvalidWidth,
                    format!("{directive}: :width: {problem}"),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "class" => flow.classes = line.value.split_whitespace().map(str::to_string).collect(),
            "name" => {
                if line.value.is_empty() {
                    report_empty_value(line, directive, diagnostics, ctx);
                } else {
                    flow.name = Some(TargetName::new(&line.value));
                }
            }
            name => match unsupported_advice(name) {
                Some(advice) => report_unsupported(line, advice, directive, diagnostics, ctx),
                None => unrecognized.push(line),
            },
        }
    }
    unrecognized
}

/// What to tell an author who wrote an option this build does not implement,
/// or `None` when the name is not one of sphinx-needs' at all.
///
/// `types` is handled apart from the table because it is the one legacy filter
/// whose replacement is worth spelling exactly: the field is `type`, singular.
fn unsupported_advice(name: &str) -> Option<&'static str> {
    if name == "types" {
        return Some("write the selection as :filter:, e.g. type == \"req\"");
    }
    UNSUPPORTED_OPTIONS
        .iter()
        .find(|(option, _)| *option == name)
        .map(|(_, advice)| *advice)
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

/// Reads `:direction:`, keeping the default when it names no layout.
fn read_direction(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<FlowDirection> {
    match FlowDirection::new(&line.value) {
        Ok(direction) => Some(direction),
        Err(problem) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityFlowInvalidDirection,
                format!("{directive}: :direction: {problem}; {DIRECTION_NOTE}"),
                ctx.line_span(line.line_index, &line.raw),
            ));
            None
        }
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

/// Reports an option this build does not implement, and what to write instead.
fn report_unsupported(
    line: &OptionLine,
    advice: &str,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityFlowUnsupportedOption,
        format!(
            "{directive}: :{}: is not supported, so it was ignored — {advice}",
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an option whose value is required but was left empty.
fn report_empty_value(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityFlowEmptyOptionValue,
        format!(
            "{directive}: :{}: needs a value, so the option was ignored",
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

#[cfg(test)]
mod tests;

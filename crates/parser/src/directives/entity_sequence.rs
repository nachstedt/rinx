//! `.. entity-sequence::`, and its sphinx-needs spelling `.. needsequence::` —
//! a sequence diagram walked from start entities along the relations that
//! carry messages.
//!
//! Everything that can be checked without the project index is checked here,
//! while the option line's own position is still in hand: the start ids'
//! shape, every relation name against the schema, and the receiver filter.
//! What is left for the renderer is the walk itself.
//!
//! Two options are mandatory. `:start:` is in sphinx-needs too; `:relations:`
//! (or its `:link_types:` spelling) is not — sphinx-needs defaults it to
//! `links` — but neither that name nor "every relation this schema declares"
//! says which edges are messages, so a diagram missing either degrades to an
//! error block rather than reaching the renderer with a guess.
//!
//! The options sphinx-needs has and this build does not draw are refused **by
//! name**, and an unreadable option is dropped and reported while the diagram
//! itself survives — the error-resilience every directive here follows.

use std::num::NonZeroU32;

use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityId, EntitySequence, EntitySequenceSource,
    ImageAlign, LengthOrPercentage, NonEmptyVector, Span, TargetName,
};
use rinx_filter::Expr;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;

use super::error_node::malformed_node;
use super::filter_option::{FilterCodes, read_filter_option};
use super::options::{OptionLine, parse_percentage, report_unknown_options, scan_option_lines};

/// The options sphinx-needs' `needsequence` accepts that this build does not,
/// each with what an author should reach for instead. Both spellings of every
/// multiword name are listed, as `.. entity-flow::`'s table does.
const UNSUPPORTED_OPTIONS: [(&str, &str); 20] = [
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
    (
        "show_link_names",
        "every message is labelled with its own title",
    ),
    (
        "show-link-names",
        "every message is labelled with its own title",
    ),
    ("highlight", "every participant is drawn the same way"),
    ("filter-func", "write the receiver selection as :filter:"),
    ("filter_func", "write the receiver selection as :filter:"),
    (
        "sort_by",
        "messages are drawn in the order the walk reaches them",
    ),
    (
        "sort-by",
        "messages are drawn in the order the walk reaches them",
    ),
    ("export_id", "this build does not export filter results"),
    ("export-id", "this build does not export filter results"),
    (
        "filter_warning",
        "an empty walk is reported as entity-sequence.empty-result",
    ),
    (
        "filter-warning",
        "an empty walk is reported as entity-sequence.empty-result",
    ),
    ("height", "set :width:; the height follows from it"),
    (
        "tags",
        "write the receiver selection as :filter:, e.g. \"api\" in tags",
    ),
    (
        "status",
        "write the receiver selection as :filter:, e.g. status == \"open\"",
    ),
    (
        "types",
        "write the receiver selection as :filter:, e.g. type == \"component\"",
    ),
    ("engine", "every diagram here is compiled by PlantUML"),
];

/// The codes a sequence diagram reports its filter's failures under.
const FILTER_CODES: FilterCodes = FilterCodes {
    invalid: DiagnosticCode::EntitySequenceInvalidFilter,
    unknown_field: DiagnosticCode::EntitySequenceUnknownField,
};

/// What was read from the option block, before the two mandatory options are
/// known to be present.
#[derive(Default)]
struct ReadOptions {
    /// `:start:` as read, `None` when the option was not written at all.
    start: Option<Vec<EntityId>>,
    /// `:relations:` as read, `None` when the option was not written at all.
    relations: Option<Vec<String>>,
    filter: Option<Expr>,
    max_items: Option<NonZeroU32>,
    config: Option<String>,
    debug: bool,
    caption: Option<String>,
    align: Option<ImageAlign>,
    scale: Option<u32>,
    width: Option<LengthOrPercentage>,
    classes: Vec<String>,
    name: Option<TargetName>,
}

/// Parses a `.. entity-sequence::` / `.. needsequence::` into a
/// [`Directive::EntitySequence`], or into a [`Directive::Malformed`] when
/// either mandatory option is missing.
pub(super) fn parse_entity_sequence(
    source: EntitySequenceSource,
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.as_str();
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    report_content(
        &unindented_lines[body_start..],
        directive,
        directive_span,
        diagnostics,
    );

    let mut read = ReadOptions {
        caption: Some(argument.trim())
            .filter(|caption| !caption.is_empty())
            .map(str::to_string),
        ..ReadOptions::default()
    };
    let unrecognized = read_options(
        &mut read,
        directive,
        &option_lines,
        &unindented_lines,
        diagnostics,
        ctx,
    );
    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveEntitySequenceUnknownOption,
        diagnostics,
        ctx,
    );

    let walk = mandatory_walk(&read, directive, directive_span, diagnostics, ctx);
    let Some((start, relations)) = walk else {
        return malformed_node(
            directive,
            argument,
            body_lines,
            format!(
                "{directive}: needs both :start: and :relations: to know where to walk, so \
                 nothing was drawn"
            ),
        );
    };

    let sequence = EntitySequence {
        filter: read.filter,
        max_items: read.max_items,
        config: read.config,
        debug: read.debug,
        caption: read.caption,
        align: read.align,
        scale: read.scale,
        width: read.width,
        classes: read.classes,
        name: read.name,
        span: directive_span,
        ..EntitySequence::new(source, start, relations)
    };
    if sequence.has_unusable_scale() {
        report_unusable_scale(&option_lines, directive, diagnostics, ctx);
    }
    Directive::EntitySequence(Box::new(sequence))
}

/// The walk's two mandatory halves, reporting whichever is absent.
///
/// An option that was written but held nothing usable — every start id
/// malformed, every relation unknown — was already reported entry by entry,
/// so only an option that was not written *at all* (or written empty) is
/// reported here; a second diagnostic about the same line would say nothing
/// new.
fn mandatory_walk(
    read: &ReadOptions,
    directive: &str,
    directive_span: Option<Span>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(NonEmptyVector<EntityId>, NonEmptyVector<String>)> {
    if read.start.is_none() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntitySequenceMissingStart,
            format!("{directive}: needs :start:, the entity or entities the walk begins at"),
            directive_span,
        ));
    }
    if read.relations.is_none() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntitySequenceMissingRelations,
            format!(
                "{directive}: needs :relations:, the relations that lead from a sender to a \
                 message and on to its receivers; the schema declares {}",
                declared_relations(ctx)
            ),
            directive_span,
        ));
    }
    let start = NonEmptyVector::try_from(read.start.clone()?).ok()?;
    let relations = NonEmptyVector::try_from(read.relations.clone()?).ok()?;
    Some((start, relations))
}

/// Reports body content below the option block: the diagram is generated, so
/// there is nowhere for written text to go.
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
        DiagnosticCode::DirectiveEntitySequenceUnknownOption,
        format!(
            "{directive}: takes no content, and its picture is generated from the entity graph; \
             write `.. entity-diagram::` to draw a diagram of your own"
        ),
        directive_span,
    ));
}

/// Reads every option onto `read`, returning the lines nobody claimed.
fn read_options<'a>(
    read: &mut ReadOptions,
    directive: &str,
    option_lines: &'a [OptionLine],
    unindented_lines: &[String],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let mut unrecognized = Vec::new();
    for line in option_lines {
        match line.name.as_str() {
            "start" => read.start = read_start(line, directive, diagnostics, ctx),
            "relations" | "link_types" | "link-types" => {
                read.relations = read_relations(line, directive, diagnostics, ctx);
            }
            "filter" => {
                let source_line = unindented_lines.get(line.line_index).map(String::as_str);
                read.filter = read_filter_option(
                    line,
                    source_line,
                    directive,
                    FILTER_CODES,
                    diagnostics,
                    ctx,
                );
            }
            "max-items" | "max_items" => {
                read.max_items = read_max_items(line, directive, diagnostics, ctx);
            }
            "config" => read.config = non_empty_value(line, directive, diagnostics, ctx),
            "debug" => read.debug = true,
            "caption" => read.caption = Some(line.value.clone()),
            "class" => read.classes = line.value.split_whitespace().map(str::to_string).collect(),
            "name" => {
                read.name = non_empty_value(line, directive, diagnostics, ctx)
                    .map(|name| TargetName::new(&name));
            }
            "align" | "scale" | "width" => read_placement(read, line, directive, diagnostics, ctx),
            name => match unsupported_advice(name) {
                Some(advice) => report_unsupported(line, advice, directive, diagnostics, ctx),
                None => unrecognized.push(line),
            },
        }
    }
    unrecognized
}

/// Reads `:align:`, `:scale:` or `:width:`, in the image directives'
/// vocabulary, reporting a value none of them accepts.
fn read_placement(
    read: &mut ReadOptions,
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let problem = match line.name.as_str() {
        "align" => match ImageAlign::parse(&line.value) {
            Some(align) => {
                read.align = Some(align);
                return;
            }
            None => (
                DiagnosticCode::EntitySequenceInvalidAlign,
                format!(
                    ":align: expects one of center, left, right, found '{}'",
                    line.value
                ),
            ),
        },
        "scale" => match parse_percentage(&line.value) {
            Some(scale) => {
                read.scale = Some(scale);
                return;
            }
            None => (
                DiagnosticCode::EntitySequenceInvalidScale,
                format!(
                    ":scale: expects a non-negative percentage, found '{}'",
                    line.value
                ),
            ),
        },
        _ => match LengthOrPercentage::new(&line.value) {
            Ok(width) => {
                read.width = Some(width);
                return;
            }
            Err(problem) => (
                DiagnosticCode::EntitySequenceInvalidWidth,
                format!(":width: {problem}"),
            ),
        },
    };
    let (code, message) = problem;
    diagnostics.push(Diagnostic::at(
        code,
        format!("{directive}: {message}"),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reads `:start:` — ids separated by `,` or `;`, as sphinx-needs splits them.
///
/// A malformed id is dropped and reported while the rest are kept, since one
/// typo should not cost the whole walk. Whether an id names an entity is only
/// known to the project index, so that is left to the renderer.
fn read_start(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Vec<EntityId>> {
    let written: Vec<&str> = line
        .value
        .split([',', ';'])
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();
    if written.is_empty() {
        // Left for `mandatory_walk`, which reports an absent start: an empty
        // `:start:` is exactly as unwalkable as a missing one.
        return None;
    }
    let mut ids = Vec::new();
    for id in written {
        match EntityId::new(id) {
            Ok(id) => ids.push(id),
            Err(problem) => diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntitySequenceInvalidStart,
                format!("{directive}: :start: '{id}' is not an entity id: {problem}"),
                ctx.line_span(line.line_index, &line.raw),
            )),
        }
    }
    Some(ids)
}

/// Reads `:relations:` — which relations carry messages.
///
/// An entry no type declares is dropped and reported while the rest are kept,
/// as `.. entity-flow::` does. An empty value is left for `mandatory_walk`.
fn read_relations(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Vec<String>> {
    let written: Vec<&str> = line
        .value
        .split([',', ';'])
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    if written.is_empty() {
        return None;
    }
    let mut relations = Vec::new();
    for name in written {
        if ctx.schema.relation(name).is_some() {
            relations.push(name.to_string());
        } else {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntitySequenceUnknownRelation,
                format!(
                    "{directive}: :{}: unknown relation '{name}'; the schema declares {}",
                    line.name,
                    declared_relations(ctx)
                ),
                ctx.line_span(line.line_index, &line.raw),
            ));
        }
    }
    Some(relations)
}

/// The relations a schema declares, for a diagnostic naming the valid choices.
fn declared_relations(ctx: &ParseCtx<'_>) -> String {
    let names = ctx.schema.relation_names();
    if names.is_empty() {
        return "none".to_string();
    }
    names.join(", ")
}

/// Reads `:max-items:` — a non-negative whole number, where `0` means no
/// limit, as sphinx-needs documents it.
fn read_max_items(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<NonZeroU32> {
    if let Ok(limit) = line.value.trim().parse::<u32>() {
        return NonZeroU32::new(limit);
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntitySequenceInvalidMaxItems,
        format!(
            "{directive}: :{}: expects a non-negative whole number (0 for no limit), found '{}'",
            line.name, line.value
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
    None
}

/// An option's value, or `None` after reporting that it was left empty.
fn non_empty_value(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<String> {
    if !line.value.is_empty() {
        return Some(line.value.clone());
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntitySequenceEmptyOptionValue,
        format!(
            "{directive}: :{}: needs a value, so the option was ignored",
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
    None
}

/// What to tell an author who wrote an option this build does not implement,
/// or `None` when the name is not one of sphinx-needs' at all.
fn unsupported_advice(name: &str) -> Option<&'static str> {
    UNSUPPORTED_OPTIONS
        .iter()
        .find(|(option, _)| *option == name)
        .map(|(_, advice)| *advice)
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
        DiagnosticCode::EntitySequenceUnsupportedOption,
        format!(
            "{directive}: :{}: is not supported, so it was ignored — {advice}",
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports a `:scale:` that has no `:width:` to apply to, against the
/// `:scale:` line the author would have to change.
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
        DiagnosticCode::EntitySequenceUnusableScale,
        format!(
            "{directive}: :scale: has no :width: to apply to, so it was ignored — the compiled \
             diagram is never opened while rendering, so it has no size of its own to scale"
        ),
        span,
    ));
}

#[cfg(test)]
mod tests;

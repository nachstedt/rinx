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

use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::spec::{Arguments, DirectiveSpec};
use std::num::NonZeroU32;

use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityId, EntitySequence, EntitySequenceSource,
    ImageAlign, LengthOrPercentage, Named, NonEmptyVector, Span, TargetName,
};
use rinx_filter::Expr;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::error_node::malformed_node;
use super::filter_option::{FilterCodes, ShorthandFilterOption, read_filter_option};
use super::option_value::{Classes, NonEmptyText, OptionValueType, Percentage};
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};
use super::spec::OptionValue;

directive_options! {
    /// Where the picture sits, in the image directives' vocabulary.
    enum PlacementOption {
        /// `:align:`.
        Align(ImageAlign) => (["align"], EntitySequenceInvalidAlign, "Where the diagram sits."),
        /// `:scale:`.
        Scale(Percentage) => (["scale"], EntitySequenceInvalidScale, "The diagram's size, as a percentage of its `:width:`."),
        /// `:width:`.
        Width(LengthOrPercentage) => (["width"], EntitySequenceInvalidWidth, "The diagram's width."),
    }
}

directive_options! {
    /// The options `.. entity-sequence::` takes: sphinx-needs'
    /// `needsequence`'s twenty-one, of which this build reads eight, and its
    /// own `:relations:`, `:caption:`, `:class:` and `:name:`. The ones it does
    /// not draw are refused by name, as `.. entity-flow::`'s are.
    enum EntitySequenceOption {
        /// `:start:`.
        Start => (["start"], Text, "The entities the walk begins at, comma-separated."),
        /// `:relations:`.
        Relations => (["relations", "link_types", "link-types"], Text, "The relations leading from a sender to a message and on to its receivers."),
        /// `:filter:`.
        Filter => (["filter"], Filter, "The receivers drawn, by a filter."),
        /// `:max-items:`.
        MaxItems(ItemLimit) => (["max-items", "max_items"], EntitySequenceInvalidMaxItems, "How many messages are drawn at most."),
        /// `:config:`.
        Config(NonEmptyText) => (["config"], "A PlantUML configuration the diagram is drawn with."),
        /// `:debug:`.
        Debug => (["debug"], Flag, "Shows the generated PlantUML source below the diagram."),
        /// `:caption:`.
        Caption(String) => (["caption"], "A caption shown below the diagram."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the diagram."),
        /// `:name:`.
        Name(Named<TargetName>) => (["name"], "A label to reference the diagram by."),
        _ => (["show_filters", "show-filters"], Flag, "Shows the filter below the diagram.", "the filter is visible in the document source"),
        _ => (["show_legend", "show-legend"], Flag, "Draws a legend of the entity types.", "no legend is drawn"),
        _ => (["show_link_names", "show-link-names"], Flag, "Labels each message with its relation.", "every message is labelled with its own title"),
        _ => (["highlight"], Filter, "The participants drawn highlighted, by a filter.", "every participant is drawn the same way"),
        _ => (["filter-func", "filter_func"], Text, "The receivers drawn, by a Python function.", "write the receiver selection as :filter:"),
        _ => (["sort_by", "sort-by"], Text, "The field the messages are ordered by.", "messages are drawn in the order the walk reaches them"),
        _ => (["export_id", "export-id"], Text, "The id the filter is exported under.", "this build does not export filter results"),
        _ => (["filter_warning", "filter-warning"], Text, "The text shown when nothing is drawn.", "an empty walk is reported as entity-sequence.empty-result"),
        _ => (["height"], Length, "The diagram's height.", "set :width:; the height follows from it"),
        _ => (["engine"], Text, "The program the diagram is laid out by.", "every diagram here is compiled by PlantUML");
        Placement(PlacementOption),
        Shorthand(ShorthandFilterOption),
    }
}

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
    name: Option<Named<TargetName>>,
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["entity-sequence", "needsequence"],
    doc: "A sequence diagram walked along the entities' relations.",
    arguments: Arguments::OPTIONAL_TEXT,
    takes_content: false,
    content_hint: Some(
        "its picture is generated from the entity graph; write `.. entity-diagram::` to draw a diagram of your own",
    ),
    options: <EntitySequenceOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. entity-sequence::` or `.. needsequence::`, recording which
/// was written.
pub(in crate::directives) fn interpret_entity_sequence(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(source) = input.name.parse::<rinx_ast::EntitySequenceSource>() else {
        return unknown(input, state);
    };
    single(parse_entity_sequence(source, input, state.diagnostics))
}

/// Parses a `.. entity-sequence::` / `.. needsequence::` into a
/// [`Directive::EntitySequence`], or into a [`Directive::Malformed`] when
/// either mandatory option is missing.
pub(super) fn parse_entity_sequence(
    source: EntitySequenceSource,
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let directive = source.as_str();
    let argument = invocation.argument(0);
    let directive_span = invocation.span;
    let body_lines = invocation.source_body;
    let ctx = &invocation.ctx;
    let unindented_lines = &invocation.lines;
    let option_lines = &invocation.options;

    let mut read = ReadOptions {
        caption: Some(argument.trim())
            .filter(|caption| !caption.is_empty())
            .map(str::to_string),
        ..ReadOptions::default()
    };
    read_options(
        &mut read,
        directive,
        option_lines,
        unindented_lines,
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
        report_unusable_scale(option_lines, directive, diagnostics, ctx);
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

/// Reads every option onto `read`, reporting the ones it does not take.
fn read_options(
    read: &mut ReadOptions,
    directive: &str,
    option_lines: &[OptionLine],
    unindented_lines: &[String],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for (option, line) in
        classify_options::<EntitySequenceOption>(option_lines, directive, diagnostics, ctx)
    {
        match option {
            EntitySequenceOption::Start => {
                read.start = read_start(line, directive, diagnostics, ctx);
            }
            EntitySequenceOption::Relations => {
                read.relations = read_relations(line, directive, diagnostics, ctx);
            }
            EntitySequenceOption::Filter => {
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
            EntitySequenceOption::MaxItems(limit) => read.max_items = limit.0,
            EntitySequenceOption::Config(config) => read.config = Some(config.0),
            EntitySequenceOption::Debug => read.debug = true,
            EntitySequenceOption::Caption(caption) => read.caption = Some(caption),
            EntitySequenceOption::Class(classes) => read.classes = classes.0,
            EntitySequenceOption::Name(name) => read.name = Some(name),
            EntitySequenceOption::Placement(PlacementOption::Align(align)) => {
                read.align = Some(align);
            }
            EntitySequenceOption::Placement(PlacementOption::Scale(scale)) => {
                read.scale = Some(scale.0);
            }
            EntitySequenceOption::Placement(PlacementOption::Width(width)) => {
                read.width = Some(width);
            }
            EntitySequenceOption::Shorthand(option) => match option {},
        }
    }
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

/// A `:max-items:`: how many messages are drawn at most, zero for no limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ItemLimit(Option<NonZeroU32>);

impl OptionValueType for ItemLimit {
    const VALUE: OptionValue<'static> = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        line.value
            .trim()
            .parse::<u32>()
            .map(|limit| Self(NonZeroU32::new(limit)))
            .map_err(|_| {
                format!(
                    "expects a non-negative whole number (0 for no limit), found '{}'",
                    line.value
                )
            })
    }
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

//! `.. entity-update::`, and its sphinx-needs spelling `.. needextend::` — a
//! project-wide mutation of one or many entities' fields.
//!
//! What is parsed here is deliberately incomplete: the target's argument may
//! resolve to a single entity or to a filter over the whole project, but
//! which reading applies can only be decided once every document is merged
//! (see [`super::filter_option::read_update_argument`]), and a field
//! mutation's value can only be converted once the matched entity's declared
//! type is known. Both are therefore kept here as *questions* — an
//! [`rinx_ast::UpdateTarget`] and a list of
//! [`rinx_ast::FieldMutation`]s carrying raw text — for
//! `rinx_analyzer::apply_entity_updates` to answer. What *is* checked
//! here is everything a single document's schema already settles: whether a
//! field name is declared at all, and whether it is one of the identity
//! fields or a section this directive refuses to touch.
//!
//! The body is ordinary RST prose — the justification for the change — parsed
//! exactly as `.. dropdown::`'s is.

use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::spec::{Arguments, DirectiveSpec};
use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityUpdate, EntityUpdateSource, FieldMutation,
    FieldMutationMode,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;

use super::error_node::malformed_node;
use super::filter_option::read_update_argument;
use super::option_value::Boolean;
use super::options::{DirectiveOptions, OptionLine, OptionRead, directive_options};

/// Identity fields this directive refuses to mutate — exactly
/// `rinx_entity::field::BUILTIN_FIELDS`, spelled out rather than
/// imported: that constant lists every field *every* entity has for the
/// filter language's purposes, and the point here is narrower and worth
/// naming on its own terms. `title` is protected alongside the other four
/// because `EntityRecord` keeps a denormalized `title` field separate from
/// `attributes["title"]`; allowing `Set` on the attribute alone would desync
/// the two. See `docs/decisions/019-entity-update.md`.
pub(in crate::directives) const PROTECTED_FIELDS: [&str; 5] =
    ["id", "type", "type_name", "docname", "title"];

directive_options! {
    /// The option `.. entity-update::` lists: `:strict:`. Every other line is
    /// a field mutation — `:field:`, `:+field:` or `:-field:` for each field
    /// the schema declares — which its spec says by `accepts_further`, and
    /// [`read_field_mutation`] checks against the schema.
    enum UpdateOption {
        /// `:strict:`.
        Strict(Boolean) => (["strict"], EntityUpdateInvalidStrict, "Whether matching no entity is reported; `true` unless set to `false`."),
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["entity-update", "needextend"],
    doc: "Changes fields of the entities a filter selects, project-wide.",
    arguments: Arguments::ONE_TEXT,
    takes_content: true,
    content_hint: None,
    options: <UpdateOption as DirectiveOptions>::OPTIONS,
    accepts_further: true,
};

/// Interprets `.. entity-update::` or `.. needextend::`, recording which was
/// written.
pub(in crate::directives) fn interpret_entity_update(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(source) = input.name.parse::<rinx_ast::EntityUpdateSource>() else {
        return unknown(input, state);
    };
    single(parse_entity_update(
        source,
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses a `.. entity-update::` / `.. needextend::` into a
/// [`Directive::EntityUpdate`].
pub(super) fn parse_entity_update(
    source: EntityUpdateSource,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let directive = source.as_str();
    let argument = invocation.argument(0);
    let directive_span = invocation.span;
    let body_lines = invocation.source_body;
    let ctx = &invocation.ctx;
    let option_lines = &invocation.options;
    let Some(target) = read_update_argument(argument, directive_span, directive, diagnostics)
    else {
        return malformed_node(
            directive,
            argument,
            body_lines,
            format!("{directive}: needs a target — an entity id or a filter"),
        );
    };

    let mut update = EntityUpdate::new(source, target);
    for line in option_lines {
        match UpdateOption::read(line, directive, diagnostics, ctx) {
            OptionRead::Read(UpdateOption::Strict(strict)) => update.strict = strict.0,
            // A value that is not a boolean was reported, and leaves the safer
            // default: a typo still gets the empty-result check.
            OptionRead::Refused => {}
            OptionRead::NotTaken => {
                if let Some(mutation) = read_field_mutation(line, directive, diagnostics, ctx) {
                    update.fields.push(mutation);
                }
            }
        }
    }
    update.span = directive_span;

    update.body = invocation.content().parse(adornment_order, diagnostics);

    Directive::EntityUpdate(Box::new(update))
}

/// Splits `:+field:`/`:-field:`/`:field:` into its bare name and mode,
/// validates the name against the schema's *full* vocabulary — every type's
/// fields, unioned, since the target may match several types at once — and
/// stores the value unconverted: the matched entity's declared type, and so
/// the [`rinx_entity::AttributeType`] the text must fit, is only known at apply time.
fn read_field_mutation(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<FieldMutation> {
    let span = ctx.line_span(line.line_index, &line.raw);
    let (field, mode) = split_mode(&line.name, &line.value);

    if PROTECTED_FIELDS.contains(&field.as_str()) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityUpdateProtectedField,
            format!(
                "{directive}: ':{field}:' is an identity field and cannot be changed by {directive}"
            ),
            span,
        ));
        return None;
    }
    if is_declared_section(&field, ctx) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityUpdateSectionNotSupported,
            format!(
                "{directive}: ':{field}:' is a section, not an attribute or relation; sections are \
                 documents, deliberately kept out of the project index this directive's effects \
                 live in, so they cannot be changed this way"
            ),
            span,
        ));
        return None;
    }
    if !ctx.schema.declares_field(&field) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityUpdateUnknownField,
            format!(
                "{directive}: unknown field '{field}'; the schema declares {}",
                ctx.schema.field_names().join(", ")
            ),
            span,
        ));
        return None;
    }

    Some(FieldMutation { field, mode, span })
}

/// Reads the mode a `+`/`-` prefix (or its absence) spells, stripping it from
/// the field name.
///
/// `-field:` with an empty value is [`FieldMutationMode::Clear`], not
/// `Remove("")`: removing the empty string from a list is not what an author
/// asking to clear a field means.
fn split_mode(name: &str, value: &str) -> (String, FieldMutationMode) {
    if let Some(field) = name.strip_prefix('+') {
        return (
            field.to_string(),
            FieldMutationMode::Append(value.to_string()),
        );
    }
    if let Some(field) = name.strip_prefix('-') {
        return if value.is_empty() {
            (field.to_string(), FieldMutationMode::Clear)
        } else {
            (
                field.to_string(),
                FieldMutationMode::Remove(value.to_string()),
            )
        };
    }
    (name.to_string(), FieldMutationMode::Set(value.to_string()))
}

/// Whether `field` names a section any declared type carries — checked
/// separately from [`rinx_entity::EntitySchema::declares_field`],
/// which deliberately excludes sections (they are not index data at all), so
/// a section name would otherwise fall through to the generic
/// unknown-field diagnostic rather than the one explaining why.
fn is_declared_section(field: &str, ctx: &ParseCtx<'_>) -> bool {
    ctx.schema
        .types()
        .iter()
        .any(|entity_type| entity_type.section(field).is_some())
}

#[cfg(test)]
mod tests;

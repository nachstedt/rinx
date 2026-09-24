//! `.. entity-update::`, and its sphinx-needs spelling `.. needextend::` — a
//! project-wide mutation of one or many entities' fields.
//!
//! What is parsed here is deliberately incomplete: the target's argument may
//! resolve to a single entity or to a filter over the whole project, but
//! which reading applies can only be decided once every document is merged
//! (see [`super::filter_option::read_update_argument`]), and a field
//! mutation's value can only be converted once the matched entity's declared
//! type is known. Both are therefore kept here as *questions* — an
//! [`rusty_sphinx_ast::UpdateTarget`] and a list of
//! [`rusty_sphinx_ast::FieldMutation`]s carrying raw text — for
//! `rusty_sphinx_analyzer::apply_entity_updates` to answer. What *is* checked
//! here is everything a single document's schema already settles: whether a
//! field name is declared at all, and whether it is one of the identity
//! fields or a section this directive refuses to touch.
//!
//! The body is ordinary RST prose — the justification for the change — parsed
//! exactly as `.. dropdown::`'s is.

use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityUpdate, EntityUpdateSource, FieldMutation,
    FieldMutationMode, Span,
};
use rusty_sphinx_entity::{AttributeType, parse_attribute_value};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

use super::error_node::malformed_node;
use super::filter_option::read_update_argument;
use super::options::{OptionLine, scan_option_lines};

/// Identity fields this directive refuses to mutate — exactly
/// `rusty_sphinx_entity::field::BUILTIN_FIELDS`, spelled out rather than
/// imported: that constant lists every field *every* entity has for the
/// filter language's purposes, and the point here is narrower and worth
/// naming on its own terms. `title` is protected alongside the other four
/// because `EntityRecord` keeps a denormalized `title` field separate from
/// `attributes["title"]`; allowing `Set` on the attribute alone would desync
/// the two. See `docs/decisions/019-entity-update.md`.
const PROTECTED_FIELDS: [&str; 5] = ["id", "type", "type_name", "docname", "title"];

/// Parses a `.. entity-update::` / `.. needextend::` into a
/// [`Directive::EntityUpdate`].
pub(super) fn parse_entity_update(
    source: EntityUpdateSource,
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.as_str();
    let Some(target) = read_update_argument(argument, directive_span, directive, diagnostics)
    else {
        return malformed_node(
            directive,
            argument,
            body_lines,
            format!("{directive}: needs a target — an entity id or a filter"),
        );
    };

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let mut update = EntityUpdate::new(source, target);
    for line in &option_lines {
        if line.name == "strict" {
            update.strict = read_strict(line, directive, diagnostics, ctx);
            continue;
        }
        if let Some(mutation) = read_field_mutation(line, directive, diagnostics, ctx) {
            update.fields.push(mutation);
        }
    }
    update.span = directive_span;

    // The body starts below the option block, so every position inside it is
    // short by that many lines unless the context is rebased first — the same
    // reason `.. dropdown::`'s body is parsed this way.
    let body_ctx = ctx.nested(body_start, 0);
    let content: Vec<&str> = unindented_lines[body_start..]
        .iter()
        .map(String::as_str)
        .collect();
    update.body = parse_blocks(&content, adornment_order, diagnostics, &body_ctx);

    Directive::EntityUpdate(Box::new(update))
}

/// Splits `:+field:`/`:-field:`/`:field:` into its bare name and mode,
/// validates the name against the schema's *full* vocabulary — every type's
/// fields, unioned, since the target may match several types at once — and
/// stores the value unconverted: the matched entity's declared type, and so
/// the [`AttributeType`] the text must fit, is only known at apply time.
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
/// separately from [`rusty_sphinx_entity::EntitySchema::declares_field`],
/// which deliberately excludes sections (they are not index data at all), so
/// a section name would otherwise fall through to the generic
/// unknown-field diagnostic rather than the one explaining why.
fn is_declared_section(field: &str, ctx: &ParseCtx<'_>) -> bool {
    ctx.schema
        .types()
        .iter()
        .any(|entity_type| entity_type.section(field).is_some())
}

/// Reads `:strict:` as a boolean flag, defaulting to `true` on a bad value —
/// the safer of the two, so a typo here still gets the empty-result check
/// rather than silently suppressing it.
fn read_strict(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> bool {
    if let Ok(rusty_sphinx_ast::AttributeValue::Bool(value)) =
        parse_attribute_value(&AttributeType::Bool, &line.value)
    {
        return value;
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::EntityUpdateInvalidStrict,
        format!(
            "{directive}: :strict: expects true or false, found '{}'",
            line.value
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
    true
}

#[cfg(test)]
mod tests;

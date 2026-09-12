//! Parsing an instance of a project-declared entity type.
//!
//! Everything the schema can decide is decided here, while the source position
//! of each option is still in hand: unknown options, values outside their
//! declared type, missing required attributes and relations, sections that the
//! type does not declare or that appear too often. What survives into the AST
//! has already been validated, so no later phase re-checks the schema.
//!
//! The body is parsed by the ordinary block parser under a context carrying
//! the enclosing type, and only *then* partitioned into sections. Doing it
//! that way buys indentation handling, nesting, span rebasing and error
//! resilience for free, instead of a bespoke splitter that would have to
//! reimplement all four.

use std::collections::BTreeMap;

use rusty_sphinx_ast::{
    AttributeValue, Diagnostic, DiagnosticCode, Directive, EntityBody, EntityId, EntitySection,
    Node, Span,
};
use rusty_sphinx_entity::{EntityType, ID_OPTION, IdContext, parse_attribute_value, split_list};

use super::options::{OptionLine, scan_option_lines};
use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

/// Parses `.. <type>:: <argument>` into an [`EntityBody`].
///
/// `discriminator` distinguishes this entity from others of its type in the
/// same document; the caller passes the source line, which makes a generated
/// id unique within the document without depending on the entity's content.
pub(in crate::directives) struct EntityDirective<'a> {
    /// The type the directive's name resolved to.
    pub entity_type: &'a EntityType,
    /// The text after the `::`.
    pub argument: &'a str,
    /// Where the directive marker was written.
    pub span: Option<Span>,
    /// The directive's body, indentation intact.
    pub body_lines: &'a [&'a str],
    /// What distinguishes this entity from others of its type in the document.
    pub discriminator: u32,
    /// The document being parsed, for a generated id.
    pub doc_path: &'a str,
}

pub(in crate::directives) fn parse_entity(
    directive: &EntityDirective<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let &EntityDirective {
        entity_type,
        argument,
        span: directive_span,
        body_lines,
        discriminator,
        doc_path,
    } = directive;
    // The body arrives with its directive indent intact; every option scan and
    // nested parse below works on the unindented form, exactly as the
    // admonition and table parsers do. The context was already rebased by the
    // same amount by the caller, so positions still land on the right column.
    let unindented = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented);

    let mut attributes = BTreeMap::new();
    collect_argument_attributes(
        entity_type,
        argument,
        directive_span,
        &mut attributes,
        diagnostics,
    );
    let relations = collect_options(
        entity_type,
        &option_lines,
        &mut attributes,
        diagnostics,
        ctx,
    );
    apply_defaults(entity_type, &mut attributes);
    report_missing_attributes(entity_type, &attributes, directive_span, diagnostics);
    report_relation_cardinality(entity_type, &relations, directive_span, diagnostics);

    let id = determine_id(
        entity_type,
        &option_lines,
        &attributes,
        discriminator,
        doc_path,
        directive_span,
        diagnostics,
    );

    let sections = parse_sections(
        &EnclosingEntity {
            entity_type,
            id: &id,
        },
        &unindented[body_start.min(unindented.len())..],
        body_start,
        directive_span,
        adornment_order,
        diagnostics,
        ctx,
    );

    Directive::Entity(Box::new(EntityBody {
        type_name: entity_type.name.clone(),
        id,
        attributes,
        relations,
        sections,
        span: directive_span,
    }))
}

/// Fills the attributes the directive's argument maps onto.
fn collect_argument_attributes(
    entity_type: &EntityType,
    argument: &str,
    span: Option<Span>,
    attributes: &mut BTreeMap<String, AttributeValue>,
    diagnostics: &mut Diagnostics,
) {
    match entity_type.argument.map_argument(argument) {
        Ok(pairs) => {
            for (field, text) in pairs {
                // A field named by `argument.fields` is a declared attribute:
                // the schema loader refused the type otherwise.
                if let Some(schema) = entity_type.attribute(&field) {
                    store_attribute(schema, &field, &text, span, attributes, diagnostics);
                }
            }
        }
        Err(error) => diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityMalformedArgument,
            format!("`.. {}::`: {error}", entity_type.name),
            span,
        )),
    }
}

/// Reads every `:option:` line, returning the relations among them.
fn collect_options(
    entity_type: &EntityType,
    option_lines: &[OptionLine],
    attributes: &mut BTreeMap<String, AttributeValue>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> BTreeMap<String, Vec<EntityId>> {
    let mut relations: BTreeMap<String, Vec<EntityId>> = BTreeMap::new();

    for line in option_lines {
        let span = ctx.line_span(line.line_index, &line.raw);
        if line.name == ID_OPTION {
            // Read separately by `determine_id`, which needs it before the
            // attributes it may be derived from are complete.
            continue;
        }
        if let Some(schema) = entity_type.attribute(&line.name) {
            store_attribute(
                schema,
                &line.name,
                &line.value,
                span,
                attributes,
                diagnostics,
            );
        } else if entity_type.relation(&line.name).is_some() {
            relations.insert(
                line.name.clone(),
                collect_relation_targets(&line.value, span, diagnostics),
            );
        } else {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityUnknownAttribute,
                format!(
                    "`.. {}::` has no option `:{}:`; it accepts {}",
                    entity_type.name,
                    line.name,
                    describe_options(entity_type)
                ),
                span,
            ));
        }
    }

    relations
}

/// Parses one attribute's text and records it, or diagnoses why it could not.
fn store_attribute(
    schema: &rusty_sphinx_entity::AttributeSchema,
    name: &str,
    text: &str,
    span: Option<Span>,
    attributes: &mut BTreeMap<String, AttributeValue>,
    diagnostics: &mut Diagnostics,
) {
    match parse_attribute_value(&schema.value_type, text) {
        Ok(value) => {
            attributes.insert(name.to_string(), value);
        }
        Err(error) => diagnostics.push(Diagnostic::at(
            DiagnosticCode::EntityInvalidAttributeValue,
            format!("`:{name}:`: {error}"),
            span,
        )),
    }
}

/// Reads a relation option's comma-separated ids, diagnosing illegal ones.
fn collect_relation_targets(
    text: &str,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> Vec<EntityId> {
    let mut targets = Vec::new();
    for written in split_list(text) {
        match EntityId::new(&written) {
            Ok(id) => targets.push(id),
            Err(error) => diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityInvalidId,
                format!("link target {written:?}: {error}"),
                span,
            )),
        }
    }
    targets
}

/// Supplies the declared default for every attribute left unset.
fn apply_defaults(entity_type: &EntityType, attributes: &mut BTreeMap<String, AttributeValue>) {
    for schema in &entity_type.attributes {
        let Some(default) = schema.default.as_deref() else {
            continue;
        };
        if attributes.contains_key(&schema.name) {
            continue;
        }
        // A default that does not fit its own type is a schema fault, not an
        // author's, and this is not the phase that reports it — a value that
        // fails to parse here is simply left unset.
        if let Ok(value) = parse_attribute_value(&schema.value_type, default) {
            attributes.insert(schema.name.clone(), value);
        }
    }
}

/// Reports every `required` attribute the entity left unset.
fn report_missing_attributes(
    entity_type: &EntityType,
    attributes: &BTreeMap<String, AttributeValue>,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    for schema in &entity_type.attributes {
        if schema.required && !attributes.contains_key(&schema.name) {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityMissingRequiredAttribute,
                format!("`.. {}::` requires `:{}:`", entity_type.name, schema.name),
                span,
            ));
        }
    }
}

/// Reports relations written too few or too many times.
fn report_relation_cardinality(
    entity_type: &EntityType,
    relations: &BTreeMap<String, Vec<EntityId>>,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    for relation in &entity_type.relations {
        let targets = relations.get(&relation.name).map_or(0, Vec::len);
        if relation.required && targets == 0 {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityMissingRequiredRelation,
                format!(
                    "`.. {}::` requires at least one target on `:{}:`",
                    entity_type.name, relation.name
                ),
                span,
            ));
        }
        if !relation.multiple && targets > 1 {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityMultipleRelationTargets,
                format!(
                    "`:{}:` takes one target, but {targets} were given",
                    relation.name
                ),
                span,
            ));
        }
    }
}

/// Determines the entity's id, degrading to a generated one on failure.
///
/// A parse that could not determine an id still yields an entity: this parser
/// is the live-preview path too, and refusing to produce a node would take the
/// whole document's remaining content with it.
fn determine_id(
    entity_type: &EntityType,
    option_lines: &[OptionLine],
    attributes: &BTreeMap<String, AttributeValue>,
    discriminator: u32,
    doc_path: &str,
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) -> EntityId {
    let explicit = option_lines
        .iter()
        .find(|line| line.name == ID_OPTION)
        .map(|line| line.value.as_str());
    let context = IdContext {
        explicit,
        attributes,
        type_name: &entity_type.name,
        doc_path,
        discriminator,
    };
    match entity_type.id.derive(&context) {
        Ok(id) => id,
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityInvalidId,
                format!("`.. {}::`: {error}", entity_type.name),
                span,
            ));
            fallback_id(&entity_type.name, doc_path, discriminator)
        }
    }
}

/// A last-resort id for an entity whose own id could not be determined.
///
/// Derived the same deterministic way a generated id is, so a document that
/// fails this way still parses to the same AST every time — which a Bazel
/// action output must.
fn fallback_id(type_name: &str, doc_path: &str, discriminator: u32) -> EntityId {
    let unusable = BTreeMap::new();
    let context = IdContext {
        explicit: None,
        attributes: &unusable,
        type_name,
        doc_path,
        discriminator,
    };
    rusty_sphinx_entity::IdSpec::default()
        .derive(&context)
        .unwrap_or_else(|_| {
            EntityId::new("entity").expect("a constant fallback id is always legal")
        })
}

/// The entity a body is being parsed inside: what it is, and which one.
///
/// One value rather than two parameters because the two are always read
/// together and answer the same question from different sides — the type says
/// which sections are legal, the id says what an `.. entity-arch::` draws —
/// and because [`ParseCtx`] binds them as a pair.
pub(super) struct EnclosingEntity<'a> {
    pub entity_type: &'a EntityType,
    pub id: &'a EntityId,
}

/// Parses the entity's body and divides it into sections.
fn parse_sections(
    entity: &EnclosingEntity<'_>,
    body_lines: &[String],
    body_start: usize,
    directive_span: Option<Span>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<EntitySection> {
    // The id is bound as well as the type, so an `.. entity-arch::` written
    // anywhere in this body — including inside one of its sections, where the
    // *type* is deliberately cleared — knows which entity it draws. Safe to do
    // here because `determine_id` runs before this, which is the whole reason
    // this call sits where it does in the pipeline.
    let body_ctx = ctx
        .nested(body_start, 0)
        .inside_entity(entity.entity_type)
        .inside_entity_id(entity.id);
    let lines: Vec<&str> = body_lines.iter().map(String::as_str).collect();
    let nodes = parse_blocks(&lines, adornment_order, diagnostics, &body_ctx);
    let sections = partition_sections(nodes);
    report_section_cardinality(entity.entity_type, &sections, directive_span, diagnostics);
    sections
}

/// Splits a parsed body into the content section and the named ones.
///
/// Nodes before the first section directive are the content; each section
/// directive starts a new one. Document order is preserved, because that is
/// the order the default rendering follows.
fn partition_sections(nodes: Vec<Node>) -> Vec<EntitySection> {
    let mut sections: Vec<EntitySection> = Vec::new();
    let mut content: Vec<Node> = Vec::new();

    for node in nodes {
        match node {
            Node::Directive(Directive::EntitySection { name, body, span }) => {
                sections.push(EntitySection::named(name, body, span));
            }
            // Body content appearing *after* a section directive joins the
            // content section rather than the section above it: a section's
            // own content is its indented body, so anything at this level
            // belongs to the entity itself.
            other => content.push(other),
        }
    }

    if !content.is_empty() {
        sections.insert(0, EntitySection::content(content));
    }
    sections
}

/// Reports sections written too few or too many times.
fn report_section_cardinality(
    entity_type: &EntityType,
    sections: &[EntitySection],
    span: Option<Span>,
    diagnostics: &mut Diagnostics,
) {
    for declared in &entity_type.sections {
        let written = sections
            .iter()
            .filter(|section| section.name() == Some(declared.name.as_str()))
            .count();
        if declared.required && written == 0 {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityMissingRequiredSection,
                format!(
                    "`.. {}::` requires a `.. {}::` section",
                    entity_type.name, declared.name
                ),
                span,
            ));
        }
        if !declared.multiple && written > 1 {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::EntityDuplicateSection,
                format!(
                    "`.. {}::` was written {written} times, but `.. {}::` declares it only once",
                    declared.name, entity_type.name
                ),
                span,
            ));
        }
    }
}

/// Lists a type's option vocabulary for an unknown-option diagnostic.
fn describe_options(entity_type: &EntityType) -> String {
    let names = entity_type.option_names();
    names
        .iter()
        .map(|name| format!(":{name}:"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;

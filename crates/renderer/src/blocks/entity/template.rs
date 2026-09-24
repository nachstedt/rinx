//! Rendering one entity through a per-type template.
//!
//! The escape hatch from the built-in box, and the reason it exists: `CPython`'s
//! `.. audit-event::` reads as prose ("Raises an auditing event X with
//! arguments Y"), while a requirement reads as a titled block with a field
//! table. Same model, different presentation.
//!
//! Every section's body is rendered to HTML **before** the template sees it.
//! `MiniJinja` cannot render RST, so the node tree must never reach it — the same
//! containment `math.rs` and `highlight.rs` give their backends.

use std::collections::{BTreeMap, BTreeSet};

use minijinja::{Environment, Value, context};
use rusty_sphinx_ast::{EntityBody, EntityId, FieldMutationMode};
use rusty_sphinx_index::{AppliedFieldUpdate, AppliedRelationUpdate};

use crate::RenderCtx;

/// Template sources the site supplied, keyed by the name a type refers to.
pub type EntityTemplates = BTreeMap<String, String>;

/// Renders `entity` through its type's template, if it declares one.
///
/// Returns `None` when the type names no template, which is the ordinary case
/// and means the caller should use the built-in rendering. A template that is
/// *named* but missing, or that fails to render, is a different matter: that is
/// a build misconfiguration, so it is reported and the built-in rendering
/// stands in rather than the page losing the entity.
pub(super) fn render_with_template(
    entity: &EntityBody,
    ctx: &mut RenderCtx<'_>,
) -> Option<Result<String, String>> {
    let name = ctx
        .schema
        .entity_type(&entity.type_name)?
        .template
        .clone()?;
    let Some(source) = ctx.entity_templates.get(&name) else {
        return Some(Err(format!(
            "entity type '{}' names the template '{name}', which the site does not supply",
            entity.type_name
        )));
    };
    let value = build_context(entity, ctx);
    Some(render_source(&name, source, &value))
}

/// Renders one template source against an already-built context.
fn render_source(name: &str, source: &str, value: &Value) -> Result<String, String> {
    let mut env = Environment::new();
    env.add_template(name, source)
        .map_err(|error| format!("entity template '{name}' is invalid: {error}"))?;
    let template = env
        .get_template(name)
        .map_err(|error| format!("entity template '{name}' could not be loaded: {error}"))?;
    template
        .render(value)
        .map_err(|error| format!("entity template '{name}' failed to render: {error}"))
}

/// Builds the values a template may read.
///
/// Five separate namespaces — `attributes`, `sections`, `outgoing`,
/// `incoming`, `history` — rather than one flat map. That is what makes a
/// back-link named `title` harmless, and is why the schema needs no list of
/// reserved words.
///
/// `attributes` and `outgoing` are the entity's *effective* values —
/// `super::effective_attribute`/`super::effective_relation_targets`, the same
/// accessors the built-in rendering reads through — never `entity.attributes`/
/// `entity.relations` directly, or a `.. entity-update::`'s effect would be
/// invisible to a custom template exactly as it once was to the built-in box.
/// See `docs/decisions/019-entity-update.md`.
fn build_context(entity: &EntityBody, ctx: &mut RenderCtx<'_>) -> Value {
    let entity_type = ctx.schema.entity_type(&entity.type_name);

    let attribute_names: BTreeSet<&str> = entity_type
        .map(|t| t.attributes.iter().map(|a| a.name.as_str()).collect())
        .unwrap_or_default();
    let attribute_names: BTreeSet<&str> = attribute_names
        .into_iter()
        .chain(entity.attributes.keys().map(String::as_str))
        .collect();
    let attributes: BTreeMap<String, String> = attribute_names
        .into_iter()
        .filter_map(|name| {
            Some((
                name.to_string(),
                super::effective_attribute(entity, name, ctx)?.to_string(),
            ))
        })
        .collect();

    let relation_names: BTreeSet<&str> = entity_type
        .map(|t| t.relations.iter().map(|r| r.name.as_str()).collect())
        .unwrap_or_default();
    let relation_names: BTreeSet<&str> = relation_names
        .into_iter()
        .chain(entity.relations.keys().map(String::as_str))
        .collect();
    let outgoing: BTreeMap<String, Vec<EntityId>> = relation_names
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                super::effective_relation_targets(entity, name, ctx).to_vec(),
            )
        })
        .collect();

    // Rendered first, and to HTML, because the template engine cannot render
    // RST. `from_safe_string` because this *is* HTML and must not be escaped
    // again; every value that is not already markup goes in as a plain string
    // and is escaped by MiniJinja as usual.
    let content = Value::from_safe_string(render_nodes_to_html(entity.content(), ctx));
    let mut sections: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut section_list: Vec<Value> = Vec::new();
    for section in super::sections_in_render_order(entity, ctx.schema) {
        if let Some(name) = section.name() {
            let key = template_key(name);
            let body = Value::from_safe_string(render_nodes_to_html(&section.body, ctx));
            let label = ctx
                .schema
                .types()
                .iter()
                .find_map(|t| t.section(name))
                .map_or(name, |s| s.display_label())
                .to_string();
            section_list.push(context! {
                name => key.clone(),
                label => label,
                body => body.clone(),
            });
            sections.entry(key).or_default().push(body);
        }
    }

    let label = entity_type.map_or(entity.type_name.clone(), |t| t.display_label().to_string());
    let history = build_history(entity, ctx);

    context! {
        labels => declared_labels(entity_type, &entity.type_name, ctx),
        section_list => section_list,
        id => entity.id.as_str(),
        type => entity.type_name.clone(),
        label => label,
        title => entity.title(),
        anchor => super::entity_anchor(entity.id.as_str()),
        content => content,
        sections => sections,
        attributes => attributes,
        history => history,
        outgoing => link_namespace(&outgoing, ctx),
        incoming => ctx
            .index
            .entity_backlinks
            .get(&entity.id)
            .map(|backlinks| link_namespace(backlinks, ctx))
            .unwrap_or_default(),
    }
}

/// The entity's traceable update history — original value, every applied
/// change (with its source and, when written, a rendered justification), and
/// the resulting current value — for a custom template that wants to build
/// its own changelog/audit UI. Empty namespaces for an entity no
/// `.. entity-update::` ever touched, rather than an absent `history`
/// variable, so a template can read `history.attributes.status.current`
/// unconditionally.
fn build_history(entity: &EntityBody, ctx: &mut RenderCtx<'_>) -> Value {
    let Some(field_history) = ctx.index.entity_update_history.get(&entity.id).cloned() else {
        return context! {
            attributes => BTreeMap::<String, Value>::new(),
            relations => BTreeMap::<String, Value>::new(),
        };
    };
    let attributes: BTreeMap<String, Value> = field_history
        .attributes
        .iter()
        .map(|(name, history)| {
            let applied: Vec<Value> = history
                .applied
                .iter()
                .map(|entry| applied_attribute_value(entry, ctx))
                .collect();
            (
                template_key(name),
                context! {
                    original => history.original.as_ref().map(ToString::to_string),
                    current => history.current.as_ref().map(ToString::to_string),
                    applied => applied,
                },
            )
        })
        .collect();
    let relations: BTreeMap<String, Value> = field_history
        .relations
        .iter()
        .map(|(name, history)| {
            let applied: Vec<Value> = history
                .applied
                .iter()
                .map(|entry| applied_relation_value(entry, ctx))
                .collect();
            (
                template_key(name),
                context! {
                    original => history.original.iter().map(EntityId::to_string).collect::<Vec<_>>(),
                    current => history.current.iter().map(EntityId::to_string).collect::<Vec<_>>(),
                    applied => applied,
                },
            )
        })
        .collect();
    context! { attributes => attributes, relations => relations }
}

/// One entry of an attribute's audit trail.
fn applied_attribute_value(entry: &AppliedFieldUpdate, ctx: &mut RenderCtx<'_>) -> Value {
    context! {
        doc_path => entry.doc_path.clone(),
        line => entry.span.map(|s| s.start.line),
        mode => mode_label(&entry.mode),
        value => entry.resulting_value.as_ref().map(ToString::to_string),
        conflict => entry.conflicts_with.is_some(),
        justification => Value::from_safe_string(justification_html(entry.update_index, ctx)),
    }
}

/// One entry of a relation's audit trail.
fn applied_relation_value(entry: &AppliedRelationUpdate, ctx: &mut RenderCtx<'_>) -> Value {
    context! {
        doc_path => entry.doc_path.clone(),
        line => entry.span.map(|s| s.start.line),
        mode => mode_label(&entry.mode),
        targets => entry.resulting_targets.iter().map(EntityId::to_string).collect::<Vec<_>>(),
        conflict => entry.conflicts_with.is_some(),
        justification => Value::from_safe_string(justification_html(entry.update_index, ctx)),
    }
}

/// The written spelling of a mutation's operation, for a template to switch
/// or label on.
fn mode_label(mode: &FieldMutationMode) -> &'static str {
    match mode {
        FieldMutationMode::Set(_) => "set",
        FieldMutationMode::Append(_) => "append",
        FieldMutationMode::Remove(_) => "remove",
        FieldMutationMode::Clear => "clear",
    }
}

/// The `.. entity-update::`/`.. needextend::` directive's own justification
/// prose, rendered to HTML — empty when the index carries no directive at
/// that position (should not happen: `update_index` always names a position
/// in `ctx.index.entity_updates`, the same vector every history entry's
/// index was taken from).
///
/// The body is cloned out of the index before rendering, rather than
/// borrowed: rendering needs `&mut RenderCtx`, which cannot coexist with a
/// borrow of `ctx.index` itself.
fn justification_html(update_index: usize, ctx: &mut RenderCtx<'_>) -> String {
    let Some(body) = ctx
        .index
        .entity_updates
        .get(update_index)
        .map(|record| record.update.body.clone())
    else {
        return String::new();
    };
    render_nodes_to_html(&body, ctx)
}

/// The labels the type declares, so a template renders an attribute, a section
/// or a relation under its declared heading rather than hardcoding a second
/// copy of it that can drift from the schema.
///
/// Keyed the same way the value namespaces are, so `sections.foo_bar` and
/// `labels.sections.foo_bar` always line up.
fn declared_labels(
    entity_type: Option<&rusty_sphinx_entity::EntityType>,
    type_name: &str,
    ctx: &RenderCtx<'_>,
) -> Value {
    let Some(entity_type) = entity_type else {
        return Value::from(());
    };

    let attributes: BTreeMap<String, String> = entity_type
        .attributes
        .iter()
        .map(|a| (template_key(&a.name), a.display_label().to_string()))
        .collect();
    let sections: BTreeMap<String, String> = entity_type
        .sections
        .iter()
        .map(|s| (template_key(&s.name), s.display_label().to_string()))
        .collect();

    // Both directions in one namespace, because a template reads
    // `outgoing.links` and `incoming.implemented_by` from two maps but wants
    // one place to look a heading up.
    //
    // The incoming half comes from `backlinks_for`, not from this type's own
    // relations: a back-link is declared on the type at the *other* end, so
    // `req` never mentions the `implemented_by` that lands on it. Deriving it
    // the same way the built-in rendering does is what keeps the two agreeing.
    let mut relations: BTreeMap<String, String> = entity_type
        .relations
        .iter()
        .map(|r| (template_key(&r.name), r.display_label().to_string()))
        .collect();
    for backlink in ctx.schema.backlinks_for(type_name) {
        relations.insert(template_key(&backlink.name), backlink.label.clone());
    }

    context! { attributes => attributes, sections => sections, relations => relations }
}

/// Turns a map of relation name to ids into `{name: [{id, href}]}`.
fn link_namespace(
    relations: &BTreeMap<String, Vec<rusty_sphinx_ast::EntityId>>,
    ctx: &RenderCtx<'_>,
) -> BTreeMap<String, Vec<Value>> {
    relations
        .iter()
        .map(|(name, targets)| {
            let links: Vec<Value> = targets
                .iter()
                .map(|target| {
                    let href = ctx.index.entities.get(target).map(|record| {
                        crate::inline::entity_reference::entity_href(
                            record,
                            ctx.doc_path,
                            target.as_str(),
                        )
                    });
                    context! { id => target.as_str(), href => href }
                })
                .collect();
            (template_key(name), links)
        })
        .collect()
}

/// The key a template addresses a name by.
///
/// Hyphens become underscores, because `sections.verification-criteria` is a
/// subtraction in a Jinja expression while `sections.verification_criteria` is
/// a lookup. Doing it here rather than asking schema authors to avoid hyphens
/// keeps the directive spelling natural, which is what an `.rst` author sees.
fn template_key(name: &str) -> String {
    name.replace('-', "_")
}

/// Renders body nodes to an HTML string.
fn render_nodes_to_html(nodes: &[rusty_sphinx_ast::Node], ctx: &mut RenderCtx<'_>) -> String {
    let mut html = String::new();
    super::super::render_nodes(&mut html, nodes, ctx);
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_key_makes_a_hyphenated_name_addressable() {
        // Given — `sections.verification-criteria` would parse as a subtraction
        let name = "verification-criteria";

        // When
        let key = template_key(name);

        // Then
        assert_eq!(key, "verification_criteria");
    }

    #[test]
    fn test_template_key_leaves_a_plain_name_alone() {
        // Given / When / Then
        assert_eq!(template_key("rationale"), "rationale");
    }

    #[test]
    fn test_render_source_renders_a_valid_template() {
        // Given
        let value = context! { id => "REQ_001" };

        // When
        let html = render_source("t.html", "<p>{{ id }}</p>", &value).unwrap();

        // Then
        assert_eq!(html, "<p>REQ_001</p>");
    }

    #[test]
    fn test_render_source_reports_an_invalid_template_by_name() {
        // Given
        let value = context! {};

        // When
        let error = render_source("t.html", "{% for %}", &value).unwrap_err();

        // Then
        assert!(error.contains("t.html"), "{error}");
    }

    #[test]
    fn test_render_source_does_not_escape_safe_html() {
        // Given — a section's body is already HTML and must not be escaped again
        let value = context! {
            content => Value::from_safe_string("<p>prose</p>".to_string()),
        };

        // When
        let html = render_source("t.html", "{{ content }}", &value).unwrap();

        // Then
        assert_eq!(html, "<p>prose</p>");
    }

    #[test]
    fn test_render_source_escapes_a_plain_value() {
        // Given — an attribute value is text, not markup
        let value = context! { title => "a < b" };

        // When
        let html = render_source("t.html", "{{ title }}", &value).unwrap();

        // Then
        assert_eq!(html, "a &lt; b");
    }
}

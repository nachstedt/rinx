//! Applying every `.. entity-update::`/`.. needextend::` in the project.
//!
//! The one project-wide phase this crate runs that is not a mutation of
//! `ProjectIndex::entities` — it never touches that map at all. Instead it
//! builds `ProjectIndex::entity_update_history`, a derived, non-destructive
//! overlay recording each touched field's original value, every applied
//! change (with its source file/line and, via `update_index`, its
//! justification), and the resulting current value. Every reader of an
//! entity's attributes or outgoing relations — the renderer,
//! `EntitySubject`'s filter evaluation, this very phase's own later
//! iterations — must read the *effective* value through
//! `ProjectIndex::effective_attribute`/`effective_relation_targets` rather
//! than off a record directly, or an update's effect is invisible to it. See
//! `docs/decisions/019-entity-update.md`.

use std::collections::BTreeMap;

use rinx_ast::{
    AttributeValue, Diagnostic, DiagnosticCode, EntityId, FieldMutation, FieldMutationMode, Span,
    UpdateTarget,
};
use rinx_entity::{AttributeSchema, EntitySchema, RelationSpec, parse_attribute_value, split_list};
use rinx_index::{
    AppliedFieldUpdate, AppliedRelationUpdate, AttributeFieldHistory, EntitySubject, ProjectIndex,
    RelationFieldHistory,
};

use super::DocumentDiagnostics;

/// The three facts every mutation function needs about the update directive
/// applying it, bundled into one argument rather than three repeated ones.
#[derive(Debug, Clone, Copy)]
struct UpdateSite<'a> {
    /// Position in the (sorted) `index.entity_updates` — what a written
    /// history entry's `update_index` records.
    index: usize,
    doc_path: &'a str,
    span: Option<Span>,
}

/// Applies every collected `.. entity-update::`/`.. needextend::` to the
/// merged graph, building `index.entity_update_history` beside it.
///
/// Run **between** `merge_document_analyses` and `derive_entity_backlinks`
/// (see `crates/analyzer/src/project_index.rs`): it can change an entity's
/// effective outgoing edges, and back-links must be derived from those, not
/// from the as-authored ones.
///
/// Sorts `index.entity_updates` **in place** by `(doc_path, span start line)`
/// ascending (a stable sort, so ties keep merge order) — the directive's
/// canonical, deterministic application order. Every history entry's
/// `update_index` is simply its position in this now-ordered vector.
pub(crate) fn apply_entity_updates(
    index: &mut ProjectIndex,
    schema: &EntitySchema,
) -> Vec<DocumentDiagnostics> {
    index.entity_updates.sort_by(|a, b| {
        (a.doc_path.as_str(), span_line(a.update.span))
            .cmp(&(b.doc_path.as_str(), span_line(b.update.span)))
    });

    let mut by_document: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
    for update_index in 0..index.entity_updates.len() {
        // Cloned out rather than borrowed: the loop body needs `&mut index`
        // to write history, which cannot coexist with a borrow of
        // `index.entity_updates[update_index]` itself.
        let record = index.entity_updates[update_index].clone();
        let doc_path = record.doc_path;
        let target = record.update.target;
        let strict = record.update.strict;
        let fields = record.update.fields;
        let span = record.update.span;

        let matched = resolve_targets(&target, span, &doc_path, index, schema, &mut by_document);
        if matched.is_empty() && strict {
            report(
                &mut by_document,
                &doc_path,
                DiagnosticCode::EntityUpdateEmptyResult,
                format!(
                    "entity-update: '{}' matched no entity; add :strict: false to silence this",
                    target.raw
                ),
                span,
            );
        }
        let site = UpdateSite {
            index: update_index,
            doc_path: &doc_path,
            span,
        };
        for target_id in matched {
            apply_mutations(site, &target_id, &fields, index, schema, &mut by_document);
        }
    }

    group(by_document)
}

/// The line an update's own span starts on, for the deterministic sort — `0`
/// for a positionless update, which sorts first within its document (an
/// update that somehow lost its position is at least still grouped with the
/// rest of the same file rather than scattered by insertion order).
fn span_line(span: Option<Span>) -> u32 {
    span.map_or(0, |s| s.start.line)
}

/// Every entity id one update's target names, resolved against the merged
/// graph — upstream sphinx-needs' exact disambiguation: an id that exists
/// wins over any filter reading, even when the argument also happens to
/// parse as one.
///
/// Reports an unknown field in the *filter* reading only once it is known
/// that reading is the one actually driving selection — deferred from parse
/// time for the reason `read_update_argument` gives: warning about a filter
/// reading that turns out unused, because the argument was actually a good
/// id, would be a false positive on the common case.
fn resolve_targets(
    target: &UpdateTarget,
    update_span: Option<Span>,
    doc_path: &str,
    index: &ProjectIndex,
    schema: &EntitySchema,
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
) -> Vec<EntityId> {
    if let Some(candidate) = &target.candidate_id
        && index.entities.contains_key(candidate)
    {
        return vec![candidate.clone()];
    }
    let Some(filter) = &target.filter else {
        return Vec::new();
    };
    for name in filter.field_names() {
        if !schema.declares_field(name.as_str()) {
            report(
                by_document,
                doc_path,
                DiagnosticCode::EntityUpdateUnknownField,
                format!(
                    "entity-update: '{}' filters on unknown field '{name}'; the schema declares {}",
                    target.raw,
                    schema.field_names().join(", ")
                ),
                update_span,
            );
        }
    }
    index
        .entities
        .iter()
        .filter(|(id, record)| {
            let subject = EntitySubject {
                id,
                record,
                index,
                schema,
            };
            filter.matches(&subject)
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// Applies every field mutation of one update to one matched entity.
///
/// Target existence and relation-type-acceptance are deliberately **not**
/// re-checked here: `collect_entity_diagnostics`, which runs after this in
/// `build_project_index_reporting`, already walks every entity's *effective*
/// outgoing edges (see its own doc comment) regardless of how they got
/// there, so a bad id an `Append` just wrote is caught by the existing pass
/// for free.
fn apply_mutations(
    site: UpdateSite<'_>,
    target_id: &EntityId,
    fields: &[FieldMutation],
    index: &mut ProjectIndex,
    schema: &EntitySchema,
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
) {
    let Some(type_name) = index.entities.get(target_id).map(|r| r.type_name.clone()) else {
        return;
    };
    let Some(entity_type) = schema.entity_type(&type_name) else {
        return;
    };
    for mutation in fields {
        if let Some(attr) = entity_type.attribute(&mutation.field).cloned() {
            apply_attribute_mutation(site, target_id, mutation, &attr, index, by_document);
        } else if let Some(relation) = entity_type.relation(&mutation.field).cloned() {
            apply_relation_mutation(site, target_id, mutation, &relation, index, by_document);
        } else {
            report(
                by_document,
                site.doc_path,
                DiagnosticCode::EntityUpdateFieldNotApplicable,
                format!(
                    "entity-update: '{target_id}' has no field ':{}:' — its type '{type_name}' \
                     declares no such attribute or relation",
                    mutation.field
                ),
                site.span,
            );
        }
    }
}

/// Converts one mutation's raw text into a new attribute value, writes the
/// resulting history entry, and detects a conflict with an earlier `Set`/
/// `Clear` from a different update.
fn apply_attribute_mutation(
    site: UpdateSite<'_>,
    target_id: &EntityId,
    mutation: &FieldMutation,
    attr: &AttributeSchema,
    index: &mut ProjectIndex,
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
) {
    let current = index
        .effective_attribute(target_id, &mutation.field)
        .cloned();

    let new_value = match &mutation.mode {
        FieldMutationMode::Set(text) => match parse_attribute_value(&attr.value_type, text) {
            Ok(value) => Some(value),
            Err(error) => {
                report_invalid_value(by_document, site, target_id, &mutation.field, &error);
                return;
            }
        },
        FieldMutationMode::Clear => None,
        FieldMutationMode::Append(text) => {
            if !attr.value_type.is_list() {
                report_list_op_on_scalar(by_document, site, target_id, &mutation.field);
                return;
            }
            let addition = match parse_attribute_value(&attr.value_type, text) {
                Ok(AttributeValue::List(items)) => items,
                Ok(_) => Vec::new(),
                Err(error) => {
                    report_invalid_value(by_document, site, target_id, &mutation.field, &error);
                    return;
                }
            };
            let mut items = list_items(current.as_ref());
            for item in addition {
                if !items.contains(&item) {
                    items.push(item);
                }
            }
            Some(AttributeValue::List(items))
        }
        FieldMutationMode::Remove(text) => {
            if !attr.value_type.is_list() {
                report_list_op_on_scalar(by_document, site, target_id, &mutation.field);
                return;
            }
            let removal = match parse_attribute_value(&attr.value_type, text) {
                Ok(AttributeValue::List(items)) => items,
                Ok(_) => Vec::new(),
                Err(error) => {
                    report_invalid_value(by_document, site, target_id, &mutation.field, &error);
                    return;
                }
            };
            let mut items = list_items(current.as_ref());
            items.retain(|item| !removal.contains(item));
            Some(AttributeValue::List(items))
        }
    };

    let history = index
        .entity_update_history
        .entry(target_id.clone())
        .or_default();
    let attr_history = history
        .attributes
        .entry(mutation.field.clone())
        .or_insert_with(|| AttributeFieldHistory {
            original: current.clone(),
            applied: Vec::new(),
            current: current.clone(),
        });

    let conflicts_with =
        conflicting_attribute_update(attr_history, &mutation.mode, site.index, new_value.as_ref());
    if let Some(earlier) = &conflicts_with {
        let earlier = earlier.clone();
        report_conflict(by_document, site, target_id, &mutation.field, &earlier);
    }

    attr_history.applied.push(AppliedFieldUpdate {
        update_index: site.index,
        doc_path: site.doc_path.to_string(),
        span: site.span,
        mode: mutation.mode.clone(),
        resulting_value: new_value.clone(),
        conflicts_with: conflicts_with.as_ref().map(|e| e.update_index),
    });
    attr_history.current = new_value;
}

/// The items of a list-valued current value, empty for a scalar or absent one.
fn list_items(current: Option<&AttributeValue>) -> Vec<String> {
    match current {
        Some(AttributeValue::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// The earlier `Set`/`Clear` entry (from a different update) this new mode
/// disagrees with, if any — only ever computed for a `Set`/`Clear` mode
/// itself, since `Append`/`Remove` are explicitly incremental and never
/// count as a disagreement.
fn conflicting_attribute_update(
    history: &AttributeFieldHistory,
    mode: &FieldMutationMode,
    update_index: usize,
    new_value: Option<&AttributeValue>,
) -> Option<AppliedFieldUpdate> {
    if !matches!(mode, FieldMutationMode::Set(_) | FieldMutationMode::Clear) {
        return None;
    }
    history
        .applied
        .iter()
        .rev()
        .find(|entry| {
            matches!(
                entry.mode,
                FieldMutationMode::Set(_) | FieldMutationMode::Clear
            )
        })
        .filter(|entry| {
            entry.update_index != update_index && entry.resulting_value.as_ref() != new_value
        })
        .cloned()
}

/// As [`apply_attribute_mutation`], for a relation's target list.
fn apply_relation_mutation(
    site: UpdateSite<'_>,
    target_id: &EntityId,
    mutation: &FieldMutation,
    relation: &RelationSpec,
    index: &mut ProjectIndex,
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
) {
    let current = index
        .effective_relation_targets(target_id, &mutation.field)
        .to_vec();

    let new_targets = match &mutation.mode {
        FieldMutationMode::Set(text) => match parse_ids(text) {
            Ok(ids) => ids,
            Err(bad) => {
                report_invalid_id(by_document, site, target_id, &mutation.field, &bad);
                return;
            }
        },
        FieldMutationMode::Clear => Vec::new(),
        FieldMutationMode::Append(text) => {
            let addition = match parse_ids(text) {
                Ok(ids) => ids,
                Err(bad) => {
                    report_invalid_id(by_document, site, target_id, &mutation.field, &bad);
                    return;
                }
            };
            let mut ids = current.clone();
            for id in addition {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            ids
        }
        FieldMutationMode::Remove(text) => {
            let removal = match parse_ids(text) {
                Ok(ids) => ids,
                Err(bad) => {
                    report_invalid_id(by_document, site, target_id, &mutation.field, &bad);
                    return;
                }
            };
            let mut ids = current.clone();
            ids.retain(|id| !removal.contains(id));
            ids
        }
    };

    if !relation.multiple && new_targets.len() > 1 {
        report(
            by_document,
            site.doc_path,
            DiagnosticCode::EntityMultipleRelationTargets,
            format!(
                "entity-update: '{target_id}:{}' accepts at most one target, but this mutation \
                 would leave {} — keeping all of them anyway",
                mutation.field,
                new_targets.len()
            ),
            site.span,
        );
    }

    let history = index
        .entity_update_history
        .entry(target_id.clone())
        .or_default();
    let relation_history = history
        .relations
        .entry(mutation.field.clone())
        .or_insert_with(|| RelationFieldHistory {
            original: current.clone(),
            applied: Vec::new(),
            current: current.clone(),
        });

    let conflicts_with =
        conflicting_relation_update(relation_history, &mutation.mode, site.index, &new_targets);
    if let Some(earlier) = &conflicts_with {
        let earlier = earlier.clone();
        report_conflict(by_document, site, target_id, &mutation.field, &earlier);
    }

    relation_history.applied.push(AppliedRelationUpdate {
        update_index: site.index,
        doc_path: site.doc_path.to_string(),
        span: site.span,
        mode: mutation.mode.clone(),
        resulting_targets: new_targets.clone(),
        conflicts_with: conflicts_with.map(|e| e.update_index),
    });
    relation_history.current = new_targets;
}

/// As [`conflicting_attribute_update`], for a relation's applied entries.
fn conflicting_relation_update(
    history: &RelationFieldHistory,
    mode: &FieldMutationMode,
    update_index: usize,
    new_targets: &[EntityId],
) -> Option<AppliedRelationUpdate> {
    if !matches!(mode, FieldMutationMode::Set(_) | FieldMutationMode::Clear) {
        return None;
    }
    history
        .applied
        .iter()
        .rev()
        .find(|entry| {
            matches!(
                entry.mode,
                FieldMutationMode::Set(_) | FieldMutationMode::Clear
            )
        })
        .filter(|entry| {
            entry.update_index != update_index && entry.resulting_targets != new_targets
        })
        .cloned()
}

/// Splits a relation option's text into entity ids, the way a written
/// `:links:` option would — a comma-separated list, each item a legal id.
fn parse_ids(text: &str) -> Result<Vec<EntityId>, String> {
    split_list(text)
        .into_iter()
        .map(|item| EntityId::new(&item).map_err(|error| format!("'{item}': {error}")))
        .collect()
}

/// Reports a value that does not fit its field's declared type.
fn report_invalid_value(
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
    site: UpdateSite<'_>,
    target_id: &EntityId,
    field: &str,
    error: &rinx_entity::AttributeParseError,
) {
    report(
        by_document,
        site.doc_path,
        DiagnosticCode::EntityUpdateInvalidValue,
        format!("entity-update: '{target_id}:{field}': {error}"),
        site.span,
    );
}

/// Reports an id in a relation mutation's value that is not a legal
/// [`EntityId`] spelling.
fn report_invalid_id(
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
    site: UpdateSite<'_>,
    target_id: &EntityId,
    field: &str,
    bad: &str,
) {
    report(
        by_document,
        site.doc_path,
        DiagnosticCode::EntityUpdateInvalidValue,
        format!("entity-update: '{target_id}:{field}': {bad}"),
        site.span,
    );
}

/// Reports a `+`/`-` operation attempted on a field that is not list-valued.
fn report_list_op_on_scalar(
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
    site: UpdateSite<'_>,
    target_id: &EntityId,
    field: &str,
) {
    report(
        by_document,
        site.doc_path,
        DiagnosticCode::EntityUpdateListOperationOnScalar,
        format!(
            "entity-update: '{target_id}:{field}' is not list-valued; +/- only apply to list \
             attributes and relations"
        ),
        site.span,
    );
}

/// Reports a conflict **symmetrically**: once against the later directive
/// (the one this call is currently applying) and once against the earlier
/// one it disagreed with, each carrying its own span in its own file.
///
/// A cross-file conflict has no meaningful "later" an author could act on —
/// `.. noqa:` matches a diagnostic only against its own file's span, so
/// attributing one shared diagnostic to whichever directive happens to sort
/// last would leave the other author with no way to suppress it at all. See
/// `docs/decisions/019-entity-update.md`.
fn report_conflict<M: EarlierMutation>(
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
    site: UpdateSite<'_>,
    target_id: &EntityId,
    field: &str,
    earlier: &M,
) {
    report(
        by_document,
        site.doc_path,
        DiagnosticCode::EntityUpdateConflictingUpdate,
        format!(
            "entity-update: '{target_id}:{field}' is set here, conflicting with a different \
             value set by an entity-update in '{}'",
            earlier.doc_path()
        ),
        site.span,
    );
    report(
        by_document,
        earlier.doc_path(),
        DiagnosticCode::EntityUpdateConflictingUpdate,
        format!(
            "entity-update: '{target_id}:{field}' is set here, conflicting with a different \
             value set by an entity-update in '{}'",
            site.doc_path
        ),
        earlier.span(),
    );
}

/// The doc path/span an [`AppliedFieldUpdate`]/[`AppliedRelationUpdate`]
/// carries — enough for [`report_conflict`] to report against either kind
/// without knowing which one it was given.
trait EarlierMutation {
    fn doc_path(&self) -> &str;
    fn span(&self) -> Option<Span>;
}

impl EarlierMutation for AppliedFieldUpdate {
    fn doc_path(&self) -> &str {
        &self.doc_path
    }

    fn span(&self) -> Option<Span> {
        self.span
    }
}

impl EarlierMutation for AppliedRelationUpdate {
    fn doc_path(&self) -> &str {
        &self.doc_path
    }

    fn span(&self) -> Option<Span> {
        self.span
    }
}

/// Pushes one diagnostic under `doc_path`, carrying `span` — every diagnostic
/// this phase raises has a real span (the update directive's own), which is
/// what makes it `.. noqa:`-suppressible: `Suppression::suppresses` refuses a
/// positionless diagnostic outright.
fn report(
    by_document: &mut BTreeMap<String, Vec<Diagnostic>>,
    doc_path: &str,
    code: DiagnosticCode,
    message: String,
    span: Option<Span>,
) {
    by_document
        .entry(doc_path.to_string())
        .or_default()
        .push(Diagnostic::at(code, message, span));
}

/// Turns the per-document diagnostic map into the reporting shape.
fn group(by_document: BTreeMap<String, Vec<Diagnostic>>) -> Vec<DocumentDiagnostics> {
    by_document
        .into_iter()
        .map(|(source_path, diagnostics)| DocumentDiagnostics {
            source_path,
            diagnostics,
        })
        .collect()
}

#[cfg(test)]
mod tests;

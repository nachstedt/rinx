//! Indexing entities: the per-document record, and the two project-wide
//! passes that only the whole document set can perform.
//!
//! The split follows the same line `page_order` and `section_numbers` draw.
//! An entity's own data — its type, title, attributes and *outgoing* edges —
//! is a fact about the document it was written in, so it merges. Its
//! *incoming* edges are a fact about the project, so they are recomputed here
//! from the merged whole, and never stored per document.

use std::collections::BTreeMap;

use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, EntityBody, EntityId, Node, Span, walk_nodes,
};

use super::DocumentDiagnostics;
use rinx_entity::EntitySchema;
use rinx_index::{EntityRecord, ProjectIndex};

/// Records one entity in the document-local index.
pub(crate) fn index_entity(entity: &EntityBody, doc_path: &str, index: &mut ProjectIndex) {
    let record = EntityRecord {
        type_name: entity.type_name.clone(),
        doc_path: doc_path.to_string(),
        title: entity.title(),
        attributes: entity.attributes.clone(),
        outgoing: entity.relations.clone(),
        uml: collect_entity_umls(entity),
        relation_spans: entity.relation_spans.clone(),
    };
    index.entities.insert(entity.id.clone(), record);
}

/// The diagram templates written inside `entity`, by `:key:`.
///
/// Indexed because another document's diagram may import one by id, and only
/// the project index spans documents. Kept as the *template* rather than as
/// its expansion: an imported diagram is expanded in the context of whoever
/// imports it, which is what makes one architecture block reusable across the
/// pages that reference it.
///
/// Two diagrams sharing a key is the author writing the same name twice; the
/// last in document order wins, which is `BTreeMap::insert`'s own rule and the
/// same one a duplicate section name follows.
fn collect_entity_umls(entity: &EntityBody) -> BTreeMap<String, String> {
    let mut umls = BTreeMap::new();
    for section in &entity.sections {
        walk_nodes(&section.body, &mut |node| {
            if let Node::Directive(Directive::Uml(uml)) = node {
                umls.insert(uml.key.clone().unwrap_or_default(), uml.template.clone());
            }
        });
    }
    umls
}

/// Recomputes every entity's incoming edges from the merged graph.
///
/// Reads the schema to learn which back-link name a relation feeds, because
/// that is a schema fact and not recorded on the edge itself — storing it on
/// each edge would duplicate the schema into the index, where it could
/// disagree with it.
///
/// Reads each entity's *effective* outgoing edges — [`ProjectIndex::effective_relation_targets`],
/// not `record.outgoing` directly — so a back-link reflects whatever
/// `apply_entity_updates` appended or removed, not just the as-authored
/// graph. That phase already ran by the time this one does (see
/// `project_index::build_project_index_reporting`), which is exactly why this
/// ordering matters.
///
/// An edge whose target does not exist contributes nothing here; naming that
/// is [`collect_entity_diagnostics`]'s job, so the derivation stays a pure
/// function of the graph it is given.
pub(crate) fn derive_entity_backlinks(
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> BTreeMap<EntityId, BTreeMap<String, Vec<EntityId>>> {
    let mut backlinks: BTreeMap<EntityId, BTreeMap<String, Vec<EntityId>>> = BTreeMap::new();

    for (source_id, record) in &index.entities {
        let Some(entity_type) = schema.entity_type(&record.type_name) else {
            continue;
        };
        for relation in &entity_type.relations {
            let Some(incoming) = relation.incoming.as_deref() else {
                continue;
            };
            let targets = index.effective_relation_targets(source_id, &relation.name);
            for target in targets {
                if !index.entities.contains_key(target) {
                    continue;
                }
                let bucket = backlinks
                    .entry(target.clone())
                    .or_default()
                    .entry(incoming.to_string())
                    .or_default();
                // One source may name the same target twice; the reader should
                // see it once.
                if !bucket.contains(source_id) {
                    bucket.push(source_id.clone());
                }
            }
        }
    }

    backlinks
}

/// Reports the entity faults that only the whole document set reveals.
///
/// Everything checkable from one document — an unknown option, a value
/// outside its enum, a missing required section — was already reported while
/// parsing. What is left needs the merged graph: whether a target exists at
/// all, and whether its type is one the relation accepts.
///
/// Reads each entity's *effective* outgoing edges, exactly as
/// [`derive_entity_backlinks`] does and for the same reason: a target an
/// `.. entity-update::`'s `+relation` just appended must still be validated,
/// not only a target the author wrote by hand.
pub(crate) fn collect_entity_diagnostics(
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> Vec<DocumentDiagnostics> {
    let mut by_document: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();

    for (source_id, record) in &index.entities {
        let Some(entity_type) = schema.entity_type(&record.type_name) else {
            continue;
        };
        for relation in &entity_type.relations {
            let relation_name = &relation.name;
            let targets = index.effective_relation_targets(source_id, relation_name);
            for target in targets {
                let found = index.entities.get(target);
                let at = written_at(record, relation_name, target);
                let diagnostic = match found {
                    // Declared twice rather than not at all: already reported
                    // on both declarations as `entity.duplicate-id`.
                    None if index.ambiguous_definitions.entities.contains_key(target) => None,
                    None => Some(entity_diagnostic(
                        DiagnosticCode::EntityUnknownTarget,
                        format!(
                            "entity '{source_id}' links to '{target}' via ':{relation_name}:', which no document declares"
                        ),
                        at,
                    )),
                    Some(found) if !relation.accepts_target(&found.type_name) => {
                        Some(entity_diagnostic(
                            DiagnosticCode::EntityDisallowedRelation,
                            format!(
                                "entity '{source_id}' links to '{target}' via ':{relation_name}:', but that relation does not accept an entity of type '{}'",
                                found.type_name
                            ),
                            at,
                        ))
                    }
                    Some(_) => None,
                };
                // Attributed to the document that wrote the *source* entity:
                // that is the file whose text has to change, and the one an
                // author can actually open.
                if let Some(diagnostic) = diagnostic {
                    by_document
                        .entry(record.doc_path.clone())
                        .or_default()
                        .push(diagnostic);
                }
            }
        }
    }

    group(by_document)
}

/// Turns a per-document diagnostic map into the reporting shape.
fn group(by_document: BTreeMap<String, Vec<Diagnostic>>) -> Vec<DocumentDiagnostics> {
    by_document
        .into_iter()
        .map(|(source_path, diagnostics)| DocumentDiagnostics {
            source_path,
            diagnostics,
        })
        .collect()
}

/// Reports a document parsed against a schema other than the one in use.
///
/// A mismatch means the parser worked from a different vocabulary than this
/// index is being built with, so every entity in that document is suspect.
/// Saying so once, naming the document, beats the cascade of unknown-directive
/// diagnostics the mismatch would otherwise produce.
pub(crate) fn collect_schema_mismatches(
    documents: &[(&str, Option<&str>)],
    schema: &EntitySchema,
) -> Vec<DocumentDiagnostics> {
    // The rule is narrower than "every document carries this hash": a document
    // parsed against *no* schema is silent, because a library that uses no
    // entities has no reason to declare one, and most libraries in a
    // multi-library site are exactly that. Demanding the hash everywhere
    // reports every such library, which is noise, not a finding.
    //
    // What is worth reporting is a document parsed against *some* schema that
    // is not this one — including the case where this build has none at all,
    // which is the likelier misconfiguration (the library declares it and the
    // site forgot to).
    //
    // The remaining case — a library that uses entities but forgot to declare
    // the schema — is not silently lost: its directives were never recognised,
    // so it surfaces as unknown directives and dangling references, which point
    // at the offending line rather than at the whole document.
    let expected = (!schema.is_empty()).then(|| schema.hash());
    documents
        .iter()
        .filter(|(_, hash)| hash.is_some() && *hash != expected)
        .map(|(path, _)| DocumentDiagnostics {
            source_path: (*path).to_string(),
            diagnostics: vec![entity_diagnostic(
                DiagnosticCode::EntitySchemaMismatch,
                format!(
                    "document '{path}' was parsed against a different entity schema than this build is using; check that every rinx_library and the site name the same entity_schema"
                ),
                // About the whole document, so no line of it.
                None,
            )],
        })
        .collect()
}

/// The line `target` was written on as a target of `relation` of the entity
/// `record` describes, or `None` when the author did not write it there.
///
/// A target only an `.. entity-update::` appended has no line in the
/// entity's document — the update may sit in another one — so it is
/// reported without a position rather than at a wrong one, the guideline
/// for a finding with no line of its own. So is an entity no author wrote,
/// such as an imported need.
fn written_at(record: &EntityRecord, relation: &str, target: &EntityId) -> Option<Span> {
    if !record.targets(relation).contains(target) {
        return None;
    }
    record.relation_spans.get(relation).copied()
}

/// Builds an entity diagnostic, at `span` when the finding has a line.
///
/// These are found by walking the merged index. An entity's record names its
/// document, and the line each relation was written on — kept for this, as a
/// toctree entry's span is — so a relation's finding lands on the option
/// line; a finding about something no line wrote is reported without one,
/// and its message names the entity, which is what an author searches for.
fn entity_diagnostic(code: DiagnosticCode, message: String, span: Option<Span>) -> Diagnostic {
    Diagnostic::at(code, message, span)
}

#[cfg(test)]
mod tests;

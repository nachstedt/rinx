use crate::{
    DocumentNumbers, DocumentOutline, DocumentToctree, EntityFieldHistory, EntityRecord,
    EntityUpdateRecord, EquationLocation, GenIndexEntry, TargetLocation,
};
use rusty_sphinx_ast::{AttributeValue, EntityId, ObjectType, SectnumOptions, TargetName};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A global symbol table built from all documents in the project.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIndex {
    /// Maps target names to document paths.
    pub targets: BTreeMap<TargetName, TargetLocation>,
    /// Maps document paths to their top-level title.
    pub document_titles: BTreeMap<String, String>,
    /// Each document's `.. toctree::` directives, in document order, with
    /// their entries still unexpanded — a `:glob:` is stored as a pattern.
    ///
    /// This is the navigation *graph*, stored instead of a pre-flattened tree
    /// so that each toctree's own options travel with its own entries. It is
    /// per-document, so it merges, which is what keeps the live-preview path
    /// from rendering navigation out of a stale global index.
    #[serde(default)]
    pub toctrees: BTreeMap<String, Vec<DocumentToctree>>,
    /// The documents nothing else references, in sorted order — the roots
    /// navigation, page order and section numbering all start from.
    ///
    /// Project-wide rather than per-document, so like `page_order` it is
    /// recomputed by `build_project_index` and not merged.
    #[serde(default)]
    pub root_documents: Vec<String>,
    /// Every document in reading order — a depth-first walk of the toctree
    /// graph from the roots, first visit winning. Prev/next links are a
    /// position in this rather than two stored strings per document, so they
    /// cannot disagree with the order they came from.
    ///
    /// Project-wide, so it is recomputed rather than merged. A document no
    /// toctree reaches is absent, which is what gives an orphan no neighbours.
    #[serde(default)]
    pub page_order: Vec<String>,
    /// The `:numbered:` section numbers, keyed by document path. Project-wide
    /// — a number depends on the whole toctree graph, not on one document — so
    /// it is recomputed rather than merged.
    #[serde(default)]
    pub section_numbers: BTreeMap<String, DocumentNumbers>,
    /// Each document's heading hierarchy, so a toctree in *another* document
    /// can list this one's sections. Per-document data, so unlike `page_order`
    /// it merges — see [`DocumentOutline`].
    #[serde(default)]
    pub document_outlines: BTreeMap<String, DocumentOutline>,
    /// Maps normalized glossary term names to the document path containing their definition.
    #[serde(default)]
    pub glossary_terms: BTreeMap<TargetName, String>,
    /// Maps a domain object's qualified name (e.g. `xmlrpc.client.fault`) to
    /// every object type it's been defined under and the document path
    /// containing that `Directive::DomainObject` definition. Keyed by name
    /// first (rather than baking the object type into a single flat key)
    /// so a reference can be resolved against any of the object types real
    /// Sphinx treats as mutually aliasable for the same name (see
    /// [`ObjectType::role_alias_candidates`]) — e.g. `CPython` documents
    /// `Fault` via `.. class::` but references it via `:exc:`.
    #[serde(default)]
    pub domain_objects: BTreeMap<TargetName, BTreeMap<ObjectType, String>>,
    /// Entries for the site-wide general index page, accumulated (not
    /// deduplicated) across every document — the same term legitimately
    /// appearing from multiple locations is expected, not an error.
    #[serde(default)]
    pub genindex_entries: Vec<GenIndexEntry>,
    /// Maps a `.. math::` label to the document defining it and the equation
    /// number it was given, so an `:eq:` in any document can render that
    /// number as its link text. See [`EquationLocation`] for the numbering
    /// rules.
    #[serde(default)]
    pub equations: BTreeMap<TargetName, EquationLocation>,
    /// Each document's own `.. sectnum::`/`.. section-numbering::` options,
    /// keyed by document path — the last one found in that document if it
    /// wrote more than one. Per-document data, so unlike `section_numbers`
    /// (which this feeds, alongside `:numbered:` toctrees) it merges. See
    /// `rusty_sphinx_analyzer::section_numbering` for how the two combine —
    /// an ancestor `:numbered:` toctree always wins over a document's own
    /// `.. sectnum::`.
    #[serde(default)]
    pub sectnum: BTreeMap<String, SectnumOptions>,
    /// Every entity in the project, keyed by its id.
    ///
    /// Per-document data, so it merges — and a merge that finds one id twice
    /// reports it, the way a duplicate glossary term is reported. This is the
    /// entity *graph*: each record carries only its own outgoing edges, which
    /// is what lets a single document's entities be merged into a stale index
    /// on the live-preview path.
    #[serde(default)]
    pub entities: BTreeMap<EntityId, EntityRecord>,
    /// Which entities point *at* each entity, keyed by the target's id and
    /// then by the back-link's name.
    ///
    /// Project-wide rather than per-document, because an entity's incoming
    /// edges come from documents it has never heard of. So, like `page_order`
    /// and `section_numbers`, this is recomputed by `build_project_index`
    /// rather than merged — storing it per document would be storing a
    /// conclusion that only the whole graph can reach.
    #[serde(default)]
    pub entity_backlinks: BTreeMap<EntityId, BTreeMap<String, Vec<EntityId>>>,
    /// Every `.. entity-update::`/`.. needextend::` in the project, collected
    /// per document. Accumulated, not deduplicated, like `genindex_entries`:
    /// two authors extending the same entities is expected, and order
    /// matters, so nothing here collapses duplicates. Sorted into canonical,
    /// deterministic application order by
    /// `rusty_sphinx_analyzer::apply_entity_updates` the first time the full
    /// index is built — every [`crate::AppliedFieldUpdate`]/
    /// [`crate::AppliedRelationUpdate`]'s `update_index` refers to a position
    /// in *this* vector.
    #[serde(default)]
    pub entity_updates: Vec<EntityUpdateRecord>,
    /// The derived, non-destructive result of applying every entity update —
    /// never merged, always recomputed globally, exactly like
    /// `entity_backlinks`. `entities` itself is never touched by this: read
    /// through [`Self::effective_attribute`]/
    /// [`Self::effective_relation_targets`] rather than directly, or an
    /// update's effect will be invisible to you.
    #[serde(default)]
    pub entity_update_history: BTreeMap<EntityId, EntityFieldHistory>,
}

impl ProjectIndex {
    /// Registers a domain object's definition under its qualified name,
    /// keyed further by its own object type — shared by
    /// `analyzer::index_domain_object` and by tests, so both always agree on
    /// how a `domain_objects` entry is shaped. Last-writer-wins if the same
    /// `(qualified_name, object_type)` pair is inserted twice.
    pub fn insert_domain_object(
        &mut self,
        object_type: ObjectType,
        qualified_name: &str,
        doc_path: impl Into<String>,
    ) {
        self.domain_objects
            .entry(TargetName::new(qualified_name))
            .or_default()
            .insert(object_type, doc_path.into());
    }

    /// The current value of one attribute on one entity, after every applied
    /// `.. entity-update::`/`.. needextend::` — the entity's own declared
    /// value when nothing touched this field, else the derived history's
    /// current value.
    ///
    /// Never mutates anything: `entities[id].attributes` stays exactly as
    /// authored. Every reader of an entity's attribute values — the built-in
    /// rendering, a custom template, `EntitySubject`'s filter evaluation —
    /// must go through this rather than `entities` directly, or an update's
    /// effect will be invisible to it. See
    /// `docs/decisions/019-entity-update.md`.
    #[must_use]
    pub fn effective_attribute(&self, id: &EntityId, field: &str) -> Option<&AttributeValue> {
        if let Some(history) = self
            .entity_update_history
            .get(id)
            .and_then(|h| h.attributes.get(field))
        {
            return history.current.as_ref();
        }
        self.entities.get(id)?.attributes.get(field)
    }

    /// The current outgoing targets of one relation on one entity, after
    /// every applied `.. entity-update::`/`.. needextend::` — same rule as
    /// [`Self::effective_attribute`].
    #[must_use]
    pub fn effective_relation_targets(&self, id: &EntityId, relation: &str) -> &[EntityId] {
        if let Some(history) = self
            .entity_update_history
            .get(id)
            .and_then(|h| h.relations.get(relation))
        {
            return &history.current;
        }
        self.entities.get(id).map_or(&[], |r| r.targets(relation))
    }

    /// Merge another `ProjectIndex` into this one.
    ///
    /// Emits a diagnostic string for each glossary term defined in both indices
    /// (case-insensitive duplicate detection). Last-writer-wins for the mapping value.
    pub fn merge(&mut self, other: Self) -> MergeConflicts {
        self.targets.extend(other.targets);
        self.document_titles.extend(other.document_titles);
        self.document_outlines.extend(other.document_outlines);
        self.toctrees.extend(other.toctrees);
        for (name, object_types) in other.domain_objects {
            self.domain_objects
                .entry(name)
                .or_default()
                .extend(object_types);
        }
        self.genindex_entries.extend(other.genindex_entries);
        self.equations.extend(other.equations);
        self.sectnum.extend(other.sectnum);
        // Accumulates like `genindex_entries`; applying it is a later phase's
        // job, not this merge's.
        self.entity_updates.extend(other.entity_updates);
        // root_documents, page_order, section_numbers, entity_backlinks and
        // entity_update_history are built globally from the whole graph, so
        // they are recomputed rather than merged
        let mut conflicts = MergeConflicts::default();
        for (id, record) in other.entities {
            if let Some(existing) = self.entities.get(&id) {
                conflicts.duplicate_entity_ids.push(DuplicateEntityId {
                    id: id.clone(),
                    first_doc: existing.doc_path.clone(),
                    second_doc: record.doc_path.clone(),
                });
            }
            self.entities.insert(id, record);
        }
        for (term, path) in other.glossary_terms {
            if let Some(existing) = self.glossary_terms.get(&term) {
                conflicts.duplicate_glossary_terms.push(format!(
                    "Duplicate glossary term '{}': defined in '{}' and '{}'. The latter definition wins.",
                    term.as_str(),
                    existing,
                    path,
                ));
            }
            self.glossary_terms.insert(term, path);
        }
        conflicts
    }
}

/// What a [`ProjectIndex::merge`] found defined twice.
///
/// Two fields rather than one list of messages, because the two are reported
/// differently: a duplicate entity id becomes a coded diagnostic attributed to
/// a document, while duplicate glossary terms have no diagnostic code yet and
/// are still only a message. Keeping them apart means neither can be
/// mislabelled as the other.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MergeConflicts {
    pub duplicate_entity_ids: Vec<DuplicateEntityId>,
    pub duplicate_glossary_terms: Vec<String>,
}

impl MergeConflicts {
    /// Reports whether the merge found nothing defined twice.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.duplicate_entity_ids.is_empty() && self.duplicate_glossary_terms.is_empty()
    }
}

/// One entity id claimed by two documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateEntityId {
    pub id: EntityId,
    /// The document that defined it first, and whose definition is discarded.
    pub first_doc: String,
    /// The document merged in second, whose definition wins.
    pub second_doc: String,
}

impl DuplicateEntityId {
    /// The author-facing explanation, naming both documents and which won.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "Duplicate entity id '{}': defined in '{}' and '{}'. The latter definition wins.",
            self.id.as_str(),
            self.first_doc,
            self.second_doc,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{CObjectType, Domain, PyObjectType};

    /// Looks up a domain object by the pre-refactor flat `"domain:objtype:name"`
    /// key shape (e.g. `"py:function:greet"`), so test expectations can stay
    /// expressed as a single string instead of repeating two-level map
    /// navigation at every call site below. Duplicated from
    /// `rusty_sphinx_analyzer`'s own test-only helper of the same name — that
    /// copy backs `analyze()`/`build_project_index()` tests which stay in
    /// `analyzer`, this one backs `ProjectIndex`-only tests.
    fn lookup_domain_object<'a>(index: &'a ProjectIndex, flat_key: &str) -> Option<&'a String> {
        let mut parts = flat_key.splitn(3, ':');
        let domain: Domain = parts.next()?.parse().ok()?;
        let objtype_str = parts.next()?;
        let name = parts.next()?;
        let object_type = ObjectType::from_directive_name(domain, objtype_str)?;
        index
            .domain_objects
            .get(&TargetName::new(name))?
            .get(&object_type)
    }

    #[test]
    fn test_lookup_domain_object_finds_inserted_entry() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "greet", "api.rst");

        // When / Then
        assert_eq!(
            lookup_domain_object(&index, "py:function:greet"),
            Some(&"api.rst".to_string())
        );
    }

    #[test]
    fn test_lookup_domain_object_returns_none_for_missing_name() {
        // Given
        let index = ProjectIndex::default();

        // When / Then
        assert_eq!(lookup_domain_object(&index, "py:function:greet"), None);
    }

    #[test]
    fn test_lookup_domain_object_returns_none_when_name_present_under_different_objtype() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Class), "Fault", "xmlrpc.rst");

        // When / Then
        assert_eq!(lookup_domain_object(&index, "py:function:Fault"), None);
    }

    #[test]
    fn test_merge_carries_toctrees_and_outlines_from_the_other_index() {
        // Given — the live-preview shape: a stale global index merged with a
        // freshly analyzed local one. Both fields must survive, or the preview
        // renders navigation the edited document no longer describes.
        let mut stale = ProjectIndex::default();
        stale.toctrees.insert(
            "index.rst".to_string(),
            vec![crate::DocumentToctree {
                toctree: rusty_sphinx_ast::Toctree::default(),
                section: None,
            }],
        );

        let mut fresh = ProjectIndex::default();
        fresh.toctrees.insert(
            "guide.rst".to_string(),
            vec![crate::DocumentToctree {
                toctree: rusty_sphinx_ast::Toctree::default(),
                section: None,
            }],
        );
        fresh.document_outlines.insert(
            "guide.rst".to_string(),
            crate::DocumentOutline {
                sections: vec![crate::OutlineSection {
                    title: "Setup".to_string(),
                    id: rusty_sphinx_ast::SectionId::from_title("Setup"),
                    children: Vec::new(),
                }],
            },
        );

        // When
        let diagnostics = stale.merge(fresh);

        // Then
        assert!(diagnostics.is_empty());
        assert!(stale.toctrees.contains_key("index.rst"));
        assert!(stale.toctrees.contains_key("guide.rst"));
        assert_eq!(stale.document_outlines["guide.rst"].sections.len(), 1);
    }

    #[test]
    fn test_merge_carries_sectnum_options_from_the_other_index() {
        // Given
        let mut stale = ProjectIndex::default();
        let mut fresh = ProjectIndex::default();
        fresh
            .sectnum
            .insert("guide.rst".to_string(), SectnumOptions::default());

        // When
        stale.merge(fresh);

        // Then
        assert!(stale.sectnum.contains_key("guide.rst"));
    }

    #[test]
    fn test_merge_combines_indices_without_error() {
        // Given
        let mut idx1 = ProjectIndex::default();
        let idx2 = ProjectIndex::default();

        // When
        idx1.merge(idx2);

        // Then
        // Since we don't have fields to assert equality on right now,
        // we just ensure the execution path is hit without issues.
        let _ = format!("{idx1:?}");
    }

    #[test]
    fn test_merge_combines_glossary_terms_from_two_documents() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.glossary_terms
            .insert(TargetName::new("foo"), "glossary_a.rst".to_string());

        let mut idx2 = ProjectIndex::default();
        idx2.glossary_terms
            .insert(TargetName::new("bar"), "glossary_b.rst".to_string());

        // When
        let diagnostics = idx1.merge(idx2);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(idx1.glossary_terms.len(), 2);
        assert!(idx1.glossary_terms.contains_key(&TargetName::new("foo")));
        assert!(idx1.glossary_terms.contains_key(&TargetName::new("bar")));
    }

    #[test]
    fn test_merge_emits_diagnostic_for_duplicate_glossary_term() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        let mut idx2 = ProjectIndex::default();
        idx2.glossary_terms
            .insert(TargetName::new("environment"), "other.rst".to_string());

        // When
        let conflicts = idx1.merge(idx2);

        // Then
        assert_eq!(conflicts.duplicate_glossary_terms.len(), 1);
        assert!(conflicts.duplicate_glossary_terms[0].contains("Duplicate glossary term"));
        assert!(conflicts.duplicate_glossary_terms[0].contains("environment"));
        // Last-writer-wins: idx2's path should be kept
        assert_eq!(
            idx1.glossary_terms.get(&TargetName::new("environment")),
            Some(&"other.rst".to_string())
        );
    }

    #[test]
    fn test_merge_combines_domain_objects_from_two_documents() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.insert_domain_object(ObjectType::Py(PyObjectType::Function), "foo", "a.rst");

        let mut idx2 = ProjectIndex::default();
        idx2.insert_domain_object(ObjectType::C(CObjectType::Function), "bar", "b.rst");

        // When
        idx1.merge(idx2);

        // Then
        assert_eq!(idx1.domain_objects.len(), 2);
        assert!(lookup_domain_object(&idx1, "py:function:foo").is_some());
        assert!(lookup_domain_object(&idx1, "c:function:bar").is_some());
    }

    #[test]
    fn test_merge_combines_domain_objects_with_different_object_types_for_same_name() {
        // Given — mirrors CPython's `xmlrpc.client.rst`: one document defines
        // `Fault` via `.. class::`, another (hypothetically) documents it via
        // `.. exception::` — merge must keep both coexisting under the same
        // qualified name rather than one clobbering the other.
        let mut idx1 = ProjectIndex::default();
        idx1.insert_domain_object(ObjectType::Py(PyObjectType::Class), "Fault", "a.rst");

        let mut idx2 = ProjectIndex::default();
        idx2.insert_domain_object(ObjectType::Py(PyObjectType::Exception), "Fault", "b.rst");

        // When
        idx1.merge(idx2);

        // Then
        assert_eq!(idx1.domain_objects.len(), 1);
        assert_eq!(
            lookup_domain_object(&idx1, "py:class:Fault"),
            Some(&"a.rst".to_string())
        );
        assert_eq!(
            lookup_domain_object(&idx1, "py:exception:Fault"),
            Some(&"b.rst".to_string())
        );
    }

    #[test]
    fn test_merge_accumulates_genindex_entries_from_two_documents() {
        // Given
        let mut idx1 = ProjectIndex::default();
        idx1.genindex_entries.push(GenIndexEntry {
            primary: "foo".to_string(),
            subentry: None,
            main: false,
            doc_path: "a.rst".to_string(),
            anchor: "index-0".to_string(),
        });

        let mut idx2 = ProjectIndex::default();
        idx2.genindex_entries.push(GenIndexEntry {
            primary: "foo".to_string(),
            subentry: None,
            main: false,
            doc_path: "b.rst".to_string(),
            anchor: "index-0".to_string(),
        });

        // When
        let diagnostics = idx1.merge(idx2);

        // Then — both locations kept, no dedup/diagnostics
        assert!(diagnostics.is_empty());
        assert_eq!(idx1.genindex_entries.len(), 2);
    }

    #[test]
    fn test_merge_duplicate_detection_is_case_insensitive() {
        // Given — "Environment" and "environment" should collide
        let mut idx1 = ProjectIndex::default();
        idx1.glossary_terms
            .insert(TargetName::new("Environment"), "a.rst".to_string());

        let mut idx2 = ProjectIndex::default();
        idx2.glossary_terms
            .insert(TargetName::new("environment"), "b.rst".to_string());

        // When
        let conflicts = idx1.merge(idx2);

        // Then
        assert_eq!(
            conflicts.duplicate_glossary_terms.len(),
            1,
            "Expected duplicate diagnostic"
        );
    }

    #[test]
    fn test_project_index_round_trips_through_json() {
        // Given — a populated index exercising every field
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("section-1"),
            TargetLocation::Internal("api.rst".to_string()),
        );
        index
            .document_titles
            .insert("api.rst".to_string(), "API".to_string());
        index.root_documents.push("api.rst".to_string());
        index.page_order.push("api.rst".to_string());
        index.toctrees.insert(
            "api.rst".to_string(),
            vec![DocumentToctree {
                toctree: rusty_sphinx_ast::Toctree::default(),
                section: None,
            }],
        );
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "greet", "api.rst");
        index.genindex_entries.push(GenIndexEntry {
            primary: "greet (function)".to_string(),
            subentry: None,
            main: false,
            doc_path: "api.rst".to_string(),
            anchor: "py:function:greet".to_string(),
        });
        index.sectnum.insert(
            "api.rst".to_string(),
            SectnumOptions {
                prefix: "Appendix ".to_string(),
                ..SectnumOptions::default()
            },
        );
        index.entity_updates.push(crate::EntityUpdateRecord {
            doc_path: "api.rst".to_string(),
            update: rusty_sphinx_ast::EntityUpdate::new(
                rusty_sphinx_ast::EntityUpdateSource::EntityUpdate,
                rusty_sphinx_ast::UpdateTarget {
                    candidate_id: EntityId::new("REQ_001").ok(),
                    filter: None,
                    raw: "REQ_001".to_string(),
                },
            ),
        });

        // When — serialize and deserialize
        let json = serde_json::to_string(&index).unwrap();
        let deserialized: ProjectIndex = serde_json::from_str(&json).unwrap();

        // Then — round-trips correctly
        assert_eq!(deserialized, index);
    }

    #[test]
    fn test_project_index_deserializes_when_optional_fields_are_missing() {
        // Given — a JSON blob only carrying the two non-`#[serde(default)]`
        // fields, matching an older on-disk `.index` format written before
        // the toctree graph, glossary terms, domain objects and genindex
        // entries existed.
        let json = r#"{"targets": {}, "document_titles": {}}"#;

        // When
        let index: ProjectIndex = serde_json::from_str(json).unwrap();

        // Then — missing fields default to empty rather than failing to parse
        assert!(index.toctrees.is_empty());
        assert!(index.root_documents.is_empty());
        assert!(index.page_order.is_empty());
        assert!(index.glossary_terms.is_empty());
        assert!(index.domain_objects.is_empty());
        assert!(index.genindex_entries.is_empty());
        assert!(index.sectnum.is_empty());
        assert!(index.entity_updates.is_empty());
        assert!(index.entity_update_history.is_empty());
    }

    fn requirement_record() -> EntityRecord {
        EntityRecord {
            type_name: "req".to_string(),
            doc_path: "specs/boot.rst".to_string(),
            title: None,
            attributes: BTreeMap::from([(
                "status".to_string(),
                AttributeValue::String("open".to_string()),
            )]),
            outgoing: BTreeMap::from([(
                "links".to_string(),
                vec![EntityId::new("SPEC_001").unwrap()],
            )]),
            uml: BTreeMap::new(),
        }
    }

    #[test]
    fn test_effective_attribute_falls_back_to_the_record_when_untouched() {
        // Given — no history entry at all for this entity
        let mut index = ProjectIndex::default();
        let id = EntityId::new("REQ_001").unwrap();
        index.entities.insert(id.clone(), requirement_record());

        // When
        let value = index.effective_attribute(&id, "status");

        // Then
        assert_eq!(value, Some(&AttributeValue::String("open".to_string())));
    }

    #[test]
    fn test_effective_attribute_prefers_the_history_current_value() {
        // Given — an update has touched `status`
        let mut index = ProjectIndex::default();
        let id = EntityId::new("REQ_001").unwrap();
        index.entities.insert(id.clone(), requirement_record());
        let mut history = EntityFieldHistory::default();
        history.attributes.insert(
            "status".to_string(),
            crate::AttributeFieldHistory {
                original: Some(AttributeValue::String("open".to_string())),
                applied: Vec::new(),
                current: Some(AttributeValue::String("closed".to_string())),
            },
        );
        index.entity_update_history.insert(id.clone(), history);

        // When
        let value = index.effective_attribute(&id, "status");

        // Then — the record itself is never touched
        assert_eq!(value, Some(&AttributeValue::String("closed".to_string())));
        assert_eq!(
            index.entities[&id].attributes["status"],
            AttributeValue::String("open".to_string())
        );
    }

    #[test]
    fn test_effective_attribute_is_none_when_the_history_cleared_the_field() {
        // Given
        let mut index = ProjectIndex::default();
        let id = EntityId::new("REQ_001").unwrap();
        index.entities.insert(id.clone(), requirement_record());
        let mut history = EntityFieldHistory::default();
        history.attributes.insert(
            "status".to_string(),
            crate::AttributeFieldHistory {
                original: Some(AttributeValue::String("open".to_string())),
                applied: Vec::new(),
                current: None,
            },
        );
        index.entity_update_history.insert(id.clone(), history);

        // When
        let value = index.effective_attribute(&id, "status");

        // Then
        assert_eq!(value, None);
    }

    #[test]
    fn test_effective_relation_targets_falls_back_to_the_record_when_untouched() {
        // Given
        let mut index = ProjectIndex::default();
        let id = EntityId::new("REQ_001").unwrap();
        index.entities.insert(id.clone(), requirement_record());

        // When
        let targets = index.effective_relation_targets(&id, "links");

        // Then
        assert_eq!(targets, [EntityId::new("SPEC_001").unwrap()]);
    }

    #[test]
    fn test_effective_relation_targets_prefers_the_history_current_targets() {
        // Given
        let mut index = ProjectIndex::default();
        let id = EntityId::new("REQ_001").unwrap();
        index.entities.insert(id.clone(), requirement_record());
        let mut history = EntityFieldHistory::default();
        history.relations.insert(
            "links".to_string(),
            crate::RelationFieldHistory {
                original: vec![EntityId::new("SPEC_001").unwrap()],
                applied: Vec::new(),
                current: vec![
                    EntityId::new("SPEC_001").unwrap(),
                    EntityId::new("SPEC_002").unwrap(),
                ],
            },
        );
        index.entity_update_history.insert(id.clone(), history);

        // When
        let targets = index.effective_relation_targets(&id, "links");

        // Then — the record itself is never touched
        assert_eq!(
            targets,
            [
                EntityId::new("SPEC_001").unwrap(),
                EntityId::new("SPEC_002").unwrap()
            ]
        );
        assert_eq!(
            index.entities[&id].outgoing["links"],
            [EntityId::new("SPEC_001").unwrap()]
        );
    }

    #[test]
    fn test_effective_attribute_is_none_for_an_entity_that_does_not_exist() {
        // Given
        let index = ProjectIndex::default();
        let id = EntityId::new("REQ_999").unwrap();

        // When / Then
        assert_eq!(index.effective_attribute(&id, "status"), None);
    }

    #[test]
    fn test_merge_accumulates_entity_updates_from_both_indices() {
        // Given
        let mut stale = ProjectIndex::default();
        stale.entity_updates.push(crate::EntityUpdateRecord {
            doc_path: "a.rst".to_string(),
            update: rusty_sphinx_ast::EntityUpdate::new(
                rusty_sphinx_ast::EntityUpdateSource::EntityUpdate,
                rusty_sphinx_ast::UpdateTarget {
                    candidate_id: EntityId::new("REQ_001").ok(),
                    filter: None,
                    raw: "REQ_001".to_string(),
                },
            ),
        });
        let mut fresh = ProjectIndex::default();
        fresh.entity_updates.push(crate::EntityUpdateRecord {
            doc_path: "b.rst".to_string(),
            update: rusty_sphinx_ast::EntityUpdate::new(
                rusty_sphinx_ast::EntityUpdateSource::NeedExtend,
                rusty_sphinx_ast::UpdateTarget {
                    candidate_id: EntityId::new("REQ_002").ok(),
                    filter: None,
                    raw: "REQ_002".to_string(),
                },
            ),
        });

        // When
        stale.merge(fresh);

        // Then
        assert_eq!(stale.entity_updates.len(), 2);
    }
}

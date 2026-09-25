//! The entity graph as a template sees it.
//!
//! An owned copy rather than a borrow, for one unavoidable reason: `MiniJinja`
//! functions must be `Send + Sync + 'static`, so nothing a closure captures may
//! borrow the [`ProjectIndex`] the caller holds. Everything a template can ask
//! about is therefore materialized here first and shared into the functions
//! behind an `Arc`.
//!
//! That cost is paid only by a *templated* diagram — [`expand`](crate::expand)
//! returns before building one for a plain `.. plantuml::` — and per diagram
//! rather than per document, which is the honest trade for keeping
//! [`expand`](crate::expand) a function of its arguments alone.
//!
//! What each field *name* is worth is never decided here. Every value comes
//! from [`EntitySubject`], the one authority both a listing directive's
//! `:filter:` and a diagram's `filter()` resolve through — this module only
//! records its answers so they outlive the borrow.

use std::collections::BTreeMap;

use minijinja::Value;
use rusty_sphinx_ast::EntityId;
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_filter::{FieldName, FieldValue, FilterSubject};
use rusty_sphinx_index::{
    EntityRecord, EntitySubject, ProjectIndex, entity_anchor, relative_doc_href,
};

/// One entity's fields, resolved once and owned.
///
/// A flat namespace, because sphinx-needs' own templates write `need.status`
/// and `need.links` with attributes and relations sharing the namespace the
/// directive's options do. A [`BTreeMap`] rather than a hash map so iteration
/// order is stable — the expansion's bytes are hashed, so an unstable order
/// would produce a page pointing at an SVG nothing compiled.
#[derive(Debug, Clone)]
pub(crate) struct EntityFacts {
    fields: BTreeMap<String, FieldValue>,
}

impl EntityFacts {
    /// The fields as a template reads them.
    fn template_value(&self) -> Value {
        Value::from(
            self.fields
                .iter()
                .map(|(name, value)| (name.clone(), template_value(value)))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    /// The text this entity is shown by — its title, falling back to its id.
    fn display_text(&self) -> &str {
        match self.fields.get("title") {
            Some(FieldValue::Text(title)) => title,
            _ => self.text("id"),
        }
    }

    /// One field's value as plain text, empty when it is not text.
    fn text(&self, name: &str) -> &str {
        match self.fields.get(name) {
            Some(FieldValue::Text(value)) => value,
            _ => "",
        }
    }
}

/// A filter evaluates against the recorded answers, so a `filter()` in a
/// diagram and a `:filter:` in a table select the same entities.
impl FilterSubject for EntityFacts {
    fn field(&self, name: &FieldName) -> FieldValue {
        self.fields
            .get(name.as_str())
            .cloned()
            .unwrap_or(FieldValue::Missing)
    }
}

/// Everything a template may ask the project, owned.
pub(crate) struct Snapshot {
    /// Every entity in the project, by id, in id order.
    entities: BTreeMap<String, EntityFacts>,
    /// The page each entity is written on, for building links.
    doc_paths: BTreeMap<String, String>,
    /// The diagram templates written inside each entity, by `:key:`.
    umls: BTreeMap<String, BTreeMap<String, String>>,
    /// The outgoing edges of each entity, by relation name, for `imports()`.
    outgoing: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// The page doing the linking, so an href is relative to it.
    doc_path: String,
}

impl Snapshot {
    /// Materializes the whole entity graph as templates see it.
    pub(crate) fn build(index: &ProjectIndex, schema: &EntitySchema, doc_path: &str) -> Self {
        let mut entities = BTreeMap::new();
        let mut doc_paths = BTreeMap::new();
        let mut umls = BTreeMap::new();
        let mut outgoing = BTreeMap::new();

        for (id, record) in &index.entities {
            entities.insert(id.to_string(), facts_of(id, record, index, schema));
            doc_paths.insert(id.to_string(), record.doc_path.clone());
            umls.insert(id.to_string(), record.uml.clone());
            outgoing.insert(
                id.to_string(),
                record
                    .outgoing
                    .iter()
                    .map(|(relation, targets)| {
                        (
                            relation.clone(),
                            targets.iter().map(ToString::to_string).collect(),
                        )
                    })
                    .collect(),
            );
        }

        Self {
            entities,
            doc_paths,
            umls,
            outgoing,
            doc_path: doc_path.to_string(),
        }
    }

    /// Whether an entity by this id exists.
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.entities.contains_key(id)
    }

    /// One entity's fields, as a template value.
    pub(crate) fn entity(&self, id: &str) -> Option<Value> {
        self.entities.get(id).map(EntityFacts::template_value)
    }

    /// Every entity, by id — what the bare `needs` name is bound to.
    pub(crate) fn all(&self) -> Value {
        Value::from(
            self.entities
                .iter()
                .map(|(id, facts)| (id.clone(), facts.template_value()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    /// Every entity's id, in id order — what a flowchart with no `:filter:`
    /// draws.
    ///
    /// Order is not tidiness here, for the reason it is not in
    /// [`Self::matching`]: it decides the order nodes appear in the generated
    /// `PlantUML`, and so the bytes that get hashed into the compiled
    /// picture's filename.
    pub(crate) fn ids(&self) -> Vec<&String> {
        self.entities.keys().collect()
    }

    /// The ids of every entity matching `filter`, in id order.
    ///
    /// Order is not tidiness here: it decides the order nodes appear in the
    /// generated `PlantUML`, and so the bytes that get hashed.
    pub(crate) fn matching(&self, filter: &rusty_sphinx_filter::Expr) -> Vec<&String> {
        self.entities
            .iter()
            .filter(|(_, facts)| filter.matches(*facts))
            .map(|(id, _)| id)
            .collect()
    }

    /// Whether the entity `id` exists and `filter` selects it — the one-entity
    /// form of [`Self::matching`], for a walk that meets entities one at a time.
    pub(crate) fn matches(&self, id: &str, filter: &rusty_sphinx_filter::Expr) -> bool {
        self.entities
            .get(id)
            .is_some_and(|facts| filter.matches(facts))
    }

    /// The text an entity is shown by — its title, falling back to its id.
    pub(crate) fn title<'a>(&'a self, id: &'a str) -> &'a str {
        self.entities.get(id).map_or(id, EntityFacts::display_text)
    }

    /// The type an entity was declared as.
    pub(crate) fn type_name(&self, id: &str) -> &str {
        self.entities.get(id).map_or("", |facts| facts.text("type"))
    }

    /// The diagram template `id` stored under `key`, if it wrote one.
    ///
    /// An absent key is not an error here — a template importing an entity
    /// that drew nothing gets nothing, which is what lets one architecture
    /// diagram import a whole relation's targets without every one of them
    /// having to carry a diagram.
    pub(crate) fn uml_template(&self, id: &str, key: &str) -> Option<&str> {
        self.umls.get(id)?.get(key).map(String::as_str)
    }

    /// The ids `id` points at along `relation`, in the order they were written.
    pub(crate) fn targets(&self, id: &str, relation: &str) -> Vec<String> {
        self.outgoing
            .get(id)
            .and_then(|edges| edges.get(relation))
            .cloned()
            .unwrap_or_default()
    }

    /// The href from the page being rendered to an entity's anchor.
    ///
    /// Built from the very functions the renderer builds a page's own links
    /// with, so a diagram's clickable node lands exactly where a `:ref:` to
    /// the same entity would.
    pub(crate) fn href(&self, id: &str) -> Option<String> {
        let target_doc = self.doc_paths.get(id)?;
        Some(format!(
            "{}#{}",
            relative_doc_href(target_doc, &self.doc_path),
            entity_anchor(id)
        ))
    }
}

/// Every field name this entity answers to, resolved through [`EntitySubject`].
///
/// The name list is assembled from what the entity actually carries plus the
/// built-ins, because the field vocabulary is open — an attribute or relation
/// name comes from the project's own schema. What each name is *worth* is
/// still `EntitySubject`'s answer, never this module's.
fn facts_of(
    id: &EntityId,
    record: &EntityRecord,
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> EntityFacts {
    let subject = EntitySubject {
        id,
        record,
        index,
        schema,
    };

    let mut names: Vec<String> = ["id", "type", "type_name", "docname", "title"]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    names.extend(record.attributes.keys().cloned());
    names.extend(record.outgoing.keys().cloned());
    if let Some(incoming) = index.entity_backlinks.get(id) {
        names.extend(incoming.keys().cloned());
    }

    let mut fields = BTreeMap::new();
    for name in names {
        let Ok(field) = FieldName::new(&name) else {
            // A name the filter language cannot spell cannot be asked for
            // either, so there is nothing a template could do with it.
            continue;
        };
        fields.insert(name, subject.field(&field));
    }

    EntityFacts { fields }
}

/// One resolved field as a template sees it.
///
/// A missing field becomes none rather than an empty string, so `{% if
/// need.status %}` distinguishes "not set" from "set to nothing" — the same
/// distinction the filter language's `is None` makes.
fn template_value(value: &FieldValue) -> Value {
    match value {
        FieldValue::Missing => Value::from(()),
        FieldValue::Text(text) => Value::from(text.clone()),
        FieldValue::Int(number) => Value::from(*number),
        FieldValue::Bool(flag) => Value::from(*flag),
        FieldValue::List(items) => Value::from(items.clone()),
    }
}

#[cfg(test)]
pub(crate) mod tests;

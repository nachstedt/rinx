//! Resolving a filter's field names against one entity of the project index.
//!
//! In this crate rather than beside the listing directive that first needed
//! it, because two phases that may not depend on each other now resolve the
//! same names: the **renderer**, for an `.. entity-table::`'s `:filter:`, and
//! **`rinx_uml`**, for a diagram template's `filter()`. A filter
//! meaning one thing in a table and another in a diagram would be a bug no
//! test in either crate could see, and a second copy of these rules is the
//! only way to get there.
//!
//! `rinx_entity::field` owns *which* names exist, because the parser
//! must check them without an index in hand. This owns what each one is
//! *worth*, because that needs a record. Splitting the two is what keeps the
//! parser's check and this lookup from drifting: a name that validates there
//! resolves here, and neither side holds a second copy of the list.

use crate::{EntityRecord, ProjectIndex};
use rinx_ast::EntityId;
use rinx_entity::EntitySchema;
use rinx_filter::{FieldName, FieldValue, FilterSubject};

/// One entity, as a filter and a table cell see it.
pub struct EntitySubject<'a> {
    pub id: &'a EntityId,
    pub record: &'a EntityRecord,
    pub index: &'a ProjectIndex,
    pub schema: &'a EntitySchema,
}

impl EntitySubject<'_> {
    /// The type's human label, falling back to its directive name.
    ///
    /// `type_name` in sphinx-needs' vocabulary — the label, where `type` is
    /// the directive name. The pairing reads backwards and is kept anyway, so
    /// a migrating project's filters mean here what they meant there.
    fn type_label(&self) -> String {
        self.schema
            .entity_type(&self.record.type_name)
            .map_or(self.record.type_name.clone(), |declared| {
                declared.display_label().to_string()
            })
    }

    /// The ids pointing *at* this entity under `name`, if that is a back-link.
    fn backlink_targets(&self, name: &str) -> Option<Vec<String>> {
        self.index
            .entity_backlinks
            .get(self.id)
            .and_then(|links| links.get(name))
            .map(|ids| ids.iter().map(ToString::to_string).collect())
    }
}

impl FilterSubject for EntitySubject<'_> {
    fn field(&self, name: &FieldName) -> FieldValue {
        match name.as_str() {
            "id" => FieldValue::Text(self.id.to_string()),
            "type" => FieldValue::Text(self.record.type_name.clone()),
            "type_name" => FieldValue::Text(self.type_label()),
            "docname" => FieldValue::Text(self.record.doc_path.clone()),
            // The title itself, not `display_text`'s id fallback: a filter
            // asking `title is None` is asking whether one was written.
            "title" => self
                .record
                .title
                .clone()
                .map_or(FieldValue::Missing, FieldValue::Text),
            other => self.declared_field(other),
        }
    }
}

impl EntitySubject<'_> {
    /// A field the schema declares: an attribute, an outgoing relation, or a
    /// derived back-link, in that order.
    ///
    /// The order is the schema loader's own: a back-link may not collide with
    /// anything the target type declares itself, so at most one of the three
    /// can match and the precedence never actually arbitrates.
    ///
    /// Attributes and outgoing relations are read through
    /// [`ProjectIndex::effective_attribute`]/
    /// [`ProjectIndex::effective_relation_targets`] rather than off
    /// `self.record` directly — the one change that makes every filter
    /// evaluation already built on `EntitySubject` (`entity-table`,
    /// `entity-flow`, `entity-pie`, a diagram's `filter()`, and
    /// `apply_entity_updates`'s own target resolution) see the effect of a
    /// `.. entity-update::`/`.. needextend::` automatically. Back-links are
    /// unaffected by updates, so they still read `self.index.entity_backlinks`
    /// directly.
    fn declared_field(&self, name: &str) -> FieldValue {
        let entity_type = self.schema.entity_type(&self.record.type_name);
        if let Some(value) = self.index.effective_attribute(self.id, name) {
            return FieldValue::from(value);
        }
        if entity_type.is_some_and(|t| t.relation(name).is_some()) {
            let targets = self.index.effective_relation_targets(self.id, name);
            return FieldValue::List(targets.iter().map(ToString::to_string).collect());
        }
        self.backlink_targets(name)
            .map_or(FieldValue::Missing, FieldValue::List)
    }
}

#[cfg(test)]
mod tests;

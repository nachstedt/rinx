//! Resolving a filter's field names against one entity of the project index.
//!
//! `rusty_sphinx_entity::field` owns *which* names exist, because the parser
//! must check them without an index in hand. This owns what each one is
//! *worth*, because that needs a record. Splitting the two is what keeps the
//! parser's check and this lookup from drifting: a name that validates there
//! resolves here, and neither side holds a second copy of the list.

use rusty_sphinx_ast::{AttributeValue, EntityId};
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_filter::{FieldName, FieldValue, FilterSubject};
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

/// One entity, as a filter and a table cell see it.
pub(crate) struct EntitySubject<'a> {
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
    fn declared_field(&self, name: &str) -> FieldValue {
        if let Some(value) = self.record.attributes.get(name) {
            return attribute_value(value);
        }
        if let Some(targets) = self.record.outgoing.get(name) {
            return FieldValue::List(targets.iter().map(ToString::to_string).collect());
        }
        self.backlink_targets(name)
            .map_or(FieldValue::Missing, FieldValue::List)
    }
}

/// An entity's stored attribute value as the filter language sees it.
fn attribute_value(value: &AttributeValue) -> FieldValue {
    match value {
        AttributeValue::String(text) => FieldValue::Text(text.clone()),
        AttributeValue::Int(number) => FieldValue::Int(*number),
        AttributeValue::Bool(flag) => FieldValue::Bool(*flag),
        AttributeValue::List(items) => FieldValue::List(items.clone()),
    }
}

#[cfg(test)]
mod tests;

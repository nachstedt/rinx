//! The project every chart-counting test counts against.
//!
//! Its own module because a pie's wedges, a bar chart's cells and the shared
//! counting underneath both are tested against the same small graph, and a
//! divergence between three copies of it would make their results
//! incomparable.
#![cfg(test)]

use std::collections::BTreeMap;

use rinx_ast::{AttributeValue, EntityId};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rinx_index::{EntityRecord, ProjectIndex};

/// Two types — `req` with a `status`, and `test` verifying it.
pub(super) fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
          [[entity_type.attribute]]
          name = "status"
          type = "string"

        [[entity_type]]
        name = "test"
          [[entity_type.relation]]
          name = "verifies"
          to = ["req"]
          incoming = "verified_by"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

/// An entity id the fixture knows to be legal.
fn id(raw: &str) -> EntityId {
    EntityId::new(raw).unwrap()
}

/// A requirement with `status`, declared in `doc`.
fn requirement(status: &str, doc: &str) -> EntityRecord {
    EntityRecord {
        type_name: "req".to_string(),
        doc_path: doc.to_string(),
        title: Some("A requirement".to_string()),
        attributes: BTreeMap::from([(
            "status".to_string(),
            AttributeValue::String(status.to_string()),
        )]),
        outgoing: BTreeMap::new(),
        uml: BTreeMap::new(),
    }
}

/// Three requirements — two open, one closed — and one test.
pub(super) fn index() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_1"), requirement("open", "specs/boot"));
    index
        .entities
        .insert(id("REQ_2"), requirement("open", "specs/boot"));
    index
        .entities
        .insert(id("REQ_3"), requirement("closed", "other/power"));
    index.entities.insert(
        id("TEST_1"),
        EntityRecord {
            type_name: "test".to_string(),
            doc_path: "tests/boot".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::from([("verifies".to_string(), vec![id("REQ_1")])]),
            uml: BTreeMap::new(),
        },
    );
    index
}

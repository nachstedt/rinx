//! What hovering an entity shows: its fields, as its box on the page lists
//! them.
//!
//! The fields are `rinx_index::EntityFields`, the list the renderer's
//! built-in box draws, so the hover shows a field exactly when the page does,
//! under the same label and with every `.. entity-update::` applied. Only the
//! presentation is the editor's: Markdown lines rather than a table, since not
//! every client's hover renders one.

use std::fmt::Write as _;

use rinx_ast::{EntityBody, EntityId};
use rinx_entity::EntitySchema;
use rinx_index::{EntityFields, OwnFields, ProjectIndex, ShownLinks};

use crate::hover::escape_markdown;

/// How many ids a relation or back-link lists before it says how many more
/// there are: an entity many others trace to would otherwise fill the screen.
const LISTED_IDS: usize = 20;

/// The hover text for the entity `id` the project index holds, below
/// `location` — the line naming where it is — or `None` when the index holds
/// no such entity.
#[must_use]
pub fn indexed_entity_markdown(
    id: &EntityId,
    index: &ProjectIndex,
    schema: &EntitySchema,
    location: &str,
) -> Option<String> {
    let record = index.entities.get(id)?;
    let own = OwnFields {
        attributes: &record.attributes,
        relations: &record.outgoing,
    };
    let fields = EntityFields::of(id, &record.type_name, own, index, schema);
    Some(entity_markdown(
        id,
        record.title.as_deref(),
        &fields,
        Some(location),
    ))
}

/// The hover text for `entity`, on its own directive: its fields as written
/// where the index holds no record of it — an unsaved one, or one whose id
/// another document declares too — and with no location, being where the
/// reader already is.
#[must_use]
pub fn declared_entity_markdown(
    entity: &EntityBody,
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> String {
    let own = OwnFields {
        attributes: &entity.attributes,
        relations: &entity.relations,
    };
    let fields = EntityFields::of(entity.id.value(), &entity.type_name, own, index, schema);
    entity_markdown(entity.id.value(), entity.title().as_deref(), &fields, None)
}

/// The hover text for the entity `id`, titled `title`, showing `fields`
/// below `location`, when there is one.
#[must_use]
pub fn entity_markdown(
    id: &EntityId,
    title: Option<&str>,
    fields: &EntityFields<'_>,
    location: Option<&str>,
) -> String {
    let mut text = format!(
        "**{}** {}",
        escape_markdown(fields.type_label),
        escape_markdown(id.as_str())
    );
    if let Some(title) = title {
        let _ = write!(text, " — {}", escape_markdown(title));
    }
    if let Some(location) = location {
        let _ = write!(text, "\n\n{location}");
    }

    let attributes: Vec<String> = fields
        .attributes
        .iter()
        .map(|attribute| {
            field_line(
                attribute.label,
                &escape_markdown(&attribute.value.to_string()),
                attribute.conflict,
            )
        })
        .collect();
    let links: Vec<String> = fields
        .relations
        .iter()
        .chain(&fields.backlinks)
        .map(links_line)
        .collect();
    for block in [attributes, links] {
        if !block.is_empty() {
            let _ = write!(text, "\n\n{}", block.join("\n"));
        }
    }
    text
}

/// One list item: `label`, then `value`, already escaped, noting when
/// conflicting updates dispute it.
fn field_line(label: &str, value: &str, conflict: bool) -> String {
    let note = if conflict {
        " *(set by conflicting updates)*"
    } else {
        ""
    };
    format!("- **{}:** {value}{note}", escape_markdown(label))
}

/// The list item of one relation or back-link: its ids, cut after
/// [`LISTED_IDS`].
fn links_line(links: &ShownLinks<'_>) -> String {
    let mut ids: Vec<String> = links
        .targets
        .iter()
        .take(LISTED_IDS)
        .map(|id| escape_markdown(id.as_str()))
        .collect();
    let more = links.targets.len().saturating_sub(LISTED_IDS);
    if more > 0 {
        ids.push(format!("and {more} more"));
    }
    field_line(links.label, &ids.join(", "), links.conflict)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rinx_ast::AttributeValue;
    use rinx_entity::{NoReservedNames, load_schema};
    use rinx_index::{EntityRecord, ShownAttribute};

    use super::*;

    fn id(raw: &str) -> EntityId {
        EntityId::new(raw).unwrap()
    }

    fn fields<'a>(
        attributes: Vec<ShownAttribute<'a>>,
        relations: Vec<ShownLinks<'a>>,
        backlinks: Vec<ShownLinks<'a>>,
    ) -> EntityFields<'a> {
        EntityFields {
            type_label: "Requirement",
            attributes,
            relations,
            backlinks,
        }
    }

    #[test]
    fn test_entity_markdown_heads_the_fields_with_type_id_title_and_location() {
        // Given
        let status = AttributeValue::String("open".to_string());
        let targets = [id("SPEC_1"), id("SPEC_2")];
        let shown = fields(
            vec![ShownAttribute {
                name: "status",
                label: "Status",
                value: &status,
                conflict: false,
            }],
            vec![ShownLinks {
                label: "Links to",
                targets: &targets,
                conflict: false,
            }],
            Vec::new(),
        );

        // When
        let text = entity_markdown(&id("REQ_1"), Some("Boot fast"), &shown, Some("reqs\\.rst"));

        // Then
        assert_eq!(
            text,
            "**Requirement** REQ\\_1 — Boot fast\n\nreqs\\.rst\n\n\
             - **Status:** open\n\n\
             - **Links to:** SPEC\\_1, SPEC\\_2"
        );
    }

    #[test]
    fn test_entity_markdown_has_no_title_location_or_empty_blocks_it_does_not_need() {
        // Given
        let shown = fields(Vec::new(), Vec::new(), Vec::new());

        // When
        let text = entity_markdown(&id("os.system"), None, &shown, None);

        // Then
        assert_eq!(text, "**Requirement** os\\.system");
    }

    #[test]
    fn test_entity_markdown_notes_a_value_conflicting_updates_dispute() {
        // Given
        let status = AttributeValue::String("closed".to_string());
        let shown = fields(
            vec![ShownAttribute {
                name: "status",
                label: "Status",
                value: &status,
                conflict: true,
            }],
            Vec::new(),
            Vec::new(),
        );

        // When
        let text = entity_markdown(&id("REQ_1"), None, &shown, None);

        // Then
        assert!(
            text.ends_with("- **Status:** closed *(set by conflicting updates)*"),
            "{text}"
        );
    }

    #[test]
    fn test_links_line_cuts_a_long_list_and_says_how_many_more() {
        // Given
        let targets: Vec<EntityId> = (0..23).map(|n| id(&format!("T{n}"))).collect();
        let links = ShownLinks {
            label: "Verified by",
            targets: &targets,
            conflict: false,
        };

        // When
        let line = links_line(&links);

        // Then
        assert!(line.starts_with("- **Verified by:** T0, T1, "), "{line}");
        assert!(line.ends_with("T19, and 3 more"), "{line}");
    }

    #[test]
    fn test_declared_entity_markdown_shows_what_the_directive_wrote_without_a_location() {
        // Given — an entity the index does not hold yet
        let schema = load_schema(
            "[[entity_type]]\nname = \"req\"\n\n  [[entity_type.attribute]]\n  name = \"title\"\n  type = \"string\"\n\n  [[entity_type.attribute]]\n  name = \"status\"\n  type = \"string\"\n",
            &NoReservedNames,
        )
        .unwrap();
        let entity = EntityBody {
            type_name: "req".to_string(),
            id: id("REQ_1").into(),
            attributes: BTreeMap::from([
                (
                    "title".to_string(),
                    AttributeValue::String("Boot fast".to_string()),
                ),
                (
                    "status".to_string(),
                    AttributeValue::String("draft".to_string()),
                ),
            ]),
            relations: BTreeMap::new(),
            sections: Vec::new(),
            span: None,
            relation_spans: BTreeMap::new(),
        };

        // When
        let text = declared_entity_markdown(&entity, &ProjectIndex::default(), &schema);

        // Then
        assert_eq!(text, "**req** REQ\\_1 — Boot fast\n\n- **status:** draft");
    }

    #[test]
    fn test_indexed_entity_markdown_reads_the_record_and_its_back_links() {
        // Given
        let schema = load_schema(
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
              incoming = "verified_by"
              incoming_label = "Verified by"
            "#,
            &NoReservedNames,
        )
        .unwrap();
        let mut index = ProjectIndex::default();
        index.entities.insert(
            id("REQ_1"),
            EntityRecord {
                type_name: "req".to_string(),
                doc_path: "reqs.rst".to_string(),
                title: Some("Boot fast".to_string()),
                attributes: BTreeMap::from([(
                    "status".to_string(),
                    AttributeValue::String("open".to_string()),
                )]),
                outgoing: BTreeMap::new(),
                uml: BTreeMap::new(),
                relation_spans: BTreeMap::new(),
            },
        );
        index.entity_backlinks.insert(
            id("REQ_1"),
            BTreeMap::from([("verified_by".to_string(), vec![id("TEST_1")])]),
        );

        // When
        let text = indexed_entity_markdown(&id("REQ_1"), &index, &schema, "reqs\\.rst");

        // Then
        assert_eq!(
            text.as_deref(),
            Some(
                "**req** REQ\\_1 — Boot fast\n\nreqs\\.rst\n\n\
                 - **status:** open\n\n\
                 - **Verified by:** TEST\\_1"
            )
        );
        assert_eq!(
            indexed_entity_markdown(&id("REQ_2"), &index, &schema, "x"),
            None
        );
    }
}

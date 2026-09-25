use std::collections::BTreeMap;

use rinx_ast::{
    AttributeValue, Directive, Document, EntityBody, EntityId, EntitySection, InlineNode, Node,
};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rinx_index::{EntityRecord, ProjectIndex};

use super::*;
use crate::EmbeddedAssets;

/// The fixture schema's source, so a variant can be built from the same
/// text rather than a second copy that could drift from it.
fn schema_source() -> &'static str {
    r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          label = "Current status"
          type = "string"

          [[entity_type.section]]
          name = "verification-criteria"
          label = "Verification criteria"

          [[entity_type.section]]
          name = "safety-comment"
          label = "Safety comment"
          multiple = true

          [[entity_type.relation]]
          name = "links"
          label = "Links to"
          to = ["spec"]
          incoming = "linked_by"
          incoming_label = "Linked by"

          [[entity_type.relation]]
          name = "supersedes"
          to = ["req"]
          incoming = "superseded_by"
          incoming_label = "Superseded by"

        [[entity_type]]
        name = "spec"

          [[entity_type.relation]]
          name = "implements"
          label = "Implements"
          to = ["req"]
          incoming = "implemented_by"
          incoming_label = "Implemented by"
        "#
}

fn schema() -> EntitySchema {
    load_schema(schema_source(), &NoReservedNames).unwrap()
}

fn paragraph(text: &str) -> Node {
    Node::Paragraph(vec![InlineNode::Text(text.to_string())])
}

fn requirement() -> EntityBody {
    EntityBody {
        type_name: "req".to_string(),
        id: EntityId::new("REQ_001").unwrap(),
        attributes: BTreeMap::from([
            (
                "title".to_string(),
                AttributeValue::String("Boot quickly".to_string()),
            ),
            (
                "status".to_string(),
                AttributeValue::String("open".to_string()),
            ),
        ]),
        relations: BTreeMap::from([(
            "links".to_string(),
            vec![EntityId::new("SPEC_003").unwrap()],
        )]),
        sections: vec![
            EntitySection::content(vec![paragraph("Leading prose.")]),
            EntitySection::named(
                "verification-criteria".to_string(),
                vec![paragraph("Measured.")],
                None,
            ),
        ],
        span: None,
    }
}

/// An `EntityRecord` for `REQ_001` agreeing exactly with [`requirement`]'s own
/// `EntityBody` — the "as-authored, untouched" case, distinct from a test
/// that gives the record a *different* value to prove the index wins.
fn requirement_record() -> EntityRecord {
    EntityRecord {
        type_name: "req".to_string(),
        doc_path: "specs/boot".to_string(),
        title: Some("Boot quickly".to_string()),
        attributes: BTreeMap::from([(
            "status".to_string(),
            AttributeValue::String("open".to_string()),
        )]),
        outgoing: BTreeMap::from([(
            "links".to_string(),
            vec![EntityId::new("SPEC_003").unwrap()],
        )]),
        uml: BTreeMap::new(),
    }
}

/// An index holding the requirement's link target, plus its back-link.
fn index() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    index.entities.insert(
        EntityId::new("SPEC_003").unwrap(),
        EntityRecord {
            type_name: "spec".to_string(),
            doc_path: "specs/detail".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
        },
    );
    index.entities.insert(
        EntityId::new("REQ_002").unwrap(),
        EntityRecord {
            type_name: "req".to_string(),
            doc_path: "specs/other".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
        },
    );
    index
}

/// Renders one entity through the real pipeline, so the ctx wiring is exercised.
fn render(entity: EntityBody, index: &ProjectIndex) -> String {
    render_with(entity, index, &schema())
}

/// The fixture schema with `req` rendered through a template named `req.html`.
fn template_schema() -> EntitySchema {
    load_schema(
        &schema_source().replace(
            "name = \"req\"\n        label = \"Requirement\"",
            "name = \"req\"\n        label = \"Requirement\"\n        template = \"req.html\"",
        ),
        &NoReservedNames,
    )
    .unwrap()
}

/// One template registered under the name `template_schema` points at.
fn one_template(source: &str) -> crate::blocks::EntityTemplates {
    let mut templates = crate::blocks::EntityTemplates::new();
    templates.insert("req.html".to_string(), source.to_string());
    templates
}

/// Renders one entity with both a schema and a template table supplied.
fn render_through(
    entity: EntityBody,
    index: &ProjectIndex,
    schema: &EntitySchema,
    templates: &crate::blocks::EntityTemplates,
) -> String {
    let doc = Document::new(
        "specs/boot".to_string(),
        vec![Node::Directive(Directive::Entity(Box::new(entity)))],
    );
    crate::render_with_assets(
        &doc,
        index,
        "specs/boot",
        &crate::config::SiteConfig::default(),
        &EmbeddedAssets::new(),
        schema,
        templates,
    )
    .html
}

/// Renders against a caller-supplied schema, so a per-type template or a
/// different declaration can be exercised through the same pipeline.
fn render_with(entity: EntityBody, index: &ProjectIndex, schema: &EntitySchema) -> String {
    let doc = Document::new(
        "specs/boot".to_string(),
        vec![Node::Directive(Directive::Entity(Box::new(entity)))],
    );
    crate::render_with_assets(
        &doc,
        index,
        "specs/boot",
        &crate::config::SiteConfig::default(),
        &EmbeddedAssets::new(),
        schema,
        &crate::blocks::EntityTemplates::new(),
    )
    .html
}

#[test]
fn test_entity_anchor_is_one_spelling_shared_by_both_ends() {
    // Given / When
    let anchor = entity_anchor("REQ_001");

    // Then
    assert_eq!(anchor, "entity-REQ_001");
}

#[test]
fn test_an_entity_carries_its_id_as_an_anchor() {
    // Given / When
    let html = render(requirement(), &index());

    // Then
    assert!(
        html.contains("id=\"entity-REQ_001\""),
        "unexpected html: {html}"
    );
}

#[test]
fn test_the_header_shows_the_type_label_the_title_and_the_id() {
    // Given / When
    let html = render(requirement(), &index());

    // Then
    assert!(html.contains(">Requirement<"), "no type label: {html}");
    assert!(html.contains(">Boot quickly<"), "no title: {html}");
    assert!(html.contains(">REQ_001<"), "no id: {html}");
}

#[test]
fn test_attributes_render_under_their_declared_labels() {
    // Given / When
    let html = render(requirement(), &index());

    // Then
    assert!(
        html.contains("<th>Current status</th><td>open</td>"),
        "unexpected html: {html}"
    );
}

#[test]
fn test_render_attributes_prefers_the_index_record_over_the_ast() {
    // Given — REQ_001's own EntityBody says `status: open`, but the merged
    // project index (as a `.. entity-update::` would leave it) says `closed`
    let mut index = index();
    index.entities.insert(
        EntityId::new("REQ_001").unwrap(),
        EntityRecord {
            type_name: "req".to_string(),
            doc_path: "specs/boot".to_string(),
            title: Some("Boot quickly".to_string()),
            attributes: BTreeMap::from([(
                "status".to_string(),
                AttributeValue::String("closed".to_string()),
            )]),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
        },
    );

    // When
    let html = render(requirement(), &index);

    // Then — the index's value wins, not the as-authored one
    assert!(
        html.contains("<th>Current status</th><td>closed</td>"),
        "unexpected html: {html}"
    );
    assert!(!html.contains(">open<"), "stale value leaked: {html}");
}

#[test]
fn test_render_attributes_prefers_the_update_history_current_value() {
    // Given — the record's own value agrees with the AST, but a
    // `.. entity-update::` has since set a different current value
    let mut index = index();
    index
        .entities
        .insert(EntityId::new("REQ_001").unwrap(), requirement_record());
    let mut history = rinx_index::EntityFieldHistory::default();
    history.attributes.insert(
        "status".to_string(),
        rinx_index::AttributeFieldHistory {
            original: Some(AttributeValue::String("open".to_string())),
            applied: Vec::new(),
            current: Some(AttributeValue::String("closed".to_string())),
        },
    );
    index
        .entity_update_history
        .insert(EntityId::new("REQ_001").unwrap(), history);

    // When
    let html = render(requirement(), &index);

    // Then
    assert!(
        html.contains("<th>Current status</th><td>closed</td>"),
        "unexpected html: {html}"
    );
}

#[test]
fn test_render_attributes_marks_a_conflicting_value() {
    // Given — the most recent applied change to `status` conflicted with an
    // earlier one from a different `.. entity-update::`
    let mut index = index();
    index
        .entities
        .insert(EntityId::new("REQ_001").unwrap(), requirement_record());
    let mut history = rinx_index::EntityFieldHistory::default();
    history.attributes.insert(
        "status".to_string(),
        rinx_index::AttributeFieldHistory {
            original: Some(AttributeValue::String("open".to_string())),
            applied: vec![rinx_index::AppliedFieldUpdate {
                update_index: 1,
                doc_path: "specs/other".to_string(),
                span: None,
                mode: rinx_ast::FieldMutationMode::Set("in_progress".to_string()),
                resulting_value: Some(AttributeValue::String("in_progress".to_string())),
                conflicts_with: Some(0),
            }],
            current: Some(AttributeValue::String("in_progress".to_string())),
        },
    );
    index
        .entity_update_history
        .insert(EntityId::new("REQ_001").unwrap(), history);

    // When
    let html = render(requirement(), &index);

    // Then
    assert!(html.contains("entity-conflict"), "unexpected html: {html}");
    assert!(html.contains("in_progress"), "unexpected html: {html}");
}

#[test]
fn test_render_attributes_does_not_mark_an_undisputed_value() {
    // Given — a plain update, no conflict
    let mut index = index();
    index
        .entities
        .insert(EntityId::new("REQ_001").unwrap(), requirement_record());
    let mut history = rinx_index::EntityFieldHistory::default();
    history.attributes.insert(
        "status".to_string(),
        rinx_index::AttributeFieldHistory {
            original: Some(AttributeValue::String("open".to_string())),
            applied: vec![rinx_index::AppliedFieldUpdate {
                update_index: 0,
                doc_path: "specs/boot".to_string(),
                span: None,
                mode: rinx_ast::FieldMutationMode::Set("closed".to_string()),
                resulting_value: Some(AttributeValue::String("closed".to_string())),
                conflicts_with: None,
            }],
            current: Some(AttributeValue::String("closed".to_string())),
        },
    );
    index
        .entity_update_history
        .insert(EntityId::new("REQ_001").unwrap(), history);

    // When
    let html = render(requirement(), &index);

    // Then
    assert!(!html.contains("entity-conflict"), "unexpected html: {html}");
}

#[test]
fn test_render_links_marks_a_conflicting_relation() {
    // Given
    let mut index = index();
    index
        .entities
        .insert(EntityId::new("REQ_001").unwrap(), requirement_record());
    let mut history = rinx_index::EntityFieldHistory::default();
    history.relations.insert(
        "links".to_string(),
        rinx_index::RelationFieldHistory {
            original: vec![EntityId::new("SPEC_003").unwrap()],
            applied: vec![rinx_index::AppliedRelationUpdate {
                update_index: 1,
                doc_path: "specs/other".to_string(),
                span: None,
                mode: rinx_ast::FieldMutationMode::Set("SPEC_999".to_string()),
                resulting_targets: vec![EntityId::new("SPEC_999").unwrap()],
                conflicts_with: Some(0),
            }],
            current: vec![EntityId::new("SPEC_999").unwrap()],
        },
    );
    index
        .entity_update_history
        .insert(EntityId::new("REQ_001").unwrap(), history);

    // When
    let html = render(requirement(), &index);

    // Then
    assert!(html.contains("entity-conflict"), "unexpected html: {html}");
}

#[test]
fn test_render_links_prefers_the_update_history_current_targets() {
    // Given — an update appended SPEC_004 to REQ_001's `links`
    let mut index = index();
    index.entities.insert(
        EntityId::new("SPEC_004").unwrap(),
        EntityRecord {
            type_name: "spec".to_string(),
            doc_path: "specs/other-detail".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
        },
    );
    index
        .entities
        .insert(EntityId::new("REQ_001").unwrap(), requirement_record());
    let mut history = rinx_index::EntityFieldHistory::default();
    history.relations.insert(
        "links".to_string(),
        rinx_index::RelationFieldHistory {
            original: vec![EntityId::new("SPEC_003").unwrap()],
            applied: Vec::new(),
            current: vec![
                EntityId::new("SPEC_003").unwrap(),
                EntityId::new("SPEC_004").unwrap(),
            ],
        },
    );
    index
        .entity_update_history
        .insert(EntityId::new("REQ_001").unwrap(), history);

    // When
    let html = render(requirement(), &index);

    // Then
    assert!(
        html.contains("href=\"other-detail.html#entity-SPEC_004\""),
        "the appended target should be linked: {html}"
    );
}

#[test]
fn test_the_title_is_not_repeated_in_the_attribute_table() {
    // Given — it is already the header
    let html = render(requirement(), &index());

    // Then
    assert!(!html.contains("<th>title</th>"), "title duplicated: {html}");
}

#[test]
fn test_the_content_section_renders_as_body_content() {
    // Given / When
    let html = render(requirement(), &index());

    // Then
    assert!(
        html.contains("<div class=\"entity-content\"><p>Leading prose.</p>"),
        "unexpected html: {html}"
    );
}

#[test]
fn test_a_named_section_renders_under_its_declared_label() {
    // Given / When
    let html = render(requirement(), &index());

    // Then
    assert!(
        html.contains("entity-section-verification-criteria"),
        "unexpected html: {html}"
    );
    assert!(html.contains(">Verification criteria<"), "no label: {html}");
    assert!(html.contains("<p>Measured.</p>"), "no body: {html}");
}

#[test]
fn test_a_section_body_is_real_rendered_content_not_text() {
    // Given — a nested directive, which only a node tree could hold
    let mut entity = requirement();
    entity.sections = vec![EntitySection::named(
        "verification-criteria".to_string(),
        vec![Node::Directive(Directive::Admonition {
            kind: rinx_ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![paragraph("Careful.")],
        })],
        None,
    )];

    // When
    let html = render(entity, &index());

    // Then
    assert!(html.contains("admonition"), "unexpected html: {html}");
}

#[test]
fn test_an_outgoing_relation_links_to_its_target() {
    // Given / When
    let html = render(requirement(), &index());

    // Then
    assert!(html.contains(">Links to<"), "no relation label: {html}");
    assert!(
        html.contains("href=\"detail.html#entity-SPEC_003\""),
        "unexpected href: {html}"
    );
}

#[test]
fn test_a_derived_backlink_is_shown_under_its_label() {
    // Given — incoming edges come from the index, not from the node
    let mut index = index();
    index.entity_backlinks.insert(
        EntityId::new("REQ_001").unwrap(),
        BTreeMap::from([(
            "superseded_by".to_string(),
            vec![EntityId::new("REQ_002").unwrap()],
        )]),
    );

    // When
    let html = render(requirement(), &index);

    // Then
    assert!(
        html.contains(">Superseded by<"),
        "no backlink label: {html}"
    );
    assert!(
        html.contains("href=\"other.html#entity-REQ_002\""),
        "unexpected href: {html}"
    );
}

#[test]
fn test_a_relation_target_that_does_not_exist_is_shown_as_plain_text() {
    // Given — the diagnostic belongs to the index phase, which sees the graph
    let mut entity = requirement();
    entity
        .relations
        .insert("links".to_string(), vec![EntityId::new("NOWHERE").unwrap()]);

    // When
    let html = render(entity, &index());

    // Then
    assert!(
        html.contains("<span class=\"broken-link\">NOWHERE</span>"),
        "unexpected html: {html}"
    );
}

#[test]
fn test_an_entity_with_no_links_renders_no_link_block() {
    // Given
    let mut entity = requirement();
    entity.relations.clear();

    // When
    let html = render(entity, &index());

    // Then
    assert!(!html.contains("entity-links"), "unexpected html: {html}");
}

#[test]
fn test_an_entity_with_no_attributes_renders_no_table() {
    // Given
    let mut entity = requirement();
    entity.attributes.clear();

    // When
    let html = render(entity, &index());

    // Then
    assert!(
        !html.contains("entity-attributes"),
        "unexpected html: {html}"
    );
}

#[test]
fn test_an_entity_of_an_undeclared_type_still_renders() {
    // Given — a stale `.ast` from before a schema change must not lose content
    let mut entity = requirement();
    entity.type_name = "gone".to_string();

    // When
    let html = render(entity, &index());

    // Then — the type name stands in for the missing label
    assert!(html.contains(">gone<"), "unexpected html: {html}");
    assert!(html.contains("<p>Leading prose.</p>"), "body lost: {html}");
}

#[test]
fn test_a_named_section_written_before_the_prose_still_renders_after_it() {
    // Given — the prose is what a reader wants first, whatever order the
    // author happened to write the two in
    let mut entity = requirement();
    entity.sections = vec![
        EntitySection::named(
            "verification-criteria".to_string(),
            vec![paragraph("Criteria.")],
            None,
        ),
        EntitySection::content(vec![paragraph("Prose.")]),
    ];

    // When
    let html = render(entity, &index());

    // Then
    assert!(
        html.find("Prose.").unwrap() < html.find("Criteria.").unwrap(),
        "unexpected html: {html}"
    );
}

#[test]
fn test_a_template_reads_declared_labels_rather_than_copying_them() {
    // Given — a type whose presentation is a template, which must be able to
    // render a section or a relation under the label the schema declares
    // instead of hardcoding a second copy of it
    let schema = load_schema(
        &schema_source().replace(
            "name = \"req\"\n        label = \"Requirement\"",
            "name = \"req\"\n        label = \"Requirement\"\n        template = \"req.html\"",
        ),
        &NoReservedNames,
    )
    .unwrap();
    let mut templates = crate::blocks::EntityTemplates::new();
    templates.insert(
        "req.html".to_string(),
        "{{ labels.attributes.status }}|{{ labels.sections.verification_criteria }}\
         |{{ labels.relations.links }}|{{ labels.relations.superseded_by }}"
            .to_string(),
    );

    // When
    let doc = Document::new(
        "specs/boot".to_string(),
        vec![Node::Directive(Directive::Entity(Box::new(requirement())))],
    );
    let html = crate::render_with_assets(
        &doc,
        &index(),
        "specs/boot",
        &crate::config::SiteConfig::default(),
        &EmbeddedAssets::new(),
        &schema,
        &templates,
    )
    .html;

    // Then
    assert!(
        html.contains("Current status|Verification criteria|Links to|Superseded by"),
        "unexpected html: {html}"
    );
}

#[test]
fn test_a_template_reads_a_back_link_label_declared_on_the_other_type() {
    // Given — `spec` declares the relation whose back-link lands on `req`, so
    // the label cannot be found by looking at `req`'s own relations
    let schema = template_schema();
    let templates = one_template("{{ labels.relations.implemented_by }}");

    // When
    let html = render_through(requirement(), &index(), &schema, &templates);

    // Then
    assert!(html.contains("Implemented by"), "unexpected html: {html}");
}

#[test]
fn test_a_template_reads_the_effective_attribute_value_not_the_ast() {
    // Given — REQ_001's own EntityBody says `status: open`, but the index
    // (as a `.. entity-update::` would leave it) says `closed`
    let schema = template_schema();
    let templates = one_template("{{ attributes.status }}");
    let mut index = index();
    index.entities.insert(
        EntityId::new("REQ_001").unwrap(),
        EntityRecord {
            type_name: "req".to_string(),
            doc_path: "specs/boot".to_string(),
            title: Some("Boot quickly".to_string()),
            attributes: BTreeMap::from([(
                "status".to_string(),
                AttributeValue::String("closed".to_string()),
            )]),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
        },
    );

    // When
    let html = render_through(requirement(), &index, &schema, &templates);

    // Then
    assert_eq!(html.trim(), "closed");
}

#[test]
fn test_a_template_reads_the_effective_outgoing_targets() {
    // Given — an update appended SPEC_004
    let schema = template_schema();
    let templates = one_template("{% for l in outgoing.links %}{{ l.id }} {% endfor %}");
    let mut index = index();
    index.entities.insert(
        EntityId::new("SPEC_004").unwrap(),
        EntityRecord {
            type_name: "spec".to_string(),
            doc_path: "specs/other-detail".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
        },
    );
    index
        .entities
        .insert(EntityId::new("REQ_001").unwrap(), requirement_record());
    let mut history = rinx_index::EntityFieldHistory::default();
    history.relations.insert(
        "links".to_string(),
        rinx_index::RelationFieldHistory {
            original: vec![EntityId::new("SPEC_003").unwrap()],
            applied: Vec::new(),
            current: vec![
                EntityId::new("SPEC_003").unwrap(),
                EntityId::new("SPEC_004").unwrap(),
            ],
        },
    );
    index
        .entity_update_history
        .insert(EntityId::new("REQ_001").unwrap(), history);

    // When
    let html = render_through(requirement(), &index, &schema, &templates);

    // Then
    assert!(html.contains("SPEC_003"), "unexpected html: {html}");
    assert!(html.contains("SPEC_004"), "unexpected html: {html}");
}

#[test]
fn test_a_template_reads_the_update_history() {
    // Given
    let schema = template_schema();
    let templates = one_template(
        "{{ history.attributes.status.original }}|{{ history.attributes.status.current }}\
         |{{ history.attributes.status.applied.0.doc_path }}\
         |{{ history.attributes.status.applied.0.mode }}",
    );
    let mut index = index();
    index
        .entities
        .insert(EntityId::new("REQ_001").unwrap(), requirement_record());
    let mut history = rinx_index::EntityFieldHistory::default();
    history.attributes.insert(
        "status".to_string(),
        rinx_index::AttributeFieldHistory {
            original: Some(AttributeValue::String("open".to_string())),
            applied: vec![rinx_index::AppliedFieldUpdate {
                update_index: 0,
                doc_path: "specs/review.rst".to_string(),
                span: None,
                mode: rinx_ast::FieldMutationMode::Set("closed".to_string()),
                resulting_value: Some(AttributeValue::String("closed".to_string())),
                conflicts_with: None,
            }],
            current: Some(AttributeValue::String("closed".to_string())),
        },
    );
    index
        .entity_update_history
        .insert(EntityId::new("REQ_001").unwrap(), history);

    // When
    let html = render_through(requirement(), &index, &schema, &templates);

    // Then — `doc_path` is a plain string value, escaped like any other by
    // MiniJinja's default auto-escaping (see `test_render_source_escapes_a_plain_value`)
    assert_eq!(html.trim(), "open|closed|specs&#x2f;review.rst|set");
}

#[test]
fn test_the_update_history_is_empty_for_an_untouched_entity() {
    // Given — no `.. entity-update::` ever touched REQ_001
    let schema = template_schema();
    let templates =
        one_template("{{ history.attributes | length }}|{{ history.relations | length }}");

    // When
    let html = render_through(requirement(), &index(), &schema, &templates);

    // Then — empty namespaces, not an undefined `history`
    assert_eq!(html.trim(), "0|0");
}

#[test]
fn test_a_template_sees_sections_in_the_order_they_render() {
    // Given — a map keyed by name carries no order at all, so an ordered list
    // sits beside it, matching what the built-in rendering does
    let schema = template_schema();
    let templates =
        one_template("{% for s in section_list %}{{ s.name }}:{{ s.label }};{% endfor %}");

    // When
    let mut entity = requirement();
    entity.sections = vec![
        EntitySection::named("safety-comment".to_string(), vec![], None),
        EntitySection::named("verification-criteria".to_string(), vec![], None),
    ];
    let html = render_through(entity, &index(), &schema, &templates);

    // Then — the schema's declared order, not the alphabetical order a map
    // would give nor the order these two were written in
    assert!(
        html.contains("verification_criteria:Verification criteria;safety_comment:Safety comment;"),
        "unexpected html: {html}"
    );
}

#[test]
fn test_sections_render_prose_first_then_in_declared_order() {
    // Given — written in the opposite order to the schema's declaration
    let mut entity = requirement();
    entity.sections = vec![
        EntitySection::named("safety-comment".to_string(), vec![], None),
        EntitySection::content(vec![]),
        EntitySection::named("verification-criteria".to_string(), vec![], None),
    ];

    // When
    let ordered = sections_in_render_order(&entity, &schema());

    // Then — prose first, then the schema's order, so two entities of one type
    // are comparable however their authors wrote them
    let names: Vec<Option<&str>> = ordered.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        vec![None, Some("verification-criteria"), Some("safety-comment")]
    );
}

#[test]
fn test_sections_keep_document_order_within_one_name() {
    // Given — `safety-comment` is `multiple`, and the order two of them were
    // written in is the only order they have
    let mut entity = requirement();
    entity.sections = vec![
        EntitySection::named(
            "safety-comment".to_string(),
            vec![paragraph("First.")],
            None,
        ),
        EntitySection::named(
            "safety-comment".to_string(),
            vec![paragraph("Second.")],
            None,
        ),
    ];

    // When
    let html = render(entity, &index());

    // Then
    assert!(
        html.find("First.").unwrap() < html.find("Second.").unwrap(),
        "unexpected html: {html}"
    );
}

#[test]
fn test_a_section_the_schema_does_not_declare_is_appended_not_dropped() {
    // Given — losing an author's prose to a lookup miss would be worse than
    // showing it last
    let mut entity = requirement();
    entity.sections = vec![
        EntitySection::named("mystery".to_string(), vec![], None),
        EntitySection::named("verification-criteria".to_string(), vec![], None),
    ];

    // When
    let ordered = sections_in_render_order(&entity, &schema());

    // Then
    let names: Vec<Option<&str>> = ordered.iter().map(|s| s.name()).collect();
    assert_eq!(names, vec![Some("verification-criteria"), Some("mystery")]);
}

#[test]
fn test_attributes_render_in_declared_order_not_alphabetically() {
    // Given — `req` declares status before tags before owner, which is not the
    // order their names sort in
    let html = render(requirement(), &index());

    // When / Then
    let status = html.find("Current status").expect("no status");
    let owner = html.find("<th>owner</th>");
    assert!(
        owner.is_none_or(|owner| status < owner),
        "declared order not followed: {html}"
    );
}

#[test]
fn test_split_at_first_named_section_divides_prose_from_sections() {
    // Given — already in render order, so prose is exactly the prefix
    let sections = [
        EntitySection::content(vec![]),
        EntitySection::named("verification-criteria".to_string(), vec![], None),
    ];
    let refs: Vec<&EntitySection> = sections.iter().collect();

    // When
    let (visible, folded) = split_at_first_named_section(&refs);

    // Then
    assert_eq!(visible.len(), 1);
    assert_eq!(folded.len(), 1);
}

#[test]
fn test_split_at_first_named_section_folds_nothing_when_no_section_is_named() {
    // Given
    let sections = [EntitySection::content(vec![])];
    let refs: Vec<&EntitySection> = sections.iter().collect();

    // When
    let (visible, folded) = split_at_first_named_section(&refs);

    // Then
    assert_eq!(visible.len(), 1);
    assert!(folded.is_empty());
}

#[test]
fn test_an_entity_collapses_by_default() {
    // Given / When
    let html = render(requirement(), &index());

    // Then — the header and the leading prose stay outside the disclosure
    let disclosure = html
        .find("<details class=\"entity-more\">")
        .expect("no disclosure");
    assert!(html.find("entity-header").unwrap() < disclosure, "{html}");
    assert!(html.find("entity-content").unwrap() < disclosure, "{html}");
    for folded in [
        "entity-attributes",
        "entity-section-verification",
        "entity-links",
    ] {
        assert!(
            html.find(folded).unwrap() > disclosure,
            "{folded} not folded: {html}"
        );
    }
}

#[test]
fn test_the_flat_rendering_can_be_asked_for() {
    // Given — a site that wants every entity open
    let config = crate::config::SiteConfig {
        collapse_entities: false,
        ..crate::config::SiteConfig::default()
    };

    // When
    let doc = Document::new(
        "specs/boot".to_string(),
        vec![Node::Directive(Directive::Entity(Box::new(requirement())))],
    );
    let html = crate::render_with_assets(
        &doc,
        &index(),
        "specs/boot",
        &config,
        &EmbeddedAssets::new(),
        &schema(),
        &crate::blocks::EntityTemplates::new(),
    )
    .html;

    // Then
    assert!(!html.contains("<details"), "unexpected disclosure: {html}");
    assert!(
        html.contains("entity-attributes"),
        "attributes lost: {html}"
    );
}

#[test]
fn test_all_prose_stays_visible_wherever_it_was_written() {
    // Given — prose on both sides of a sub-directive. Sections no longer
    // render in document order, so there is nothing to protect by folding the
    // trailing paragraph: what is not inside a sub-directive stays visible.
    let mut entity = requirement();
    entity.sections = vec![
        EntitySection::content(vec![paragraph("Leading.")]),
        EntitySection::named(
            "verification-criteria".to_string(),
            vec![paragraph("Criteria.")],
            None,
        ),
        EntitySection::content(vec![paragraph("Trailing.")]),
    ];

    // When
    let html = render(entity, &index());
    let disclosure = html.find("<details").expect("no disclosure");

    // Then
    assert!(html.find("Leading.").unwrap() < disclosure, "{html}");
    assert!(html.find("Trailing.").unwrap() < disclosure, "{html}");
    assert!(html.find("Criteria.").unwrap() > disclosure, "{html}");
}

use rusty_sphinx_ast::{AttributeValue, Directive, EntityBody, Node};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

/// The schema every test below parses against.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
        argument = { fields = ["title"] }
        id = { prefix = "REQ_" }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]
          default = "open"

          [[entity_type.attribute]]
          name = "priority"
          type = "int"

          [[entity_type.attribute]]
          name = "tags"
          type = "list<string>"

          [[entity_type.attribute]]
          name = "owner"
          type = "string"
          required = true

          [[entity_type.section]]
          name = "verification-criteria"
          required = true

          [[entity_type.section]]
          name = "safety-comment"
          multiple = true

          [[entity_type.relation]]
          name = "links"
          to = ["spec"]
          multiple = true
          incoming = "linked_by"

          [[entity_type.relation]]
          name = "supersedes"
          to = ["req"]

        [[entity_type]]
        name = "spec"

        [[entity_type]]
        name = "audit-event"
        argument = { fields = ["name", "args", "version"], split = "comma" }
        id = { from = ["name"] }

          [[entity_type.attribute]]
          name = "name"
          type = "string"

          [[entity_type.attribute]]
          name = "args"
          type = "string"

          [[entity_type.attribute]]
          name = "version"
          type = "string"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

/// Parses `rst` against the test schema, returning the whole document.
fn parse(rst: &str) -> rusty_sphinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rusty_sphinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("specs/boot", rst, &ctx)
}

/// The first entity in `rst`.
fn parse_entity_of(rst: &str) -> EntityBody {
    let doc = parse(rst);
    first_entity(&doc.nodes).unwrap_or_else(|| panic!("no entity parsed from:\n{rst}"))
}

fn first_entity(nodes: &[Node]) -> Option<EntityBody> {
    nodes.iter().find_map(|node| match node {
        Node::Directive(Directive::Entity(entity)) => Some((**entity).clone()),
        _ => None,
    })
}

/// The diagnostic codes a parse reported, as their author-facing ids.
fn codes(rst: &str) -> Vec<String> {
    parse(rst)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

// ── The shape of a parsed entity ────────────────────────────────────

#[test]
fn test_parses_an_entity_with_every_kind_of_declaration() {
    // Given
    let rst = "\
.. req:: The system shall boot
   :id: REQ_001
   :owner: platform
   :status: closed
   :priority: 3
   :tags: boot, kernel
   :links: SPEC_003, SPEC_004

   Leading prose.

   .. verification-criteria::

      Measured on the reference board.
";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(entity.type_name, "req");
    assert_eq!(entity.id.as_str(), "REQ_001");
    assert_eq!(
        entity.attributes["title"],
        AttributeValue::String("The system shall boot".to_string())
    );
    assert_eq!(
        entity.attributes["status"],
        AttributeValue::String("closed".to_string())
    );
    assert_eq!(entity.attributes["priority"], AttributeValue::Int(3));
    assert_eq!(
        entity.attributes["tags"],
        AttributeValue::List(vec!["boot".to_string(), "kernel".to_string()])
    );
    assert_eq!(entity.relation_targets("links").len(), 2);
}

#[test]
fn test_the_argument_becomes_the_declared_field() {
    // Given
    let rst =
        ".. req:: Boot quickly\n   :owner: platform\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(entity.title().as_deref(), Some("Boot quickly"));
}

#[test]
fn test_a_comma_split_argument_fills_several_fields() {
    // Given — the CPython audit-event shape
    let rst = ".. audit-event:: os.system, command, 3.8\n";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(
        entity.attributes["name"],
        AttributeValue::String("os.system".to_string())
    );
    assert_eq!(
        entity.attributes["version"],
        AttributeValue::String("3.8".to_string())
    );
}

#[test]
fn test_an_id_is_derived_from_the_named_attributes() {
    // Given
    let rst = ".. audit-event:: os.system, command, 3.8\n";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(entity.id.as_str(), "os.system");
}

#[test]
fn test_a_generated_id_carries_the_declared_prefix() {
    // Given — no `:id:` written
    let rst = ".. req:: Boot\n   :owner: platform\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert!(
        entity.id.as_str().starts_with("REQ_req-"),
        "unexpected id {}",
        entity.id
    );
}

#[test]
fn test_generated_ids_are_stable_across_parses() {
    // Given — a `.ast` file is a Bazel output cached on its inputs
    let rst = ".. req:: Boot\n   :owner: platform\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let first = parse_entity_of(rst);
    let second = parse_entity_of(rst);

    // Then
    assert_eq!(first.id, second.id);
}

#[test]
fn test_an_unset_attribute_takes_its_declared_default() {
    // Given
    let rst = ".. req:: Boot\n   :owner: platform\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(
        entity.attributes["status"],
        AttributeValue::String("open".to_string())
    );
}

// ── Sections ────────────────────────────────────────────────────────

#[test]
fn test_leading_prose_becomes_the_content_section() {
    // Given
    let rst = "\
.. req:: Boot
   :owner: platform

   Leading prose.

   .. verification-criteria::

      Measured.
";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(entity.content().len(), 1);
    assert_eq!(entity.named_sections("verification-criteria").len(), 1);
}

#[test]
fn test_a_section_body_is_fully_parsed_rst() {
    // Given — a nested directive inside a section, which a string could not hold
    let rst = "\
.. req:: Boot
   :owner: platform

   .. verification-criteria::

      .. note::

         Measured on the reference board.
";

    // When
    let entity = parse_entity_of(rst);

    // Then
    let section = entity.named_sections("verification-criteria")[0];
    assert!(matches!(
        section.body.first(),
        Some(Node::Directive(Directive::Admonition { .. }))
    ));
}

#[test]
fn test_a_multiple_section_may_be_written_twice() {
    // Given
    let rst = "\
.. req:: Boot
   :owner: platform

   .. verification-criteria::

      Measured.

   .. safety-comment::

      One.

   .. safety-comment::

      Two.
";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(entity.named_sections("safety-comment").len(), 2);
    assert!(!codes(rst).contains(&"entity.duplicate-section".to_string()));
}

#[test]
fn test_sections_keep_their_document_order() {
    // Given
    let rst = "\
.. req:: Boot
   :owner: platform

   .. safety-comment::

      First.

   .. verification-criteria::

      Second.
";

    // When
    let entity = parse_entity_of(rst);

    // Then
    let names: Vec<Option<&str>> = entity.sections.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        vec![Some("safety-comment"), Some("verification-criteria")]
    );
}

// ── Diagnostics ─────────────────────────────────────────────────────

#[test]
fn test_an_unknown_option_is_diagnosed_rather_than_swallowed() {
    // Given — the gap this closes for domain objects, which swallow silently
    let rst = ".. req:: Boot\n   :owner: platform\n   :nonsense: x\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.unknown-attribute".to_string()));
}

#[test]
fn test_an_unknown_option_diagnostic_lists_the_accepted_ones() {
    // Given
    let rst = ".. req:: Boot\n   :owner: platform\n   :nonsense: x\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let doc = parse(rst);

    // Then
    let message = &doc
        .diagnostics
        .iter()
        .find(|d| d.code == rusty_sphinx_ast::DiagnosticCode::EntityUnknownAttribute)
        .unwrap()
        .message;
    assert!(message.contains(":status:"), "unhelpful message: {message}");
    assert!(message.contains(":links:"), "unhelpful message: {message}");
}

#[test]
fn test_a_value_outside_its_enum_is_diagnosed() {
    // Given
    let rst = ".. req:: Boot\n   :owner: platform\n   :status: pending\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.invalid-attribute-value".to_string()));
}

#[test]
fn test_a_non_numeric_int_is_diagnosed() {
    // Given
    let rst = ".. req:: Boot\n   :owner: platform\n   :priority: high\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.invalid-attribute-value".to_string()));
}

#[test]
fn test_a_missing_required_attribute_is_diagnosed() {
    // Given
    let rst = ".. req:: Boot\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.missing-required-attribute".to_string()));
}

#[test]
fn test_a_missing_required_section_is_diagnosed() {
    // Given
    let rst = ".. req:: Boot\n   :owner: platform\n\n   Just prose.\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.missing-required-section".to_string()));
}

#[test]
fn test_a_repeated_single_section_is_diagnosed() {
    // Given
    let rst = "\
.. req:: Boot
   :owner: platform

   .. verification-criteria::

      One.

   .. verification-criteria::

      Two.
";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.duplicate-section".to_string()));
}

#[test]
fn test_a_section_of_another_type_is_diagnosed() {
    // Given — `spec` declares no sections at all
    let rst = ".. spec:: x\n\n   .. verification-criteria::\n\n      Measured.\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.unknown-section".to_string()));
}

#[test]
fn test_a_section_outside_any_entity_is_diagnosed() {
    // Given — it would otherwise be an unknown directive and render as nothing
    let rst = ".. verification-criteria::\n\n   Measured.\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.section-outside-entity".to_string()));
}

#[test]
fn test_a_required_relation_with_no_target_is_diagnosed() {
    // Given
    let schema_text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.relation]]
          name = "links"
          required = true
    "#;
    let schema = load_schema(schema_text, &NoReservedNames).unwrap();
    let ctx = ParseCtx::with_domain(rusty_sphinx_ast::Domain::Py).with_schema(&schema);

    // When
    let doc = parse_with_ctx("d", ".. req::\n", &ctx);

    // Then
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == rusty_sphinx_ast::DiagnosticCode::EntityMissingRequiredRelation)
    );
}

#[test]
fn test_several_targets_on_a_single_target_relation_are_diagnosed() {
    // Given — `supersedes` does not declare `multiple`
    let rst = ".. req:: Boot\n   :owner: platform\n   :supersedes: REQ_A, REQ_B\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.multiple-relation-targets".to_string()));
}

#[test]
fn test_an_illegal_link_target_is_diagnosed() {
    // Given
    let rst = ".. req:: Boot\n   :owner: platform\n   :links: not a legal id\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.invalid-id".to_string()));
}

#[test]
fn test_an_argument_a_type_does_not_take_is_diagnosed() {
    // Given — `spec` declares no argument fields
    let rst = ".. spec:: unexpected title\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.malformed-argument".to_string()));
}

#[test]
fn test_too_many_comma_parts_are_diagnosed() {
    // Given
    let rst = ".. audit-event:: a, b, c, d\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(reported.contains(&"entity.malformed-argument".to_string()));
}

#[test]
fn test_a_diagnostic_points_at_the_option_line_that_caused_it() {
    // Given
    let rst = ".. req:: Boot\n   :owner: platform\n   :status: pending\n\n   .. verification-criteria::\n\n      x\n";

    // When
    let doc = parse(rst);

    // Then — the `:status:` line, not the directive marker
    let diagnostic = doc
        .diagnostics
        .iter()
        .find(|d| d.code == rusty_sphinx_ast::DiagnosticCode::EntityInvalidAttributeValue)
        .unwrap();
    assert_eq!(diagnostic.span.unwrap().start.line, 3);
}

#[test]
fn test_a_faulty_entity_still_parses_to_a_node() {
    // Given — the live-preview path must not lose the document over one mistake
    let rst = ".. req::\n   :status: pending\n\n   Prose survives.\n";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert_eq!(entity.type_name, "req");
    assert!(!entity.content().is_empty());
}

// ── Interaction with the rest of the parser ─────────────────────────

#[test]
fn test_an_unknown_directive_is_untouched_without_a_schema() {
    // Given — a project that declares no entities behaves exactly as before
    let ctx = ParseCtx::with_domain(rusty_sphinx_ast::Domain::Py);

    // When
    let doc = parse_with_ctx("d", ".. req:: Boot\n", &ctx);

    // Then
    assert!(matches!(
        doc.nodes.first(),
        Some(Node::Directive(Directive::Unknown { .. }))
    ));
    assert_eq!(
        doc.diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<rusty_sphinx_ast::DiagnosticCode>>(),
        vec![rusty_sphinx_ast::DiagnosticCode::DirectiveUnknown],
    );
}

#[test]
fn test_an_entity_may_nest_inside_a_section() {
    // Given
    let rst = "\
.. req:: Outer
   :owner: platform

   .. verification-criteria::

      .. spec:: Inner
";

    // When
    let entity = parse_entity_of(rst);

    // Then
    let section = entity.named_sections("verification-criteria")[0];
    assert!(matches!(
        section.body.first(),
        Some(Node::Directive(Directive::Entity(_)))
    ));
}

#[test]
fn test_a_section_inside_a_section_is_not_recognised() {
    // Given — sections are one level deep by design
    let rst = "\
.. req:: Boot
   :owner: platform

   .. verification-criteria::

      .. safety-comment::

         Nested.
";

    // When
    let entity = parse_entity_of(rst);

    // Then — the inner one is reported, not folded in as a section
    assert_eq!(entity.named_sections("safety-comment").len(), 0);
    assert!(codes(rst).contains(&"entity.section-outside-entity".to_string()));
}

/// The first diagram in `rst`, wherever it was written.
fn first_diagram(rst: &str) -> rusty_sphinx_ast::Uml {
    let doc = parse(rst);
    let mut found = None;
    rusty_sphinx_ast::walk_nodes(&doc.nodes, &mut |node| {
        if let Node::Directive(Directive::Uml(uml)) = node
            && found.is_none()
        {
            found = Some((**uml).clone());
        }
    });
    found.unwrap_or_else(|| panic!("no diagram parsed from:\n{rst}"))
}

#[test]
fn test_a_diagram_in_an_entity_body_records_the_entity_it_sits_in() {
    // Given — an architecture diagram written directly in a requirement
    let rst = "\
.. req:: Boot sequence
   :owner: alice

   .. entity-arch::

      {{ flow(need.id) }}
";

    // When
    let uml = first_diagram(rst);
    let entity = parse_entity_of(rst);

    // Then — the diagram names the very entity it was written in, whatever
    // that entity's id turned out to be
    assert_eq!(uml.entity.as_ref(), Some(&entity.id));
}

#[test]
fn test_a_diagram_in_a_named_section_still_records_the_entity() {
    // Given — a section body deliberately clears the enclosing entity *type*,
    // so that sections cannot nest. The id must survive that: a diagram
    // written under `.. verification-criteria::` is still a diagram of the
    // requirement it sits in.
    let rst = "\
.. req:: Boot sequence
   :owner: alice

   .. verification-criteria::

      .. entity-arch::

         {{ flow(need.id) }}
";

    // When
    let uml = first_diagram(rst);

    // Then
    assert!(uml.entity.is_some(), "the enclosing entity was lost");
}

use rinx_ast::{Directive, EntityFlow, EntityFlowSource, FlowDirection, ImageAlign, Node};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

/// The schema every test below parses against: two relations across two types,
/// plus an attribute and a derived back-link, which is every kind of field a
/// filter can name and every kind of edge a flowchart can draw.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]

        [[entity_type]]
        name = "test"

          [[entity_type.relation]]
          name = "verifies"
          to = ["req"]
          incoming = "verified_by"

          [[entity_type.relation]]
          name = "depends_on"
          to = ["test"]
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn parse(rst: &str) -> rinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("specs/boot", rst, &ctx)
}

/// The first flowchart in `rst`.
fn parse_flow(rst: &str) -> EntityFlow {
    let doc = parse(rst);
    doc.nodes
        .iter()
        .find_map(|node| match node {
            Node::Directive(Directive::EntityFlow(flow)) => Some((**flow).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no flowchart parsed from:\n{rst}"))
}

/// The diagnostic codes a parse reported, as their author-facing ids.
fn codes(rst: &str) -> Vec<String> {
    parse(rst)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// The first diagnostic's message.
fn message(rst: &str) -> String {
    let doc = parse(rst);
    let Some(diagnostic) = doc.diagnostics.first() else {
        panic!("expected a diagnostic from:\n{rst}");
    };
    diagnostic.message.clone()
}

#[test]
fn test_the_bare_directive_parses() {
    // Given
    let rst = ".. entity-flow::\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(flow.source, EntityFlowSource::EntityFlow);
    assert_eq!(flow.filter, None);
    assert_eq!(flow.relations, None);
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_both_spellings_parse_to_one_node_recording_which_was_written() {
    // Given
    let spellings = [
        ("entity-flow", EntityFlowSource::EntityFlow),
        ("needflow", EntityFlowSource::NeedFlow),
    ];

    for (name, expected) in spellings {
        // When
        let rst = format!(".. {name}::\n");
        let flow = parse_flow(&rst);

        // Then
        assert_eq!(flow.source, expected, "{name}");
        assert_eq!(codes(&rst), Vec::<String>::new(), "{name}");
    }
}

#[test]
fn test_a_filter_is_parsed_while_the_option_line_is_still_in_hand() {
    // Given
    let rst = ".. entity-flow::\n   :filter: status == \"open\"\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(
        flow.filter,
        Some(rinx_filter::parse_filter("status == \"open\"").unwrap())
    );
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_a_broken_filter_is_reported_and_the_flowchart_still_draws_everything() {
    // Given
    let rst = ".. entity-flow::\n   :filter: status === \"open\"\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(flow.filter, None);
    assert_eq!(codes(rst), ["entity-flow.invalid-filter"]);
}

#[test]
fn test_a_filter_naming_an_undeclared_field_is_reported_with_the_vocabulary() {
    // Given
    let rst = ".. entity-flow::\n   :filter: severity == \"high\"\n";

    // When
    let reported = message(rst);

    // Then
    assert_eq!(codes(rst), ["entity-flow.unknown-field"]);
    assert!(reported.contains("severity"), "{reported}");
    assert!(reported.contains("status"), "{reported}");
}

#[test]
fn test_relations_are_read_in_the_order_written() {
    // Given
    let rst = ".. entity-flow::\n   :relations: verifies, depends_on\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(
        flow.relations,
        Some(vec!["verifies".to_string(), "depends_on".to_string()])
    );
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_link_types_is_the_same_option_under_sphinx_needs_spelling() {
    // Given
    let rst = ".. needflow::\n   :link_types: verifies\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(flow.relations, Some(vec!["verifies".to_string()]));
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_an_unknown_relation_is_reported_and_the_others_are_still_drawn() {
    // Given
    let rst = ".. entity-flow::\n   :relations: verifies, links\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(flow.relations, Some(vec!["verifies".to_string()]));
    assert_eq!(codes(rst), ["entity-flow.unknown-relation"]);
    assert!(message(rst).contains("depends_on"), "{}", message(rst));
}

#[test]
fn test_an_option_naming_no_usable_relation_leaves_every_relation_drawn() {
    // Given — `None` is what an omitted option means, so a wholly unusable one
    // must not be confused with "draw no edges at all"
    let rst = ".. entity-flow::\n   :relations: links\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(flow.relations, None);
    assert_eq!(codes(rst), ["entity-flow.unknown-relation"]);
}

#[test]
fn test_show_link_names_is_a_flag_needing_no_value() {
    // Given
    let rst = ".. entity-flow::\n   :show-link-names:\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert!(flow.show_link_names);
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_the_flag_is_also_read_under_sphinx_needs_underscored_spelling() {
    // Given
    let rst = ".. needflow::\n   :show_link_names:\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert!(flow.show_link_names);
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_a_direction_plantuml_can_draw_is_read() {
    // Given
    let rst = ".. entity-flow::\n   :direction: LR\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(flow.direction, FlowDirection::LeftToRight);
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_a_direction_plantuml_cannot_draw_is_reported_and_the_default_kept() {
    // Given
    let rst = ".. entity-flow::\n   :direction: RL\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(flow.direction, FlowDirection::TopToBottom);
    assert_eq!(codes(rst), ["entity-flow.invalid-direction"]);
    assert!(message(rst).contains("PlantUML"), "{}", message(rst));
}

#[test]
fn test_reads_the_presentation_options() {
    // Given
    let rst = ".. entity-flow::\n   :caption: How the tests reach the requirements\n   \
               :align: center\n   :width: 400px\n   :scale: 50\n   :class: wide framed\n   \
               :name: coverage-flow\n   :config: monochrome\n   :debug:\n";

    // When
    let flow = parse_flow(rst);

    // Then
    assert_eq!(
        flow.caption.as_deref(),
        Some("How the tests reach the requirements")
    );
    assert_eq!(flow.align, Some(ImageAlign::Center));
    assert_eq!(flow.rendered_width().unwrap().to_string(), "200px");
    assert_eq!(flow.classes, ["wide", "framed"]);
    assert_eq!(flow.name.as_ref().unwrap().as_str(), "coverage-flow");
    assert_eq!(flow.config.as_deref(), Some("monochrome"));
    assert!(flow.debug);
    assert_eq!(codes(rst), Vec::<String>::new());
}

#[test]
fn test_a_scale_without_a_width_is_reported_against_its_own_line() {
    // Given
    let rst = ".. entity-flow::\n   :scale: 50\n";

    // When
    let doc = parse(rst);

    // Then
    assert_eq!(codes(rst), ["entity-flow.unusable-scale"]);
    let span = doc.diagnostics[0].span.expect("a span on the scale line");
    assert_eq!(span.start.line, 2);
}

#[test]
fn test_a_malformed_presentation_value_is_reported_and_the_flowchart_survives() {
    // Given
    let cases = [
        (":align: sideways", "entity-flow.invalid-align"),
        (":scale: half", "entity-flow.invalid-scale"),
        (":width: wide", "entity-flow.invalid-width"),
        (":config:", "entity-flow.empty-option-value"),
        (":name:", "entity-flow.empty-option-value"),
    ];

    for (option, expected) in cases {
        // When
        let rst = format!(".. entity-flow::\n   {option}\n");
        let flow = parse_flow(&rst);

        // Then
        assert_eq!(flow.source, EntityFlowSource::EntityFlow, "{option}");
        assert_eq!(codes(&rst), [expected], "{option}");
    }
}

#[test]
fn test_an_option_sphinx_needs_has_but_this_build_lacks_is_refused_by_name() {
    // Given
    let options = [
        ":show_legend:",
        ":show_filters:",
        ":highlight: status == \"open\"",
        ":border_color: red",
        ":engine: graphviz",
        ":root_id: REQ_001",
        ":filter-func: my_module:my_func",
        ":tags: api",
        ":status: open",
        ":types: req",
    ];

    for option in options {
        // When
        let rst = format!(".. entity-flow::\n   {option}\n");

        // Then
        assert_eq!(codes(&rst), ["entity-flow.unsupported-option"], "{option}");
        assert!(
            message(&rst).contains("not supported"),
            "{option}: {}",
            message(&rst)
        );
    }
}

#[test]
fn test_the_legacy_filter_options_are_answered_with_the_filter_to_write() {
    // Given
    let rst = ".. needflow::\n   :types: req\n";

    // When
    let reported = message(rst);

    // Then
    assert!(reported.contains(":filter:"), "{reported}");
    assert!(reported.contains("type == \"req\""), "{reported}");
}

#[test]
fn test_an_option_nobody_has_is_reported_as_unknown_rather_than_unsupported() {
    // Given
    let rst = ".. entity-flow::\n   :colour: red\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["directive.entity-flow-unknown-option"]);
}

#[test]
fn test_an_argument_is_answered_with_the_option_that_would_have_worked() {
    // Given
    let rst = ".. entity-flow:: status == \"open\"\n";

    // When
    let reported = message(rst);

    // Then
    assert_eq!(codes(rst), ["directive.entity-flow-unknown-option"]);
    assert!(reported.contains(":filter:"), "{reported}");
}

#[test]
fn test_written_content_is_reported_and_points_at_the_directive_that_takes_some() {
    // Given — the directive sits one letter away from `.. entity-diagram::`,
    // whose body *is* the picture
    let rst = ".. entity-flow::\n\n   A -> B\n";

    // When
    let reported = message(rst);

    // Then
    assert_eq!(codes(rst), ["directive.entity-flow-unknown-option"]);
    assert!(reported.contains("entity-diagram"), "{reported}");
}

#[test]
fn test_the_directive_names_are_reserved_against_an_entity_schema() {
    // Given — an entity type may not claim a name this build already parses
    let names = ["entity-flow", "needflow"];

    // When
    let reserved: Vec<bool> = names
        .iter()
        .map(|name| crate::directives::is_builtin_directive_name(name))
        .collect();

    // Then
    assert_eq!(reserved, [true, true]);
}

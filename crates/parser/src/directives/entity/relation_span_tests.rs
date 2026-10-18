//! The line each relation is written on, kept for the findings only the whole
//! project reveals about its targets.

use super::tests::parse_entity_of;

#[test]
fn test_a_relation_records_the_line_its_option_is_written_on() {
    // Given — `:links:` on the document's fourth line
    let rst = "\
.. req:: The system shall boot
   :id: REQ_001
   :owner: platform
   :links: SPEC_003, SPEC_004
";

    // When
    let entity = parse_entity_of(rst);

    // Then — the option line, not the directive's
    let span = entity.relation_spans["links"];
    assert_eq!(span.start.line, 4);
    assert_eq!(span.start.column, 4);
    assert_eq!(entity.span.map(|span| span.start.line), Some(1));
}

#[test]
fn test_an_attribute_records_no_relation_line() {
    // Given — options that are attributes only
    let rst = "\
.. req:: The system shall boot
   :id: REQ_001
   :owner: platform
";

    // When
    let entity = parse_entity_of(rst);

    // Then
    assert!(entity.relation_spans.is_empty());
}

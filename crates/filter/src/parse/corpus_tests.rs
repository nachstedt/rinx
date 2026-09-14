//! Every `:filter:` written in the sphinx-needs demo corpus.
//!
//! These are transcribed verbatim from useblocks' demo (the project
//! `scripts/benchmark_entities.py` builds), not invented: a hand-built suite
//! can agree with itself about a grammar that no real document uses. If one of
//! these stops parsing, a real project stops migrating.

use super::*;

/// The 17 `.. needtable::` filters, deduplicated by shape.
const CORPUS_FILTERS: &[&str] = &[
    r#"docname is not None and "basic_example" in docname"#,
    r#"docname is not None and "automotive-adas" in docname"#,
    r#"docname is not None and "safety_example" in docname"#,
    r#"type == "safety_goal" and docname is not None and "safety_example" in docname"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname"#,
    r#"type == "sysreq" and docname is not None and "safety_example" in docname"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and "Controller" in title"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and "Sensor" in title"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and "Power" in title"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and ("Process" in title or "Component" in title)"#,
    r#"type == "sysreq" and docname is not None and "safety_example" in docname and "VDC" in id"#,
    r#"type == "sysreq" and docname is not None and "safety_example" in docname and "WRDC" in id"#,
    r#"type == "sysreq" and docname is not None and "safety_example" in docname and "STEER" in id"#,
    r#"type == "sysreq" and docname is not None and "safety_example" in docname and "BRAKE" in id"#,
    r#"type == "sysreq" and docname is not None and "safety_example" in docname and "DRIVE" in id"#,
];

#[test]
fn test_every_corpus_filter_parses() {
    // Given the filters real sphinx-needs documents are written with

    // When
    let failures: Vec<(&str, FilterError)> = CORPUS_FILTERS
        .iter()
        .filter_map(|filter| parse_filter(filter).err().map(|error| (*filter, error)))
        .collect();

    // Then
    assert!(
        failures.is_empty(),
        "these filters did not parse: {failures:?}"
    );
}

#[test]
fn test_every_corpus_filter_reads_only_the_fields_it_names() {
    // Given — the parser validates these names against the entity schema, so
    // the set it extracts is what decides whether a real document builds
    let expected: &[&[&str]] = &[
        &["docname", "docname"],
        &["docname", "docname"],
        &["docname", "docname"],
        &["type", "docname", "docname"],
        &["type", "docname", "docname"],
        &["type", "docname", "docname"],
        &["type", "docname", "docname", "title"],
        &["type", "docname", "docname", "title"],
        &["type", "docname", "docname", "title"],
        &["type", "docname", "docname", "title", "title"],
        &["type", "docname", "docname", "id"],
        &["type", "docname", "docname", "id"],
        &["type", "docname", "docname", "id"],
        &["type", "docname", "docname", "id"],
        &["type", "docname", "docname", "id"],
    ];

    // When
    let found: Vec<Vec<String>> = CORPUS_FILTERS
        .iter()
        .map(|filter| {
            parse_filter(filter)
                .unwrap()
                .field_names()
                .iter()
                .map(ToString::to_string)
                .collect()
        })
        .collect();

    // Then
    assert_eq!(found, expected);
}

#[test]
fn test_the_longest_corpus_filter_groups_its_disjunction() {
    // Given — the one filter with parentheses, and the reason precedence had to
    // be right rather than merely present
    let input = r#"type == "fsr" and ("Process" in title or "Component" in title)"#;

    // When
    let expr = parse_filter(input).unwrap();

    // Then — the `or` stays inside the `and`'s right-hand side
    let Expr::And(_, right) = expr else {
        panic!("expected a conjunction at the top");
    };
    assert!(matches!(*right, Expr::Or(_, _)));
}

#[test]
fn test_a_corpus_filter_survives_a_serialization_round_trip() {
    // Given — the parsed filter is stored in the `.ast` and read back to render
    let expr = parse_filter(CORPUS_FILTERS[9]).unwrap();

    // When
    let json = serde_json::to_string(&expr).unwrap();
    let decoded: Expr = serde_json::from_str(&json).unwrap();

    // Then
    assert_eq!(decoded, expr);
}

/// The filters the corpus' `.. needpie::` slices are written with, verbatim.
///
/// Kept apart from [`CORPUS_FILTERS`] because these come from a different
/// construct — a pie's body, one filter per line — and because the eight
/// `startswith` ones are why this language grew a string method at all. Before
/// that, every one of them was refused as "attribute access", which left the
/// slice counting the whole project.
const CORPUS_PIE_FILTERS: &[&str] = &[
    r#"type == "hazard" and docname is not None and "safety_example" in docname"#,
    r#"type == "safety_goal" and docname is not None and "safety_example" in docname and (id == "SG_01" or id == "SG_02" or id == "SG_03" or id == "SG_04")"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and asil == "D""#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and id.startswith("FSR_STEER")"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and id.startswith("FSR_BRAKE")"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and id.startswith("FSR_DRIVE")"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and id.startswith("FSR_WRDC")"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and id.startswith("FSR_VDC")"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and (id.startswith("FSR_VEHICLE_SENS") or id.startswith("FSR_WHEEL_SENS"))"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and id.startswith("FSR_POWER")"#,
    r#"type == "fsr" and docname is not None and "safety_example" in docname and id.startswith("FSR_PROC")"#,
];

#[test]
fn test_every_corpus_pie_filter_parses() {
    for filter in CORPUS_PIE_FILTERS {
        // Given a filter written in the corpus' own pies

        // When
        let parsed = parse_filter(filter);

        // Then
        assert!(parsed.is_ok(), "{filter}: {:?}", parsed.unwrap_err());
    }
}

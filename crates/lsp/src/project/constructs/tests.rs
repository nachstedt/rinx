use std::path::Path;

use rinx_parser::{IgnoredRoles, ParseCtx, ParserConfig, parse};

use super::*;

/// The tally of one document holding `text`, at `/docs/index.rst`, parsed with
/// `ignored` as the roles its library ignores.
fn tally_ignoring(ignored: &[&str], text: &str) -> ConstructTally {
    let config = ParserConfig {
        ignored_roles: IgnoredRoles::new(ignored.iter().map(ToString::to_string).collect())
            .expect("valid names"),
    };
    let ctx = ParseCtx::default().with_parser_config(&config);
    let parsed = parse("index.rst", text, &ctx);
    let mut tally = ConstructTally::default();
    tally.add(
        Path::new("/docs/index.rst"),
        Path::new("/docs"),
        &DocumentConstructs::of(&parsed.document, &parsed.source_map),
    );
    tally
}

/// The tally of one document holding `text`.
fn tally(text: &str) -> ConstructTally {
    tally_ignoring(&[], text)
}

/// The names and counts of `list`.
fn counts(list: &BTreeMap<String, ConstructCount>) -> Vec<(&str, usize)> {
    list.iter()
        .map(|(name, count)| (name.as_str(), count.count))
        .collect()
}

#[test]
fn test_an_unknown_directive_is_counted_where_it_stands() {
    // When
    let tally = tally("Home\n====\n\n.. automodule:: spam\n");

    // Then
    assert_eq!(
        tally.unknown_directives["automodule"],
        ConstructCount {
            count: 1,
            examples: vec![ConstructSite {
                path: "/docs/index.rst".into(),
                line: 4,
                column: 1,
            }],
        }
    );
    assert_eq!(tally.diagnostics["directive.unknown"], 1);
}

#[test]
fn test_an_unknown_directive_nested_in_a_container_is_counted() {
    // When
    let tally = tally(".. dropdown:: Title\n\n   .. automodule:: spam\n");

    // Then
    assert_eq!(counts(&tally.unknown_directives), [("automodule", 1)]);
}

#[test]
fn test_a_construct_a_noqa_silences_is_still_counted() {
    // When
    let tally =
        tally(".. noqa: directive.unknown\n\n.. automodule:: spam\n\n:kdb:`C-x` and :kbd:`C-x`\n");

    // Then
    assert_eq!(counts(&tally.unknown_directives), [("automodule", 1)]);
    assert_eq!(counts(&tally.unknown_roles), [("kdb", 1)]);
    assert_eq!(counts(&tally.unsupported_roles), [("kbd", 1)]);
}

#[test]
fn test_a_malformed_directive_is_counted_apart_from_an_unknown_one() {
    // When
    let tally = tally(".. figure::\n\n.. automodule:: spam\n");

    // Then
    assert_eq!(counts(&tally.malformed_directives), [("figure", 1)]);
    assert_eq!(tally.malformed_directives["figure"].examples[0].line, 1);
    assert_eq!(counts(&tally.unknown_directives), [("automodule", 1)]);
}

#[test]
fn test_an_unsupported_option_is_counted_by_directive_and_option() {
    // When
    let tally = tally(".. button-link:: https://example.com\n   :ref-type: doc\n\n   Go\n");

    // Then
    assert_eq!(
        counts(&tally.unsupported_options),
        [("button-link:ref-type", 1)]
    );
}

#[test]
fn test_an_ignored_role_is_counted_as_ignored_in_either_spelling() {
    // When
    let tally = tally_ignoring(&["issue"], ":issue:`1` and `2`:issue: and :gh:`3`\n");

    // Then
    assert_eq!(counts(&tally.ignored_roles), [("issue", 2)]);
    assert_eq!(tally.ignored_roles["issue"].examples[0].column, 1);
    assert_eq!(counts(&tally.unknown_roles), [("gh", 1)]);
}

#[test]
fn test_the_options_a_toctree_writes_are_counted_as_an_author_spells_them() {
    // When
    let tally =
        tally(".. toctree::\n   :maxdepth: 2\n   :titlesonly:\n\n.. toctree::\n   :maxdepth: 1\n");

    // Then
    assert_eq!(
        tally
            .toctree_options
            .iter()
            .map(|(name, count)| (name.as_str(), *count))
            .collect::<Vec<_>>(),
        [("maxdepth", 2), ("titlesonly", 1)]
    );
}

#[test]
fn test_a_count_keeps_only_the_first_few_places() {
    // Given
    let text = ":kdb:`x`\n\n".repeat(MAX_EXAMPLES + 2);

    // When
    let tally = tally(&text);

    // Then
    let kdb = &tally.unknown_roles["kdb"];
    assert_eq!(kdb.count, MAX_EXAMPLES + 2);
    assert_eq!(kdb.examples.len(), MAX_EXAMPLES);
    assert_eq!(kdb.examples[0].line, 1);
}

#[test]
fn test_a_construct_in_no_file_is_counted_without_a_place() {
    // Given
    let constructs = DocumentConstructs {
        uses: vec![ConstructUse {
            kind: ConstructKind::UnknownRole,
            name: "kdb".to_string(),
            file: Some(RST_PROLOG.to_string()),
            place: Some(rinx_ast::Position::new(1, 1)),
        }],
        ..DocumentConstructs::default()
    };
    let mut tally = ConstructTally::default();

    // When
    tally.add(Path::new("/docs/a.rst"), Path::new("/docs"), &constructs);

    // Then
    assert_eq!(
        tally.unknown_roles["kdb"],
        ConstructCount {
            count: 1,
            examples: Vec::new(),
        }
    );
}

#[test]
fn test_a_construct_in_an_included_fragment_is_placed_in_the_fragment() {
    // Given
    let constructs = DocumentConstructs {
        uses: vec![ConstructUse {
            kind: ConstructKind::UnknownRole,
            name: "kdb".to_string(),
            file: Some("parts/a.inc".to_string()),
            place: Some(rinx_ast::Position::new(3, 2)),
        }],
        ..DocumentConstructs::default()
    };
    let mut tally = ConstructTally::default();

    // When
    tally.add(Path::new("/docs/a.rst"), Path::new("/docs"), &constructs);

    // Then
    assert_eq!(
        tally.unknown_roles["kdb"].examples,
        [ConstructSite {
            path: "/docs/parts/a.inc".into(),
            line: 3,
            column: 2,
        }]
    );
}

#[test]
fn test_two_documents_fold_into_one_tally() {
    // Given
    let parsed = parse("a.rst", ":kdb:`x`\n", &ParseCtx::default());
    let constructs = DocumentConstructs::of(&parsed.document, &parsed.source_map);
    let mut tally = ConstructTally::default();

    // When
    tally.add(Path::new("/docs/a.rst"), Path::new("/docs"), &constructs);
    tally.add(Path::new("/docs/b.rst"), Path::new("/docs"), &constructs);

    // Then
    assert_eq!(tally.unknown_roles["kdb"].count, 2);
    assert_eq!(tally.diagnostics["interpreted.unknown-role"], 2);
}

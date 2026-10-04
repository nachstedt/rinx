//! Definitions two documents claim, end to end: the index defines neither,
//! each claimant is warned, and a reference names the claimants — the same
//! whichever order the documents are read in.

use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_parser as parser;
use rinx_renderer as renderer;

/// The documents `sources` parse to, each `(path, text)`.
fn parse_all(sources: &[(&str, &str)]) -> Vec<ast::Document> {
    sources
        .iter()
        .map(|(path, text)| parser::parse(path, text))
        .collect()
}

/// The `(document, code)` of every project-wide warning about a duplicate.
fn duplicate_warnings(build: &analyzer::ProjectIndexBuild) -> Vec<(String, String)> {
    let mut warnings: Vec<(String, String)> = build
        .diagnostics
        .iter()
        .flat_map(|group| {
            group
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_str().contains("duplicate"))
                .map(|diagnostic| {
                    (
                        group.source_path.clone(),
                        diagnostic.code.as_str().to_string(),
                    )
                })
        })
        .collect();
    warnings.sort();
    warnings
}

const SETUP_A: &str = ".. _setup:\n\nSetup\n=====\n\nOne way.\n";
const SETUP_B: &str = ".. _setup:\n\nSetup again\n===========\n\nAnother way.\n";
const READER: &str = "Reader\n======\n\nSee :ref:`setup`.\n";

#[test]
fn test_e2e_a_label_two_documents_define_links_to_neither_in_any_order() {
    // Given
    let orders = [
        [
            ("a.rst", SETUP_A),
            ("b.rst", SETUP_B),
            ("reader.rst", READER),
        ],
        [
            ("reader.rst", READER),
            ("b.rst", SETUP_B),
            ("a.rst", SETUP_A),
        ],
    ];

    for order in orders {
        let docs = parse_all(&order);

        // When
        let build = analyzer::build_project_index_reporting(
            &docs,
            &analyzer::IndexSettings::new("reader"),
            &rinx_entity::EntitySchema::empty(),
        );
        let reader = docs
            .iter()
            .find(|doc| doc.path == "reader.rst")
            .expect("the reader is parsed");
        let output = renderer::render(reader, &build.index, &reader.path);

        // Then — both claimants warned, and the reference names them.
        assert_eq!(
            duplicate_warnings(&build),
            [
                ("a.rst".to_string(), "target.duplicate-name".to_string()),
                ("b.rst".to_string(), "target.duplicate-name".to_string()),
            ]
        );
        assert_eq!(output.broken_links.len(), 1);
        assert_eq!(
            output.broken_links[0].kind,
            renderer::BrokenLinkKind::AmbiguousTarget {
                documents: vec!["a.rst".to_string(), "b.rst".to_string()],
            }
        );
        assert_eq!(
            output.broken_links[0].kind.code().as_str(),
            "link.ambiguous-target"
        );
    }
}

#[test]
fn test_e2e_no_index_settles_an_object_described_twice() {
    // Given — CPython's `bytearray`, described in two documents, first
    // without and then with `:noindex:` on the second description.
    let main = ".. class:: bytearray\n\n   The type.\n";
    let repeat = ".. class:: bytearray\n\n   Again.\n";
    let settled = ".. class:: bytearray\n   :noindex:\n\n   Again.\n";
    let reader = "Use :class:`bytearray`.\n";
    let build = |second: &str| {
        let docs = parse_all(&[
            ("stdtypes.rst", main),
            ("functions.rst", second),
            ("reader.rst", reader),
        ]);
        let build = analyzer::build_project_index_reporting(
            &docs,
            &analyzer::IndexSettings::new("reader"),
            &rinx_entity::EntitySchema::empty(),
        );
        let output = renderer::render(&docs[2], &build.index, &docs[2].path);
        (duplicate_warnings(&build), output.broken_links)
    };

    // When
    let (contested_warnings, contested_links) = build(repeat);
    let (settled_warnings, settled_links) = build(settled);

    // Then
    assert_eq!(
        contested_warnings,
        [
            (
                "functions.rst".to_string(),
                "object.duplicate-description".to_string()
            ),
            (
                "stdtypes.rst".to_string(),
                "object.duplicate-description".to_string()
            ),
        ]
    );
    assert_eq!(contested_links.len(), 1);
    assert!(settled_warnings.is_empty());
    assert!(settled_links.is_empty(), "{settled_links:?}");
}

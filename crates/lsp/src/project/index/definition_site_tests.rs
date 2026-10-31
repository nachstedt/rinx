//! Where a project's definitions were written, and why a definition merely
//! moving leaves the fold alone.

use super::test_support::*;
use super::*;
use rinx_ast::{Position, TargetName};

/// The analysis of `source`, the document named `doc_path` at `/work/docs`.
fn indexed(doc_path: &str, source: &str) -> IndexedDocument {
    IndexedDocument::of(
        Path::new("/work/docs").join(doc_path),
        &rinx_parser::parse(doc_path, source, &rinx_parser::ParseCtx::default()).document,
        BTreeSet::new(),
    )
}

fn label(name: &str) -> DefinitionKey {
    DefinitionKey::Label(TargetName::new(name))
}

/// Where `project`'s document `doc_path` wrote the label `name`.
fn site_of(project: &mut Project, doc_path: &str, name: &str) -> Option<DefinitionLocation> {
    project
        .index_and_definitions()
        .1
        .site(doc_path, &label(name))
}

/// The fragment `project`'s document `doc_path` wrote the label `name` in.
fn fragment_of(project: &mut Project, doc_path: &str, name: &str) -> Option<String> {
    project
        .index_and_definitions()
        .1
        .fragment_of(doc_path, &label(name))
}

#[test]
fn test_definition_site_is_in_the_file_of_the_document_defining_it() {
    // Given
    let mut folder = folder();
    folder.record(
        "guide.rst".to_string(),
        indexed("guide.rst", "Intro\n\n.. _setup:\n\nText.\n"),
    );

    // When
    let site = site_of(&mut folder, "guide.rst", "setup");

    // Then
    assert_eq!(
        site,
        Some(DefinitionLocation {
            path: PathBuf::from("/work/docs/guide.rst"),
            start: Position::new(3, 1),
            end: Position::new(3, 11),
            name: Some(rinx_index::NameRange {
                start: Position::new(3, 5),
                end: Position::new(3, 10),
            }),
        })
    );
}

#[test]
fn test_definition_site_names_the_fragment_a_definition_was_included_from() {
    // Given — the server's loader names a fragment by its absolute path
    let mut folder = with_fragment();

    // When
    let site = site_of(&mut folder, "guide.rst", "setup");

    // Then
    assert_eq!(
        site.map(|site| site.path),
        Some(PathBuf::from("/work/docs/shared/setup.rst"))
    );
}

#[test]
fn test_definitions_lend_the_sites_without_a_fold() {
    // Given
    let mut folder = folder();
    folder.record(
        "guide.rst".to_string(),
        indexed("guide.rst", "Intro\n\n.. _setup:\n\nText.\n"),
    );

    // When — through a shared borrow, which cannot fold
    let site = folder.definitions().site("guide.rst", &label("setup"));

    // Then
    assert_eq!(site, site_of(&mut folder, "guide.rst", "setup"));
    assert!(site.is_some());
}

#[test]
fn test_definition_site_is_none_for_a_document_or_definition_it_does_not_know() {
    // Given
    let mut folder = folder();
    folder.record("a.rst".to_string(), labelled("a.rst", "setup"));

    // When / Then
    assert_eq!(site_of(&mut folder, "b.rst", "setup"), None);
    assert_eq!(site_of(&mut folder, "a.rst", "install"), None);
}

#[test]
fn test_record_moves_a_definition_without_folding_again() {
    // Given
    let mut folder = folder();
    folder.record(
        "guide.rst".to_string(),
        indexed("guide.rst", ".. _setup:\n\nText.\n"),
    );
    let before = folder.generation();

    // When — a line is inserted above the label, which defines nothing new
    folder.record(
        "guide.rst".to_string(),
        indexed("guide.rst", "\n.. _setup:\n\nText.\n"),
    );

    // Then — the fold is the one it was, and the label is on its new line
    assert_eq!(folder.generation(), before);
    assert_eq!(
        site_of(&mut folder, "guide.rst", "setup").map(|site| site.start.line),
        Some(2)
    );
}

/// A project holding `guide.rst`, which defines `own` itself and `setup` in
/// the fragment `shared/setup.rst`.
fn with_fragment() -> Project {
    let mut document = rinx_parser::parse(
        "guide.rst",
        ".. _own:\n\nText.\n",
        &rinx_parser::ParseCtx::default(),
    )
    .document;
    let fragment = document.intern_source_file("/work/docs/shared/setup.rst");
    document.nodes.push(rinx_ast::Node::Target {
        name: TargetName::new("setup").into(),
        destination: None,
        span: Some(
            rinx_ast::Span::new(Position::new(2, 1), Position::new(2, 11))
                .with_file(Some(fragment)),
        ),
    });
    let mut folder = folder();
    folder.record(
        "guide.rst".to_string(),
        IndexedDocument::of(
            PathBuf::from("/work/docs/guide.rst"),
            &document,
            BTreeSet::new(),
        ),
    );
    folder
}

#[test]
fn test_fragment_of_names_a_fragment_from_the_source_root() {
    // Given
    let mut folder = with_fragment();

    // When / Then
    assert_eq!(
        fragment_of(&mut folder, "guide.rst", "setup"),
        Some("shared/setup.rst".to_string())
    );
}

#[test]
fn test_fragment_of_is_none_for_a_definition_the_document_wrote_itself() {
    // Given
    let mut folder = with_fragment();

    // When / Then
    assert_eq!(fragment_of(&mut folder, "guide.rst", "own"), None);
    assert_eq!(fragment_of(&mut folder, "guide.rst", "missing"), None);
}

#[test]
fn test_index_and_definitions_lends_the_fold_beside_the_sites() {
    // Given
    let mut folder = with_fragment();

    // When
    let (index, definitions) = folder.index_and_definitions();

    // Then
    assert!(index.targets.contains_key(&TargetName::new("setup")));
    assert_eq!(
        definitions
            .site("guide.rst", &label("setup"))
            .map(|site| site.start.line),
        Some(2)
    );
}

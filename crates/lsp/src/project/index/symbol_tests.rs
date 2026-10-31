//! What a project's documents define, as a workspace search lists it: every
//! document's, by its latest analysis, each in the file it was written in.

use super::test_support::*;
use super::*;
use rinx_ast::TargetName;

/// Each of `project`'s symbols: its document, name and file.
fn listed(project: &Project) -> Vec<(String, String, PathBuf)> {
    project
        .symbols()
        .map(|candidate| {
            (
                candidate.doc_path.to_string(),
                candidate.entry.name.clone(),
                candidate.path(),
            )
        })
        .collect()
}

#[test]
fn test_symbols_list_every_documents_definitions() {
    // Given
    let mut folder = folder();
    folder.record("guide.rst".to_string(), labelled("guide.rst", "setup"));
    folder.record("usage.rst".to_string(), labelled("usage.rst", "usage"));

    // When / Then
    assert_eq!(
        listed(&folder),
        [
            (
                "guide.rst".to_string(),
                "setup".to_string(),
                PathBuf::from("/work/docs/guide.rst")
            ),
            (
                "usage.rst".to_string(),
                "usage".to_string(),
                PathBuf::from("/work/docs/usage.rst")
            ),
        ]
    );
}

#[test]
fn test_symbols_list_a_label_two_documents_define_under_both() {
    // Given — a name the fold keeps in neither document
    let mut folder = folder();
    folder.record("guide.rst".to_string(), labelled("guide.rst", "setup"));
    folder.record("usage.rst".to_string(), labelled("usage.rst", "setup"));
    assert!(
        !folder
            .project_index()
            .targets
            .contains_key(&TargetName::new("setup"))
    );

    // When
    let documents: Vec<String> = listed(&folder)
        .into_iter()
        .map(|(doc_path, _, _)| doc_path)
        .collect();

    // Then
    assert_eq!(documents, ["guide.rst", "usage.rst"]);
}

#[test]
fn test_symbols_place_an_included_definition_in_its_fragment() {
    // Given — the server's loader names a fragment by its absolute path
    let mut document =
        rinx_parser::parse("guide.rst", "Text.\n", &rinx_parser::ParseCtx::default()).document;
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

    // When / Then
    assert_eq!(
        listed(&folder),
        [(
            "guide.rst".to_string(),
            "setup".to_string(),
            PathBuf::from("/work/docs/shared/setup.rst")
        )]
    );
}

#[test]
fn test_symbols_follow_the_latest_analysis() {
    // Given
    let mut folder = folder();
    folder.record("guide.rst".to_string(), labelled("guide.rst", "setup"));

    // When
    folder.record("guide.rst".to_string(), labelled("guide.rst", "install"));

    // Then
    let names: Vec<String> = listed(&folder)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(names, ["install"]);
}

#[test]
fn test_site_path_joins_a_fragment_to_the_source_root() {
    // Given
    let (document, root) = (Path::new("/work/docs/guide.rst"), Path::new("/work/docs"));

    // When / Then
    assert_eq!(site_path(document, root, None), document);
    assert_eq!(
        site_path(document, root, Some("shared/setup.rst")),
        Path::new("/work/docs/shared/setup.rst")
    );
    assert_eq!(
        site_path(document, root, Some("/elsewhere/part.rst")),
        Path::new("/elsewhere/part.rst")
    );
}

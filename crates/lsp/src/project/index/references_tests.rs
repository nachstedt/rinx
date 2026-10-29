//! Where a project's definitions are used, and how long what a document's
//! uses resolved to is kept.

use super::test_support::*;
use super::*;
use rinx_ast::TargetName;

/// The analysis of `source`, the document named `doc_path` at `/work/docs`.
fn indexed(doc_path: &str, source: &str) -> IndexedDocument {
    IndexedDocument::of(
        Path::new("/work/docs").join(doc_path),
        &rinx_parser::parse(doc_path, source),
        BTreeSet::new(),
    )
}

fn install() -> UseTarget {
    UseTarget {
        doc_path: "setup.rst".to_string(),
        definition: Some(DefinitionKey::Label(TargetName::new("install"))),
    }
}

/// The lines `project`'s documents use `install` on, by file name.
fn install_lines(project: &mut Project) -> Vec<(String, u32)> {
    project
        .uses_of(&install())
        .into_iter()
        .map(|site| {
            let name = site
                .path
                .as_deref()
                .and_then(Path::file_name)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (name, site.start.line)
        })
        .collect()
}

/// A folder project where `setup.rst` defines `install` and `a.rst` and
/// `b.rst` each use it.
fn project() -> Project {
    let mut folder = folder();
    folder.record("setup.rst".to_string(), labelled("setup.rst", "install"));
    folder.record("a.rst".to_string(), indexed("a.rst", ":ref:`install`\n"));
    folder.record("b.rst".to_string(), indexed("b.rst", "\n:ref:`install`\n"));
    folder
}

#[test]
fn test_uses_of_lists_every_document_s_uses_in_document_order() {
    // Given
    let mut project = project();

    // When / Then
    assert_eq!(
        install_lines(&mut project),
        vec![("a.rst".to_string(), 1), ("b.rst".to_string(), 2)]
    );
}

#[test]
fn test_an_edit_leaving_the_fold_alone_resolves_that_document_again_alone() {
    // Given — every document's uses resolved
    let mut project = project();
    install_lines(&mut project);
    let generation = project.generation();

    // When — a use added to prose, which the fold does not read
    project.record(
        "a.rst".to_string(),
        indexed("a.rst", ":ref:`install`\n\n:ref:`install`\n"),
    );

    // Then — only `a.rst` is to be resolved again, and it is
    assert_eq!(project.generation(), generation);
    assert!(!project.uses.contains_key("a.rst"));
    assert!(project.uses.contains_key("b.rst"));
    assert_eq!(
        install_lines(&mut project),
        vec![
            ("a.rst".to_string(), 1),
            ("a.rst".to_string(), 3),
            ("b.rst".to_string(), 2)
        ]
    );
}

#[test]
fn test_an_edit_changing_the_fold_resolves_every_document_again() {
    // Given
    let mut project = project();
    install_lines(&mut project);

    // When — the label moves to another document
    project.record("setup.rst".to_string(), indexed("setup.rst", "Text.\n"));
    project.record("c.rst".to_string(), labelled("c.rst", "install"));

    // Then — the uses now lead to `c.rst`'s label, not `setup.rst`'s
    assert!(project.uses.is_empty());
    assert!(install_lines(&mut project).is_empty());
    let moved = UseTarget {
        doc_path: "c.rst".to_string(),
        ..install()
    };
    assert_eq!(project.uses_of(&moved).len(), 2);
}

#[test]
fn test_definition_at_prefers_a_label_to_a_section() {
    // Given — a label and a heading's section on lines 1 and 3
    let mut folder = folder();
    folder.record(
        "setup.rst".to_string(),
        indexed("setup.rst", ".. _install:\n\nInstalling\n==========\n"),
    );

    // When
    let definitions = folder.definitions();

    // Then
    assert_eq!(
        definitions.at("setup.rst", Position::new(1, 5)),
        Some(DefinitionKey::Label(TargetName::new("install")))
    );
    assert!(matches!(
        definitions.at("setup.rst", Position::new(3, 2)),
        Some(DefinitionKey::Section(_))
    ));
    assert_eq!(definitions.at("setup.rst", Position::new(2, 1)), None);
    assert_eq!(definitions.at("other.rst", Position::new(1, 5)), None);
}

//! What a project calls its documents, and which files are its own.

use super::test_support::*;
use super::*;

#[test]
fn test_doc_path_of_names_a_document_by_its_path_in_the_folder() {
    // When / Then
    assert_eq!(
        folder().doc_path_of(Path::new("/work/docs/guide/setup.rst")),
        Some("guide/setup.rst".to_string())
    );
    assert_eq!(
        folder().doc_path_of(Path::new("/elsewhere/setup.rst")),
        None
    );
    assert_eq!(folder().doc_path_of(Path::new("/work/docs/data.csv")), None);
}

#[test]
fn test_path_of_inverts_doc_path_of() {
    // When / Then
    assert_eq!(
        folder().path_of("guide/setup.rst"),
        PathBuf::from("/work/docs/guide/setup.rst")
    );
}

#[test]
fn test_doc_path_of_keeps_the_first_file_of_a_document() {
    // Given — `intro` recorded from `intro.rst`
    let mut project = sphinx("source_suffix = ['.rst', '.txt']\n");
    project.record("intro.rst".to_string(), labelled("intro.rst", "a"));

    // When / Then — `intro.txt` comes after it by name
    assert_eq!(project.doc_path_of(Path::new("/work/docs/intro.txt")), None);
    assert_eq!(
        project.doc_path_of(Path::new("/work/docs/intro.rst")),
        Some("intro.rst".to_string())
    );
}

#[test]
fn test_path_of_finds_a_recorded_documents_own_file() {
    // Given
    let mut project = sphinx("source_suffix = '.txt'\n");
    let document = rinx_parser::parse("intro.rst", "Text.\n");
    project.record(
        "intro.rst".to_string(),
        IndexedDocument::of(
            PathBuf::from("/work/docs/intro.txt"),
            &document,
            BTreeSet::new(),
        ),
    );

    // When / Then
    assert_eq!(
        project.path_of("intro.rst"),
        PathBuf::from("/work/docs/intro.txt")
    );
    assert_eq!(
        project.path_of("other.rst"),
        PathBuf::from("/work/docs/other.txt")
    );
}

#[test]
fn test_root_doc_falls_back_to_contents_as_sphinx_does() {
    // Given — no `index`, but a `contents`
    let mut project = sphinx("project = 'P'\n");
    project.record("contents.rst".to_string(), labelled("contents.rst", "a"));

    // When / Then
    assert_eq!(project.root_doc(), "contents");
    assert_eq!(project.site_config().root_doc, "contents");
    assert_eq!(project.project_index().root_documents, ["contents.rst"]);
}

#[test]
fn test_root_doc_keeps_index_when_it_exists_or_is_not_sphinxs() {
    // Given
    let mut project = sphinx("project = 'P'\n");
    project.record("contents.rst".to_string(), labelled("contents.rst", "a"));
    project.record("index.rst".to_string(), labelled("index.rst", "b"));
    let mut plain = folder();
    plain.record("contents.rst".to_string(), labelled("contents.rst", "a"));

    // When / Then
    assert_eq!(project.root_doc(), "index");
    assert_eq!(plain.root_doc(), "index");
    assert_eq!(sphinx("root_doc = 'start'\n").root_doc(), "start");
}

#[test]
fn test_a_projects_conf_and_folder_are_its_own() {
    // Given
    let project = sphinx("root_doc = os.getenv('X')\n");

    // When
    let (findings, text) = project.conf_findings();

    // Then
    assert_eq!(project.folder(), 1);
    assert_eq!(project.root(), Path::new("/work/docs"));
    assert_eq!(project.conf_path(), Some(Path::new("/work/docs/conf.py")));
    assert_eq!(findings.len(), 1);
    assert!(text.starts_with("root_doc"));
    assert_eq!(folder().conf_path(), None);
    assert_eq!(project.model().settings.root_doc, "index");
}

#[test]
fn test_a_project_found_again_takes_its_conf_py_reading() {
    // Given a project whose `conf.py` reading fell short
    let mut project = sphinx("root_doc = os.getenv('X')\n");
    assert!(project.wants_conf_run());

    // When it is found again with every setting a literal
    let mut found = DiscoveredProject::folder(PathBuf::from("/work/docs"));
    found.conf_text = "root_doc = 'index'\n".to_string();
    project.set_conf_reading(&found);

    // Then
    assert!(!project.wants_conf_run());
    assert_eq!(project.conf_text(), "root_doc = 'index'\n");
    assert_eq!(project.conf_findings().0, []);
}

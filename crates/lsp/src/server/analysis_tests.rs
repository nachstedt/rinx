//! The `rinx/analyseProject` request: what a project wrote that this build
//! does not support, for the client's report.

use super::test_support::*;
use crate::analysis::{
    ANALYSE_PROJECT_METHOD, AnalyseProjectParams, AnalysisReport, ConstructKind, ExtensionSupport,
};
use crate::progress::ProjectStatus;

fn analyse(state: &mut ServerState, document: Option<Uri>) -> AnalysisReport {
    let response = handle_request(
        state,
        Request::new(
            RequestId::from(7),
            ANALYSE_PROJECT_METHOD.to_string(),
            AnalyseProjectParams {
                text_document: document.map(TextDocumentIdentifier::new),
            },
        ),
    );
    match response.response_result {
        Ok(result) => serde_json::from_value(result).expect("a report"),
        Err(error) => panic!("a result, not {error:?}"),
    }
}

const CONF: &str = "extensions = ['sphinx.ext.autodoc', 'my_ext']\n";
const INDEX: &str = "Home\n====\n\n.. automodule:: spam\n\n:gh:`1` and :kbd:`C-x`\n";

#[test]
fn test_a_sphinx_project_reports_its_extensions_and_what_it_wrote() {
    // Given
    let (workspace, mut state) =
        scanned("analyse_sphinx", &[("conf.py", CONF), ("index.rst", INDEX)]);

    // When
    let report = analyse(&mut state, None);

    // Then
    assert!(report.complete);
    let [project] = report.projects.as_slice() else {
        panic!("one project: {report:?}");
    };
    assert_eq!(
        project.project,
        ProjectStatus::Sphinx {
            conf: "conf.py".to_string(),
            executed_with: None,
        }
    );
    assert_eq!(project.documents, 1);
    let support: Vec<(&str, ExtensionSupport)> = project
        .extensions
        .iter()
        .map(|extension| (extension.name.as_str(), extension.support))
        .collect();
    assert_eq!(
        support,
        [
            ("sphinx.ext.autodoc", ExtensionSupport::Incomplete),
            ("my_ext", ExtensionSupport::Unknown),
        ]
    );
    let automodule = &project.constructs[0];
    assert_eq!(automodule.kind, ConstructKind::UnknownDirective);
    assert_eq!(automodule.provider.as_deref(), Some("sphinx.ext.autodoc"));
    assert!(automodule.listed);
    assert_eq!(automodule.examples[0].uri, workspace.uri("index.rst"));
    assert_eq!(automodule.examples[0].line, 4);
    let kinds: Vec<(ConstructKind, &str)> = project
        .constructs
        .iter()
        .map(|construct| (construct.kind, construct.name.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (ConstructKind::UnknownDirective, "automodule"),
            (ConstructKind::UnsupportedRole, "kbd"),
            (ConstructKind::UnknownRole, "gh"),
        ]
    );
}

#[test]
fn test_a_project_ignoring_a_role_lists_it_as_ignored() {
    // Given
    let (_workspace, mut state) = scanned(
        "analyse_ignored",
        &[
            ("conf.py", "extensions = []\n"),
            ("rinx_parser.toml", "ignored_roles = [\"gh\"]\n"),
            ("index.rst", "Home\n====\n\n:gh:`1`\n"),
        ],
    );

    // When
    let report = analyse(&mut state, None);

    // Then
    let construct = &report.projects[0].constructs[0];
    assert_eq!(construct.kind, ConstructKind::IgnoredRole);
    assert_eq!(construct.name, "gh");
}

#[test]
fn test_a_folder_declares_no_extensions() {
    // Given
    let (_workspace, mut state) = scanned("analyse_folder", &[("index.rst", ".. foo::\n")]);

    // When
    let report = analyse(&mut state, None);

    // Then
    let project = &report.projects[0];
    assert!(matches!(project.project, ProjectStatus::Folder { .. }));
    assert_eq!(project.extensions, []);
    assert_eq!(project.constructs[0].name, "foo");
    assert!(!project.constructs[0].listed);
}

#[test]
fn test_a_document_asks_about_its_own_project_only() {
    // Given two Sphinx projects
    let (workspace, mut state) = scanned(
        "analyse_two",
        &[
            ("a/conf.py", "extensions = []\n"),
            ("a/index.rst", ".. foo::\n"),
            ("b/conf.py", "extensions = []\n"),
            ("b/index.rst", ".. bar::\n"),
        ],
    );

    // When
    let everything = analyse(&mut state, None);
    let one = analyse(&mut state, Some(workspace.uri("b/index.rst")));

    // Then
    assert_eq!(everything.projects.len(), 2);
    let [project] = one.projects.as_slice() else {
        panic!("one project: {one:?}");
    };
    assert_eq!(project.constructs[0].name, "bar");
}

#[test]
fn test_a_report_during_the_scan_says_it_is_incomplete() {
    // Given a server whose scan has not finished
    let workspace = Workspace::new("analyse_scanning", &[("index.rst", ".. foo::\n")]);
    let (mut state, _) = workspace.server(false);

    // When
    let report = analyse(&mut state, None);

    // Then
    assert!(!report.complete);
}

#[test]
fn test_an_edit_shows_in_the_next_report() {
    // Given
    let (workspace, mut state) = scanned(
        "analyse_edit",
        &[
            ("conf.py", "extensions = []\n"),
            ("index.rst", "Home\n====\n"),
        ],
    );
    assert_eq!(analyse(&mut state, None).projects[0].constructs, []);

    // When
    handle_notification(&mut state, workspace.open("index.rst", ".. foo::\n"));

    // Then
    assert_eq!(
        analyse(&mut state, None).projects[0].constructs[0].name,
        "foo"
    );
}

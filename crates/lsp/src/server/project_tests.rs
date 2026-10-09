//! Sphinx projects: what a `conf.py` changes about the documents under it,
//! what the server reports in it, and the folder's projects found again when
//! it changes.

use super::test_support::*;
use crate::project::{ProjectSource, discover_projects};
use lsp_types::FileChangeType;

const HOME: &str = ".. _home:\n\nHome\n====\n";
const DRAFT: &str = ".. _draft:\n\nDraft\n=====\n";

#[test]
fn test_an_excluded_file_is_not_indexed_or_rendered() {
    // Given
    let (workspace, mut state) = scanned(
        "project_excluded",
        &[
            ("conf.py", "exclude_patterns = ['drafts']\n"),
            ("index.rst", HOME),
            ("drafts/wip.rst", DRAFT),
        ],
    );

    // When — the excluded file is opened, and every render runs
    handle_notification(&mut state, workspace.open("drafts/wip.rst", DRAFT));
    render_all(&mut state);

    // Then
    assert_eq!(project_labels(&mut state, 0), ["home"]);
}

#[test]
fn test_the_root_document_is_the_one_conf_py_names() {
    // Given
    let (_, mut state) = scanned(
        "project_root_doc",
        &[
            ("conf.py", "root_doc = 'contents'\n"),
            ("contents.rst", "Contents\n========\n"),
            ("index.rst", "Index\n=====\n"),
        ],
    );

    // When
    let roots = project(&mut state, 0)
        .project_index()
        .root_documents
        .clone();

    // Then
    assert_eq!(roots, ["contents.rst"]);
}

#[test]
fn test_the_default_role_is_the_one_conf_py_names() {
    // Given — under `:ref:`, bare text names a label no document defines
    let text = "Home\n====\n\nSee `install`.\n";
    let (workspace, mut state) = scanned(
        "project_default_role",
        &[("conf.py", "default_role = 'ref'\n"), ("index.rst", text)],
    );
    handle_notification(&mut state, workspace.open("index.rst", text));

    // When
    let rendered = render_all(&mut state);

    // Then
    assert_eq!(
        workspace.codes_for(&rendered, "index.rst"),
        Some(vec!["link.broken-ref".to_string()])
    );
}

#[test]
fn test_the_primary_domain_is_the_one_conf_py_names() {
    // Given
    let (_, mut state) = scanned(
        "project_primary_domain",
        &[
            ("conf.py", "primary_domain = 'c'\n"),
            ("index.rst", ".. function:: int f(void)\n"),
        ],
    );

    // When
    let index = project(&mut state, 0).project_index();

    // Then
    let types = &index.domain_objects[&rinx_ast::TargetName::new("f")];
    assert!(
        types
            .keys()
            .all(|kind| kind.domain() == rinx_ast::Domain::C)
    );
}

#[test]
fn test_numfig_is_the_one_conf_py_sets() {
    // Given — a `:numref:` to a captioned figure
    let text = "Home\n====\n\n.. _chart:\n\n.. figure:: chart.png\n\n   A chart.\n\nSee :numref:`chart`.\n";
    let rendered_codes = |name: &str, conf: &str| {
        let (workspace, mut state) = scanned(name, &[("conf.py", conf), ("index.rst", text)]);
        handle_notification(&mut state, workspace.open("index.rst", text));
        let rendered = render_all(&mut state);
        workspace
            .codes_for(&rendered, "index.rst")
            .unwrap_or_default()
    };

    // When
    let numbered = rendered_codes("project_numfig_on", "numfig = True\n");
    let unnumbered = rendered_codes("project_numfig_off", "project = 'P'\n");

    // Then
    assert!(
        !numbered.contains(&"numref.disabled".to_string()),
        "{numbered:?}"
    );
    assert!(
        unnumbered.contains(&"numref.disabled".to_string()),
        "{unnumbered:?}"
    );
}

#[test]
fn test_the_scan_reports_what_reading_conf_py_found_in_it() {
    // Given
    let workspace = Workspace::new(
        "project_conf_findings",
        &[
            ("conf.py", "import os\nroot_doc = os.getenv('ROOT')\n"),
            ("index.rst", HOME),
        ],
    );
    let (mut state, _) = workspace.server(false);

    // When
    let finished = state.on_scan_event(workspace.scanned());

    // Then
    assert_eq!(
        workspace.codes_for(&finished, "conf.py"),
        Some(vec!["conf.unread-setting".to_string()])
    );
}

#[test]
fn test_a_file_belongs_to_the_nearest_project() {
    // Given
    let (_, mut state) = scanned(
        "project_nested",
        &[
            ("README.rst", ".. _top:\n\nTop\n===\n"),
            ("docs/conf.py", ""),
            ("docs/index.rst", HOME),
        ],
    );

    // When / Then — the folder keeps what lies outside `docs/`
    assert_eq!(project_labels(&mut state, 0), ["top"]);
    assert_eq!(project_labels(&mut state, 1), ["home"]);
    assert_eq!(
        project(&mut state, 1).project_index().root_documents,
        ["index.rst"]
    );
}

#[test]
fn test_each_workspace_folder_has_projects_of_its_own() {
    // Given
    let first = Workspace::new("project_multi_a", &[("conf.py", ""), ("index.rst", HOME)]);
    let second = Workspace::new("project_multi_b", &[("index.rst", DRAFT)]);
    let mut state = ServerState::new(PositionEncoding::Utf16)
        .with_workspace(vec![first.root.clone(), second.root.clone()], false);
    state.start_scan();

    // When
    state.on_scan_event(ScanEvent::Finished {
        folder: 0,
        projects: scan_folder(&first.root, &|_, _| {}),
        elapsed: Duration::ZERO,
    });
    let finished = state.on_scan_event(ScanEvent::Finished {
        folder: 1,
        projects: scan_folder(&second.root, &|_, _| {}),
        elapsed: Duration::ZERO,
    });

    // Then
    assert_eq!(
        statuses(&finished)[0]["projects"],
        serde_json::json!([
            {"kind": "sphinx", "conf": "conf.py"},
            {"kind": "folder", "root": "rinx_lsp_workspace_project_multi_b"},
        ])
    );
    let mut labels: Vec<Vec<String>> = (0..2).map(|i| project_labels(&mut state, i)).collect();
    labels.sort();
    assert_eq!(
        labels,
        [vec!["draft".to_string()], vec!["home".to_string()]]
    );
}

#[test]
fn test_the_status_names_a_sphinx_project_by_its_conf_py() {
    // Given
    let workspace = Workspace::new(
        "project_status",
        &[("docs/conf.py", ""), ("docs/index.rst", HOME)],
    );
    let (mut state, _) = workspace.server(false);

    // When
    let finished = state.on_scan_event(workspace.scanned());

    // Then — the folder itself holds no document of its own
    assert_eq!(
        statuses(&finished)[0]["projects"],
        serde_json::json!([{"kind": "sphinx", "conf": "docs/conf.py"}])
    );
}

#[test]
fn test_changing_conf_py_finds_the_folders_projects_again() {
    // Given
    let (workspace, mut state) = scanned(
        "project_reload",
        &[
            ("conf.py", "project = 'P'\n"),
            ("index.rst", HOME),
            ("drafts/wip.rst", DRAFT),
        ],
    );
    assert_eq!(project_labels(&mut state, 0), ["draft", "home"]);

    // When
    workspace.write("conf.py", "exclude_patterns = ['drafts']\n");
    let replies = handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CHANGED)]),
    );

    // Then
    assert_eq!(project_labels(&mut state, 0), ["home"]);
    assert_eq!(statuses(&replies)[0]["state"], "ready");
    assert_eq!(statuses(&replies)[0]["documents"], 1);
}

#[test]
fn test_creating_conf_py_turns_the_folder_into_a_sphinx_project() {
    // Given
    let (workspace, mut state) = scanned("project_created", &[("index.rst", HOME)]);

    // When
    workspace.write("conf.py", "root_doc = 'index'\n");
    let replies = handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CREATED)]),
    );

    // Then
    assert!(matches!(
        project(&mut state, 0).model().source,
        ProjectSource::SphinxConf { .. }
    ));
    assert_eq!(project_labels(&mut state, 0), ["home"]);
    assert_eq!(
        statuses(&replies)[0]["projects"],
        serde_json::json!([{"kind": "sphinx", "conf": "conf.py"}])
    );
}

#[test]
fn test_deleting_a_nested_conf_py_gives_its_documents_back_to_the_folder() {
    // Given
    let (workspace, mut state) = scanned(
        "project_deleted",
        &[
            ("index.rst", HOME),
            ("docs/conf.py", ""),
            ("docs/index.rst", DRAFT),
        ],
    );
    assert_eq!(state.projects.len(), 2);

    // When
    workspace.remove("docs/conf.py");
    handle_notification(
        &mut state,
        workspace.watched(&[("docs/conf.py", FileChangeType::DELETED)]),
    );

    // Then
    assert_eq!(state.projects.len(), 1);
    assert_eq!(project_labels(&mut state, 0), ["draft", "home"]);
}

#[test]
fn test_fixing_conf_py_clears_what_it_reported() {
    // Given
    let (workspace, mut state) = scanned(
        "project_fixed",
        &[("conf.py", "root_doc = compute()\n"), ("index.rst", HOME)],
    );

    // When
    workspace.write("conf.py", "root_doc = 'index'\n");
    let replies = handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CHANGED)]),
    );

    // Then
    assert_eq!(workspace.codes_for(&replies, "conf.py"), Some(Vec::new()));
}

#[test]
fn test_a_conf_py_in_an_excluded_directory_changes_nothing() {
    // Given
    let (workspace, mut state) = scanned(
        "project_excluded_conf",
        &[
            ("conf.py", "exclude_patterns = ['venv']\n"),
            ("index.rst", HOME),
        ],
    );

    // When
    workspace.write("venv/pkg/conf.py", "");
    let replies = handle_notification(
        &mut state,
        workspace.watched(&[("venv/pkg/conf.py", FileChangeType::CREATED)]),
    );

    // Then
    assert!(replies.is_empty(), "{replies:?}");
    assert_eq!(state.projects.len(), 1);
}

#[test]
fn test_finding_the_projects_reparses_a_document_opened_before() {
    // Given — opened while the folder was still one project with no
    // configuration, so `.. function::` was read as Python's
    let workspace = Workspace::new(
        "project_reparsed",
        &[
            ("conf.py", "primary_domain = 'c'\n"),
            ("index.rst", ".. function:: int f(void)\n"),
        ],
    );
    let (mut state, _) = workspace.server(false);
    handle_notification(
        &mut state,
        workspace.open("index.rst", ".. function:: int f(void)\n"),
    );

    // When
    state.on_scan_event(ScanEvent::Projects {
        folder: 0,
        projects: discover_projects(&workspace.root),
    });

    // Then
    let index = project(&mut state, 0).project_index();
    let types = &index.domain_objects[&rinx_ast::TargetName::new("f")];
    assert!(
        types
            .keys()
            .all(|kind| kind.domain() == rinx_ast::Domain::C)
    );
}

#[test]
fn test_a_scan_result_older_than_the_projects_is_not_recorded_into_them() {
    // Given — the scan read the folder before `conf.py` excluded `drafts`
    let workspace = Workspace::new(
        "project_stale_scan",
        &[
            ("conf.py", "project = 'P'\n"),
            ("index.rst", HOME),
            ("drafts/wip.rst", DRAFT),
        ],
    );
    let (mut state, _) = workspace.server(false);
    let outdated_scan = workspace.scanned();
    workspace.write("conf.py", "exclude_patterns = ['drafts']\n");
    handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CHANGED)]),
    );

    // When
    state.on_scan_event(outdated_scan);

    // Then
    assert_eq!(project_labels(&mut state, 0), ["home"]);
}

#[test]
fn test_the_projects_event_after_a_reload_is_ignored() {
    // Given — the folder's projects found again before the scan's own event
    let workspace = Workspace::new(
        "project_late_event",
        &[("conf.py", "project = 'P'\n"), ("index.rst", HOME)],
    );
    let (mut state, _) = workspace.server(false);
    let late = ScanEvent::Projects {
        folder: 0,
        projects: discover_projects(&workspace.root),
    };
    workspace.write("conf.py", "root_doc = 'start'\n");
    handle_notification(
        &mut state,
        workspace.watched(&[("conf.py", FileChangeType::CHANGED)]),
    );

    // When
    let replies = state.on_scan_event(late);

    // Then
    assert!(replies.is_empty());
    assert_eq!(project(&mut state, 0).model().settings.root_doc, "start");
}

#[test]
fn test_deleting_a_projects_directory_drops_the_project() {
    // Given
    let (workspace, mut state) = scanned(
        "project_deleted_directory",
        &[
            ("index.rst", HOME),
            ("docs/conf.py", ""),
            ("docs/index.rst", DRAFT),
        ],
    );

    // When
    workspace.remove("docs");
    handle_notification(
        &mut state,
        workspace.watched(&[("docs", FileChangeType::DELETED)]),
    );

    // Then
    assert_eq!(state.projects.len(), 1);
    assert_eq!(project_labels(&mut state, 0), ["home"]);
}

#[test]
fn test_a_document_opened_before_the_projects_are_found_leaves_the_folders_index() {
    // Given — `docs/index.rst` opened while the folder was one project
    let workspace = Workspace::new(
        "project_moved",
        &[
            ("README.rst", "Top\n===\n"),
            ("docs/conf.py", ""),
            ("docs/index.rst", HOME),
        ],
    );
    let (mut state, _) = workspace.server(false);
    handle_notification(&mut state, workspace.open("docs/index.rst", HOME));
    assert_eq!(project_labels(&mut state, 0), ["home"]);

    // When
    state.on_scan_event(ScanEvent::Projects {
        folder: 0,
        projects: discover_projects(&workspace.root),
    });

    // Then — the label moved with its document into `docs/`'s project
    assert_eq!(project_labels(&mut state, 0), Vec::<String>::new());
    assert_eq!(project_labels(&mut state, 1), ["home"]);
}

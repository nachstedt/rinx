//! sphinx-needs projects: the entity schema their configuration converts
//! into, what it changes about their documents, what is reported in their
//! `conf.py`, and their need ids completing.

use super::test_support::*;
use lsp_types::request::Completion;
use lsp_types::{
    CompletionParams, CompletionResponse, FileChangeType, Position, TextDocumentPositionParams,
};

const CONF: &str = "extensions = ['sphinx_needs']\nneeds_from_toml = 'ubproject.toml'\n";
const UBPROJECT: &str = "[[needs.types]]\ndirective = \"req\"\ntitle = \"Requirement\"\n";
const REQS: &str =
    "Requirements\n============\n\n.. req:: Brakes\n   :id: R_1\n\nSee :need:`R_1`.\n";

/// The codes the published diagnostics of `file` carry, in `replies`.
fn codes(workspace: &Workspace, replies: &[Message], file: &str) -> Vec<String> {
    workspace.codes_for(replies, file).unwrap_or_default()
}

#[test]
fn test_a_need_type_is_a_directive_and_its_ids_resolve() {
    // Given
    let (workspace, mut state) = scanned(
        "needs_types",
        &[
            ("conf.py", CONF),
            ("ubproject.toml", UBPROJECT),
            ("index.rst", REQS),
        ],
    );

    // When
    let opened = handle_notification(&mut state, workspace.open("index.rst", REQS));
    let rendered = render_all(&mut state);

    // Then neither the directive nor the reference is reported
    assert_eq!(
        codes(&workspace, &opened, "index.rst"),
        Vec::<String>::new()
    );
    assert_eq!(workspace.codes_for(&rendered, "index.rst"), None);
    assert!(
        project(&mut state, 0)
            .project_index()
            .entities
            .keys()
            .any(|id| id.as_str() == "R_1")
    );
}

#[test]
fn test_literal_needs_settings_in_conf_py_are_read_too() {
    // Given a project configured in `conf.py` alone
    let conf = "extensions = ['sphinx_needs']\nneeds_types = [{'directive': 'req', 'title': 'Requirement'}]\n";
    let (workspace, mut state) =
        scanned("needs_literal", &[("conf.py", conf), ("index.rst", REQS)]);

    // When
    let opened = handle_notification(&mut state, workspace.open("index.rst", REQS));

    // Then
    assert_eq!(
        codes(&workspace, &opened, "index.rst"),
        Vec::<String>::new()
    );
}

#[test]
fn test_needs_settings_are_read_only_when_sphinx_needs_is_declared() {
    // Given the settings, but not the extension
    let conf = "needs_types = [{'directive': 'req'}]\n";
    let (workspace, mut state) = scanned(
        "needs_undeclared",
        &[("conf.py", conf), ("index.rst", REQS)],
    );

    // When
    let opened = handle_notification(&mut state, workspace.open("index.rst", REQS));

    // Then
    assert!(codes(&workspace, &opened, "index.rst").contains(&"directive.unknown".to_string()));
}

#[test]
fn test_what_the_entity_model_cannot_express_is_reported_in_conf_py() {
    // Given a link whose targets a function computes
    let ubproject = format!(
        "{UBPROJECT}\n[needs.links.runs]\npredicates = [[\"type == 'req'\", \"[[f()]]\"]]\n"
    );
    let workspace = Workspace::new(
        "needs_unconverted",
        &[
            ("conf.py", CONF),
            ("ubproject.toml", ubproject.as_str()),
            ("index.rst", REQS),
        ],
    );
    let (mut state, _) = workspace.server(false);

    // When
    let finished = state.on_scan_event(workspace.scanned());

    // Then — the generic `.. need::` too, since no type is called `need`
    assert_eq!(
        codes(&workspace, &finished, "conf.py"),
        ["conf.needs-unconverted", "conf.needs-unconverted"]
    );
}

#[test]
fn test_a_missing_ubproject_toml_is_reported_in_conf_py() {
    // Given
    let workspace = Workspace::new("needs_missing", &[("conf.py", CONF), ("index.rst", REQS)]);
    let (mut state, _) = workspace.server(false);

    // When
    let finished = state.on_scan_event(workspace.scanned());

    // Then
    assert!(codes(&workspace, &finished, "conf.py").contains(&"conf.needs-invalid".to_string()));
}

#[test]
fn test_changing_ubproject_toml_reads_the_project_again() {
    // Given a document of a type the configuration does not declare yet
    let specs = "Specs\n=====\n\n.. spec:: Pads\n   :id: S_1\n";
    let (workspace, mut state) = scanned(
        "needs_reload",
        &[
            ("conf.py", CONF),
            ("ubproject.toml", UBPROJECT),
            ("index.rst", specs),
        ],
    );
    let opened = handle_notification(&mut state, workspace.open("index.rst", specs));
    assert!(codes(&workspace, &opened, "index.rst").contains(&"directive.unknown".to_string()));

    // When
    workspace.write(
        "ubproject.toml",
        &format!("{UBPROJECT}[[needs.types]]\ndirective = \"spec\"\n"),
    );
    let replies = handle_notification(
        &mut state,
        workspace.watched(&[("ubproject.toml", FileChangeType::CHANGED)]),
    );

    // Then
    assert_eq!(
        codes(&workspace, &replies, "index.rst"),
        Vec::<String>::new()
    );
}

#[test]
fn test_a_need_role_completes_the_projects_need_ids() {
    // Given
    let text = "See :need:`\n";
    let (workspace, mut state) = scanned(
        "needs_complete",
        &[
            ("conf.py", CONF),
            ("ubproject.toml", UBPROJECT),
            ("reqs.rst", REQS),
            ("index.rst", text),
        ],
    );
    handle_notification(&mut state, workspace.open("index.rst", text));

    // When
    let response = handle_request(
        &mut state,
        Request::new(
            RequestId::from(7),
            Completion::METHOD.to_string(),
            CompletionParams {
                text_document_position: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(workspace.uri("index.rst")),
                    Position::new(0, 11),
                ),
                work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                partial_result_params: lsp_types::PartialResultParams::default(),
                context: None,
            },
        ),
    );

    // Then
    let result = response.response_result.expect("a result");
    let Some(CompletionResponse::List(list)) = serde_json::from_value(result).expect("a list")
    else {
        panic!("a completion list");
    };
    let shown: Vec<(&str, Option<&str>)> = list
        .items
        .iter()
        .map(|item| (item.label.as_str(), item.detail.as_deref()))
        .collect();
    assert_eq!(shown, [("R_1", Some("Brakes"))]);
}

#[test]
fn test_a_relation_to_a_need_no_document_declares_is_reported() {
    // Given a link option, and a spec linking to a requirement nobody wrote
    let ubproject =
        format!("{UBPROJECT}[[needs.types]]\ndirective = \"spec\"\n\n[needs.links.implements]\n");
    let specs = "Specs\n=====\n\n.. spec:: Pads\n   :id: S_1\n   :implements: R_GHOST\n";
    let (workspace, mut state) = scanned(
        "needs_unknown_target",
        &[
            ("conf.py", CONF),
            ("ubproject.toml", ubproject.as_str()),
            ("index.rst", specs),
        ],
    );
    let opened = handle_notification(&mut state, workspace.open("index.rst", specs));

    // When
    let rendered = render_all(&mut state);

    // Then — the parse cannot know; the fold of the project can, and names
    // the `:implements:` line, the one to change
    assert_eq!(
        codes(&workspace, &opened, "index.rst"),
        Vec::<String>::new()
    );
    let uri = workspace.uri("index.rst");
    let published = rendered
        .iter()
        .filter(|reply| {
            matches!(reply, Message::Notification(n) if n.method == PublishDiagnostics::METHOD)
        })
        .map(published)
        .filter(|params| params.uri == uri)
        .last()
        .expect("the render publishes index.rst");
    let found: Vec<(String, u32)> = published
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let Some(NumberOrString::String(code)) = &diagnostic.code else {
                panic!("a code");
            };
            (code.clone(), diagnostic.range.start.line)
        })
        .collect();
    assert_eq!(found, [("entity.unknown-target".to_string(), 5)]);
}

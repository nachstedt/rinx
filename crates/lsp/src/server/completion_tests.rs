//! Completing a `:ref:` or `:doc:` target, a directive's name, option or
//! option value, and a role's name: the capability the server announces,
//! and the items a request is answered with from the workspace index, the
//! directive specs and the role table.

use super::test_support::*;
use lsp_types::request::Completion;
use lsp_types::{
    CompletionList, CompletionOptions, CompletionParams, CompletionResponse, Position,
    TextDocumentPositionParams,
};

/// A section labelled `install` in `setup.rst`, and a titled document in a
/// subdirectory that references nothing yet.
const SETUP: &str = ".. _install:\n\nInstalling\n==========\n\nText.\n";
const NOTES: &str = "Notes\n=====\n\nText.\n";

fn completion(uri: Uri, line: u32, character: u32) -> Request {
    Request::new(
        RequestId::from(7),
        Completion::METHOD.to_string(),
        CompletionParams {
            text_document_position: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(uri),
                Position::new(line, character),
            ),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
            context: None,
        },
    )
}

/// The completion list `response` carries, or `None` for a null result.
fn list(response: &Response) -> Option<CompletionList> {
    let result = response.response_result.clone().expect("a result");
    let result: Option<CompletionResponse> = serde_json::from_value(result).expect("a completion");
    result.map(|result| match result {
        CompletionResponse::List(list) => list,
        CompletionResponse::Array(items) => CompletionList {
            is_incomplete: false,
            items,
        },
    })
}

/// Each item's label and detail.
fn shown(list: &CompletionList) -> Vec<(String, Option<String>)> {
    list.items
        .iter()
        .map(|item| (item.label.clone(), item.detail.clone()))
        .collect()
}

/// A scanned server over `setup.rst` and `guide/notes.rst`, with `notes`
/// opened at `text`.
fn server_with_notes(name: &str, text: &str) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, &[("setup.rst", SETUP), ("guide/notes.rst", NOTES)]);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("guide/notes.rst", text));
    (workspace, state)
}

#[test]
fn test_initialize_result_announces_completion_on_its_trigger_characters() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    let Some(CompletionOptions {
        trigger_characters: Some(triggers),
        ..
    }) = result.capabilities.completion_provider
    else {
        panic!("completion announced with trigger characters");
    };
    assert_eq!(triggers, ["`", "<", "/", ".", ":"]);
}

#[test]
fn test_completion_lists_every_label_with_its_title() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_ref", "See :ref:`\n");

    // When
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 0, 10),
    );

    // Then
    let list = list(&response).expect("items");
    assert!(!list.is_incomplete);
    assert_eq!(
        shown(&list),
        [("install".to_string(), Some("Installing".to_string()))]
    );
}

#[test]
fn test_completion_names_documents_relative_to_the_current_one() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_doc", "See :doc:`se`\n");

    // When — the cursor after `se`
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 0, 12),
    );

    // Then
    let list = list(&response).expect("items");
    assert_eq!(
        shown(&list),
        [
            ("notes".to_string(), Some("<no title>".to_string())),
            ("../setup".to_string(), Some("Installing".to_string())),
        ]
    );
    let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &list.items[1].text_edit else {
        panic!("a text edit");
    };
    assert_eq!(
        edit.range,
        lsp_types::Range::new(Position::new(0, 10), Position::new(0, 12))
    );
}

#[test]
fn test_completion_sees_a_label_defined_in_an_edited_buffer() {
    // Given — the label exists only in the open buffer, never saved
    let (workspace, mut state) = server_with_notes(
        "complete_buffer",
        ".. _notes-start:\n\nNotes\n=====\n\n:ref:`\n",
    );

    // When
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 5, 6),
    );

    // Then
    let labels: Vec<String> = shown(&list(&response).expect("items"))
        .into_iter()
        .map(|(label, _)| label)
        .collect();
    assert_eq!(labels, ["install", "notes-start"]);
}

#[test]
fn test_completion_is_incomplete_while_the_scan_runs() {
    // Given — the scan has not finished
    let workspace = Workspace::new("complete_scanning", &[("setup.rst", SETUP)]);
    let (mut state, _) = workspace.server(false);
    handle_notification(&mut state, workspace.open("notes.rst", ":ref:`\n"));

    // When
    let response = handle_request(&mut state, completion(workspace.uri("notes.rst"), 0, 6));

    // Then
    assert!(list(&response).expect("items").is_incomplete);
}

#[test]
fn test_completion_answers_null_outside_a_role() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_outside", "See :ref:`install`.\n");

    // When
    let response = handle_request(
        &mut state,
        completion(workspace.uri("guide/notes.rst"), 0, 19),
    );

    // Then
    assert_eq!(list(&response), None);
}

#[test]
fn test_completion_answers_null_outside_every_workspace_folder() {
    // Given — `uri()` lies in no folder
    let mut state = ServerState::new(PositionEncoding::Utf16);
    handle_notification(&mut state, did_open(":ref:`\n"));

    // When
    let response = handle_request(&mut state, completion(uri(), 0, 6));

    // Then
    assert_eq!(list(&response), None);
}

#[test]
fn test_completion_answers_null_for_a_document_not_open() {
    // Given
    let (workspace, mut state) = server_with_notes("complete_closed", ":ref:`\n");

    // When
    let response = handle_request(&mut state, completion(workspace.uri("setup.rst"), 0, 0));

    // Then
    assert_eq!(list(&response), None);
}

#[test]
fn test_completion_refuses_malformed_parameters() {
    // Given
    let mut state = ServerState::new(PositionEncoding::Utf16);
    let request = Request::new(
        RequestId::from(7),
        Completion::METHOD.to_string(),
        serde_json::json!({ "textDocument": 3 }),
    );

    // When
    let response = handle_request(&mut state, request);

    // Then
    assert_eq!(response.id, RequestId::from(7));
    assert_eq!(
        response.response_result.err().map(|error| error.code),
        Some(ErrorCode::InvalidParams as i32)
    );
}

/// The labels of the items completing the open document `file`, opened in a
/// scanned workspace of `files`, at `line` and `character`.
fn labels_at(
    name: &str,
    files: &[(&str, &str)],
    (file, text): (&str, &str),
    (line, character): (u32, u32),
) -> Vec<String> {
    let (workspace, mut state) = scanned(name, files);
    handle_notification(&mut state, workspace.open(file, text));
    let response = handle_request(&mut state, completion(workspace.uri(file), line, character));
    list(&response)
        .map(|list| list.items.into_iter().map(|item| item.label).collect())
        .unwrap_or_default()
}

#[test]
fn test_completion_lists_directive_names_after_a_marker() {
    // Given / When
    let labels = labels_at(
        "complete_directive",
        &[("index.rst", "")],
        ("index.rst", ".. no\n"),
        (0, 5),
    );

    // Then
    assert!(labels.iter().any(|label| label == "note"));
    assert!(labels.iter().any(|label| label == "py:function"));
}

#[test]
fn test_completion_resolves_bare_object_names_against_the_primary_domain() {
    // Given
    let (workspace, mut state) = scanned(
        "complete_primary_domain",
        &[("conf.py", "primary_domain = 'c'\n"), ("index.rst", "")],
    );
    handle_notification(&mut state, workspace.open("index.rst", ".. \n"));

    // When
    let response = handle_request(&mut state, completion(workspace.uri("index.rst"), 0, 3));

    // Then — a bare `function` is C's, and `namespace` is offered bare
    let list = list(&response).expect("items");
    let function = list
        .items
        .iter()
        .find(|item| item.label == "function")
        .expect("a `function` item");
    assert_eq!(function.detail.as_deref(), Some("Describes a C function."));
    assert!(list.items.iter().any(|item| item.label == "namespace"));
}

#[test]
fn test_completion_writes_a_directive_as_a_snippet_for_a_client_taking_one() {
    // Given
    let workspace = Workspace::new("complete_snippet", &[("index.rst", "")]);
    let (state, _) = workspace.server(false);
    let mut state = state.with_completion_format(crate::completion::TextFormat::Snippet);
    state.on_scan_event(workspace.scanned());
    handle_notification(&mut state, workspace.open("index.rst", ".. \n"));

    // When
    let response = handle_request(&mut state, completion(workspace.uri("index.rst"), 0, 3));

    // Then
    let list = list(&response).expect("items");
    let note = list
        .items
        .iter()
        .find(|item| item.label == "note")
        .expect("a `note` item");
    assert_eq!(
        note.insert_text_format,
        Some(lsp_types::InsertTextFormat::SNIPPET)
    );
    let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &note.text_edit else {
        panic!("a text edit");
    };
    assert_eq!(edit.new_text, "note::\n\n   $0");
}

#[test]
fn test_completion_lists_the_options_a_directive_takes_and_has_not_written() {
    // Given / When
    let labels = labels_at(
        "complete_option",
        &[("index.rst", "")],
        ("index.rst", ".. toctree::\n   :hidden:\n   :\n"),
        (2, 4),
    );

    // Then
    assert!(labels.iter().any(|label| label == "maxdepth"));
    assert!(!labels.iter().any(|label| label == "hidden"));
}

#[test]
fn test_completion_lists_a_choice_option_s_values() {
    // Given / When
    let labels = labels_at(
        "complete_value",
        &[("index.rst", "")],
        ("index.rst", ".. |logo| image:: a.png\n   :align: \n"),
        (1, 11),
    );

    // Then — a substitution's image may align to the text baseline too
    assert_eq!(
        labels,
        ["left", "center", "right", "top", "middle", "bottom"]
    );
}

#[test]
fn test_completion_lists_the_fields_of_the_types_an_update_matches() {
    // Given — a requirement type with a status, and a spec type without one
    let conf = "extensions = ['sphinx_needs']\n\
        needs_types = [{'directive': 'req'}, {'directive': 'spec'}]\n\
        needs_extra_options = ['status']\n";
    let reqs = ".. req:: Brakes\n   :id: R_1\n";
    let update = ".. entity-update:: R_1\n   :\n";

    // When
    let labels = labels_at(
        "complete_update",
        &[("conf.py", conf), ("reqs.rst", reqs), ("index.rst", "")],
        ("index.rst", update),
        (1, 4),
    );

    // Then — `:strict:`, then the fields of `req`
    assert_eq!(labels.first().map(String::as_str), Some("strict"));
    assert!(labels.iter().any(|label| label == "status"));
    assert!(!labels.iter().any(|label| label == "id"));
}

#[test]
fn test_completion_answers_null_in_a_directive_s_content() {
    // Given / When
    let (workspace, mut state) = scanned("complete_content", &[("index.rst", "")]);
    let text = ".. note::\n\n   Some text\n";
    handle_notification(&mut state, workspace.open("index.rst", text));
    let response = handle_request(&mut state, completion(workspace.uri("index.rst"), 2, 7));

    // Then
    assert_eq!(list(&response), None);
}

#[test]
fn test_completion_lists_role_names_after_a_colon() {
    // When
    let labels = labels_at(
        "complete_role_name",
        &[("index.rst", "")],
        ("index.rst", "See :\n"),
        (0, 5),
    );

    // Then — the table's roles, spelled for the default domain `py`
    for name in ["ref", "doc", "py:func", "class", "c:macro", "entity"] {
        assert!(labels.iter().any(|label| label == name), "{name}");
    }
    assert!(!labels.iter().any(|label| label == "macro"));
}

#[test]
fn test_completion_spells_bare_role_names_for_the_primary_domain() {
    // When
    let labels = labels_at(
        "complete_role_name_c",
        &[("index.rst", ""), ("conf.py", "primary_domain = 'c'\n")],
        ("index.rst", "See :\n"),
        (0, 5),
    );

    // Then
    assert!(labels.iter().any(|label| label == "macro"));
    assert!(!labels.iter().any(|label| label == "class"));
}

#[test]
fn test_completion_lists_the_roles_defined_above_the_cursor() {
    // Given — one role defined above the cursor, one below
    let text = ".. role:: python(code)\n\nSee :\n\n.. role:: shell(code)\n";

    // When
    let labels = labels_at(
        "complete_defined_role",
        &[("index.rst", "")],
        ("index.rst", text),
        (2, 5),
    );

    // Then
    assert!(labels.iter().any(|label| label == "python"));
    assert!(!labels.iter().any(|label| label == "shell"));
}

#[test]
fn test_completion_lists_options_not_roles_in_an_option_block() {
    // When
    let labels = labels_at(
        "complete_option_not_role",
        &[("index.rst", "")],
        ("index.rst", ".. image:: a.png\n   :\n"),
        (1, 4),
    );

    // Then
    assert!(labels.iter().any(|label| label == "alt"));
    assert!(!labels.iter().any(|label| label == "ref"));
}

/// The labels and details of the items completing the open document
/// `index.rst`, opened at `text` in a scanned workspace of `files`, at
/// `line` and `character`.
fn targets_at(
    name: &str,
    files: &[(&str, &str)],
    text: &str,
    (line, character): (u32, u32),
) -> Vec<(String, Option<String>)> {
    let (workspace, mut state) = scanned(name, files);
    handle_notification(&mut state, workspace.open("index.rst", text));
    let response = handle_request(
        &mut state,
        completion(workspace.uri("index.rst"), line, character),
    );
    list(&response).map(|list| shown(&list)).unwrap_or_default()
}

/// Two methods: `pkg.Box.open`, written in `index.rst` itself, and
/// `other.Crate.close`, in `api.rst`.
const API: &str = ".. py:module:: other\n\n.. py:class:: Crate\n\n   .. py:method:: close()\n";
const METHODS: &str =
    ".. py:module:: pkg\n\n.. py:class:: Box\n\n   .. py:method:: open()\n\n   See :py:meth:`\n";

#[test]
fn test_completion_writes_a_method_of_the_current_class_by_its_bare_name() {
    // When — inside class `Box` of module `pkg`
    let shown = targets_at(
        "complete_methods",
        &[("index.rst", ""), ("api.rst", API)],
        METHODS,
        (6, 17),
    );

    // Then — the nearer method first, each titled with its qualified name
    assert_eq!(
        shown,
        [
            ("open".to_string(), Some("pkg.Box.open".to_string())),
            (
                "other.Crate.close".to_string(),
                Some("other.Crate.close".to_string())
            ),
        ]
    );
}

#[test]
fn test_completion_of_a_domain_role_is_incomplete_and_filtered_by_what_is_typed() {
    // Given
    let (workspace, mut state) = scanned(
        "complete_methods_typed",
        &[("index.rst", ""), ("api.rst", API)],
    );
    let text = METHODS.replace(":py:meth:`", ":py:meth:`oth");
    handle_notification(&mut state, workspace.open("index.rst", &text));

    // When
    let response = handle_request(&mut state, completion(workspace.uri("index.rst"), 6, 20));

    // Then
    let list = list(&response).expect("items");
    assert!(list.is_incomplete);
    assert_eq!(
        shown(&list),
        [(
            "other.Crate.close".to_string(),
            Some("other.Crate.close".to_string())
        )]
    );
}

#[test]
fn test_completion_names_an_option_of_the_ambient_program_alone() {
    // Given
    let text = ".. program:: tool\n\n.. option:: -v\n\n   Verbose.\n\nSee :option:`\n";

    // When
    let shown = targets_at("complete_option", &[("index.rst", "")], text, (6, 13));

    // Then
    assert_eq!(shown, [("-v".to_string(), Some("tool.-v".to_string()))]);
}

#[test]
fn test_completion_lists_glossary_terms_and_equations() {
    // Given
    let glossary = ".. glossary::\n\n   widget\n      A thing.\n\n.. math::\n   :label: euler\n\n   e^{i\\pi} = -1\n";

    // When
    let terms = targets_at(
        "complete_terms",
        &[("index.rst", ""), ("glossary.rst", glossary)],
        "See :term:`\n",
        (0, 11),
    );
    let equations = targets_at(
        "complete_equations",
        &[("index.rst", ""), ("glossary.rst", glossary)],
        "See :eq:`\n",
        (0, 9),
    );

    // Then
    assert_eq!(
        terms.first().map(|(label, _)| label.as_str()),
        Some("widget")
    );
    assert_eq!(
        equations.first().map(|(label, _)| label.as_str()),
        Some("euler")
    );
}

#[test]
fn test_completion_reads_a_bare_role_as_the_default_role_in_force() {
    // Given — `py:func` made the default above the cursor
    let api = ".. py:module:: tools\n\n.. py:function:: run()\n";
    let text = ".. default-role:: py:func\n\nCall `\n";

    // When
    let shown = targets_at(
        "complete_default_role",
        &[("index.rst", ""), ("api.rst", api)],
        text,
        (2, 6),
    );

    // Then
    assert_eq!(
        shown,
        [("tools.run".to_string(), Some("tools.run".to_string()))]
    );
}

#[test]
fn test_completion_offers_nothing_for_a_bare_backtick_without_a_default_role() {
    // When / Then
    assert!(
        targets_at(
            "complete_title_reference",
            &[("index.rst", "")],
            "A `\n",
            (0, 3)
        )
        .is_empty()
    );
}

#[test]
fn test_completion_lists_an_inventory_s_entries_for_a_role_naming_it() {
    // Given — a project whose `conf.py` reads Python's inventory from a file
    let conf = "extensions = ['sphinx.ext.intersphinx']\nintersphinx_mapping = {'python': ('https://docs.python.org/3/', '_inv/python.inv')}\n";
    let workspace = Workspace::new(
        "complete_inventory",
        &[("conf.py", conf), ("index.rst", "")],
    );
    write_python_inventory(
        &workspace.root.join("_inv/python.inv"),
        &[("dict", "py:class", "library/stdtypes.html#$")],
    );
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    let text = "See :external+python:py:class:`\n";
    handle_notification(&mut state, workspace.open("index.rst", text));

    // When
    let response = handle_request(&mut state, completion(workspace.uri("index.rst"), 0, 31));

    // Then
    let list = list(&response).expect("items");
    assert_eq!(
        shown(&list),
        [("dict".to_string(), Some("dict".to_string()))]
    );
    assert_eq!(
        list.items[0]
            .label_details
            .as_ref()
            .and_then(|d| d.description.as_deref()),
        Some("(in Python v3.14)")
    );
}

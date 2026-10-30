//! Renaming a label, an entity id or an equation label: the capability the
//! server announces, the name `prepareRename` finds, the edit a rename
//! answers with across the project's files, and every refusal.

use std::collections::BTreeMap;

use super::test_support::*;
use lsp_types::{
    DocumentChanges, OneOf, Position, PrepareRenameResponse, Range, RenameClientCapabilities,
    TextDocumentClientCapabilities, TextEdit, WorkspaceEdit,
};

/// A section labelled `install` in `setup.rst`, on its fourth line.
const SETUP: &str = "Setup\n=====\n\n.. _install:\n\nInstalling\n==========\n\nText.\n";

/// Two uses of `install` in a document no editor holds open, one titled.
const GUIDE: &str =
    "Guide\n=====\n\nFirst :ref:`install`.\n\nThen :ref:`the steps <install>` again.\n";

/// A scanned server over `files`, with `notes.rst` written and opened at
/// `text`.
fn server_over(name: &str, files: &[(&str, &str)], text: &str) -> (Workspace, ServerState) {
    let mut files = files.to_vec();
    files.push(("notes.rst", text));
    let (workspace, mut state) = scanned(name, &files);
    handle_notification(&mut state, workspace.open("notes.rst", text));
    (workspace, state)
}

/// The document edits of `changes`, which a client reads back as plain
/// edits when every operation is one.
fn document_edits(changes: DocumentChanges) -> Vec<lsp_types::TextDocumentEdit> {
    match changes {
        DocumentChanges::Edits(edits) => edits,
        DocumentChanges::Operations(operations) => operations
            .into_iter()
            .map(|operation| match operation {
                lsp_types::DocumentChangeOperation::Edit(edit) => edit,
                lsp_types::DocumentChangeOperation::Op(_) => panic!("no file operation"),
            })
            .collect(),
    }
}

/// The name of the file `uri` names.
fn file_name(uri: &lsp_types::Uri) -> String {
    uri.path()
        .as_str()
        .rsplit('/')
        .next()
        .expect("a name")
        .to_string()
}

/// Every edit of `edit`, by the name of the file it changes.
fn edits_by_file(edit: WorkspaceEdit) -> BTreeMap<String, Vec<TextEdit>> {
    let by_uri: Vec<(lsp_types::Uri, Vec<TextEdit>)> = match edit.document_changes {
        Some(changes) => document_edits(changes)
            .into_iter()
            .map(|edit| {
                let edits = edit
                    .edits
                    .into_iter()
                    .map(|edit| match edit {
                        OneOf::Left(edit) => edit,
                        OneOf::Right(annotated) => annotated.text_edit,
                    })
                    .collect();
                (edit.text_document.uri, edits)
            })
            .collect(),
        None => edit.changes.expect("changes").into_iter().collect(),
    };
    by_uri
        .into_iter()
        .map(|(uri, edits)| (file_name(&uri), edits))
        .collect()
}

/// Each file `edit` changes, by name, as its text on disk reads with the edit
/// applied. Every test text is ASCII, so a protocol column is a character.
fn applied(workspace: &Workspace, edit: WorkspaceEdit) -> BTreeMap<String, String> {
    edits_by_file(edit)
        .into_iter()
        .map(|(name, mut edits)| {
            let text = std::fs::read_to_string(workspace.root.join(&name)).expect("the file");
            let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
            edits.sort_by_key(|edit| {
                std::cmp::Reverse((edit.range.start.line, edit.range.start.character))
            });
            for edit in edits {
                assert_eq!(edit.range.start.line, edit.range.end.line, "{edit:?}");
                let line = &mut lines[edit.range.start.line as usize];
                let start = edit.range.start.character as usize;
                let end = edit.range.end.character as usize;
                line.replace_range(start..end, &edit.new_text);
            }
            (name, lines.join("\n") + "\n")
        })
        .collect()
}

/// The texts renaming at `line`/`character` of `notes.rst` to `new_name`
/// leaves, by file.
fn renamed(
    workspace: &Workspace,
    state: &mut ServerState,
    line: u32,
    character: u32,
    new_name: &str,
) -> BTreeMap<String, String> {
    renamed_in(workspace, state, "notes.rst", (line, character), new_name)
}

/// The texts renaming at `at` (line, character) of `file` to `new_name`
/// leaves, by file.
fn renamed_in(
    workspace: &Workspace,
    state: &mut ServerState,
    file: &str,
    at: (u32, u32),
    new_name: &str,
) -> BTreeMap<String, String> {
    let edit = rename_at(state, workspace.uri(file), at.0, at.1, new_name)
        .expect("a rename")
        .expect("an edit");
    applied(workspace, edit)
}

fn texts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
        .collect()
}

#[test]
fn test_initialize_result_announces_rename_with_prepare_when_the_client_prepares() {
    // Given
    let preparing = ClientCapabilities {
        text_document: Some(TextDocumentClientCapabilities {
            rename: Some(RenameClientCapabilities {
                prepare_support: Some(true),
                ..RenameClientCapabilities::default()
            }),
            ..TextDocumentClientCapabilities::default()
        }),
        ..ClientCapabilities::default()
    };

    // When
    let (plain, _) = initialize_result(&ClientCapabilities::default());
    let (prepared, _) = initialize_result(&preparing);

    // Then
    assert_eq!(plain.capabilities.rename_provider, Some(OneOf::Left(true)));
    assert!(matches!(
        prepared.capabilities.rename_provider,
        Some(OneOf::Right(lsp_types::RenameOptions {
            prepare_provider: Some(true),
            ..
        }))
    ));
}

#[test]
fn test_rename_of_a_label_rewrites_its_definition_and_every_use() {
    // Given
    let (workspace, mut state) = server_over(
        "rename_label",
        &[("setup.rst", SETUP), ("guide.rst", GUIDE)],
        "See :ref:`install` now.\n",
    );

    // When — the cursor inside the role
    let after = renamed(&workspace, &mut state, 0, 8, "setup-guide");

    // Then — the target alone in each, titles kept
    assert_eq!(
        after,
        texts(&[
            (
                "guide.rst",
                "Guide\n=====\n\nFirst :ref:`setup-guide`.\n\n\
                 Then :ref:`the steps <setup-guide>` again.\n"
            ),
            ("notes.rst", "See :ref:`setup-guide` now.\n"),
            (
                "setup.rst",
                "Setup\n=====\n\n.. _setup-guide:\n\nInstalling\n==========\n\nText.\n"
            ),
        ])
    );
}

#[test]
fn test_rename_rewrites_named_references_and_leaves_one_through_an_alias() {
    // Given — `docs_` reaches the label through `.. _docs: install_`
    let text = ".. _install:\n\nSee install_, `Install`_ and `the guide <install_>`_.\n\n\
                .. _docs: install_\n\nAnd docs_.\n";
    let (workspace, mut state) = server_over("rename_named", &[], text);

    // When — on the label's name
    let after = renamed(&workspace, &mut state, 0, 6, "setup guide");

    // Then — a bare reference gains backquotes for a name with a blank
    assert_eq!(
        after,
        texts(&[(
            "notes.rst",
            ".. _setup guide:\n\nSee `setup guide`_, `setup guide`_ and \
             `the guide <setup guide_>`_.\n\n.. _docs: `setup guide`_\n\nAnd docs_.\n"
        )])
    );
}

#[test]
fn test_rename_of_an_equation_rewrites_its_label_and_its_references() {
    // Given
    let text = ".. math::\n   :label: euler\n\n   e^{i\\pi} = -1\n\nBy :eq:`euler`.\n";
    let (workspace, mut state) = server_over("rename_equation", &[], text);

    // When — on `:label: euler`
    let after = renamed(&workspace, &mut state, 1, 12, "identity");

    // Then
    assert_eq!(
        after,
        texts(&[(
            "notes.rst",
            ".. math::\n   :label: identity\n\n   e^{i\\pi} = -1\n\nBy :eq:`identity`.\n"
        )])
    );
}

#[test]
fn test_rename_of_a_directive_s_name_rewrites_the_option_value() {
    // Given — a toctree, which has no span of its own, named `tree`
    let text = ".. toctree::\n   :name: tree\n\nSee :ref:`the tree <tree>`.\n";
    let (workspace, mut state) = server_over("rename_option", &[], text);

    // When — on the reference
    let after = renamed(&workspace, &mut state, 3, 8, "contents");

    // Then
    assert_eq!(
        after,
        texts(&[(
            "notes.rst",
            ".. toctree::\n   :name: contents\n\nSee :ref:`the tree <contents>`.\n"
        )])
    );
}

#[test]
fn test_prepare_rename_is_the_name_under_the_cursor_with_its_spelling() {
    // Given
    let (workspace, mut state) = server_over(
        "rename_prepare",
        &[("setup.rst", SETUP)],
        "See :ref:`Steps <Install>` now.\n",
    );

    // When — on the role's title
    let prepared = prepare_rename_at(&mut state, workspace.uri("notes.rst"), 0, 12);

    // Then — the target, as written
    assert_eq!(
        prepared,
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(Position::new(0, 17), Position::new(0, 24)),
            placeholder: "Install".to_string(),
        }))
    );
}

#[test]
fn test_prepare_rename_is_null_where_nothing_is_named() {
    // Given
    let (workspace, mut state) =
        server_over("rename_nothing", &[("setup.rst", SETUP)], "Plain text.\n");

    // When / Then
    assert_eq!(
        prepare_rename_at(&mut state, workspace.uri("notes.rst"), 0, 3),
        Ok(None)
    );
}

#[test]
fn test_rename_versions_each_open_document_when_the_client_takes_document_changes() {
    // Given
    let workspace = Workspace::new(
        "rename_versions",
        &[("setup.rst", SETUP), ("notes.rst", "See :ref:`install`.\n")],
    );
    let (state, _) = workspace.server(false);
    let mut state = state.with_edit_shape(super::renames::EditShape::Versioned);
    state.on_scan_event(workspace.scanned());
    handle_notification(
        &mut state,
        workspace.open("notes.rst", "See :ref:`install`.\n"),
    );

    // When
    let edit = rename_at(&mut state, workspace.uri("notes.rst"), 0, 8, "setup")
        .expect("a rename")
        .expect("an edit");

    // Then — the open document at its version, the closed one at none
    let changes = edit.document_changes.expect("document changes");
    let versions: Vec<(String, Option<i32>)> = document_edits(changes)
        .into_iter()
        .map(|edit| {
            (
                file_name(&edit.text_document.uri),
                edit.text_document.version,
            )
        })
        .collect();
    assert_eq!(
        versions,
        vec![
            ("notes.rst".to_string(), Some(1)),
            ("setup.rst".to_string(), None)
        ]
    );
}

#[test]
fn test_rename_refuses_a_document_a_term_and_a_section() {
    // Given
    let text = "Notes\n=====\n\n:doc:`setup` and :term:`API`.\n\n.. glossary::\n\n   API\n      A contract.\n";
    let (workspace, mut state) = server_over("rename_kinds", &[("setup.rst", SETUP)], text);
    let notes = workspace.uri("notes.rst");

    // When / Then
    assert_eq!(
        rename_at(&mut state, notes.clone(), 3, 8, "x").map(|_| ()),
        Err("a document is renamed by renaming its file".to_string())
    );
    assert_eq!(
        rename_at(&mut state, notes.clone(), 3, 27, "x").map(|_| ()),
        Err("a glossary term is its own text; edit the glossary".to_string())
    );
    assert_eq!(
        prepare_rename_at(&mut state, notes, 0, 2).map(|_| ()),
        Err("a section is named by its title; give it a label to rename that".to_string())
    );
}

#[test]
fn test_rename_refuses_a_name_already_defined_or_unwritable() {
    // Given — `setup` is a label too
    let text = ".. _install:\n\n.. _setup:\n\nSee :ref:`install`.\n";
    let (workspace, mut state) = server_over("rename_taken", &[], text);
    let notes = workspace.uri("notes.rst");

    // When / Then
    assert_eq!(
        rename_at(&mut state, notes.clone(), 4, 12, "Setup").map(|_| ()),
        Err("'Setup' is already defined in this project".to_string())
    );
    assert_eq!(
        rename_at(&mut state, notes.clone(), 4, 12, "py:mod").map(|_| ()),
        Err("'py:mod': a label cannot hold a backquote, an angle bracket or a colon".to_string())
    );
    // Its own name, cased differently, is no other definition.
    assert!(rename_at(&mut state, notes, 4, 12, "INSTALL").is_ok());
}

#[test]
fn test_rename_refuses_a_label_two_documents_define() {
    // Given
    let (workspace, mut state) = server_over(
        "rename_ambiguous",
        &[
            ("setup.rst", SETUP),
            ("again.rst", ".. _install:\n\nAgain.\n"),
        ],
        ".. _local:\n\nText.\n",
    );
    handle_notification(&mut state, workspace.open("setup.rst", SETUP));

    // When — on `.. _install:` in setup.rst
    let refused = rename_at(&mut state, workspace.uri("setup.rst"), 3, 6, "x");

    // Then
    assert_eq!(
        refused.map(|_| ()),
        Err("'install' is defined more than once in this project, so it names nothing".to_string())
    );
}

#[test]
fn test_rename_refuses_when_a_closed_file_no_longer_reads_as_its_analysis() {
    // Given — guide.rst changed on disk after the scan, unseen
    let (workspace, mut state) = server_over(
        "rename_stale",
        &[("setup.rst", SETUP), ("guide.rst", GUIDE)],
        "See :ref:`install` now.\n",
    );
    workspace.write("guide.rst", &GUIDE.replace("First :ref:", "At first :ref:"));

    // When
    let refused = rename_at(&mut state, workspace.uri("notes.rst"), 0, 8, "setup");

    // Then — nothing is offered
    let guide = workspace.root.join("guide.rst");
    assert_eq!(
        refused.map(|_| ()),
        Err(format!(
            "line 4 of {} no longer reads as 'install'; save it and try again",
            guide.display()
        ))
    );
}

/// Uses of `R_1` in `index.rst`: its role, a `:ref:` to its label, a
/// relation listing it second, and an update of it.
const ENTITY_USES: &str = "See :need:`R_1` and :ref:`r_1`.\n\n\
.. spec:: Discs\n   :id: S_2\n   :implements: S_1, R_1\n\n\
.. entity-update:: R_1\n   :priority: low\n";

#[test]
fn test_rename_of_an_entity_rewrites_its_id_wherever_it_is_written() {
    // Given
    let (workspace, mut state) = project_with("rename_entity", ENTITY_USES);

    // When — on the need role
    let after = renamed_in(&workspace, &mut state, "index.rst", (0, 12), "R_9");

    // Then — the label a `:ref:` writes takes the new id too
    assert_eq!(
        after,
        texts(&[
            (
                "index.rst",
                "See :need:`R_9` and :ref:`R_9`.\n\n.. spec:: Discs\n   :id: S_2\n   \
                 :implements: S_1, R_9\n\n.. entity-update:: R_9\n   :priority: low\n"
            ),
            (
                "reqs.rst",
                &REQS
                    .replace(":id: R_1", ":id: R_9")
                    .replace(":implements: R_1", ":implements: R_9")
            ),
        ])
    );
}

#[test]
fn test_rename_refuses_an_entity_id_taken_or_never_written() {
    // Given — an entity whose id is generated
    let text = ".. req:: Unnamed\n\nSee :need:`R_1`.\n";
    let (workspace, mut state) = project_with("rename_entity_refused", text);
    let index = workspace.uri("index.rst");

    // When
    let taken = rename_at(&mut state, index.clone(), 2, 12, "S_1");
    let generated = rename_at(&mut state, index, 0, 4, "R_2");

    // Then
    assert_eq!(
        taken.map(|_| ()),
        Err("'S_1' is already defined in this project".to_string())
    );
    let refusal = generated.expect_err("a refusal");
    assert!(
        refusal.ends_with(
            "is composed, generated or imported; write it as the entity's `:id:` to rename it"
        ),
        "{refusal}"
    );
}

#[test]
fn test_rename_refuses_a_label_conf_py_s_prolog_uses() {
    // Given — every document starts with the prolog's reference
    let conf = "rst_prolog = 'Also :ref:`install`.'\n";
    let (workspace, mut state) = server_over(
        "rename_prolog",
        &[("conf.py", conf), ("setup.rst", SETUP)],
        "See :ref:`install` now.\n",
    );

    // When
    let refused = rename_at(&mut state, workspace.uri("notes.rst"), 0, 8, "setup");

    // Then
    assert_eq!(
        refused.map(|_| ()),
        Err(
            "'install' is used in conf.py's rst_prolog or rst_epilog, which a rename does not \
             edit"
                .to_string()
        )
    );
}

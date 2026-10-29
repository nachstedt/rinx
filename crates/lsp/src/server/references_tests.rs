//! Finding every use of a definition: the capability the server announces,
//! and what a references request is answered with from the workspace's
//! documents, open or not, resolved as their pages would link.

use super::test_support::*;
use lsp_types::{Location, OneOf, Position, Range};

/// A section labelled `install` in `setup.rst`, on its fourth line.
const SETUP: &str = "Setup\n=====\n\n.. _install:\n\nInstalling\n==========\n\nText.\n";

/// Two uses of `install` in a document no editor holds open.
const GUIDE: &str =
    "Guide\n=====\n\nFirst :ref:`install`.\n\n.. note::\n\n   Then :ref:`install` again.\n";

/// The range from `start` to `end` on the 0-based `line`.
fn on_line(line: u32, start: u32, end: u32) -> Range {
    Range::new(Position::new(line, start), Position::new(line, end))
}

/// A scanned server over `files`, with `notes.rst` opened at `text`.
fn server_over(name: &str, files: &[(&str, &str)], text: &str) -> (Workspace, ServerState) {
    let mut files = files.to_vec();
    files.push(("notes.rst", text));
    let (workspace, mut state) = scanned(name, &files);
    handle_notification(&mut state, workspace.open("notes.rst", text));
    (workspace, state)
}

/// The uses of `install` in [`GUIDE`] and on the first line of `notes.rst`.
fn install_uses(workspace: &Workspace) -> Vec<Location> {
    vec![
        Location::new(workspace.uri("guide.rst"), on_line(3, 6, 20)),
        Location::new(workspace.uri("guide.rst"), on_line(7, 8, 22)),
        Location::new(workspace.uri("notes.rst"), on_line(0, 4, 18)),
    ]
}

#[test]
fn test_initialize_result_announces_references() {
    // When
    let (result, _) = initialize_result(&ClientCapabilities::default());

    // Then
    assert_eq!(
        result.capabilities.references_provider,
        Some(OneOf::Left(true))
    );
}

#[test]
fn test_references_of_a_ref_are_every_use_of_its_label_open_or_not() {
    // Given
    let (workspace, mut state) = server_over(
        "references_ref",
        &[("setup.rst", SETUP), ("guide.rst", GUIDE)],
        "See :ref:`install` now.\n",
    );

    // When — the cursor inside the role
    let found = references_at(&mut state, workspace.uri("notes.rst"), 0, 8, false);

    // Then
    assert_eq!(found, Some(install_uses(&workspace)));
}

#[test]
fn test_references_include_the_declaration_when_asked() {
    // Given
    let (workspace, mut state) = server_over(
        "references_declaration",
        &[("setup.rst", SETUP), ("guide.rst", GUIDE)],
        "See :ref:`install` now.\n",
    );

    // When
    let found = references_at(&mut state, workspace.uri("notes.rst"), 0, 8, true);

    // Then — the label's line, in file order among the uses
    let mut expected = install_uses(&workspace);
    expected.push(Location::new(workspace.uri("setup.rst"), on_line(3, 0, 12)));
    assert_eq!(found, Some(expected));
}

#[test]
fn test_references_of_a_label_s_own_line_are_its_uses() {
    // Given
    let (workspace, mut state) = server_over(
        "references_label_line",
        &[("setup.rst", SETUP), ("guide.rst", GUIDE)],
        "See :ref:`install` now.\n",
    );
    handle_notification(&mut state, workspace.open("setup.rst", SETUP));

    // When — the cursor on `.. _install:`
    let found = references_at(&mut state, workspace.uri("setup.rst"), 3, 6, false);

    // Then
    assert_eq!(found, Some(install_uses(&workspace)));
}

#[test]
fn test_references_of_a_domain_role_are_the_uses_of_that_object_alone() {
    // Given — `run` names `pkg.run` in one module and `other.run` in another
    let api = "API\n===\n\n.. py:function:: pkg.run()\n\n.. py:function:: other.run()\n";
    let usage = "Usage\n=====\n\n.. py:currentmodule:: pkg\n\n:py:func:`run`\n\n\
                 .. py:currentmodule:: other\n\n:py:func:`run`\n";
    let (workspace, mut state) = server_over(
        "references_domain",
        &[("api.rst", api), ("usage.rst", usage)],
        ":py:func:`pkg.run`\n",
    );

    // When
    let found = references_at(&mut state, workspace.uri("notes.rst"), 0, 12, false);

    // Then
    assert_eq!(
        found,
        Some(vec![
            Location::new(workspace.uri("notes.rst"), on_line(0, 0, 18)),
            Location::new(workspace.uri("usage.rst"), on_line(5, 0, 14)),
        ])
    );
}

#[test]
fn test_references_of_a_document_are_its_doc_roles_and_toctree_entries() {
    // Given
    let index = "Index\n=====\n\n.. toctree::\n\n   setup\n";
    let (workspace, mut state) = server_over(
        "references_document",
        &[("setup.rst", SETUP), ("index.rst", index)],
        "Read :doc:`setup`.\n",
    );

    // When
    let found = references_at(&mut state, workspace.uri("notes.rst"), 0, 8, true);

    // Then — a document has no declaration to add
    assert_eq!(
        found,
        Some(vec![
            Location::new(workspace.uri("index.rst"), on_line(5, 3, 8)),
            Location::new(workspace.uri("notes.rst"), on_line(0, 5, 17)),
        ])
    );
}

#[test]
fn test_references_list_a_use_written_in_an_included_fragment() {
    // Given
    let index = "Index\n=====\n\n.. include:: part.inc\n";
    let (workspace, mut state) = server_over(
        "references_fragment",
        &[
            ("setup.rst", SETUP),
            ("index.rst", index),
            ("part.inc", "Also :ref:`install`.\n"),
        ],
        "See :ref:`install` now.\n",
    );

    // When
    let found = references_at(&mut state, workspace.uri("notes.rst"), 0, 8, false);

    // Then
    assert_eq!(
        found,
        Some(vec![
            Location::new(workspace.uri("notes.rst"), on_line(0, 4, 18)),
            Location::new(workspace.uri("part.inc"), on_line(0, 5, 19)),
        ])
    );
}

#[test]
fn test_references_follow_an_edit_adding_a_use() {
    // Given — the uses resolved once
    let (workspace, mut state) = server_over(
        "references_edit",
        &[("setup.rst", SETUP)],
        "See :ref:`install` now.\n",
    );
    let before = references_at(&mut state, workspace.uri("notes.rst"), 0, 8, false);

    // When — a second use typed into the open document
    handle_notification(
        &mut state,
        workspace.change(
            "notes.rst",
            2,
            "See :ref:`install` now.\n\nAnd :ref:`install`.\n",
        ),
    );
    let after = references_at(&mut state, workspace.uri("notes.rst"), 0, 8, false);

    // Then
    assert_eq!(before.map(|found| found.len()), Some(1));
    assert_eq!(
        after,
        Some(vec![
            Location::new(workspace.uri("notes.rst"), on_line(0, 4, 18)),
            Location::new(workspace.uri("notes.rst"), on_line(2, 4, 18)),
        ])
    );
}

#[test]
fn test_references_drop_a_use_whose_label_became_ambiguous() {
    // Given — the uses resolved once
    let (workspace, mut state) = server_over(
        "references_ambiguous",
        &[("setup.rst", SETUP), ("guide.rst", GUIDE)],
        "See :ref:`install` now.\n",
    );
    references_at(&mut state, workspace.uri("notes.rst"), 0, 8, false);

    // When — a second document defines the label too
    handle_notification(
        &mut state,
        workspace.change("notes.rst", 2, ".. _install:\n\nSee :ref:`install` now.\n"),
    );

    // Then — it leads nowhere, so neither the reference nor the label has uses
    assert_eq!(
        references_at(&mut state, workspace.uri("notes.rst"), 2, 8, false),
        None
    );
}

#[test]
fn test_references_are_none_on_plain_text_or_a_broken_reference() {
    // Given
    let (workspace, mut state) = server_over(
        "references_none",
        &[("setup.rst", SETUP)],
        "Plain :ref:`ghost` text.\n",
    );

    // When / Then
    assert_eq!(
        references_at(&mut state, workspace.uri("notes.rst"), 0, 2, false),
        None
    );
    assert_eq!(
        references_at(&mut state, workspace.uri("notes.rst"), 0, 10, false),
        None
    );
}

/// A numbered table and code block, each named by its `:name:` line, and a
/// labelled equation.
const NAMED: &str = "Named\n=====\n\n.. list-table:: The formats\n   :name: formats\n   :header-rows: 1\n\n   \
                     * - Kind\n   * - figure\n\n.. code-block:: python\n   :caption: A listing\n   \
                     :name: listing\n\n   print()\n\n.. math::\n   :label: euler\n\n   e^{i\\pi} = -1\n";

/// A scanned project numbering its figures, with [`NAMED`] and `notes.rst`
/// open, the latter at `text`.
fn named_project(name: &str, text: &str) -> (Workspace, ServerState) {
    let (workspace, mut state) = server_over(
        name,
        &[("conf.py", "numfig = True\n"), ("named.rst", NAMED)],
        text,
    );
    handle_notification(&mut state, workspace.open("named.rst", NAMED));
    (workspace, state)
}

#[test]
fn test_references_of_a_name_line_are_the_uses_of_the_label_it_names() {
    // Given
    let (workspace, mut state) = named_project(
        "references_name_line",
        ":numref:`formats` and :ref:`Formats <formats>`\n",
    );

    // When — anywhere on `   :name: formats`
    let on_value = references_at(&mut state, workspace.uri("named.rst"), 4, 12, true);
    let on_option = references_at(&mut state, workspace.uri("named.rst"), 4, 4, true);

    // Then — both uses, and the table's marker the label is written on
    let expected = vec![
        Location::new(workspace.uri("named.rst"), on_line(3, 0, 27)),
        Location::new(workspace.uri("notes.rst"), on_line(0, 0, 17)),
        Location::new(workspace.uri("notes.rst"), on_line(0, 22, 46)),
    ];
    assert_eq!(on_value, Some(expected.clone()));
    assert_eq!(on_option, Some(expected));
}

#[test]
fn test_references_of_a_code_block_s_name_line_are_its_numrefs() {
    // Given
    let (workspace, mut state) = named_project("references_code_name", "See :numref:`listing`.\n");

    // When — on `   :name: listing`
    let found = references_at(&mut state, workspace.uri("named.rst"), 12, 10, false);

    // Then
    assert_eq!(
        found,
        Some(vec![Location::new(
            workspace.uri("notes.rst"),
            on_line(0, 4, 21)
        )])
    );
}

#[test]
fn test_references_of_a_math_label_line_are_its_eq_roles() {
    // Given
    let (workspace, mut state) = named_project("references_math_label", "By :eq:`euler`.\n");

    // When — on `   :label: euler`
    let found = references_at(&mut state, workspace.uri("named.rst"), 17, 12, false);

    // Then
    assert_eq!(
        found,
        Some(vec![Location::new(
            workspace.uri("notes.rst"),
            on_line(0, 3, 14)
        )])
    );
}

#[test]
fn test_references_of_a_toctree_s_name_line_are_found_without_a_site() {
    // Given — a toctree's `:name:` records no site to stand on
    let index = "Index\n=====\n\n.. toctree::\n   :name: contents-tree\n\n   setup\n";
    let (workspace, mut state) = server_over(
        "references_toctree_name",
        &[("setup.rst", SETUP), ("index.rst", index)],
        "See :ref:`the tree <contents-tree>`.\n",
    );
    handle_notification(&mut state, workspace.open("index.rst", index));

    // When
    let found = references_at(&mut state, workspace.uri("index.rst"), 4, 12, false);

    // Then
    assert_eq!(
        found,
        Some(vec![Location::new(
            workspace.uri("notes.rst"),
            on_line(0, 4, 35)
        )])
    );
}

#[test]
fn test_references_of_a_name_line_naming_another_document_s_label_are_none() {
    // Given — `install` is `setup.rst`'s, not this table's
    let text = ".. list-table:: Copy\n   :name: install\n\n   * - x\n";
    let (workspace, mut state) =
        server_over("references_foreign_name", &[("setup.rst", SETUP)], "");
    handle_notification(&mut state, workspace.open("notes.rst", text));

    // When / Then — two documents define it, so it names nothing
    assert_eq!(
        references_at(&mut state, workspace.uri("notes.rst"), 1, 12, false),
        None
    );
}

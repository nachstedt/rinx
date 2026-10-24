//! A definition's line is the editor's to know, never the index's: moving
//! lines must leave the `.index` a Bazel build writes byte for byte as it
//! was, or every page would re-render (`docs/decisions/045-definition-spans.md`).
//!
//! Toctree entries and entity relations and updates are left out of the
//! fixture: the index already carries their lines, as the ADR records.

use rinx_analyzer::{DocumentAnalysis, build_project_index};
use rinx_ast::Document;
use rinx_entity::EntitySchema;

/// One document defining every kind of definition this step positions.
const GUIDE: &str = "\
Guide
=====

.. _install:

Installing
----------

See :ref:`install` and :py:func:`shapes.Circle.area`.

.. py:module:: shapes

.. py:class:: Circle

   .. py:method:: area()

      The area.

.. c:function:: int draw(void)

.. program:: shapes-cli

.. option:: -v, --verbose

   Talk more.

.. glossary::

   API
   interface
      A contract.

.. list-table:: Sizes
   :name: sizes

   * - small
     - 1

.. code-block:: python
   :name: example
   :caption: An example

   print(1)

.. math::
   :label: energy

   e = mc^2
";

/// `source` parsed as the build's parse action would write it, and read back
/// as its index action does.
fn parsed(source: &str) -> Document {
    let doc = rinx_parser::parse("guide.rst", source);
    serde_json::from_str(&serde_json::to_string(&doc).expect("serializes")).expect("deserializes")
}

/// The `.index` the build writes for the one document.
fn index_bytes(doc: Document) -> String {
    let index = build_project_index(&[doc], "guide", &EntitySchema::empty());
    serde_json::to_string(&index).expect("serializes")
}

/// `GUIDE` with every definition moved down: blank lines above the first
/// line, and another wherever one stood.
fn shifted() -> String {
    format!("\n\n\n{}", GUIDE.replace("\n\n", "\n\n\n"))
}

#[test]
fn test_moving_lines_leaves_the_written_index_unchanged() {
    // When
    let original = index_bytes(parsed(GUIDE));
    let moved = index_bytes(parsed(&shifted()));

    // Then
    assert_eq!(original, moved);
}

#[test]
fn test_moving_lines_moves_every_definition_site() {
    // Given
    let original = DocumentAnalysis::of(&parsed(GUIDE));
    let moved = DocumentAnalysis::of(&parsed(&shifted()));

    // When
    let lines = |analysis: &DocumentAnalysis| {
        analysis
            .definition_sites()
            .iter()
            .map(|(key, site)| (key.clone(), site.start.line))
            .collect::<Vec<_>>()
    };

    // Then — the same definitions, each further down
    let (before, after) = (lines(&original), lines(&moved));
    assert_eq!(before.len(), after.len());
    assert!(before.len() >= 14, "{before:?}");
    for ((key, old), (moved_key, new)) in before.iter().zip(&after) {
        assert_eq!(key, moved_key);
        assert!(new > old, "{key:?} stayed at line {old}");
    }
}

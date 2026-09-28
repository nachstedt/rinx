//! The `:numref:` role end to end: the three-document project captured from
//! `sphinx-build` 9.1 while designing the role (ADR-028), with and without a
//! `:numbered:` toctree and at three `numfig_secnum_depth`s. Every number
//! asserted here is the one Sphinx printed for the same source.

use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_entity::EntitySchema;
use rinx_parser as parser;
use rinx_renderer as renderer;

/// The root: a labelled figure, the toctree, then a named and an unlabelled
/// figure — so the children are numbered *between* the root's own figures.
fn index_source(numbered: bool) -> String {
    let numbered = if numbered { "   :numbered:\n" } else { "" };
    format!(
        "\
Root
====

.. _fig-root:

.. figure:: a.png

   Root caption

.. toctree::
{numbered}
   one
   two

.. figure:: a.png
   :name: fig-after

   After toctree

.. figure:: a.png

   Unlabelled

Refs: :numref:`fig-root`, :numref:`fig-after`, :numref:`tab-list`,
:numref:`code-cap`, :numref:`sec-one`, :numref:`doc-two`, :numref:`deep-label`,
:numref:`Figure {{number}} ({{name}}) <fig-root>`.
"
    )
}

const ONE: &str = "\
One
===

.. _sec-one:

Sub one
-------

.. list-table:: List caption
   :name: tab-list

   * - a

.. table:: Table directive

   +---+
   | y |
   +---+

.. code-block:: python
   :caption: Code caption
   :name: code-cap

   x = 1

Deep
~~~~

.. _deep-label:

Deeper
^^^^^^

.. figure:: a.png

   deep fig
";

const TWO: &str = "\
.. _doc-two:

Two
===

.. figure:: a.png
   :name: fig-two

   Two caption
";

/// Parses the project, indexes it at `depth` and renders every page with
/// `numfig` on, returning the pages in `index`, `one`, `two` order.
fn build(numbered: bool, depth: usize) -> Vec<renderer::RenderOutput> {
    let docs = vec![
        parser::parse("index.rst", &index_source(numbered)),
        parser::parse("one.rst", ONE),
        parser::parse("two.rst", TWO),
    ];
    for doc in &docs {
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    }
    let settings = analyzer::IndexSettings {
        root_doc: "index",
        numfig_secnum_depth: depth,
    };
    let index =
        analyzer::build_project_index_reporting(&docs, &settings, &EntitySchema::empty()).index;
    let config = renderer::config::SiteConfig {
        numfig: true,
        ..renderer::config::SiteConfig::default()
    };
    docs.iter()
        .map(|doc: &ast::Document| renderer::render_with_config(doc, &index, &doc.path, &config))
        .collect()
}

/// The caption numbers of a page, in order.
fn caption_numbers(html: &str) -> Vec<&str> {
    html.split("<span class=\"caption-number\">")
        .skip(1)
        .map(|rest| rest.split(" </span>").next().unwrap_or_default())
        .collect()
}

/// The text of every resolved `:numref:` on a page, in order.
fn numref_texts(html: &str) -> Vec<&str> {
    html.split("<span class=\"std std-numref\">")
        .skip(1)
        .map(|rest| rest.split("</span>").next().unwrap_or_default())
        .collect()
}

#[test]
fn test_e2e_numref_counts_across_documents_without_numbered_sections() {
    // Given / When
    let pages = build(false, 1);

    // Then — as Sphinx: the children's figures fall between the root's own
    assert_eq!(
        caption_numbers(&pages[0].html),
        ["Fig. 1", "Fig. 4", "Fig. 5"]
    );
    assert_eq!(
        caption_numbers(&pages[1].html),
        ["Table 1", "Table 2", "Listing 1", "Fig. 2"]
    );
    assert_eq!(caption_numbers(&pages[2].html), ["Fig. 3"]);
    assert_eq!(
        numref_texts(&pages[0].html),
        [
            "Fig. 1",
            "Fig. 4",
            "Table 1",
            "Listing 1",
            "Figure 1 (Root caption)"
        ]
    );
    // …and no section is numbered, so the three section references are not
    let unnumbered: Vec<&str> = pages[0]
        .broken_links
        .iter()
        .map(|link| link.target.as_str())
        .collect();
    assert_eq!(unnumbered, ["sec-one", "doc-two", "deep-label"]);
    assert!(
        pages[0]
            .broken_links
            .iter()
            .all(|link| link.kind == renderer::BrokenLinkKind::UnnumberedReference)
    );
}

#[test]
fn test_e2e_numref_restarts_under_each_chapter_at_depth_one() {
    // Given / When
    let pages = build(true, 1);

    // Then — Sphinx's numbers for `:numbered:` with numfig_secnum_depth = 1
    assert_eq!(
        caption_numbers(&pages[0].html),
        ["Fig. 1", "Fig. 2", "Fig. 3"]
    );
    assert_eq!(
        caption_numbers(&pages[1].html),
        ["Table 1.1", "Table 1.2", "Listing 1.1", "Fig. 1.1"]
    );
    assert_eq!(caption_numbers(&pages[2].html), ["Fig. 2.1"]);
    assert!(
        pages[0].broken_links.is_empty(),
        "{:?}",
        pages[0].broken_links
    );
    assert_eq!(
        numref_texts(&pages[0].html),
        [
            "Fig. 1",
            "Fig. 2",
            "Table 1.1",
            "Listing 1.1",
            "Section 1.1",
            "Section 2",
            "Section 1.1.1.1",
            "Figure 1 (Root caption)"
        ]
    );
}

#[test]
fn test_e2e_numref_follows_numfig_secnum_depth() {
    // Given / When
    let flat = build(true, 0);
    let deep = build(true, 2);

    // Then — Sphinx's numbers at depth 0 and depth 2
    assert_eq!(
        caption_numbers(&flat[0].html),
        ["Fig. 1", "Fig. 4", "Fig. 5"]
    );
    assert_eq!(
        caption_numbers(&flat[1].html),
        ["Table 1", "Table 2", "Listing 1", "Fig. 2"]
    );
    assert_eq!(
        caption_numbers(&deep[1].html),
        ["Table 1.1.1", "Table 1.1.2", "Listing 1.1.1", "Fig. 1.1.1"]
    );
    assert_eq!(caption_numbers(&deep[2].html), ["Fig. 2.1"]);
}

#[test]
fn test_e2e_numref_links_to_the_label_on_the_other_page() {
    // Given / When
    let pages = build(true, 1);

    // Then
    let html = &pages[0].html;
    for expected in [
        "<a class=\"reference internal\" href=\"index.html#fig-root\">\
         <span class=\"std std-numref\">Fig. 1</span></a>",
        "<a class=\"reference internal\" href=\"one.html#tab-list\">\
         <span class=\"std std-numref\">Table 1.1</span></a>",
    ] {
        assert!(html.contains(expected), "expected {expected} in:\n{html}");
    }
}

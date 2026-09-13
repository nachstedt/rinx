//! `.. if-builder::` across the whole pipeline: what a selected block
//! contributes to the index and the page, and what an excluded one costs.
//!
//! The unit tests in `rusty_sphinx_parser` assert on the nodes; these assert
//! on the two things only the later phases can see — that a target written
//! inside a selected block is a *document* target a `:ref:` elsewhere
//! resolves against, and that one inside an excluded block is absent from the
//! index rather than resolving to something invisible.

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer as renderer;

/// Parses, indexes and renders one document, the way the three subcommands do.
fn build(input: &str) -> (ast::Document, renderer::RenderOutput) {
    let doc = parser::parse("guide.rst", input);
    let index = analyzer::analyze(&doc);
    let output = renderer::render(&doc, &index, &doc.path);
    (doc, output)
}

#[test]
fn test_e2e_a_selected_block_contributes_a_resolvable_target() {
    // Given — a label and a heading inside a block for this build's builder,
    // referenced from prose outside it.
    let (_, output) = build(
        "\
Guide
=====

.. if-builder:: html

   .. _install-steps:

   Install
   -------

   Run the installer.

See :ref:`install-steps` to begin.
",
    );

    // Then — the splice is transparent: the heading is a real section of the
    // document and the target resolves, neither of which could happen if the
    // body were wrapped in a container of its own.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("<h2 id=\"install\">Install</h2>"));
    assert!(output.html.contains("#install-steps"));
    // Upstream wraps the body in `nodes.container()`; this build deliberately
    // does not — see `parser::directives::if_builder`'s module comment.
    assert!(!output.html.contains("docutils container"));
}

#[test]
fn test_e2e_an_excluded_block_leaves_no_target_in_the_index() {
    // Given — the same label, but on a branch this build does not take.
    let doc = parser::parse(
        "guide.rst",
        "\
Guide
=====

.. if-builder:: simplepdf

   .. _pdf-only-steps:

   Print it.
",
    );
    let index = analyzer::analyze(&doc);

    // Then — nothing from the excluded branch is indexed, so a `:ref:` to it
    // is honestly broken rather than resolving to a page that never shows it.
    assert!(
        !index
            .targets
            .keys()
            .any(|name| name.as_str() == "pdf-only-steps"),
        "the excluded target reached the index: {:?}",
        index.targets.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_e2e_an_excluded_block_costs_the_page_nothing() {
    // Given — an excluded block holding a directive this build does not know.
    let (doc, output) = build(
        "\
Guide
=====

.. if-builder:: latex

   .. raw-latex-thing::

      \\begin{landscape}

Ordinary prose.
",
    );

    // Then — the body was never parsed, so there is no `directive.unknown`
    // report and no error block on the page. This is what makes a block for
    // another builder free rather than merely hidden.
    assert!(
        doc.diagnostics.is_empty(),
        "expected no diagnostics, got {:?}",
        doc.diagnostics
    );
    assert!(!output.html.contains("directive-error"));
    assert!(!output.html.contains("landscape"));
    assert!(output.html.contains("<p>Ordinary prose.</p>"));
}

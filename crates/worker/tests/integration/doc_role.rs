//! The `:doc:` role end to end: three documents in two directories, one of
//! them without a title, linked through the role in each of its forms. The
//! expectations follow what `sphinx-build` 9.1 renders for the same source,
//! with the deviations `docs/compatibility.rst` records.

use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_entity::EntitySchema;
use rinx_parser as parser;
use rinx_renderer as renderer;

const INDEX: &str = "\
Home
====

.. toctree::

   guide/intro
   notes
";

const NOTES: &str = "Just notes, without a title.\n";

/// Parses the three documents — `guide/intro.rst` holding `intro` — indexes
/// them together and renders the introduction.
fn render_intro(intro: &str) -> (ast::Document, renderer::RenderOutput) {
    let docs = vec![
        parser::parse("index.rst", INDEX),
        parser::parse("notes.rst", NOTES),
        parser::parse("guide/intro.rst", intro),
    ];
    for doc in &docs {
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    }
    let index = analyzer::build_project_index(&docs, "index", &EntitySchema::empty());
    let output = renderer::render(&docs[2], &index, &docs[2].path);
    let intro = docs
        .into_iter()
        .nth(2)
        .expect("three documents were parsed");
    (intro, output)
}

#[test]
fn test_e2e_doc_links_documents_relative_absolute_and_titleless() {
    // Given / When
    let (_, output) = render_intro(
        "\
Introduction *here*
===================

- :doc:`../notes`
- :doc:`/index`
- :std:doc:`/index`
- :doc:`The notes <../notes>`
- :doc:`intro`
",
    );

    // Then
    assert!(output.broken_links.is_empty(), "{:?}", output.broken_links);
    let html = &output.html;
    for expected in [
        "<a class=\"reference internal\" href=\"../notes.html\">\
         <span class=\"doc\">&lt;no title&gt;</span></a>",
        "<a class=\"reference internal\" href=\"../index.html\">\
         <span class=\"doc\">Home</span></a>",
        "<a class=\"reference internal\" href=\"../notes.html\">\
         <span class=\"doc\">The notes</span></a>",
        "<a class=\"reference internal\" href=\"intro.html\">\
         <span class=\"doc\">Introduction here</span></a>",
    ] {
        assert!(html.contains(expected), "expected {expected} in:\n{html}");
    }
}

#[test]
fn test_e2e_doc_with_a_bang_is_text_and_never_looked_up() {
    // Given / When — the target would not resolve if it were looked up
    let (_, output) = render_intro("Intro\n=====\n\n:doc:`!Somewhere <nowhere>`\n");

    // Then
    assert!(output.broken_links.is_empty(), "{:?}", output.broken_links);
    assert!(
        output
            .html
            .contains("<span class=\"xref std std-doc\">Somewhere &lt;nowhere&gt;</span>"),
        "{}",
        output.html
    );
}

#[test]
fn test_e2e_doc_reports_an_unknown_document_and_takes_a_noqa() {
    // Given — one unknown document, and a noqa naming the role's own code
    let source = "\
Intro
=====

:doc:`missing`

.. noqa: link.broken-doc

:doc:`../missing`
";

    // When
    let (intro, output) = render_intro(source);

    // Then — both are reported by the renderer; the noqa, accepted as a known
    // code, is what the worker filters the second one out with
    assert_eq!(intro.suppressions.len(), 1);
    let codes: Vec<_> = output
        .broken_links
        .iter()
        .map(renderer::BrokenLink::code)
        .collect();
    assert_eq!(
        codes,
        vec![
            ast::DiagnosticCode::LinkBrokenDoc,
            ast::DiagnosticCode::LinkBrokenDoc
        ]
    );
}

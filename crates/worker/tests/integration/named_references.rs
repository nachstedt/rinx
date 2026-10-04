//! Named hyperlink references (`` `name`_ ``) resolved end to end: parsed,
//! analysed and rendered the way docutils resolves them — within their own
//! document, against its explicit targets, its section titles and the
//! destinations embedded in references.

use rinx_analyzer as analyzer;
use rinx_parser as parser;
use rinx_renderer as renderer;

/// The rendered HTML of `input` as a document of its own, and the targets of
/// its broken links.
fn render(input: &str) -> (String, Vec<String>) {
    let ast = parser::parse("page.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);
    let broken = output
        .broken_links
        .into_iter()
        .map(|link| link.target)
        .collect();
    (output.html, broken)
}

#[test]
fn test_e2e_named_references_reach_every_kind_of_target_in_the_document() {
    // Given — the four shapes CPython writes that all used to link nowhere.
    let input = "\
Getting Started
===============

See `Getting Started`_, `the top <#getting-started>`_, `a wrapped
link <https://example.com>`_ and `an alias <python_>`_.

.. _python: https://python.org
";

    // When
    let (html, broken) = render(input);

    // Then
    assert!(broken.is_empty(), "unexpected broken links: {broken:?}");
    assert!(
        html.contains("<a href=\"#getting-started\">Getting Started</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"#getting-started\">the top</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"https://example.com\">a wrapped\nlink</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"https://python.org\">an alias</a>"),
        "{html}"
    );
}

#[test]
fn test_e2e_an_indirect_target_and_an_embedded_reference_define_names() {
    // Given — `tkinter.ttk.rst`'s indirect `.. _Layout: `Layouts`_`, and an
    // embedded reference whose text a later reference reuses.
    let input = "\
Read `Layout`_ first, then the `Python <https://python.org>`_ docs, and
`Python`_ again.

.. _Layouts: https://tkdocs.com/layouts
.. _Layout: `Layouts`_
";

    // When
    let (html, broken) = render(input);

    // Then
    assert!(broken.is_empty(), "unexpected broken links: {broken:?}");
    assert!(
        html.contains("<a href=\"https://tkdocs.com/layouts\">Layout</a>"),
        "{html}"
    );
    assert_eq!(
        html.matches("<a href=\"https://python.org\">").count(),
        2,
        "{html}"
    );
}

#[test]
fn test_e2e_an_alias_to_a_missing_name_is_a_broken_link() {
    // Given
    let input = "See `the guide <guide_>`_.\n";

    // When
    let (html, broken) = render(input);

    // Then
    assert_eq!(broken, ["guide"]);
    assert!(
        html.contains("class=\"broken-link\">the guide</a>"),
        "{html}"
    );
}

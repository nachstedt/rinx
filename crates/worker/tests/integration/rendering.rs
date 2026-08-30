//! Rendering output tests: escaping, tables (grid and list-table), and
//! enumerated lists, including cross-references and doctest blocks nested
//! inside list items.

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer as renderer;
use rusty_sphinx_worker::process_rst;

#[test]
fn test_e2e_escaped_space_empties_a_simple_table_first_cell() {
    // Given the escaped space RST prescribes for a deliberately empty
    // first-column cell — a blank one cannot open a row
    let input = "\
=====  ======
A      B
=====  ======
1      first
\\      second
=====  ======
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the cell renders empty rather than showing a backslash
    assert!(
        result.contains("<td><p></p>\n</td>\n<td><p>second</p>"),
        "expected an empty first cell, got:\n{result}"
    );
    assert!(!result.contains('\\'), "a backslash leaked into:\n{result}");
}

#[test]
fn test_e2e_backslash_escapes_are_removed_from_rendered_text() {
    // Given escapes in prose, an escaped space joining markup to its
    // neighbours, and a literal that must keep its backslash verbatim
    let input = "\
Keep \\*stars\\* as is, join foo\\ *bar*\\ baz, and a ``some\\path`` literal.
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    assert_eq!(
        result,
        "<p>Keep *stars* as is, join foo<em>bar</em>baz, and a <code>some\\path</code> literal.</p>\n"
    );
}

#[test]
fn test_e2e_simple_table_renders_header_body_and_column_span() {
    // Given a simple table with a header rule and a `-` span underline
    let input = "\
=====  =====
col 1  col 2
=====  =====
1      2
a span
------------
3      4
=====  =====
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    let expected_html = "\
<table>
<thead>
<tr>
<th><p>col 1</p>
</th>
<th><p>col 2</p>
</th>
</tr>
</thead>
<tbody>
<tr>
<td><p>1</p>
</td>
<td><p>2</p>
</td>
</tr>
<tr>
<td colspan=\"2\"><p>a span</p>
</td>
</tr>
<tr>
<td><p>3</p>
</td>
<td><p>4</p>
</td>
</tr>
</tbody>
</table>
";
    assert_eq!(result, expected_html);
}

#[test]
fn test_e2e_list_table_basic_renders_as_table() {
    // Given
    let input = "\
.. list-table::
   :header-rows: 1

   * - Fruit
     - Colour
   * - Apple
     - Red
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    let expected_html = "\
<table class=\"list-table\">
<thead>
<tr>
<th><p>Fruit</p>
</th>
<th><p>Colour</p>
</th>
</tr>
</thead>
<tbody>
<tr>
<td><p>Apple</p>
</td>
<td><p>Red</p>
</td>
</tr>
</tbody>
</table>
";
    assert_eq!(result, expected_html);
}

#[test]
fn test_e2e_csv_table_basic_renders_as_table() {
    // Given
    let input = "\
.. csv-table::
   :header-rows: 1

   Fruit, Colour
   Apple, Red
";

    // When
    let result = process_rst("test.rst", input);

    // Then — identical chrome to the list-table above apart from the class,
    // since both directives render through one code path.
    let expected_html = "\
<table class=\"csv-table\">
<thead>
<tr>
<th><p>Fruit</p>
</th>
<th><p>Colour</p>
</th>
</tr>
</thead>
<tbody>
<tr>
<td><p>Apple</p>
</td>
<td><p>Red</p>
</td>
</tr>
</tbody>
</table>
";
    assert_eq!(result, expected_html);
}

#[test]
fn test_e2e_csv_table_quoted_field_keeps_its_comma() {
    // Given
    let input = "\
.. csv-table::

   \"Apple, Braeburn\", Red
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    assert!(result.contains("<p>Apple, Braeburn</p>"), "{result}");
}

#[test]
fn test_e2e_csv_table_cell_content_is_reparsed_as_rst() {
    // Given
    let input = "\
.. csv-table::

   *Apple*, ``Red``
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    assert!(result.contains("<em>Apple</em>"), "{result}");
    assert!(result.contains("<code"), "{result}");
}

#[test]
fn test_e2e_list_table_known_bug_regression_nested_domain_object_resolves() {
    // Given — the known_bugs.md `reference/datamodel.rst` scenario: a
    // domain-object definition nested inside a list-table cell, referenced
    // elsewhere in the same document. Before `list-table` was implemented,
    // `.. py:attribute::` here was swallowed as opaque directive-body text
    // and the `:attr:` reference below would have rendered as broken.
    let input = "\
.. list-table::
   :header-rows: 1

   * - Attribute
     - Meaning
   * - .. py:attribute:: method.__self__

     - The instance to which a bound method is bound.

See :attr:`method.__self__` for details.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);

    // When
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert!(output.broken_links.is_empty());
    assert!(!output.html.contains("class=\"broken-link\""));
    assert!(output.html.contains(
        "<a class=\"reference internal\" href=\"test.html#py:attribute:method.__self__\">"
    ));
}

#[test]
fn test_e2e_enumerated_list_round_trips_from_rst_to_html() {
    // Given a parenthesised lower-roman list that does not start at one
    let input = "(iv) Fourth\n(v) Fifth\n";

    // When running it through the parse, analyze and render pipeline
    let doc = parser::parse("test.rst", input);
    let index = analyzer::analyze(&doc);
    let html = renderer::render(&doc, &index, &doc.path).html;

    // Then the sequence, format and start value all survive to the HTML
    assert_eq!(
        html,
        "<ol class=\"lowerroman parens\" start=\"4\" style=\"counter-reset: rsl 3\">\n\
         <li><p>Fourth</p>\n</li>\n<li><p>Fifth</p>\n</li>\n</ol>\n"
    );
}

#[test]
fn test_e2e_cross_reference_inside_an_enumerated_item_resolves() {
    // Given a document whose enumerated list item references a module defined
    // in the same document
    let input = "\
.. py:module:: widgets

.. py:function:: build()

   Builds a widget.

1. See :py:func:`build` for details.
2. Second item.
";

    // When indexing and rendering it
    let doc = parser::parse("test.rst", input);
    let index = analyzer::analyze(&doc);
    let output = renderer::render(&doc, &index, &doc.path);

    // Then the reference inside the list item resolves, proving the analyzer
    // descends into enumerated items rather than skipping the container
    assert!(output.broken_links.is_empty(), "{:?}", output.broken_links);
    assert!(
        output.html.contains("#py:function:widgets.build"),
        "{}",
        output.html
    );
}

#[test]
fn test_e2e_doctest_block_nested_in_an_enumerated_item_is_found() {
    // Given a doctest block written inside an enumerated list item
    let input = "1. Try it:\n\n   >>> 1 + 1\n   2\n";

    // When walking the parsed document
    let doc = parser::parse("test.rst", input);
    let mut found = 0;
    ast::walk_nodes(&doc.nodes, &mut |node| {
        if matches!(node, ast::Node::DoctestBlock(_)) {
            found += 1;
        }
    });

    // Then the walker reaches it, so doctest extraction and diagram collection
    // see content nested in enumerated lists
    assert_eq!(found, 1);
}

#[test]
fn test_e2e_numbers_a_labeled_equation_and_resolves_an_eq_reference_to_it() {
    // Given a document with two labeled equations and a reference to the second
    let input = "\
Math
====

.. math::
   :label: first

   a = b

.. math::
   :label: second

   c = d

As shown in :eq:`second`.
";

    // When the real pipeline runs: parse, analyze, render
    let doc = parser::parse("math.rst", input);
    let index = analyzer::analyze(&doc);
    let output = renderer::render(&doc, &index, "math.rst");

    // Then the second equation is numbered (2) and the reference links to it
    assert!(
        output
            .html
            .contains("<div class=\"math notranslate nohighlight\" id=\"equation-second\">"),
        "{}",
        output.html
    );
    assert!(
        output.html.contains("<span class=\"eqno\">(2)"),
        "{}",
        output.html
    );
    assert!(
        output.html.contains(
            "<a class=\"reference internal\" href=\"math.html#equation-second\">\
             <span class=\"eqno\">(2)</span></a>"
        ),
        "{}",
        output.html
    );
    assert!(output.broken_links.is_empty(), "{:?}", output.broken_links);
    assert!(output.math_errors.is_empty(), "{:?}", output.math_errors);
}

#[test]
fn test_e2e_renders_inline_math_as_mathml() {
    // Given a paragraph with an inline equation
    let input = "The identity :math:`a^2 + b^2 = c^2` is Pythagoras'.\n";

    // When
    let result = process_rst("test.rst", input);

    // Then the LaTeX became MathML on the page, with its source preserved
    assert!(
        result.contains("<span class=\"math notranslate nohighlight\"><math>"),
        "{result}"
    );
    assert!(
        result.contains("<msup><mi>a</mi><mn>2</mn></msup>"),
        "{result}"
    );
    assert!(
        result.contains("<annotation encoding=\"application/x-tex\">a^2 + b^2 = c^2</annotation>"),
        "{result}"
    );
}

#[test]
fn test_e2e_reports_an_eq_reference_to_an_unlabeled_equation_as_broken() {
    // Given an unlabeled equation and a reference that expects a number for it
    let input = ".. math::\n\n   a = b\n\nSee :eq:`nope`.\n";

    // When
    let doc = parser::parse("math.rst", input);
    let index = analyzer::analyze(&doc);
    let output = renderer::render(&doc, &index, "math.rst");

    // Then the reference is reported, under the code a `.. noqa:` would name
    assert_eq!(output.broken_links.len(), 1);
    assert_eq!(
        output.broken_links[0].code(),
        ast::DiagnosticCode::LinkBrokenEquation
    );
    assert_eq!(output.broken_links[0].target, "nope");
}

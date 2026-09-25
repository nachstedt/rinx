//! Rendering output tests: escaping, tables (grid and list-table), and
//! enumerated lists, including cross-references and doctest blocks nested
//! inside list items.

use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_parser as parser;
use rinx_renderer as renderer;
use rinx_worker::process_rst;

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

#[test]
fn test_e2e_code_block_options_reach_the_rendered_page() {
    // Given a code block using the options that each need a different phase:
    // `:caption:` and `:name:` are parsed, the language drives highlighting at
    // render time, and `:linenos:`/`:emphasize-lines:` shape the line markup
    let input = "\
.. code-block:: python
   :linenos:
   :emphasize-lines: 2
   :caption: An example
   :name: my-block

   def greet(name):
       return name
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the caption, the anchor, the numbers and the emphasis band
    assert!(
        result.contains("<div class=\"highlight-python notranslate\" id=\"my-block\">"),
        "expected a named, language-classed wrapper in:\n{result}"
    );
    assert!(
        result.contains("<span class=\"caption-text\">An example</span>"),
        "expected the caption in:\n{result}"
    );
    assert!(
        result.contains("<span class=\"linenos\">1</span>"),
        "expected line numbers in:\n{result}"
    );
    assert!(
        result.contains("<span class=\"hll\">"),
        "expected an emphasized line in:\n{result}"
    );
    // And the code itself is highlighted rather than merely escaped
    assert!(
        result.contains("hl-keyword"),
        "expected token classes in:\n{result}"
    );
    // And no option line leaked into the code, which is what this feature
    // exists to fix
    assert!(
        !result.contains(":linenos:"),
        "an option line was rendered as code in:\n{result}"
    );
}

#[test]
fn test_e2e_highlight_directive_sets_the_language_for_following_blocks() {
    // Given a `.. highlight::` followed by blocks that name no language —
    // both a directive and a `::` literal block, which Sphinx also highlights
    let input = "\
.. highlight:: rust

.. code-block::

   let x = 1;

Some prose::

    let y = 2;
";

    // When
    let result = process_rst("test.rst", input);

    // Then — both inherited Rust
    assert!(
        result.contains("highlight-rust"),
        "the directive should inherit Rust in:\n{result}"
    );
    assert_eq!(
        result.matches("hl-source hl-rust").count(),
        2,
        "both blocks should be highlighted as Rust in:\n{result}"
    );
}

#[test]
fn test_e2e_code_block_name_resolves_as_a_reference_target() {
    // Given a named code block and a `:ref:` pointing at it
    let input = "\
See :ref:`my-code`.

.. code-block:: python
   :name: my-code

   x = 1
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the analyzer registered the name, so the link resolves
    assert!(
        result.contains("href=\"test.html#my-code\""),
        "expected a resolved reference in:\n{result}"
    );
}

#[test]
fn test_e2e_unknown_code_block_language_still_shows_the_source() {
    // Given a language no grammar covers
    let input = "\
.. code-block:: nonesuch-language

   keep me visible
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the block degrades to plain text rather than vanishing
    assert!(
        result.contains("keep me visible"),
        "the source must survive a highlighting failure in:\n{result}"
    );
    assert!(
        !result.contains("<span class=\"hl-"),
        "nothing should be highlighted in:\n{result}"
    );
}

#[test]
fn test_e2e_unknown_directive_shows_its_source_instead_of_vanishing() {
    // Given a directive name this build does not implement, holding content
    // that would otherwise be lost with it — an unknown directive's body is
    // never parsed
    let input = "\
.. mermaid:: A flowchart

   graph TD;
     A --> B;
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the page names what it could not render and quotes the source
    assert!(
        result.contains("unknown directive type 'mermaid'"),
        "the page must say what it could not render in:\n{result}"
    );
    assert!(
        result.contains("graph TD;"),
        "the directive's content must survive in:\n{result}"
    );
    assert!(
        result.contains("class=\"directive-error\""),
        "the block must be marked up so a stylesheet can make it visible in:\n{result}"
    );
}

#[test]
fn test_e2e_malformed_directive_is_reported_against_what_is_wrong_with_it() {
    // Given a directive whose *name* this build knows and whose content it
    // cannot use
    let input = "\
.. figure::

   A caption for a picture that was never named.
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the block says what is actually wrong, never that the name was
    // unknown
    assert!(
        result.contains("figure: the directive needs an image path or URL as its argument"),
        "the block must carry the diagnostic's own reason in:\n{result}"
    );
    assert!(
        !result.contains("unknown directive type"),
        "a recognized name must not be blamed in:\n{result}"
    );
}

#[test]
fn test_e2e_replace_substitution_renders_its_resolved_content() {
    // Given — the definition follows its use, as it typically does in real
    // documents (CPython's own docs define `|release|` once, near the bottom
    // of a shared prelude).
    let input = "\
Version |release| is current.

.. |release| replace:: 3.13.0
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the reference is spliced with the definition's text, and the
    // definition itself contributes no visible output of its own.
    assert_eq!(result, "<p>Version 3.13.0 is current.</p>\n");
}

#[test]
fn test_e2e_replace_substitution_carries_inline_markup() {
    // Given — docutils documents `replace` as a workaround for the still
    // missing support for nested inline markup.
    let input = "\
.. |Python| replace:: *Python*

I recommend you try |Python|.
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    assert!(
        result.contains("I recommend you try <em>Python</em>."),
        "expected the substitution's emphasis to survive, got:\n{result}"
    );
}

#[test]
fn test_e2e_unicode_substitution_renders_the_decoded_character() {
    // Given
    let input = "\
.. |copy| unicode:: 0xA9 .. copyright sign

Copyright |copy| 2024.
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    assert!(
        result.contains("Copyright \u{a9} 2024."),
        "expected the decoded copyright sign, got:\n{result}"
    );
}

#[test]
fn test_e2e_image_substitution_renders_an_inline_img_element() {
    // Given — docutils' own canonical example of an image substitution
    let input = "\
|biohazard| ahead.

.. |biohazard| image:: biohazard.png
   :alt: a biohazard symbol
";

    // When
    let result = process_rst("test.rst", input);

    // Then — an `<img>` sits inline in the paragraph, not a block of its own
    assert!(
        result.contains(
            "<p><img src=\"_images/biohazard.png\" alt=\"a biohazard symbol\" /> ahead.</p>"
        ),
        "expected an inline image, got:\n{result}"
    );
}

#[test]
fn test_e2e_undefined_substitution_reference_keeps_the_written_text() {
    // Given
    let input = "See |no-such-thing| for details.\n";

    // When
    let result = process_rst("test.rst", input);

    // Then — degrades visibly rather than vanishing silently
    assert!(
        result.contains("See |no-such-thing| for details."),
        "expected the literal reference text, got:\n{result}"
    );
}

#[test]
fn test_e2e_contents_directive_lists_sections_with_working_links() {
    // Given a document with a `.. contents::` and two real sections — real
    // RST text through the full parse -> analyze -> render pipeline, so the
    // entry hrefs and the heading anchors they point at both come from the
    // same slug algorithm rather than from hand-written test doubles.
    let input = "\
Guide
=====

.. contents::

Getting Started
----------------

Text.

Advanced Topics
----------------

More text.
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the table of contents lists both sections, and each entry's
    // href actually lands on the heading it names.
    assert!(
        result.contains("<div class=\"contents topic\" id=\"contents\">"),
        "expected a contents block, got:\n{result}"
    );
    assert!(
        result.contains("<a id=\"toc-entry-1\" href=\"#getting-started\">Getting Started</a>"),
        "expected a Getting Started entry, got:\n{result}"
    );
    assert!(
        result.contains("<h2 id=\"getting-started\">"),
        "expected the heading it links to, got:\n{result}"
    );
    assert!(
        result.contains("<a id=\"toc-entry-2\" href=\"#advanced-topics\">Advanced Topics</a>"),
        "expected an Advanced Topics entry, got:\n{result}"
    );
}

#[test]
fn test_e2e_contents_local_lists_only_the_enclosing_sections_subsections() {
    // Given — the `:local:` contents sits under "Advanced", so it should
    // list only its own subsections, not the sibling "Basics".
    let input = "\
Guide
=====

Basics
------

Text.

Advanced
--------

.. contents::
   :local:

Details
~~~~~~~

Text.

Tips
~~~~

Text.
";

    // When
    let result = process_rst("test.rst", input);

    // Then
    assert!(
        result.contains("<a id=\"toc-entry-1\" href=\"#details\">Details</a>"),
        "expected a Details entry, got:\n{result}"
    );
    assert!(
        result.contains("<a id=\"toc-entry-2\" href=\"#tips\">Tips</a>"),
        "expected a Tips entry, got:\n{result}"
    );
    assert!(
        !result.contains("href=\"#basics\""),
        "Basics is a sibling, not a subsection, and should not be listed:\n{result}"
    );
}

#[test]
fn test_e2e_contents_backlinks_and_name_round_trip() {
    // Given — an explicit `:name:` (so a `:ref:` could reach it) and the
    // default `:backlinks: entry`, which should link the heading back to its
    // own table-of-contents entry.
    let input = "\
Guide
=====

.. contents::
   :name: toc

Overview
--------

Text.
";

    // When
    let result = process_rst("test.rst", input);

    // Then — the block uses the explicit name as its own anchor...
    assert!(
        result.contains("<div class=\"contents topic\" id=\"toc\">"),
        "expected the explicit name as the block's anchor, got:\n{result}"
    );
    // ...the entry carries a backlink target id...
    assert!(
        result.contains("<a id=\"toc-entry-1\" href=\"#overview\">Overview</a>"),
        "expected the entry to carry a backlink id, got:\n{result}"
    );
    // ...and the heading links back to exactly that id.
    assert!(
        result.contains("<a class=\"toc-backref\" href=\"#toc-entry-1\">Overview</a>"),
        "expected the heading to link back to its own entry, got:\n{result}"
    );
}

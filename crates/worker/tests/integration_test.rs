//! Integration tests for Rusty Sphinx public API.

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer as renderer;
use rusty_sphinx_worker::process_rst;

#[test]
fn test_parser_step() {
    let input = "Title\n=====\n\nParagraph text here.\nMore text.";
    let ast = parser::parse("test.rst", input);
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(
        ast.nodes[0],
        ast::Node::Heading {
            level: 1,
            text: vec![ast::InlineNode::Text("Title".to_string())]
        }
    );
    assert_eq!(
        ast.nodes[1],
        ast::Node::Paragraph(vec![ast::InlineNode::Text(
            "Paragraph text here.\nMore text.".to_string()
        )])
    );
}

#[test]
fn test_renderer_step() {
    let doc = ast::Document::new(
        "test.rst".to_string(),
        vec![
            ast::Node::Heading {
                level: 1,
                text: vec![ast::InlineNode::Text("Section".to_string())],
            },
            ast::Node::Paragraph(vec![ast::InlineNode::Text("A line of text.".to_string())]),
        ],
    );
    let index = analyzer::analyze(&doc);
    let html = renderer::render(&doc, &index, &doc.path).html;

    let expected = "<h1>Section</h1>\n<p>A line of text.</p>\n";
    assert_eq!(html, expected);
}

#[test]
fn test_e2e_translation() {
    let input = "\
Overview
--------

This is a simple paragraph.
It spans multiple lines.

Another Heading
~~~~~~~~~~~~~~~

And another paragraph.
";

    // Overview uses '-' (first seen) -> h1
    // Another Heading uses '~' (second seen) -> h2
    let expected_html = "\
<h1>Overview</h1>
<p>This is a simple paragraph.\nIt spans multiple lines.</p>
<h2>Another Heading</h2>
<p>And another paragraph.</p>
";

    let result = process_rst("test.rst", input);
    assert_eq!(result, expected_html);
}

#[test]
fn test_e2e_multi_level_headings() {
    let input = "\
Level 1
=======

Text 1.

Level 2
-------

Text 2.

Level 3
~~~~~~~

Text 3.

Another Level 1
===============

Text 4.
";

    let expected_html = "\
<h1>Level 1</h1>
<p>Text 1.</p>
<h2>Level 2</h2>
<p>Text 2.</p>
<h3>Level 3</h3>
<p>Text 3.</p>
<h1>Another Level 1</h1>
<p>Text 4.</p>
";

    let result = process_rst("test.rst", input);
    assert_eq!(result, expected_html);
}

#[test]
fn test_e2e_module_qualifies_sibling_function_for_cross_reference() {
    // Given — the real-world CPython shape that surfaced the
    // "broken domain object 'types.coroutine'" warning: `py:module` and the
    // `py:function` it documents are written as siblings (not nested), and
    // a reference elsewhere in the document uses the fully qualified name.
    let input = "\
.. py:module:: types

.. py:function:: coroutine(gen_func)

   Converts a generator function into a coroutine function.

See :py:func:`types.coroutine` for details.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no broken domain object reference, and the function's own
    // signature is still rendered unqualified.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:function:types.coroutine\">")
    );
    assert!(
        output
            .html
            .contains("<code class=\"sig-name\">coroutine(gen_func)</code>")
    );
    assert!(output.html.contains("#py:function:types.coroutine\""));
}

#[test]
fn test_e2e_module_qualifies_bare_reference_to_sibling_exception() {
    // Given — the real-world CPython shape that surfaced the
    // "broken domain object 'ZipImportError'" warning: `py:module` and the
    // `py:exception` it documents are written as siblings, and a reference
    // elsewhere in the document uses the *bare*, unqualified name (as real
    // Sphinx docs idiomatically do) rather than spelling out the module
    // prefix by hand.
    let input = "\
.. module:: zipimport

.. exception:: ZipImportError

   Exception raised by zipimporter objects.

Raising a :exc:`ZipImportError` signals an import failure.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no broken domain object reference, and the bare reference
    // resolves to the module-qualified anchor.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:exception:zipimport.zipimporterror\">")
    );
    assert!(
        output
            .html
            .contains("#py:exception:zipimport.zipimporterror\"")
    );
}

#[test]
fn test_e2e_class_qualifies_bare_reference_to_nested_method() {
    // Given — the real-world CPython shape that surfaced the
    // "broken domain object 'find_spec'" warning: `py:module` and `py:class`
    // are siblings, `py:method` is nested inside the class body, and a bare
    // reference elsewhere in that same body must resolve against the
    // enclosing class's qualified name, not just its own module.
    let input = "\
.. module:: zipimport

.. class:: zipimporter(archivepath)

   Create a new zipimporter instance.

   .. method:: find_spec(fullname, target=None)

      An implementation of importlib.abc.PathEntryFinder.find_spec.

   Use :meth:`find_spec` instead of the deprecated finder methods.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no broken domain object reference, and the bare reference
    // resolves to the class-and-module-qualified anchor.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:method:zipimport.zipimporter.find_spec\">")
    );
    assert!(
        output
            .html
            .contains("#py:method:zipimport.zipimporter.find_spec\"")
    );
}

#[test]
fn test_e2e_bare_reference_inside_class_falls_back_to_module_scope() {
    // Given — the real-world CPython shape that surfaced the remaining
    // "broken domain object 'ZipImportError'" warnings even after
    // module- and class-scoped resolution were added individually: the
    // exception is a *module*-level sibling (never nested in the class),
    // but it's referenced bare from *inside* the class's own body. The
    // class-qualified guess must miss and fall through to the
    // module-qualified one, not go straight to a bare global lookup.
    let input = "\
.. module:: zipimport

.. exception:: ZipImportError

   Exception raised by zipimporter objects.

.. class:: zipimporter(archivepath)

   :exc:`ZipImportError` is raised if *archivepath* doesn't point to a
   valid ZIP archive.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no broken domain object reference, and the bare reference
    // resolves to the module-qualified anchor, not a class-qualified one.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:exception:zipimport.zipimporterror\">")
    );
    assert!(
        output
            .html
            .contains("#py:exception:zipimport.zipimporterror\"")
    );
}

#[test]
fn test_e2e_dotted_method_scopes_bare_reference_in_its_own_body() {
    // Given — the real-world CPython shape that surfaced the
    // "broken domain object 'read'" warning in `Doc/library/zipfile`:
    // `ZipFile`'s methods are documented *flat*, as siblings with dotted
    // signatures rather than nested inside `.. class:: ZipFile`, and one
    // method's body refers to a sibling method by its bare name. The
    // enclosing method's own name-prefix has to supply the class scope —
    // there is no enclosing `.. class::` body to supply it.
    let input = "\
.. module:: zipfile

.. method:: ZipFile.open(name, mode='r')

   The :meth:`read` method can also take a filename.

.. method:: ZipFile.read(name, pwd=None)

   Return the bytes of the file in the archive.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no broken domain object reference, and the bare reference
    // resolves to the sibling method's class-and-module-qualified anchor.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:method:zipfile.zipfile.read\">")
    );
    assert!(output.html.contains("#py:method:zipfile.zipfile.read\""));
}

#[test]
fn test_e2e_method_repeating_bare_class_name_is_not_double_qualified() {
    // Given — the real-world CPython shape from `Doc/library/random`: a
    // method nested inside its class, but written with the class's *bare*
    // name repeated in its own signature, while the class itself is
    // module-qualified. The qualifier ("random.Random") and the repeated
    // prefix ("Random.") don't match textually, so a naive already-qualified
    // guard double-prepends and indexes it as "random.Random.Random.seed".
    let input = "\
.. module:: random

.. class:: Random([seed])

   Default pseudo-random number generator.

   .. method:: Random.seed(a=None, version=2)

      Reinitialize the generator.

   Call :meth:`Random.seed` to reseed the generator.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — the method is indexed and anchored under the class exactly
    // once, and the bare reference resolves to that same anchor.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:method:random.random.seed\">")
    );
    assert!(!output.html.contains("random.random.random.seed"));
    assert!(output.html.contains("#py:method:random.random.seed\""));
}

#[test]
fn test_e2e_class_definition_resolves_both_class_and_exc_role_references() {
    // Given — the real-world CPython shape that surfaced the
    // "broken domain object 'Fault'" warning in `xmlrpc.client.rst`: the
    // object is defined via `.. class::` but referenced via both `:exc:`
    // and `:class:` roles, which real Sphinx treats as mutually aliasable.
    let input = "\
.. class:: Fault

   Encapsulates the content of an XML-RPC fault tag.

Both :exc:`Fault` and :class:`Fault` refer to the same object.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no broken domain object reference, and both references' hrefs
    // point at the anchor the `class` definition actually rendered.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("<dt id=\"py:class:fault\">"));
    assert_eq!(output.html.matches("#py:class:fault\"").count(), 2);
}

#[test]
fn test_e2e_exception_definition_resolves_class_role_reference() {
    // Given — the mirror case: an object defined via `.. exception::`
    // referenced via `:class:`, proving the aliasing is bidirectional.
    let input = "\
.. exception:: Fault

   Encapsulates the content of an XML-RPC fault tag.

Use :class:`Fault` to catch it.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("<dt id=\"py:exception:fault\">"));
    assert!(output.html.contains("#py:exception:fault\""));
}

#[test]
fn test_e2e_classmethod_alias_directive_is_indexed_and_resolves() {
    // Given — the real-world CPython shape that surfaced the
    // "broken domain object 'ZoneInfo.clear_cache'" warning in
    // `Doc/library/zoneinfo`: the method is defined via the legacy
    // `.. classmethod::` directive spelling (a `py:method` alias), as a
    // sibling of the `py:module` and `py:class` it belongs to, and referenced
    // via `:meth:` using its class-qualified name.
    let input = "\
.. module:: zoneinfo

.. class:: ZoneInfo(key)

   A concrete tzinfo subclass.

.. classmethod:: ZoneInfo.clear_cache(*, only_keys=None)

   Clear the ZoneInfo cache.

Invalidate the cache via :meth:`ZoneInfo.clear_cache`.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — the alias directive was indexed as a `py:method` (not dropped as
    // an unknown directive), so the reference resolves with no broken link.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:method:zoneinfo.zoneinfo.clear_cache\">")
    );
    assert!(
        output
            .html
            .contains("#py:method:zoneinfo.zoneinfo.clear_cache\"")
    );
    // And — the `classmethod` prefix label is rendered, from the forced flag.
    assert!(
        output
            .html
            .contains("<em class=\"property\">classmethod</em>")
    );
}

#[test]
fn test_e2e_decorator_directive_is_indexed_and_resolves() {
    // Given — `known_bugs.md`'s bug #1: CPython's `Doc/reference/datamodel`
    // documents `classmethod` via `.. decorator::` and references it with
    // `:func:` (real Sphinx registers a decorator exactly as a `py:function`
    // — `PyDecoratorFunction.run()` forces `self.name = 'py:function'` before
    // delegating), which rusty-sphinx used to drop entirely as an unknown
    // directive, breaking every reference to it.
    let input = "\
.. decorator:: classmethod

   Transform a method into a class method.

Retrieving a :func:`classmethod` object.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — the reference resolves against the `py:function` key a plain
    // `.. function::` would have produced.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("<dt id=\"py:function:classmethod\">"));
    assert!(output.html.contains("#py:function:classmethod\""));
    // And — real Sphinx's `PyDecoratorFunction` prefixes the rendered
    // signature with a literal `@` (`desc_addname('@', '@')`).
    assert!(
        output
            .html
            .contains("<code class=\"sig-name\">@classmethod</code>")
    );
}

#[test]
fn test_e2e_decoratormethod_directive_is_indexed_and_resolves() {
    // Given — the `py:method` counterpart: `.. decoratormethod::` nested
    // inside a class, referenced via `:meth:` using its class-qualified name.
    let input = "\
.. class:: Traits

   A trait-registering metaclass helper.

   .. decoratormethod:: register(cls)

      Registers the decorated class as a trait.

See :meth:`Traits.register` for details.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(
        output
            .html
            .contains("<dt id=\"py:method:traits.register\">")
    );
    assert!(output.html.contains("#py:method:traits.register\""));
    assert!(
        output
            .html
            .contains("<code class=\"sig-name\">@register(cls)</code>")
    );
}

#[test]
fn test_e2e_c_macro_definition_resolves_c_func_role_reference() {
    // Given — the real-world CPython shape from `c-api/gcsupport.rst`: the
    // function-like macro `Py_VISIT` is defined via `.. c:macro::` but
    // referenced via `:c:func:` from the same file. `gcsupport.rst` is not in
    // CPython's `Doc/tools/.nitignore`, so real Sphinx resolves this.
    let input = "\
.. c:macro:: Py_VISIT(o)

   A macro.

A typical traverse function calls the :c:func:`Py_VISIT` macro.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no broken reference, and the href points at the anchor the
    // `c:macro` definition actually rendered.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("<dt id=\"c:macro:py_visit\">"));
    assert_eq!(output.html.matches("#c:macro:py_visit\"").count(), 1);
}

#[test]
fn test_e2e_c_function_definition_resolves_c_macro_role_reference() {
    // Given — the mirror case, equally real: `Py_REFCNT` is defined
    // `.. c:function::` in `c-api/refcounting.rst` and referenced via
    // `:c:macro:` from `c-api/structures.rst`, proving the aliasing is
    // bidirectional.
    let input = "\
.. c:function:: Py_ssize_t Py_REFCNT(PyObject *o)

   Returns the reference count.

The reference count is accessible via :c:macro:`Py_REFCNT`.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("<dt id=\"c:function:py_refcnt\">"));
    assert_eq!(output.html.matches("#c:function:py_refcnt\"").count(), 1);
}

#[test]
fn test_e2e_c_macro_definition_does_not_resolve_c_member_role_reference() {
    // Given — the alias relation is symmetric but deliberately not
    // transitive. `c:function` aliases `c:macro`, and `c:macro` aliases
    // `c:member`, but a `.. c:function::` definition must NOT resolve a
    // `:c:member:` reference.
    let input = "\
.. c:function:: int add(int a, int b)

   Adds two numbers.

See :c:member:`add` for details.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert_eq!(output.broken_links.len(), 1);
}

#[test]
fn test_e2e_function_definition_does_not_resolve_exc_role_reference() {
    // Given — a negative case guarding the alias table stays intentionally
    // small: it holds only the collisions the corpus confirms (`py`'s
    // `class`/`exception`, and `c`'s `macro`/`member` and `function`/`macro`),
    // so a `.. function::` definition must NOT resolve an `:exc:` reference.
    let input = "\
.. function:: Fault(x)

   Not actually an exception.

See :exc:`Fault` for details.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert!(
        !output.broken_links.is_empty(),
        "expected a broken link for the unaliased objtype mismatch"
    );
    assert!(output.html.contains("class=\"broken-link\""));
}

#[test]
fn test_e2e_dot_prefixed_reference_prefers_the_enclosing_module() {
    // Given — the CPython `datetime` shape behind the benchmark's
    // highest-frequency unresolved reference (57 occurrences of
    // `'.datetime' (referenced as py:class)`): a module and its main class
    // share a name, and the leading dot says "the one nearby".
    let input = "\
.. module:: datetime

.. class:: datetime(year, month, day)

   A combined date and time.

Use :class:`.datetime` to combine both.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — resolved to the class, not left broken, and the dot is markup
    // that never reaches the reader.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("#py:class:datetime.datetime\""));
    assert!(output.html.contains(">datetime</code>"));
    assert!(!output.html.contains(">.datetime<"));
}

#[test]
fn test_e2e_undotted_reference_prefers_the_unqualified_name() {
    // Given — the same document, referenced without a dot: Sphinx searches
    // the unqualified name first, which here is the module itself.
    let input = "\
.. module:: datetime

.. class:: datetime(year, month, day)

   A combined date and time.

The :mod:`datetime` module supplies classes.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("#py:module:datetime\""));
}

#[test]
fn test_e2e_dot_prefixed_reference_falls_back_to_a_suffix_match() {
    // Given — the Sphinx documentation's own example: `:meth:`.TarFile.close``
    // resolves as a suffix "even if the current module is not tarfile".
    let input = "\
.. module:: shutil

.. class:: TarFile()

   .. method:: TarFile.close()

      Close the archive.

.. module:: other

Call :meth:`.TarFile.close` when done.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then — no exact tier can match under module `other`, so the suffix
    // search finds the method anyway.
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    assert!(output.html.contains("#py:method:shutil.tarfile.close\""));
}

#[test]
fn test_e2e_ambiguous_dot_prefixed_reference_is_left_unresolved() {
    // Given — two classes documenting a `close` method and a dot-prefixed
    // reference naming neither unambiguously. Real Sphinx links the first
    // match; here nothing is linked and the candidates are reported, so the
    // author disambiguates rather than inheriting an arbitrary choice.
    let input = "\
.. class:: TarFile()

   .. method:: TarFile.close()

      Close the archive.

.. class:: ZipFile()

   .. method:: ZipFile.close()

      Close the archive.

Call :meth:`.close` when done.
";

    let ast = parser::parse("test.rst", input);
    let index = analyzer::analyze(&ast);
    let output = renderer::render(&ast, &index, &ast.path);

    // Then
    assert!(output.html.contains("class=\"broken-link\""));
    assert_eq!(
        output.broken_links,
        vec![renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::AmbiguousDomainObjectReference {
                object_type: ast::ObjectType::Py(ast::PyObjectType::Method),
                candidates: vec!["tarfile.close".to_string(), "zipfile.close".to_string()],
            },
            target: "close".to_string(),
        }]
    );
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

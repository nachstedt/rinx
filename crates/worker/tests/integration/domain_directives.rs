//! `py` domain directive/role aliasing: `class`/`exception`, the legacy
//! `classmethod`/`decorator`/`decoratormethod` directive spellings, and the
//! negative case guarding the alias table against growing unintended
//! collisions.

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer as renderer;

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

//! Real-world `CPython` shapes exercising the Python-domain scope stack:
//! module- and class-qualified bare references resolved from the
//! enclosing scope rather than requiring the author to spell out the
//! fully qualified name.

use rinx_analyzer as analyzer;
use rinx_parser as parser;
use rinx_renderer as renderer;

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

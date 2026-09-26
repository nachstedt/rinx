//! `c` domain `function`/`macro`/`member` role aliasing: symmetric where the
//! corpus confirms it, but deliberately not transitive.

use rinx_analyzer as analyzer;
use rinx_parser as parser;
use rinx_renderer as renderer;

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

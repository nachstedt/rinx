//! The `:any:` role end to end: one document defining a target of every kind
//! it searches, referenced through the role alone. The expectations follow
//! what `sphinx-build` 9 renders for the same source, with the deviations
//! `docs/compatibility.rst` records.

use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_parser as parser;
use rinx_renderer as renderer;

const DEFINITIONS: &str = "\
Top
===

.. _install:

Installing
----------

.. glossary::

   Widget
      A thing.

.. program:: prog

.. option:: --verbose

   Talk.

.. py:module:: pkg

.. py:function:: run()

.. py:class:: Box

   .. py:method:: close()

.. c:macro:: MY_MACRO

.. math::
   :label: euler

   e = 1

";

/// Parses, indexes and renders `DEFINITIONS` followed by `references`.
fn render(references: &str) -> renderer::RenderOutput {
    let ast = parser::parse("index.rst", &format!("{DEFINITIONS}{references}"));
    assert!(ast.diagnostics.is_empty(), "{:?}", ast.diagnostics);
    let index = analyzer::analyze(&ast);
    renderer::render(&ast, &index, &ast.path)
}

#[test]
fn test_e2e_any_resolves_every_kind_of_target_the_way_its_own_role_does() {
    // Given / When
    let output = render(
        "\
- :any:`install`
- :any:`widget`
- :any:`--verbose`
- :any:`run()`
- :any:`Box.close`
- :any:`MY_MACRO`
- :any:`euler`
- :any:`index`
",
    );

    // Then
    assert!(
        output.broken_links.is_empty(),
        "expected no broken links, got {:?}",
        output.broken_links
    );
    let html = &output.html;
    assert!(
        html.contains("href=\"index.html#install\">Installing</a>"),
        "{html}"
    );
    assert!(
        html.contains("#term-widget\"><span class=\"xref any std std-term\">"),
        "{html}"
    );
    assert!(html.contains(">--verbose</code>"), "{html}");
    assert!(
        html.contains("#py:function:pkg.run\"><code class=\"xref any py function"),
        "{html}"
    );
    assert!(html.contains("#py:method:pkg.box.close\""), "{html}");
    assert!(
        html.contains("<code class=\"xref any c macro docutils literal\">MY_MACRO</code>"),
        "{html}"
    );
    assert!(html.contains("<span class=\"eqno\">(1)</span>"), "{html}");
    assert!(html.contains("<span class=\"doc\">Top</span>"), "{html}");
}

#[test]
fn test_e2e_any_reports_an_ambiguous_target_and_links_nothing() {
    // Given — `close` is a method of `Box` and a function of another module,
    // and no scope tier names either exactly
    let output = render(
        "\
.. py:module:: other

.. py:function:: close()

.. py:module:: third

Call :any:`close`.
",
    );

    // Then
    assert_eq!(output.broken_links.len(), 1, "{:?}", output.broken_links);
    let link = &output.broken_links[0];
    assert_eq!(link.code(), ast::DiagnosticCode::LinkAmbiguousAny);
    assert_eq!(
        link.kind,
        renderer::BrokenLinkKind::AmbiguousAnyReference {
            candidates: vec![
                ":py:meth:`pkg.box.close`".to_string(),
                ":py:func:`other.close`".to_string(),
            ],
        }
    );
    assert!(output.html.contains("class=\"broken-link\""));
}

#[test]
fn test_e2e_any_prefers_the_exact_name_in_scope_over_suffix_matches() {
    // Given — the same two `close`s, referenced from inside `other`, where
    // the module-qualified tier names exactly one
    let output = render(
        "\
.. py:module:: other

.. py:function:: close()

See :any:`close`.
",
    );

    // Then
    assert!(output.broken_links.is_empty(), "{:?}", output.broken_links);
    assert!(output.html.contains("#py:function:other.close\""));
}

#[test]
fn test_e2e_any_reports_an_unknown_target_and_takes_a_noqa() {
    // Given — one unknown target, and a noqa naming the role's own code
    let source = "\
:any:`nothing`

.. noqa: link.broken-any

:any:`nowhere`
";

    // When
    let ast = parser::parse("index.rst", source);
    let output = renderer::render(&ast, &analyzer::analyze(&ast), &ast.path);

    // Then — both are reported by the renderer; the noqa, accepted as a known
    // code, is what the worker filters the second one out with
    assert!(ast.diagnostics.is_empty(), "{:?}", ast.diagnostics);
    assert_eq!(ast.suppressions.len(), 1);
    let codes: Vec<_> = output
        .broken_links
        .iter()
        .map(renderer::BrokenLink::code)
        .collect();
    assert_eq!(
        codes,
        vec![
            ast::DiagnosticCode::LinkBrokenAny,
            ast::DiagnosticCode::LinkBrokenAny
        ]
    );
}

//! `:code:` and `.. role:: name(code)` end to end: one document parsed,
//! indexed and rendered. The class lists follow what `sphinx-build` 9.1 emits
//! for the same source, minus docutils' `docutils literal notranslate`, which
//! no inline literal here carries.

use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_entity::EntitySchema;
use rinx_parser as parser;
use rinx_renderer as renderer;

fn render_page(source: &str) -> (ast::Document, renderer::RenderOutput) {
    let doc = parser::parse("index.rst", source);
    let index =
        analyzer::build_project_index(std::slice::from_ref(&doc), "index", &EntitySchema::empty());
    let output = renderer::render(&doc, &index, &doc.path);
    (doc, output)
}

#[test]
fn test_e2e_code_role_and_a_derived_role_render_as_sphinx_classes_them() {
    // Given
    let source = "\
Code
====

.. role:: python(code)
   :language: python
   :class: extra

Plain :code:`a\\*b <c>` and highlighted :python:`print(\"x\")`.
";

    // When
    let (doc, output) = render_page(source);

    // Then
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert!(
        output.highlight_errors.is_empty(),
        "{:?}",
        output.highlight_errors
    );
    let html = &output.html;
    assert!(
        html.contains("<code class=\"code\">a*b &lt;c&gt;</code>"),
        "{html}"
    );
    assert!(
        html.contains("<code class=\"code highlight extra python highlight-python hl-code\">"),
        "{html}"
    );
    assert!(html.contains("<span class=\"hl-"), "{html}");
}

#[test]
fn test_e2e_code_role_with_an_unknown_language_is_reported_and_shown_plain() {
    // Given
    let source = "\
.. role:: bogus(code)
   :language: nosuchlang

Use :bogus:`z < 1`.
";

    // When
    let (_, output) = render_page(source);

    // Then — reported at the role, under the role's own code
    assert_eq!(output.highlight_errors.len(), 1);
    let error = &output.highlight_errors[0];
    assert_eq!(error.code(), ast::DiagnosticCode::CodeRoleUnknownLanguage);
    assert_eq!(error.span.map(|span| span.start.line), Some(4));
    assert!(
        output.html.contains(
            "<code class=\"code highlight bogus nosuchlang highlight-nosuchlang\">z &lt; 1</code>"
        ),
        "{}",
        output.html
    );
}

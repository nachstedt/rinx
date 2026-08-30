//! Dot-prefixed and unqualified reference matching against the domain-object
//! index: the "prefer the enclosing scope", "unqualified name first", and
//! "fall back to a suffix match" tiers, plus the ambiguous case where no
//! tier can pick a single winner.

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer as renderer;

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
            // The role sits on the document's thirteenth line, five
            // characters in — proving a position survives the whole pipeline,
            // out of a paragraph, past eleven lines of nested `class`/`method`
            // directives, and into a render-time diagnostic.
            span: Some(ast::Span::new(
                ast::Position::new(13, 6),
                ast::Position::new(13, 20),
            )),
        }]
    );
}

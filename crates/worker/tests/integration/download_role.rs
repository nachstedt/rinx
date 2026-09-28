//! The `:download:` role end to end: parsed in a page one directory deep and
//! rendered against an index, in each of its forms. The expectations follow
//! what `sphinx-build` 9.1 renders for the same source, with the one layout
//! deviation `docs/compatibility.rst` records — a file keeps its
//! source-root-relative path under `_downloads/` instead of a hashed
//! directory. Whether the file was declared is the site's asset validation's
//! business, covered by `tests/test_download_data.sh`.

use rinx_analyzer as analyzer;
use rinx_entity::EntitySchema;
use rinx_parser as parser;
use rinx_renderer as renderer;

/// Parses `body` as `guide/intro.rst`, indexes it and renders it.
fn render_intro(body: &str) -> renderer::RenderOutput {
    let doc = parser::parse("guide/intro.rst", &format!("Intro\n=====\n\n{body}"));
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let docs = vec![doc];
    let index = analyzer::build_project_index(&docs, "guide/intro", &EntitySchema::empty());
    renderer::render(&docs[0], &index, &docs[0].path)
}

#[test]
fn test_e2e_download_links_relative_absolute_and_titled_files() {
    // Given / When
    let output = render_intro(
        "\
- :download:`script.py`
- :download:`../data/table.csv`
- :download:`/shared/tool.sh`
- :std:download:`The table <../data/table.csv>`
",
    );

    // Then
    assert!(output.broken_links.is_empty(), "{:?}", output.broken_links);
    let html = &output.html;
    for expected in [
        "<a class=\"reference download internal\" download=\"\" \
         href=\"../_downloads/guide/script.py\"><code class=\"xref download docutils literal \
         notranslate\"><span class=\"pre\">script.py</span></code></a>",
        "href=\"../_downloads/data/table.csv\"><code class=\"xref download docutils literal \
         notranslate\"><span class=\"pre\">../data/table.csv</span></code></a>",
        "href=\"../_downloads/shared/tool.sh\"><code class=\"xref download docutils literal \
         notranslate\"><span class=\"pre\">/shared/tool.sh</span></code></a>",
        "href=\"../_downloads/data/table.csv\"><code class=\"xref download docutils literal \
         notranslate\"><span class=\"pre\">The</span> <span class=\"pre\">table</span></code></a>",
    ] {
        assert!(html.contains(expected), "expected {expected} in:\n{html}");
    }
}

#[test]
fn test_e2e_download_links_an_external_url_as_written() {
    // Given / When
    let output = render_intro(":download:`archive <https://example.com/a.zip>`\n");

    // Then
    assert!(
        output.html.contains(
            "<a class=\"reference download external\" download=\"\" \
             href=\"https://example.com/a.zip\">"
        ),
        "{}",
        output.html
    );
}

#[test]
fn test_e2e_download_with_a_bang_is_an_unlinked_literal() {
    // Given / When
    let output = render_intro(":download:`!Data <table.csv>`\n");

    // Then — the whole text after the `!`, and no link
    assert!(
        output.html.contains(
            "<code class=\"xref download docutils literal notranslate\">\
             <span class=\"pre\">Data</span> <span class=\"pre\">&lt;table.csv&gt;</span></code>"
        ),
        "{}",
        output.html
    );
    assert!(
        !output.html.contains("reference download"),
        "{}",
        output.html
    );
}

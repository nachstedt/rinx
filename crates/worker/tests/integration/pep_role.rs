//! `:pep:` end to end: one document parsed, indexed and rendered, and its
//! general index. The page markup follows what `sphinx-build` 9.1 emits for
//! the same source.

use rinx_analyzer as analyzer;
use rinx_ast as ast;
use rinx_entity::EntitySchema;
use rinx_parser as parser;
use rinx_renderer as renderer;

fn build(source: &str) -> (ast::Document, rinx_index::ProjectIndex) {
    let doc = parser::parse("index.rst", source);
    let index =
        analyzer::build_project_index(std::slice::from_ref(&doc), "index", &EntitySchema::empty());
    (doc, index)
}

#[test]
fn test_e2e_pep_role_links_the_pep_and_anchors_its_index_entry() {
    // Given
    let source = "\
PEPs
====

Follow :pep:`8`, read :pep:`the Zen <20>` and :pep:`484#type-aliases`.
";

    // When
    let (doc, index) = build(source);
    let output = renderer::render(&doc, &index, &doc.path);
    let genindex = renderer::render_genindex(
        &index,
        &renderer::config::SiteConfig::default(),
        "{{ body }}",
    )
    .unwrap();

    // Then — the page
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let html = &output.html;
    assert!(
        html.contains(
            "<span class=\"target\" id=\"index-0\"></span>\
             <a class=\"pep reference external\" href=\"https://peps.python.org/pep-0008/\">\
             <strong>PEP 8</strong></a>"
        ),
        "{html}"
    );
    assert!(
        html.contains("href=\"https://peps.python.org/pep-0020/\"><strong>the Zen</strong>"),
        "{html}"
    );
    assert!(
        html.contains(
            "href=\"https://peps.python.org/pep-0484/#type-aliases\">\
             <strong>PEP 484#type-aliases</strong>"
        ),
        "{html}"
    );
    // Then — the general index groups all three under Sphinx's heading, each
    // linking to its anchor
    assert!(
        genindex.contains("Python Enhancement Proposals"),
        "{genindex}"
    );
    for (entry, anchor) in [("PEP 8", "index-0"), ("PEP 20", "index-1")] {
        assert!(genindex.contains(entry), "{entry}: {genindex}");
        assert!(
            genindex.contains(&format!("index.html#{anchor}")),
            "{anchor}: {genindex}"
        );
    }
}

#[test]
fn test_e2e_pep_role_links_below_a_configured_base_url() {
    // Given
    let (doc, index) = build("See :pep:`8`.\n");
    let config: renderer::config::SiteConfig =
        toml::from_str("pep_base_url = \"https://mirror.example/peps/\"\n").unwrap();

    // When
    let output = renderer::render_with_config(&doc, &index, &doc.path, &config);

    // Then
    assert!(
        output
            .html
            .contains("href=\"https://mirror.example/peps/pep-0008/\""),
        "{}",
        output.html
    );
}

#[test]
fn test_e2e_pep_role_with_an_invalid_number_is_reported_and_shown_as_written() {
    // Given
    let (doc, index) = build("See :pep:`eight`.\n");

    // When
    let output = renderer::render(&doc, &index, &doc.path);

    // Then
    assert_eq!(doc.diagnostics.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(
        doc.diagnostics[0].code,
        ast::DiagnosticCode::PepInvalidNumber
    );
    assert!(output.html.contains(":pep:`eight`"), "{}", output.html);
    assert!(index.genindex_entries.is_empty());
}

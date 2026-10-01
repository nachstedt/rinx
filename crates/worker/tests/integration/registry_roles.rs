//! The registry roles — `:pep:`, `:rfc:`, `:cve:`, `:cwe:` — and docutils'
//! `:pep-reference:` and `:rfc-reference:` end to end: one document parsed,
//! indexed and rendered, and its general index. The page markup follows what `sphinx-build` 9.1 emits for
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

#[test]
fn test_e2e_pep_reference_role_links_the_pep_without_an_index_entry() {
    // Given — docutils' role between two of Sphinx's
    let (doc, index) = build("See :pep:`8`, :pep-reference:`08` and :pep:`20`.\n");

    // When
    let output = renderer::render(&doc, &index, &doc.path);

    // Then — docutils' plain link, no `<strong>`, no trailing slash
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let html = &output.html;
    assert!(
        html.contains(
            ", <a class=\"reference external\" href=\"https://peps.python.org/pep-0008\">\
             PEP 08</a> and "
        ),
        "{html}"
    );
    // Then — only the two `:pep:`s are indexed, numbered without a gap
    let anchors: Vec<&str> = index
        .genindex_entries
        .iter()
        .map(|entry| entry.anchor.as_str())
        .collect();
    assert_eq!(anchors, vec!["index-0", "index-1"]);
}

#[test]
fn test_e2e_pep_reference_role_links_below_a_configured_base_url() {
    // Given
    let (doc, index) = build("See :pep-reference:`8`.\n");
    let config: renderer::config::SiteConfig =
        toml::from_str("pep_base_url = \"https://mirror.example/peps/\"\n").unwrap();

    // When
    let output = renderer::render_with_config(&doc, &index, &doc.path, &config);

    // Then
    assert!(
        output
            .html
            .contains("href=\"https://mirror.example/peps/pep-0008\""),
        "{}",
        output.html
    );
}

#[test]
fn test_e2e_pep_reference_role_out_of_range_is_reported_and_shown_as_written() {
    // Given
    let (doc, index) = build("See :pep-reference:`10000`.\n");

    // When
    let output = renderer::render(&doc, &index, &doc.path);

    // Then
    assert_eq!(doc.diagnostics.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(
        doc.diagnostics[0].code,
        ast::DiagnosticCode::PepReferenceInvalidNumber
    );
    assert!(
        output.html.contains(":pep-reference:`10000`"),
        "{}",
        output.html
    );
}

#[test]
fn test_e2e_rfc_cve_and_cwe_roles_link_and_index_under_their_groups() {
    // Given
    let source = "\
Registries
==========

See :pep:`8`, :rfc:`2324#section-2.3.2`, :cve:`2024-3094` and
:cwe:`Out-of-bounds Write <787>`.
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

    // Then — the page, one anchor sequence across every registry
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let html = &output.html;
    for expected in [
        "<span class=\"target\" id=\"index-1\"></span>\
         <a class=\"rfc reference external\" \
         href=\"https://datatracker.ietf.org/doc/html/rfc2324.html#section-2.3.2\">\
         <strong>RFC 2324 Section 2.3.2</strong></a>",
        "<span class=\"target\" id=\"index-2\"></span>\
         <a class=\"cve reference external\" \
         href=\"https://www.cve.org/CVERecord?id=CVE-2024-3094\">\
         <strong>CVE 2024-3094</strong></a>",
        "<span class=\"target\" id=\"index-3\"></span>\
         <a class=\"cwe reference external\" \
         href=\"https://cwe.mitre.org/data/definitions/787.html\">\
         <strong>Out-of-bounds Write</strong></a>",
    ] {
        assert!(html.contains(expected), "{expected}\n{html}");
    }
    // Then — the general index files each under Sphinx's group
    for text in [
        "Python Enhancement Proposals",
        "RFC 2324 Section 2.3.2",
        "Common Vulnerabilities and Exposures",
        "CVE 2024-3094",
        "Common Weakness Enumeration",
        "CWE 787",
    ] {
        assert!(genindex.contains(text), "{text}: {genindex}");
    }
}

#[test]
fn test_e2e_rfc_role_links_below_a_configured_base_url() {
    // Given
    let (doc, index) = build("See :rfc:`2324` and :rfc-reference:`2822`.\n");
    let config: renderer::config::SiteConfig =
        toml::from_str("rfc_base_url = \"https://www.rfc-editor.org/rfc/\"\n").unwrap();

    // When
    let output = renderer::render_with_config(&doc, &index, &doc.path, &config);

    // Then
    for href in [
        "href=\"https://www.rfc-editor.org/rfc/rfc2324.html\"",
        "href=\"https://www.rfc-editor.org/rfc/rfc2822.html\"",
    ] {
        assert!(output.html.contains(href), "{href}: {}", output.html);
    }
}

#[test]
fn test_e2e_cve_role_with_a_prefixed_id_is_reported_and_shown_as_written() {
    // Given
    let (doc, index) = build("See :cve:`CVE-2024-3094`.\n");

    // When
    let output = renderer::render(&doc, &index, &doc.path);

    // Then
    assert_eq!(doc.diagnostics.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(doc.diagnostics[0].code, ast::DiagnosticCode::CveInvalidId);
    assert!(
        doc.diagnostics[0]
            .message
            .contains("drop the 'CVE-' prefix"),
        "{}",
        doc.diagnostics[0].message
    );
    assert!(
        output.html.contains(":cve:`CVE-2024-3094`"),
        "{}",
        output.html
    );
    assert!(index.genindex_entries.is_empty());
}

#[test]
fn test_e2e_rfc_reference_role_links_the_rfc_without_an_index_entry() {
    // Given
    let (doc, index) = build("See :rfc-reference:`02822#section-3`.\n");

    // When
    let output = renderer::render(&doc, &index, &doc.path);

    // Then — docutils' plain link: the number normalized, the section only
    // in the href
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert!(
        output.html.contains(
            "<a class=\"reference external\" \
             href=\"https://datatracker.ietf.org/doc/html/rfc2822.html#section-3\">RFC 2822</a>"
        ),
        "{}",
        output.html
    );
    assert!(index.genindex_entries.is_empty());
}
